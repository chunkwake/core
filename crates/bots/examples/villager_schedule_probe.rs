//! Расписание жителя (`timeline/villager_schedule.json` 26.3): днём идёт к
//! рабочему блоку, в `meet` — к колоколу, ночью — к кровати.
//!
//! `villager_schedule_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

const X: i32 = 3456;
const Z: i32 = 8;
const Y: i32 = 100;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_carried: i32,
    cb_chat: i32,
    cb_meta: i32,
    cb_block: i32,
    cb_section: i32,
    cb_add: i32,
    cb_move: i32,
    cb_teleport: i32,
    cb_equip: i32,
    cb_block_event: i32,
    chat: String,
    /// Колокол рейда и признак удара (`block_event` 1).
    bell: [i32; 3],
    bell_rang: bool,
    /// ID типов рейдеров и факт их появления.
    raider_types: HashSet<i32>,
    raiders_seen: bool,
    firework_type: i32,
    firework_seen: bool,
    cat_type: i32,
    cat_id: Option<i32>,
    villager_type: i32,
    seen_villagers: HashSet<i32>,
    babies: HashSet<i32>,
    /// Позиции и yaw всех жителей (для детских игр).
    all_pos: HashMap<i32, [f64; 3]>,
    all_yaw: HashMap<i32, f32>,
    bases: HashMap<i32, [i64; 3]>,
    /// Первый житель (спавн у X+0.5) и его позиция из пакетов.
    a_id: Option<i32>,
    /// Второй житель (NoAI) из фазы встречи.
    b_id: Option<i32>,
    a_pos: Option<[f64; 3]>,
    a_yaw: f32,
    a_base: [i64; 3],
    /// Предмет в основной руке A из `set_equipment` (ShowTradesToPlayer).
    a_hand: Option<i32>,
    /// Последняя поза из `set_entity_data` (индекс 6).
    pose: Option<i32>,
    /// Дверь на пути и ID её состояний (`block_update`).
    door: [i32; 3],
    door_open: i32,
    door_closed: i32,
    saw_open: bool,
    saw_close: bool,
}

impl Probe {
    fn packet(&self, id: i32) -> Writer {
        let mut w = Writer::new();
        w.var_int(id);
        w
    }

