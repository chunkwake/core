//! Расписание жителя (`timeline/villager_schedule.json` 26.3): днём идёт к
//! рабочему блоку, в `meet` — к колоколу, ночью — к кровати.
//!
//! `villager_schedule_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

const X: i32 = 3456;
const Z: i32 = 8;
const Y: i32 = 100;

struct Probe {
    c: Client,
    sb_command: i32,
    cb_chat: i32,
    cb_meta: i32,
    cb_block: i32,
    cb_section: i32,
    cb_add: i32,
    cb_move: i32,
    cb_teleport: i32,
    chat: String,
    /// Первый житель (спавн у X+0.5) и его позиция из пакетов.
    a_id: Option<i32>,
    a_pos: Option<[f64; 3]>,
    a_yaw: f32,
    a_base: [i64; 3],
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
                let (Ok(_kind), Ok(x), Ok(y), Ok(z)) = (r.var_int(), r.f64(), r.f64(), r.f64())
                else {
                    continue;
                };
                let spawn = [X as f64 + 0.5, f64::from(Y), Z as f64 + 0.5];
                if dist_xz([x, y, z], spawn) < 0.6 {
                    self.a_id = Some(entity);
                    self.a_pos = Some([x, y, z]);
                    self.a_base = chunkwake_protocol::play::quantize([x, y, z]);
                }
            } else if id == self.cb_move {
                let mut r = Reader::new(&body);
                let Ok(entity) = r.var_int() else { continue };
                if Some(entity) != self.a_id {
                    continue;
                }
                if r.var_int().is_err() {
                    continue;
                }
                let (Ok(dx), Ok(dy), Ok(dz)) = (r.u16(), r.u16(), r.u16()) else {
                    continue;
                };
                if let Ok(yaw) = r.u8() {
                    self.a_yaw = f32::from(yaw) * 360.0 / 256.0;
                }
                self.a_base[0] += i64::from(dx as i16);
                self.a_base[1] += i64::from(dy as i16);
                self.a_base[2] += i64::from(dz as i16);
                self.a_pos = Some([
                    self.a_base[0] as f64 / 4096.0,
                    self.a_base[1] as f64 / 4096.0,
                    self.a_base[2] as f64 / 4096.0,
                ]);
            } else if id == self.cb_teleport {
                let mut r = Reader::new(&body);
                let Ok(entity) = r.var_int() else { continue };
                if Some(entity) != self.a_id {
                    continue;
                }
                let (Ok(x), Ok(y), Ok(z)) = (r.f64(), r.f64(), r.f64()) else {
                    continue;
                };
                if let Ok(yaw) = r.f32() {
                    self.a_yaw = yaw;
                }
                self.a_pos = Some([x, y, z]);
                self.a_base = chunkwake_protocol::play::quantize([x, y, z]);
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
                if let (Ok(_entity), Ok(index)) = (r.var_int(), r.u8())
                    && index == 6
                    && let (Ok(kind), Ok(pose)) = (r.var_int(), r.var_int())
                    && kind == 20
                {
                    self.pose = Some(pose);
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
    ];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        cb_chat,
        cb_meta,
        cb_block,
        cb_section,
        cb_add,
        cb_move,
        cb_teleport,
        chat: String::new(),
        a_id: None,
        a_pos: None,
        a_yaw: 0.0,
        a_base: [0; 3],
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
    // Безработный житель сам займёт компостер и станет фермером.
    p.cmd(&format!(
        "summon minecraft:villager {X}.5 {Y} {Z}.5 {{VillagerData:{{type:\"minecraft:plains\",profession:\"minecraft:none\",level:1}},Xp:0}}"
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

    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(3_000) && !p.saw_close {
        p.pump(100).await;
    }
    assert!(p.saw_close, "дверь осталась открытой после прохода");
    println!("door: closed after passing");

    // 2. Встреча: 9000..11000 — идёт к колоколу.
    p.cmd("time set 9600").await;
    let at_bell = p
        .wait_near([X as f64 + 0.5, f64::from(Y), Z as f64 + 6.5], 2.0, 12_000)
        .await
        .unwrap_or_else(|| panic!("нет позиции жителя у колокола"));
    let near = dist_xz(at_bell, [X as f64 + 0.5, f64::from(Y), Z as f64 + 6.5]);
    assert!(near <= 2.0, "житель не дошёл до колокола: {at_bell:?}");
    println!("meet: villager at bell ({near:.2})");

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
    let mut social = None;
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(30_000) {
        if let Some(pos) = p.a_pos
            && dist_xz(pos, friend) <= 2.0
        {
            social = Some((pos, p.a_yaw));
            break;
        }
        p.pump(200).await;
    }
    let (social_pos, yaw) = social.unwrap_or_else(|| {
        panic!(
            "житель не подошёл к соседу у колокола (a_pos {:?})",
            p.a_pos
        )
    });
    let want = look_yaw(social_pos, friend);
    let delta = (yaw - want + 180.0).rem_euclid(360.0) - 180.0;
    assert!(
        delta.abs() < 45.0,
        "житель не смотрит на соседа: yaw {yaw}, ожидалось {want}"
    );
    println!(
        "social: villager came to neighbour at bell ({:.2}, yaw {:.1})",
        dist_xz(social_pos, friend),
        yaw
    );

    // `Villager.gossip`: A получает слух B (25 - 20 = 5).
    let mut gossip = String::new();
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(20_000) {
        if let Some(pos) = p.a_pos {
            gossip = p.data_at(pos, "Gossips").await;
            if gossip.contains("minor_negative") {
                break;
            }
        }
        p.pump(500).await;
    }
    assert!(
        gossip.contains("minor_negative") && gossip.contains("Value:5"),
        "слух не передан при встрече: {gossip}"
    );
    println!("gossip: transferred minor_negative 25 -> 5");

    // 3. Отдых: 12000.. — идёт к своей кровати.
    p.cmd("time set 13000").await;
    let bed_foot = [X as f64 + 8.5, f64::from(Y), Z as f64 - 3.5];
    let bed_head = [X as f64 + 8.5, f64::from(Y), Z as f64 - 2.5];
    let mut at_bed = None;
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(15_000) {
        if let Some(pos) = p.pos().await {
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
            slept = p.pos().await;
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
    while start.elapsed() < Duration::from_millis(8_000) {
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

    println!("villager_schedule_probe: ok");
}