    fn drain(&mut self) {
        for (id, body) in std::mem::take(&mut self.c.captured_by) {
            if id == self.cb_add {
                let mut r = Reader::new(&body);
                let (Ok(entity), Ok(_uuid)) = (r.var_int(), r.bytes(16)) else {
                    continue;
                };
                let (Ok(kind), Ok(x), Ok(y), Ok(z)) = (r.var_int(), r.f64(), r.f64(), r.f64())
                else {
                    continue;
                };
                if self.raider_types.contains(&kind) {
                    self.raiders_seen = true;
                }
                if kind == self.firework_type {
                    self.firework_seen = true;
                }
                if kind == self.cat_type {
                    self.cat_id = Some(entity);
                    self.all_pos.insert(entity, [x, y, z]);
                    self.all_yaw.insert(entity, 0.0);
                    self.bases
                        .insert(entity, chunkwake_protocol::play::quantize([x, y, z]));
                }
                if kind == self.villager_type {
                    self.seen_villagers.insert(entity);
                    self.all_pos.insert(entity, [x, y, z]);
                    self.all_yaw.insert(entity, 0.0);
                    self.bases
                        .insert(entity, chunkwake_protocol::play::quantize([x, y, z]));
                }
                let friend = [X as f64 + 8.5, f64::from(Y), Z as f64 + 13.5];
                if kind == self.villager_type && dist_xz([x, y, z], friend) < 0.6 {
                    self.b_id = Some(entity);
                }
                let spawn = [X as f64 + 0.5, f64::from(Y), Z as f64 + 0.5];
                if dist_xz([x, y, z], spawn) < 0.6 {
                    self.a_id = Some(entity);
                    self.a_pos = Some([x, y, z]);
                    self.a_base = chunkwake_protocol::play::quantize([x, y, z]);
                }
            } else if id == self.cb_move {
                let mut r = Reader::new(&body);
                let Ok(entity) = r.var_int() else { continue };
                if r.var_int().is_err() {
                    continue;
                }
                let (Ok(dx), Ok(dy), Ok(dz)) = (r.u16(), r.u16(), r.u16()) else {
                    continue;
                };
                let yaw = r
                    .u8()
                    .map(|yaw| f32::from(yaw) * 360.0 / 256.0)
                    .unwrap_or(0.0);
                let base = self.bases.entry(entity).or_insert_with(|| {
                    self.all_pos
                        .get(&entity)
                        .map_or([0; 3], |p| chunkwake_protocol::play::quantize(*p))
                });
                base[0] += i64::from(dx as i16);
                base[1] += i64::from(dy as i16);
                base[2] += i64::from(dz as i16);
                let pos = [
                    base[0] as f64 / 4096.0,
                    base[1] as f64 / 4096.0,
                    base[2] as f64 / 4096.0,
                ];
                self.all_pos.insert(entity, pos);
                self.all_yaw.insert(entity, yaw);
                if Some(entity) == self.a_id {
                    self.a_yaw = yaw;
                    self.a_pos = Some(pos);
                    self.a_base = *base;
                }
            } else if id == self.cb_teleport {
                let mut r = Reader::new(&body);
                let Ok(entity) = r.var_int() else { continue };
                let (Ok(x), Ok(y), Ok(z)) = (r.f64(), r.f64(), r.f64()) else {
                    continue;
                };
                let yaw = r.f32().unwrap_or(0.0);
                let base = chunkwake_protocol::play::quantize([x, y, z]);
                self.bases.insert(entity, base);
                self.all_pos.insert(entity, [x, y, z]);
                self.all_yaw.insert(entity, yaw);
                if Some(entity) == self.a_id {
                    self.a_yaw = yaw;
                    self.a_pos = Some([x, y, z]);
                    self.a_base = base;
                }
            } else if id == self.cb_block_event {
                let mut r = Reader::new(&body);
                if let (Ok(pos), Ok(event)) = (r.i64(), r.u8())
                    && chunkwake_protocol::play::unpack_block_pos(pos) == self.bell
                    && event == 1
                {
                    self.bell_rang = true;
                }
            } else if id == self.cb_equip {
                let mut r = Reader::new(&body);
                let Ok(entity) = r.var_int() else { continue };
                while let Ok(slot) = r.u8() {
                    let Ok(stack) = chunkwake_protocol::inventory::read_stack(&mut r) else {
                        break;
                    };
                    if Some(entity) == self.a_id && slot & 0x7f == 0 {
                        self.a_hand = stack.map(|s| s.item);
                    }
                    if slot & 0x80 == 0 {
                        break;
                    }
                }
            } else if id == self.cb_chat {
                let mut r = Reader::new(&body);
                if let Ok(component) = text::read_component(&mut r) {
                    flatten(&component, &mut self.chat);
                    self.chat.push(' ');
                }
            } else if id == self.cb_block {
                let mut r = Reader::new(&body);
                if let (Ok(pos), Ok(state)) = (r.i64(), r.var_int()) {
                    self.door_update(chunkwake_protocol::play::unpack_block_pos(pos), state);
                }
            } else if id == self.cb_section {
                let mut r = Reader::new(&body);
                if let (Ok(packed), Ok(count)) = (r.i64(), r.var_int()) {
                    let sx = ((packed >> 42) & 0x3f_ffff) as i32;
                    let sz = ((packed >> 20) & 0x3f_ffff) as i32;
                    let sy = (packed & 0xf_ffff) as i32;
                    for _ in 0..count.clamp(0, 4096) {
                        let Ok(v) = r.var_long() else { break };
                        let state = (v >> 12) as i32;
                        let local = (v & 0xfff) as i32;
                        let at = [
                            sx * 16 + (local >> 8 & 15),
                            sy * 16 + (local & 15),
                            sz * 16 + (local >> 4 & 15),
                        ];
                        self.door_update(at, state);
                    }
                }
            } else if id == self.cb_meta {
                let mut r = Reader::new(&body);
                let Ok(entity) = r.var_int() else { continue };
                while let Ok(index) = r.u8() {
                    if index == 0xff {
                        break;
                    }
                    if index == 6 {
                        let (Ok(kind), Ok(pose)) = (r.var_int(), r.var_int()) else {
                            break;
                        };
                        if kind == 20 {
                            self.pose = Some(pose);
                        } else {
                            break;
                        }
                    } else if index == 16 {
                        let (Ok(kind), Ok(baby)) = (r.var_int(), r.bool()) else {
                            break;
                        };
                        if kind == 8 && baby {
                            self.babies.insert(entity);
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }
        }
    }

    async fn pump(&mut self, ms: u64) {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
        loop {
            match tokio::time::timeout_at(deadline, self.c.next_event()).await {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => {
                    eprintln!("event error: {e}");
                    break;
                }
                Err(_) => break,
            }
        }
        self.drain();
    }

    fn door_update(&mut self, at: [i32; 3], state: i32) {
        if at == self.door {
            self.saw_open |= state == self.door_open;
            self.saw_close |= state == self.door_closed;
        }
    }

    async fn select_slot(&mut self, slot: u16) {
        let mut w = self.packet(self.sb_carried);
        w.u16(slot);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(150).await;
    }

    async fn cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(200).await;
    }

    async fn data(&mut self, query: &str) -> String {
        self.pump(120).await;
        self.chat.clear();
        self.cmd(&format!("data get entity {query}")).await;
        self.chat.clone()
    }

    /// `/execute if block`: состояние блока (у кровати нет block entity).
    async fn block_is(&mut self, pos: [i32; 3], predicate: &str) -> bool {
        self.chat.clear();
        self.cmd(&format!(
            "execute if block {} {} {} {predicate} run say BLOCK_YES",
            pos[0], pos[1], pos[2]
        ))
        .await;
        self.chat.contains("BLOCK_YES")
    }

    async fn pos(&mut self) -> Option<[f64; 3]> {
        let text = self
            .data("@e[type=minecraft:villager,limit=1,sort=nearest] Pos")
            .await;
        let mut nums = Vec::new();
        let mut cur = String::new();
        // data get печатает [x, y, z] в конце: до этого в строке есть
        // числовые поля (1.0 и т.п.), их пропускаем.
        let values = text
            .rsplit_once('[')
            .map_or(text.as_str(), |(_, tail)| tail);
        for ch in values.chars() {
            if ch.is_ascii_digit() || ch == '.' || ch == '-' {
                cur.push(ch);
            } else if !cur.is_empty() {
                if let Ok(v) = cur.parse::<f64>() {
                    nums.push(v);
                }
                cur.clear();
            }
        }
        if !cur.is_empty()
            && let Ok(v) = cur.parse::<f64>()
        {
            nums.push(v);
        }
        (nums.len() >= 3).then(|| [nums[0], nums[1], nums[2]])
    }

    async fn data_at(&mut self, pos: [f64; 3], path: &str) -> String {
        self.pump(100).await;
        self.chat.clear();
        self.cmd(&format!(
            "data get entity @e[type=minecraft:villager,x={},y={},z={},distance=..3,limit=1,sort=nearest] {path}",
            pos[0], pos[1], pos[2]
        ))
        .await;
        self.chat.clone()
    }

    async fn data_tag(&mut self, tag: &str, path: &str) -> String {
        self.pump(100).await;
        self.chat.clear();
        self.cmd(&format!("data get entity @e[tag={tag},limit=1] {path}"))
            .await;
        self.chat.clone()
    }

    async fn wait_near(&mut self, target: [f64; 3], radius: f64, ms: u64) -> Option<[f64; 3]> {
        let start = tokio::time::Instant::now();
        let mut last = None;
        while start.elapsed() < Duration::from_millis(ms) {
            if let Some(pos) = self.pos().await {
                last = Some(pos);
                if dist_xz(pos, target) <= radius {
                    return Some(pos);
                }
            }
            self.pump(200).await;
        }
        last
    }
}

/// Число предмета в выводе `data get ... Inventory`.
fn item_count(text: &str, id: &str) -> Option<i32> {
    for part in text.split('{') {
        if !part.contains(id) {
            continue;
        }
        let at = part.find("count:")? + 6;
        let n: String = part[at..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        return n.parse().ok();
    }
    None
}

fn look_yaw(from: [f64; 3], to: [f64; 3]) -> f32 {
    let dx = to[0] - from[0];
    let dz = to[2] - from[2];
    (-dx).atan2(dz).to_degrees() as f32
}

fn dist_xz(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn flatten(v: &serde_json::Value, out: &mut String) {
    if let Some(s) = v.as_str() {
        out.push_str(s);
        return;
    }
    if let Some(s) = v.get("text").and_then(serde_json::Value::as_str) {
        out.push_str(s);
    }
    for key in ["with", "extra"] {
        if let Some(args) = v.get(key).and_then(serde_json::Value::as_array) {
            for arg in args {
                flatten(arg, out);
                out.push(' ');
            }
        }
    }
}

/// ID состояний двери (`reports/blocks.json`) для `open=true/false`.
fn door_state_ids(root: &std::path::Path) -> Option<(i32, i32)> {
    let text = std::fs::read_to_string(root.join("reports/blocks.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let states = json["minecraft:oak_door"]["states"].as_array()?;
    let mut open = None;
    let mut closed = None;
    for state in states {
        let props = &state["properties"];
        if props["facing"] != "east"
            || props["half"] != "lower"
            || props["hinge"] != "left"
            || props["powered"] != "false"
        {
            continue;
        }
        let Some(id) = state["id"].as_i64() else {
            continue;
        };
        match props["open"].as_str() {
            Some("true") => open = Some(id as i32),
            Some("false") => closed = Some(id as i32),
            _ => {}
        }
    }
    Some((open?, closed?))
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let root = std::path::Path::new(&a[2])
        .parent()
        .and_then(std::path::Path::parent)
        .unwrap()
        .to_path_buf();
    let reg: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::path::Path::new(&a[2]).with_file_name("registries.json"))
            .unwrap(),
    )
    .unwrap();
    let pid = |reg_name: &str, name: &str| {
        reg[reg_name]["entries"][name]["protocol_id"]
            .as_i64()
            .unwrap() as i32
    };
    let ids = Ids::load(&table).unwrap();
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n| table.id(State::Play, Direction::Clientbound, n).unwrap();
    let cb_chat = cb("system_chat");
    let cb_meta = cb("set_entity_data");
    let cb_block = cb("block_update");
    let cb_section = cb("section_blocks_update");
    let cb_add = cb("add_entity");
    let cb_move = cb("move_entity_pos_rot");
    let cb_teleport = cb("teleport_entity");
    let cb_equip = cb("set_equipment");
    let cb_block_event = cb("block_event");
    let raider_types: HashSet<i32> = [
        "minecraft:pillager",
        "minecraft:vindicator",
        "minecraft:evoker",
        "minecraft:witch",
        "minecraft:ravager",
    ]
    .iter()
    .map(|name| pid("minecraft:entity_type", name))
    .collect();
    let (door_open, door_closed) = door_state_ids(&root)
        .unwrap_or_else(|| panic!("нет состояний oak_door в reports/blocks.json"));
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    c.capture_also = vec![
        cb_chat,
        cb_meta,
        cb_block,
        cb_section,
        cb_add,
        cb_move,
        cb_teleport,
        cb_equip,
        cb_block_event,
    ];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_carried: sb("set_carried_item"),
        cb_chat,
        cb_meta,
        cb_block,
        cb_section,
        cb_add,
        cb_move,
        cb_teleport,
        cb_equip,
        cb_block_event,
        chat: String::new(),
        bell: [0; 3],
        bell_rang: false,
        raider_types,
        raiders_seen: false,
        firework_type: pid("minecraft:entity_type", "minecraft:firework_rocket"),
        firework_seen: false,
        cat_type: pid("minecraft:entity_type", "minecraft:cat"),
        cat_id: None,
        villager_type: pid("minecraft:entity_type", "minecraft:villager"),
        seen_villagers: HashSet::new(),
        babies: HashSet::new(),
        all_pos: HashMap::new(),
        all_yaw: HashMap::new(),
        bases: HashMap::new(),
        a_id: None,
        b_id: None,
        a_pos: None,
        a_yaw: 0.0,
        a_base: [0; 3],
        a_hand: None,
        pose: None,
        door: [X + 3, Y, Z],
        door_open,
        door_closed,
        saw_open: false,
        saw_close: false,
    };
    p.pump(1500).await;

    for line in [
        "difficulty peaceful".to_string(),
        "gamemode creative".to_string(),
        "effect clear @s".to_string(),
        "kill @e[type=!minecraft:player,distance=..96]".to_string(),
        format!("execute in minecraft:overworld run tp @s {X}.5 {Y} {Z}.5 180 20"),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            X - 10,
            Y - 1,
            Z - 8,
            X + 16,
            Y - 1,
            Z + 10
        ),
        format!(
            "fill {} {Y} {} {} {} {} minecraft:air",
            X - 10,
            Z - 8,
            X + 16,
            Y + 5,
            Z + 10
        ),
        "kill @e[type=minecraft:item]".to_string(),
        "time set 3000".to_string(),
    ] {
        p.cmd(&line).await;
    }
    p.c.confirm_loaded().await.unwrap();

    // Рабочая точка: компостер, кровать и колокол.
    p.cmd(&format!("setblock {} {Y} {} minecraft:composter", X + 9, Z))
        .await;
    // Стена с дверью на пути к компостеру.
    p.cmd(&format!(
        "fill {} {Y} {} {} {} {} minecraft:stone",
        X + 3,
        Z - 1,
        X + 3,
        Y + 1,
        Z - 1
    ))
    .await;
    p.cmd(&format!(
        "fill {} {Y} {} {} {} {} minecraft:stone",
        X + 3,
        Z + 1,
        X + 3,
        Y + 1,
        Z + 1
    ))
    .await;
    p.cmd(&format!(
        "setblock {} {Y} {Z} minecraft:oak_door[half=lower,facing=east,hinge=left,open=false,powered=false]",
        X + 3
    ))
    .await;
    p.cmd(&format!(
        "setblock {} {} {Z} minecraft:oak_door[half=upper,facing=east,hinge=left,open=false,powered=false]",
        X + 3,
        Y + 1
    ))
    .await;
    p.cmd(&format!(
        "setblock {X} {Y} {} minecraft:bell[attachment=floor,facing=north]",
        Z + 6
    ))
    .await;
    p.cmd(&format!(
        "setblock {} {Y} {} minecraft:red_bed[part=foot,facing=south]",
        X + 8,
        Z - 4
    ))
    .await;
    p.cmd(&format!(
        "setblock {} {Y} {} minecraft:red_bed[part=head,facing=south]",
        X + 8,
        Z - 3
    ))
    .await;
    // Предыдущие жители могли остаться вне радиуса первого `/kill`:
    // чистим уже после телепорта.
    p.cmd("kill @e[type=!minecraft:player,distance=..96]").await;
    // Жители прошлых прогонов могли остаться за радиусом: убираем всех.
    p.cmd("kill @e[type=minecraft:villager]").await;
    // Безработный житель сам займёт компостер и станет фермером.
    p.cmd(&format!(
        "summon minecraft:villager {X}.5 {Y} {Z}.5 {{VillagerData:{{type:\"minecraft:plains\",profession:\"minecraft:none\",level:1}},Xp:0}}"
    ))
    .await;
    // Метка на A: у соседа B та же модель, а `sort=nearest` относительно
    // игрока может выбрать не того — по тегу телепорт однозначен.
    p.cmd(&format!(
        "tag @e[type=minecraft:villager,x={},y={Y},z={},distance=..2] add probe_a",
        X as f64 + 0.5,
        Z as f64 + 0.5
    ))
    .await;
    // Второй житель (NoAI) в 8 блоках от колокола: цель социальной
    // встречи, сам он не двигается.
    p.cmd(&format!(
        "summon minecraft:villager {}.5 {Y} {}.5 {{NoAI:1b,PersistenceRequired:1b,VillagerData:{{type:\"minecraft:plains\",profession:\"minecraft:none\",level:1}}}}",
        X + 8,
        Z + 13
    ))
    .await;
    p.pump(800).await;
    // Метка на соседа B (NoAI): у A та же модель, селектор по координатам
    // может выбрать не того.
    p.cmd(&format!(
        "tag @e[type=minecraft:villager,x={},y={Y},z={},distance=..2] add probe_b",
        X as f64 + 8.5,
        Z as f64 + 13.5
    ))
    .await;

    // Стена между колоколом и соседом: сенсор не должен видеть цель
    // сквозь блоки (проверка линии видимости).
    p.cmd(&format!(
        "fill {} {Y} {} {} {} {} minecraft:stone",
        X + 4,
        Z + 6,
        X + 4,
        Y + 1,
        Z + 14
    ))
    .await;
    assert!(
        p.block_is([X + 4, Y, Z + 10], "minecraft:stone").await,
        "стена для проверки линии видимости не поставилась"
    );
    // Слух о «игроке» [1,2,3,4]: при социальной встрече A должен получить
    // его с `decayPerTransfer` 20 (25 -> 5).
    p.cmd(&format!(
        "data merge entity @e[type=minecraft:villager,x={}.5,y={Y},z={}.5,distance=..1]          {{Gossips:[{{Target:[I;1,2,3,4],Type:\"minor_negative\",Value:25}}]}}",
        X + 8,
        Z + 13
    ))
    .await;

    let data = p
        .data("@e[type=minecraft:villager,limit=1,sort=nearest] VillagerData")
        .await;
    assert!(data.contains("farmer"), "житель не занял компостер: {data}");

    // 1. Работа: 2000..9000 — идёт к компостеру через дверь.
    p.cmd("time set 3000").await;
    let mut reached = None;
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(12_000) {
        if let Some(pos) = p.pos().await
            && dist_xz(pos, [X as f64 + 9.5, f64::from(Y), Z as f64 + 0.5]) <= 2.0
        {
            reached = Some(pos);
            break;
        }
        p.pump(200).await;
    }
    let work = reached.unwrap_or_else(|| panic!("житель не дошёл до компостера"));
    let near = dist_xz(work, [X as f64 + 9.5, f64::from(Y), Z as f64 + 0.5]);
    assert!(
        p.saw_open,
        "клиент не получил `block_update` с открытой дверью"
    );
    println!("work: villager at composter ({near:.2}), door opened");

    // `HarvestFarmland`/`UseBonemeal`: грядки вокруг компостера.
    let f1 = [X + 8, Y, Z];
    let f2 = [X + 10, Y, Z];
    let f3 = [X + 9, Y, Z + 1];
    let w1 = [f1[0], Y + 1, f1[2]];
    let w2 = [f2[0], Y + 1, f2[2]];
    let w3 = [f3[0], Y + 1, f3[2]];
    for farm in [f1, f2, f3] {
        p.cmd(&format!(
            "setblock {} {} {} minecraft:farmland[moisture=7]",
            farm[0], farm[1], farm[2]
        ))
        .await;
    }
    p.cmd(&format!(
        "setblock {} {} {} minecraft:wheat[age=7]",
        w1[0], w1[1], w1[2]
    ))
    .await;
    p.cmd(&format!(
        "setblock {} {} {} minecraft:wheat[age=0]",
        w3[0], w3[1], w3[2]
    ))
    .await;
    p.cmd(&format!(
        "setblock {} {} {} minecraft:air",
        w2[0], w2[1], w2[2]
    ))
    .await;
    let a_now = p.a_pos.expect("нет позиции A у компостера");
    p.cmd(&format!(
        "data merge entity @e[type=minecraft:villager,x={},y={},z={},distance=..3,limit=1,sort=nearest] \
         {{Inventory:[{{Slot:0b,id:\"minecraft:wheat_seeds\",count:8}},{{Slot:1b,id:\"minecraft:bone_meal\",count:8}}]}}",
        a_now[0], a_now[1], a_now[2]
    ))
    .await;
    let mut harvested = false;
    let mut planted = false;
    let mut bonemealed = false;
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(180_000) {
        if !harvested {
            harvested = p.block_is(w1, "minecraft:air").await;
        }
        if !planted {
            planted =
                p.block_is(w1, "minecraft:wheat").await || p.block_is(w2, "minecraft:wheat").await;
        }
        if !bonemealed && let Some(pos) = p.a_pos {
            let inv = p.data_at(pos, "Inventory").await;
            bonemealed = item_count(&inv, "minecraft:bone_meal").is_some_and(|n| n < 8);
        }
        if harvested && planted && bonemealed {
            break;
        }
        p.pump(300).await;
    }
    assert!(harvested, "HarvestFarmland: спелая пшеница не собрана");
    assert!(planted, "HarvestFarmland: семена не посажены");
    assert!(bonemealed, "UseBonemeal: костная мука не потрачена");
    println!("farm: harvested, planted and bonemealed");

    // `Restock`: израсходованные сделки обновляются у рабочего блока.
    let a_now = p.a_pos.expect("нет позиции A для рестока");
    p.cmd(&format!(
        "data merge entity @e[type=minecraft:villager,x={},y={},z={},distance=..3,limit=1,sort=nearest] \
         {{Offers:{{Recipes:[{{buy:{{id:\"minecraft:wheat\",count:20}},sell:{{id:\"minecraft:emerald\",count:1}},uses:3,maxUses:16,xp:2,priceMultiplier:0.05f,demand:0}}]}},LastRestock:0,RestocksToday:0}}",
        a_now[0], a_now[1], a_now[2]
    ))
    .await;
    // Ставим жителя к рабочему блоку: `WorkAtPoi` стартует 1/2 раз в 300
    // тиков и только у блока, иначе ресток может не успеть за окно.
    p.cmd(&format!(
        "tp @e[type=minecraft:villager,x={},y={},z={},distance=..3,limit=1,sort=nearest] {} {Y} {}",
        a_now[0],
        a_now[1],
        a_now[2],
        X as f64 + 9.5,
        Z as f64 + 0.5
    ))
    .await;
    let composter = [X as f64 + 9.5, f64::from(Y), Z as f64 + 0.5];
    let mut restocked = false;
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(150_000) {
        // `Work` — только 2000..9000: держим время и жителя у блока,
        // иначе `WorkAtPoi` (1/2 раз в 300 тиков у блока) не стартует.
        p.cmd("time set 3000").await;
        if let Some(pos) = p.a_pos {
            if dist_xz(pos, composter) > 2.0 {
                p.cmd(&format!(
                    "tp @e[tag=probe_a,limit=1] {} {Y} {}",
                    composter[0], composter[2]
                ))
                .await;
            }
            let offers = p.data_tag("probe_a", "Offers").await;
            if offers.contains("uses:0") {
                restocked = true;
                break;
            }
        }
        p.pump(300).await;
    }
    assert!(restocked, "Restock: сделки не обновились");
    println!("restock: offers reset at workstation");

    // `ShowTradesToPlayer`: игрок рядом держит пшеницу — A показывает
    // предметы сделок; два оффера → смена раз в 40 тиков, `lookTime` 900.
    let apos = p.a_pos.expect("нет позиции A");
    p.cmd(&format!(
        "data merge entity @e[type=minecraft:villager,x={},y={},z={},distance=..3,limit=1,sort=nearest]          {{Offers:{{Recipes:[{{buy:{{id:\"minecraft:wheat\",count:20}},sell:{{id:\"minecraft:emerald\",count:1}},uses:0,maxUses:16,xp:2,priceMultiplier:0.05f,demand:0}},{{buy:{{id:\"minecraft:wheat\",count:20}},sell:{{id:\"minecraft:bread\",count:1}},uses:0,maxUses:16,xp:2,priceMultiplier:0.05f,demand:0}}]}}}}",
        apos[0], apos[1], apos[2]
    ))
    .await;
    p.cmd(&format!("tp @s {} {} {}", apos[0], apos[1], apos[2] + 2.0))
        .await;
    p.cmd("item replace entity @s hotbar.0 with minecraft:wheat 1")
        .await;
    p.select_slot(0).await;
    let emerald = pid("minecraft:item", "minecraft:emerald");
    let bread = pid("minecraft:item", "minecraft:bread");
    let mut shown = false;
    let mut switched = false;
    let mut last = None;
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(10_000) {
        if let Some(hand) = p.a_hand
            && (hand == emerald || hand == bread)
        {
            shown = true;
            if last.is_some_and(|prev| prev != hand) {
                switched = true;
            }
            last = Some(hand);
        }
        if shown && switched {
            break;
        }
        p.pump(200).await;
    }
    assert!(
        shown,
        "ShowTradesToPlayer не показал предмет: {:?}",
        p.a_hand
    );
    assert!(
        switched,
        "ShowTradesToPlayer не сменил предмет за 40 тиков: {:?}",
        p.a_hand
    );
    // `lookTime` = 900 тиков: предмет держится, а не пропадает сразу.
    p.pump(2_000).await;
    assert!(
        matches!(p.a_hand, Some(hand) if hand == emerald || hand == bread),
        "ShowTradesToPlayer снял предмет раньше 900 тиков: {:?}",
        p.a_hand
    );
    p.cmd(&format!("tp @s {}.5 {Y} {}.5", X, Z + 2)).await;
    p.cmd("item replace entity @s hotbar.0 with minecraft:air")
        .await;
    p.select_slot(0).await;
    let mut cleared = false;
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(5_000) {
        if p.a_hand.is_none() {
            cleared = true;
            break;
        }
        p.pump(200).await;
    }
    assert!(
        cleared,
        "ShowTradesToPlayer не снял предмет: {:?}",
        p.a_hand
    );
    println!("show trades: items shown (cycle 40), held 900, cleared");

    // `VillagerPanicTrigger`: зомби рядом — A убегает; после удаления
    // угрозы `VillagerCalmDown` возвращает его к работе.
    let ap = p.a_pos.expect("нет позиции A для паники");
    p.cmd("difficulty easy").await;
    let zombie = [ap[0] + 3.0, ap[1], ap[2] + 3.0];
    p.cmd(&format!(
        "summon minecraft:zombie {} {} {} {{NoAI:1b,PersistenceRequired:1b}}",
        zombie[0], zombie[1], zombie[2]
    ))
    .await;
    p.chat.clear();
    p.cmd("execute if entity @e[type=minecraft:zombie] run say ZOMBIE_YES")
        .await;
    assert!(
        p.chat.contains("ZOMBIE_YES"),
        "зомби не появился: {}",
        p.chat
    );
    let mut fled = false;
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(5_000) {
        if let Some(pos) = p.a_pos
            && dist_xz(pos, zombie) > 4.0
        {
            fled = true;
            break;
        }
        p.pump(200).await;
    }
    assert!(fled, "житель не убежал от зомби: {:?}", p.a_pos);
    println!("panic: villager fled from zombie");

    p.cmd(&format!(
        "kill @e[type=minecraft:zombie,x={},y={},z={},distance=..3]",
        zombie[0], zombie[1], zombie[2]
    ))
    .await;
    p.cmd("difficulty peaceful").await;
    let composter = [X as f64 + 9.5, f64::from(Y), Z as f64 + 0.5];
    let mut calm = false;
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(30_000) {
        if let Some(pos) = p.a_pos
            && dist_xz(pos, composter) <= 3.5
        {
            calm = true;
            break;
        }
        p.pump(200).await;
    }
    assert!(
        calm,
        "житель не вернулся к работе после паники: {:?}",
        p.a_pos
    );
    println!("panic: calmed down and returned to work");

    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(3_000) && !p.saw_close {
        p.pump(100).await;
    }
    assert!(p.saw_close, "дверь осталась открытой после прохода");
    println!("door: closed after passing");

    // 2. Встреча: 9000..11000 — идёт к колоколу.
    p.cmd("time set 9600").await;
    // `SetWalkTargetFromBlockMemory(MEETING_POINT, 0.4, 6, 100, 200)`:
    // житель останавливается в ~6 блоках от колокола (не вплотную).
    let at_bell = p
        .wait_near([X as f64 + 0.5, f64::from(Y), Z as f64 + 6.5], 7.0, 60_000)
        .await
        .unwrap_or_else(|| panic!("житель не дошёл до колокола"));
    let near = dist_xz(at_bell, [X as f64 + 0.5, f64::from(Y), Z as f64 + 6.5]);
    assert!(near <= 7.0, "житель не у колокола: {at_bell:?}");
    println!("meet: villager at bell ({near:.2})");

    // `TradeWithVillager`: у A 64 хлеба — при встрече отдаст половину B.
    let apos = p.a_pos.expect("нет позиции A у колокола");
    p.cmd(&format!(
        "data merge entity @e[type=minecraft:villager,x={},y={},z={},distance=..3,limit=1,sort=nearest]          {{Inventory:[{{Slot:0b,id:\"minecraft:bread\",count:64}},{{Slot:1b,id:\"minecraft:wheat\",count:64}}]}}",
        apos[0], apos[1], apos[2]
    ))
    .await;

    // Стена между колоколом и соседом уже стоит (поставлена до встречи).
    let friend = [X as f64 + 8.5, f64::from(Y), Z as f64 + 13.5];
    let mut blocked = true;
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(6_000) {
        if let Some(pos) = p.a_pos
            && dist_xz(pos, friend) <= 4.0
        {
            blocked = false;
            break;
        }
        p.pump(200).await;
    }
    assert!(blocked, "житель увидел соседа сквозь стену");
    println!("sensor: neighbour hidden behind wall");

    p.cmd(&format!(
        "fill {} {Y} {} {} {} {} minecraft:air",
        X + 4,
        Z + 6,
        X + 4,
        Y + 1,
        Z + 14
    ))
    .await;

    // `SocializeAtBell`: раз в 100 тиков житель у колокола выбирает
    // ближайшего жителя (NoAI-сосед в 8 блоках) и подходит к нему.
    // Подводим NoAI-соседа к колоколу: `SocializeAtBell` выбирает партнёра
    // только на `distanceToSqr <= 32.0` (≈5.66 блока), а сам житель должен
    // быть в 4 блоках от колокола.
    let social_friend = [X as f64 + 1.5, f64::from(Y), Z as f64 + 7.5];
    p.cmd(&format!(
        "tp @e[type=minecraft:villager,x={},y={},z={},distance=..2,limit=1,sort=nearest] {} {} {}",
        friend[0], friend[1], friend[2], social_friend[0], social_friend[1], social_friend[2]
    ))
    .await;

    // Ставим жителя к колоколу (в ~2.5 блока): `SocializeAtBell`
    // проверяется только в 4 блоках и раз в 100 тиков, а
    // `SetWalkTargetFromBlockMemory` сам останавливает его в ~6.
    p.cmd(&format!(
        "tp @e[tag=probe_a,limit=1] {} {Y} {}",
        X as f64 + 2.5,
        Z as f64 + 8.5
    ))
    .await;

    // Ждём именно состояние «подошёл и смотрит»: близость сама по себе
    // может быть мимоходом (StrollAroundPoi), без `SocializeAtBell`.
    // `SocializeAtBell` (1/100, только в 4 блоках от колокола) ставит
    // INTERACTION_TARGET и ведёт к соседу; `TradeWithVillager` сплетничает
    // на `distanceToSqr <= 5.0`. `SetWalkTargetFromBlockMemory` сам
    // останавливает жителя в ~6, поэтому держим его у колокола.
    let hold = [X as f64 + 3.0, f64::from(Y), Z as f64 + 7.5];
    let mut social_pos = None;
    let mut social_yaw = 0.0;
    let mut gossip = String::new();
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(120_000) {
        // `Meet` — только 9000..11000; окно длиннее, поэтому держим время.
        p.cmd("time set 9600").await;
        if let Some(pos) = p.a_pos {
            if dist_xz(pos, social_friend) > 2.5 {
                p.cmd(&format!(
                    "tp @e[tag=probe_a,limit=1] {} {Y} {}",
                    hold[0], hold[2]
                ))
                .await;
            }
            if dist_xz(pos, social_friend) <= 2.0 {
                social_pos = Some(pos);
                social_yaw = p.a_yaw;
            }
            gossip = p.data_tag("probe_a", "Gossips").await;
            if gossip.contains("minor_negative") {
                social_pos = p.a_pos;
                social_yaw = p.a_yaw;
                break;
            }
        }
        p.pump(300).await;
    }
    let social_pos = social_pos.unwrap_or_else(|| {
        panic!(
            "житель не подошёл к соседу у колокола (a_pos {:?}, yaw {:?})",
            p.a_pos, p.a_yaw
        )
    });
    println!(
        "social: villager came to neighbour at bell ({:.2}, yaw {:.1})",
        dist_xz(social_pos, social_friend),
        social_yaw
    );
    assert!(
        gossip.contains("minor_negative") && gossip.contains("Value:5"),
        "слух не передан при встрече: {gossip}"
    );
    println!("gossip: transferred minor_negative 25 -> 5");

    // A (фермер) с избытком еды бросает B половину стака хлеба:
    // в инвентаре остаётся 32 из 64.
    let mut inv = String::new();
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(10_000) {
        inv = p.data_tag("probe_a", "Inventory").await;
        if inv.contains("minecraft:bread") && inv.contains("count:24") {
            break;
        }
        p.pump(500).await;
    }
    assert!(
        inv.contains("minecraft:bread") && inv.contains("count:24"),
        "TradeWithVillager не бросал хлеб (Inventory: {inv})"
    );
    println!("trade: bread thrown to neighbour");

    // Обратный обмен: у B избыток еды, A голоден — B (уже с ИИ) бросает
    // половину стака A. Проверяем по инвентарю B (64 -> 32).
    // Обнуляем (голодный) инвентарь A и даём B хлеб; оба — по меткам,
    // координаты после телепорта соседа уже другие.
    p.cmd("data merge entity @e[tag=probe_a,limit=1] {Inventory:[]}")
        .await;
    p.cmd("data merge entity @e[tag=probe_b,limit=1] {NoAI:0b,Inventory:[{Slot:0b,id:\"minecraft:bread\",count:64}]}")
        .await;
    p.cmd("time set 9600").await;
    let mut b_inv = String::new();
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(60_000) {
        b_inv = p.data_tag("probe_b", "Inventory").await;
        if b_inv.contains("minecraft:bread") && b_inv.contains("count:24") {
            break;
        }
        p.pump(300).await;
    }
    assert!(
        b_inv.contains("minecraft:bread") && b_inv.contains("count:24"),
        "обратный обмен не сработал (B Inventory: {b_inv})"
    );
    println!("trade: reverse exchange (B gave bread to hungry A)");
    // B снова NoAI.
    p.cmd("data merge entity @e[tag=probe_b,limit=1] {NoAI:1b}")
        .await;

    p.cmd("time set 13000").await;
    let bed_foot = [X as f64 + 8.5, f64::from(Y), Z as f64 - 3.5];
    let bed_head = [X as f64 + 8.5, f64::from(Y), Z as f64 - 2.5];
    let mut at_bed = None;
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(15_000) {
        if let Some(pos) = p.a_pos {
            at_bed = Some(pos);
            if dist_xz(pos, bed_foot).min(dist_xz(pos, bed_head)) <= 2.0 {
                break;
            }
        }
        p.pump(200).await;
    }
    let at_bed = at_bed.unwrap_or_else(|| panic!("нет позиции жителя у кровати"));
    let near = dist_xz(at_bed, bed_foot).min(dist_xz(at_bed, bed_head));
    assert!(near <= 2.0, "житель не дошёл до кровати: {at_bed:?}");
    println!("rest: villager at bed ({near:.2})");

    // Сон: поза SLEEPING, кровать занята, позиция на кровати.
    let mut slept = None;
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(8_000) {
        if p.pose == Some(2)
            && p.block_is([X + 8, Y, Z - 4], "minecraft:red_bed[occupied=true]")
                .await
        {
            slept = p.a_pos;
            break;
        }
        p.pump(200).await;
    }
    let slept = slept.unwrap_or_else(|| panic!("житель не лёг спать (pose {:?})", p.pose));
    assert!(
        slept[1] > f64::from(Y) + 0.4,
        "позиция во сне не на кровати: {slept:?}"
    );
    println!("sleep: pose sleeping, bed occupied (y {:.3})", slept[1]);

    // `findSafeDismountLocation`: вокруг изголовья магма — вставать на неё
    // нельзя.
    let magma = [
        [X + 7, Y, Z - 3],
        [X + 7, Y, Z - 2],
        [X + 9, Y, Z - 3],
        [X + 9, Y, Z - 2],
    ];
    for pos in magma {
        p.cmd(&format!(
            "setblock {} {} {} minecraft:magma_block",
            pos[0], pos[1], pos[2]
        ))
        .await;
    }

    // Пробуждение: вне `rest` поза STANDING и кровать свободна.
    p.cmd("time set 1000").await;
    let mut woken = false;
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(8_000) {
        if p.pose == Some(0)
            && p.block_is([X + 8, Y, Z - 4], "minecraft:red_bed[occupied=false]")
                .await
        {
            woken = true;
            break;
        }
        p.pump(200).await;
    }
    let bed_busy = p
        .block_is([X + 8, Y, Z - 4], "minecraft:red_bed[occupied=true]")
        .await;
    assert!(
        woken,
        "житель не проснулся (pose {:?}, bed занята: {bed_busy})",
        p.pose
    );
    p.pump(300).await;
    let wake_pos = p.a_pos.expect("нет позиции после пробуждения");
    let cell = [
        wake_pos[0].floor() as i32,
        wake_pos[1].floor() as i32,
        wake_pos[2].floor() as i32,
    ];
    assert!(
        !magma.contains(&cell),
        "житель встал на опасный блок: {cell:?}"
    );
    println!("wake: pose standing, bed free, safe dismount {cell:?}");

    // 40 тиков после пробуждения житель не ложится снова.
    p.cmd("time set 13000").await;
    p.pump(300).await;
    assert_eq!(p.pose, Some(0), "житель лёг раньше 40-тикового кулдауна");
    for pos in magma {
        p.cmd(&format!(
            "setblock {} {} {} minecraft:air",
            pos[0], pos[1], pos[2]
        ))
        .await;
    }
    let mut slept_again = false;
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(15_000) {
        if p.pose == Some(2) {
            slept_again = true;
            break;
        }
        p.pump(200).await;
    }
    assert!(slept_again, "житель не лёг после кулдауна");
    println!("cooldown: slept again after 40 ticks");

    // Приоритеты: без кровати `Rest` падает в `Idle` (житель гуляет,
    // а не стоит на месте).
    let bed_pos = p
        .a_pos
        .unwrap_or([X as f64 + 8.5, f64::from(Y), Z as f64 - 3.5]);
    p.cmd(&format!("setblock {} {Y} {} minecraft:air", X + 8, Z - 4))
        .await;
    p.cmd(&format!("setblock {} {Y} {} minecraft:air", X + 8, Z - 3))
        .await;
    let mut wandered = false;
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(15_000) {
        if let Some(pos) = p.a_pos
            && dist_xz(pos, bed_pos) > 1.5
        {
            wandered = true;
            break;
        }
        p.pump(200).await;
    }
    assert!(
        wandered,
        "без кровати житель не перешёл в idle (a_pos {:?})",
        p.a_pos
    );
    println!("priority: no bed -> idle wander");

    // `VillagerMakeLove`: чистая площадка — все прежние жители убираются,
    // ставятся две кровати, пара взрослых с едой сама ищет друг друга.
    p.cmd("kill @e[type=minecraft:villager]").await;
    // Дать серверу снять убитых и освободить их точки интереса.
    p.pump(1_000).await;
    let bx = X as f64 + 22.5;
    let bz = Z as f64 + 22.5;
    // Запас кроватей: `VillagerMakeLove.takeVacantBed` ищет незанятую
    // кровать для ребёнка, а родители и прочие жители занимают по одной.
    for (x, z, part) in [
        (bx, bz, "foot"),
        (bx, bz + 1.0, "head"),
        (bx + 3.0, bz, "foot"),
        (bx + 3.0, bz + 1.0, "head"),
        (bx + 6.0, bz, "foot"),
        (bx + 6.0, bz + 1.0, "head"),
        (bx + 9.0, bz, "foot"),
        (bx + 9.0, bz + 1.0, "head"),
        (bx + 12.0, bz, "foot"),
        (bx + 12.0, bz + 1.0, "head"),
    ] {
        p.cmd(&format!(
            "setblock {} {} {} minecraft:red_bed[part={part},facing=south]",
            x.floor() as i32,
            Y,
            z.floor() as i32
        ))
        .await;
    }
    for x in [bx, bx + 2.0] {
        p.cmd(&format!(
            "summon minecraft:villager {x} {Y} {bz} {{Age:0,FoodLevel:12b,PersistenceRequired:1b,VillagerData:{{type:\"minecraft:plains\",profession:\"minecraft:none\",level:1}}}}"
        ))
        .await;
    }
    p.cmd("time set 1000").await;

    let before: HashSet<i32> = p.seen_villagers.iter().copied().collect();
    let mut born = None;
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(45_000) {
        if let Some(child) = p
            .seen_villagers
            .difference(&before)
            .copied()
            .find(|id| p.babies.contains(id))
        {
            born = Some(child);
            break;
        }
        p.pump(300).await;
    }
    let child = born.unwrap_or_else(|| {
        panic!(
            "ребёнок не родился: seen {:?}, babies {:?}",
            p.seen_villagers, p.babies
        )
    });
    println!("breed: child {child} with DATA_BABY_ID=true");

    // `PlayTagWithOtherKids`: два малыша в `play` догоняют друг друга.
    let child_pos = p.all_pos.get(&child).copied().unwrap_or(apos);
    let babies_before: HashSet<i32> = p.babies.iter().copied().collect();
    p.cmd(&format!(
        "summon minecraft:villager {} {} {} {{Age:-24000,PersistenceRequired:1b,VillagerData:{{type:\"minecraft:plains\",profession:\"minecraft:none\",level:1}}}}",
        child_pos[0] + 4.0,
        child_pos[1],
        child_pos[2]
    ))
    .await;
    p.cmd("time set 4000").await;
    let mut partner_baby = None;
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(10_000) {
        partner_baby = p
            .babies
            .difference(&babies_before)
            .copied()
            .find(|id| *id != child);
        if partner_baby.is_some() {
            break;
        }
        p.pump(200).await;
    }
    let partner_baby = partner_baby.unwrap_or_else(|| {
        panic!(
            "второй малыш не появился: babies {:?}, new {:?}",
            p.babies, babies_before
        )
    });
    let mut played = None;
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(45_000) {
        if let (Some(a), Some(b)) = (
            p.all_pos.get(&child).copied(),
            p.all_pos.get(&partner_baby).copied(),
        ) {
            let dist = dist_xz(a, b);
            if dist <= 3.0 {
                let ya = p.all_yaw.get(&child).copied().unwrap_or(0.0);
                let yb = p.all_yaw.get(&partner_baby).copied().unwrap_or(0.0);
                let want_a = look_yaw(a, b);
                let want_b = look_yaw(b, a);
                let da = (ya - want_a + 180.0).rem_euclid(360.0) - 180.0;
                let db = (yb - want_b + 180.0).rem_euclid(360.0) - 180.0;
                if da.abs() < 45.0 || db.abs() < 45.0 {
                    played = Some((a, b, dist, ya, yb));
                    break;
                }
            }
        }
        p.pump(200).await;
    }
    let (a, b, dist, ya, yb) = played.unwrap_or_else(|| {
        panic!(
            "малыши не играли: pos {:?}/{:?}",
            p.all_pos.get(&child),
            p.all_pos.get(&partner_baby)
        )
    });
    println!("play: babies tag at {dist:.2} (yaw {ya:.1}/{yb:.1}, {a:?}/{b:?})");

    // `JumpOnBed`: без второго малыша ребёнок прыгает на ближайшей кровати.
    let partner_pos = p.all_pos.get(&partner_baby).copied().unwrap_or(b);
    p.cmd(&format!(
        "kill @e[type=minecraft:villager,x={},y={},z={},distance=..2]",
        partner_pos[0], partner_pos[1], partner_pos[2]
    ))
    .await;
    p.cmd("time set 11500").await;
    let bed_a = [bx + 0.5, f64::from(Y), bz + 0.5];
    let bed_b = [bx + 3.5, f64::from(Y), bz + 0.5];
    let mut jumped = false;
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(20_000) {
        if let Some(pos) = p.all_pos.get(&child).copied() {
            let near = dist_xz(pos, bed_a).min(dist_xz(pos, bed_b)) <= 2.0;
            if near && pos[1] >= f64::from(Y) + 0.5 {
                jumped = true;
                break;
            }
        }
        p.pump(200).await;
    }
    assert!(
        jumped,
        "JumpOnBed: ребёнок не прыгал на кровати: {:?}",
        p.all_pos.get(&child)
    );
    println!("jump: baby jumped on bed");

    // `SetRaidStatus`: пре-рейд — сбор у колокола и удары; активная волна —
    // укрытие в доме. Рейд запускается Bad Omen рядом с кроватью.
    let bell_pos = [bx.floor() as i32 + 6, Y, bz.floor() as i32 + 6];
    p.bell = bell_pos;
    p.cmd(&format!(
        "setblock {} {} {} minecraft:bell[attachment=floor,facing=north]",
        bell_pos[0], bell_pos[1], bell_pos[2]
    ))
    .await;
    let before_raid: HashSet<i32> = p.seen_villagers.iter().copied().collect();
    p.cmd(&format!(
        "summon minecraft:villager {} {Y} {} {{PersistenceRequired:1b,VillagerData:{{type:\"minecraft:plains\",profession:\"minecraft:none\",level:1}}}}",
        bx + 1.5,
        bz + 3.5
    ))
    .await;
    p.cmd(&format!("tp @s {} {Y} {}", bx + 2.5, bz + 2.5)).await;
    p.cmd("difficulty easy").await;
    p.cmd("effect clear @s").await;
    p.cmd("effect give @s minecraft:bad_omen 600 0").await;
    // Новый житель (не рейдер) — объект проверки.
    let mut raider_v = None;
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(5_000) {
        raider_v = p
            .seen_villagers
            .difference(&before_raid)
            .copied()
            .find(|id| !before_raid.contains(id));
        if raider_v.is_some() {
            break;
        }
        p.pump(200).await;
    }
    let raider_v = raider_v.expect("житель для рейда не появился");

    let bell_center = [
        bell_pos[0] as f64 + 0.5,
        f64::from(Y),
        bell_pos[2] as f64 + 0.5,
    ];
    let mut gathered = false;
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(25_000) {
        let near = p
            .all_pos
            .get(&raider_v)
            .is_some_and(|pos| dist_xz(*pos, bell_center) <= 4.0);
        if p.bell_rang && near {
            gathered = true;
            break;
        }
        p.pump(300).await;
    }
    assert!(
        gathered,
        "пре-рейд: колокол {bell_center:?} rang={}, villager={:?}",
        p.bell_rang,
        p.all_pos.get(&raider_v)
    );
    println!("raid: villager rang the bell at {bell_center:?}");

    let mut wave = false;
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(40_000) {
        if p.raiders_seen {
            wave = true;
            break;
        }
        p.pump(300).await;
    }
    assert!(wave, "первая волна рейда не появилась");
    // Рейдеров уводим далеко (но в 112 блоках): волна жива, житель прячется.
    for name in ["pillager", "vindicator", "evoker", "witch", "ravager"] {
        p.cmd(&format!(
            "tp @e[type=minecraft:{name}] {} {Y} {}",
            bx + 60.5,
            bz + 60.5
        ))
        .await;
    }
    let bed_a = [bx + 0.5, f64::from(Y), bz + 0.5];
    let bed_b = [bx + 3.5, f64::from(Y), bz + 0.5];
    let mut hidden = false;
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(30_000) {
        if let Some(pos) = p.all_pos.get(&raider_v).copied() {
            let near = dist_xz(pos, bed_a).min(dist_xz(pos, bed_b)) <= 2.0;
            if near {
                hidden = true;
                break;
            }
        }
        p.pump(300).await;
    }
    assert!(
        hidden,
        "рейд: житель не спрятался в доме: {:?}",
        p.all_pos.get(&raider_v)
    );
    println!("raid: villager hid at home");

    // Победа: добиваем волны — рейд переходит в `Victory`, жители
    // празднуют (звук и фейерверк, `CelebrateVillagersSurvivedRaid`).
    let mut fireworks = false;
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(120_000) {
        for name in ["pillager", "vindicator", "evoker", "witch", "ravager"] {
            p.cmd(&format!("kill @e[type=minecraft:{name}]")).await;
        }
        p.pump(200).await;
        if p.firework_seen {
            fireworks = true;
            break;
        }
    }
    assert!(
        fireworks,
        "празднование рейда: фейерверк не появился (firework_type {})",
        p.firework_type
    );
    println!("raid: victory celebration fireworks");
    p.cmd("difficulty peaceful").await;

    // Изолированный сценарий: `GiveGiftToHero` (600..6600 тиков, 20 тиков
    // на поворот, бросок в 5 блоках) и look-пакет.
    let px = X as f64 - 20.5;
    let pz = Z as f64 + 30.5;
    p.cmd(&format!(
        "fill {} {} {} {} {} {} minecraft:stone",
        px.floor() as i32 - 4,
        Y - 1,
        pz.floor() as i32 - 4,
        px.floor() as i32 + 4,
        Y - 1,
        pz.floor() as i32 + 4
    ))
    .await;
    p.cmd(&format!(
        "fill {} {} {} {} {} {} minecraft:stone",
        px.floor() as i32 - 2,
        Y,
        pz.floor() as i32 - 2,
        px.floor() as i32 + 2,
        Y + 3,
        pz.floor() as i32 + 2
    ))
    .await;
    p.cmd(&format!(
        "fill {} {} {} {} {} {} minecraft:air",
        px.floor() as i32 - 1,
        Y,
        pz.floor() as i32 - 1,
        px.floor() as i32 + 1,
        Y + 2,
        pz.floor() as i32 + 1
    ))
    .await;
    let before_gift: HashSet<i32> = p.seen_villagers.iter().copied().collect();
    p.cmd(&format!(
        "summon minecraft:villager {px} {Y} {pz} {{PersistenceRequired:1b,VillagerData:{{type:\"minecraft:plains\",profession:\"minecraft:farmer\",level:1}}}}"
    ))
    .await;
    p.cmd(&format!(
        "summon minecraft:cat {} {Y} {} {{NoAI:1b,PersistenceRequired:1b}}",
        px - 1.0,
        pz + 1.0
    ))
    .await;
    p.cmd(&format!("tp @s {} {Y} {}", px + 1.5, pz)).await;
    p.cmd("effect give @s minecraft:hero_of_the_village 6000 0")
        .await;
    let mut gift_v = None;
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(5_000) {
        gift_v = p
            .seen_villagers
            .difference(&before_gift)
            .copied()
            .find(|id| !before_gift.contains(id));
        if gift_v.is_some() {
            break;
        }
        p.pump(200).await;
    }
    let gift_v = gift_v.expect("житель для подарка не появился");
    let cat_id = p.cat_id.expect("кошка для look-пакета не появилась");
    let cat_at = [px - 1.0, f64::from(Y), pz + 1.0];
    let mut looked_cat = false;
    let player_at = [px + 1.5, f64::from(Y), pz];
    let mut looked = false;
    let mut gift = false;
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(70_000) {
        if let (Some(pos), Some(cpos)) = (
            p.all_pos.get(&gift_v).copied(),
            p.all_pos.get(&cat_id).copied(),
        ) && dist_xz(pos, cpos) <= 2.5
            && let Some(yaw) = p.all_yaw.get(&gift_v).copied()
        {
            let want = look_yaw(pos, cpos);
            let d = (yaw - want + 180.0).rem_euclid(360.0) - 180.0;
            if d.abs() < 45.0 {
                looked_cat = true;
            }
        }
        if let Some(pos) = p.all_pos.get(&gift_v).copied() {
            let want = look_yaw(pos, player_at);
            if let Some(yaw) = p.all_yaw.get(&gift_v).copied() {
                let d = (yaw - want + 180.0).rem_euclid(360.0) - 180.0;
                if d.abs() < 45.0 {
                    looked = true;
                }
            }
        }
        p.chat.clear();
        p.cmd(&format!(
            "execute if entity @e[type=minecraft:item,x={px},y={Y},z={pz},distance=..6] run say GIFT_YES"
        ))
        .await;
        if p.chat.contains("GIFT_YES") {
            gift = true;
            break;
        }
        p.pump(300).await;
    }
    assert!(looked, "look-пакет: житель не смотрел на игрока");
    assert!(
        looked_cat,
        "InteractWith: житель не смотрел на кошку (v {:?}/{:?}, cat {:?})",
        p.all_pos.get(&gift_v),
        p.all_yaw.get(&gift_v),
        p.all_pos.get(&cat_id)
    );
    let _ = cat_at;
    assert!(gift, "GiveGiftToHero: подарок не брошен");
    println!("gift: hero received a gift; look at player ok");
    p.cmd("effect clear @s").await;

    println!("villager_schedule_probe: ok");
}
