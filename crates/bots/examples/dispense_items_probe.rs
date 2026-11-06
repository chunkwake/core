//! Особые предметы раздатчика и те же действия игрока живым ботом: один
//! сценарий на нашем сервере и на `server.jar`, вывод — журнал для сравнения.
//!
//! Каждый шаг ставит новый раздатчик (восток), кладёт в него стопку через
//! окно, даёт импульс и открывает раздатчик снова: в журнале события у
//! раздатчика и блока перед ним, смена блока перед ним, звуки, выпавшие
//! предметы и содержимое после выброса. Затем игрок: светокамень по якорю,
//! ножницы и бутылка по полному улью, бутылка воды по земле.
//!
//! `dispense_items_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer};
use std::collections::HashMap;
use std::time::Duration;

const DISP: [i32; 3] = [10, 200, 8];
const FRONT: [i32; 3] = [11, 200, 8];
/// Блок для действий игрока: под ногами впереди.
const TARGET: [i32; 3] = [9, 200, 10];

fn pack(p: [i32; 3]) -> i64 {
    ((p[0] as i64 & 0x3FF_FFFF) << 38) | ((p[2] as i64 & 0x3FF_FFFF) << 12) | (p[1] as i64 & 0xFFF)
}

fn unpack(v: i64) -> [i32; 3] {
    [(v >> 38) as i32, (v << 52 >> 52) as i32, (v << 26 >> 38) as i32]
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn label(p: [i32; 3]) -> Option<&'static str> {
    match p {
        DISP => Some("disp"),
        FRONT => Some("front"),
        TARGET => Some("target"),
        _ => None,
    }
}

struct Probe {
    c: Client,
    table: PacketTable,
    seq: i32,
    time: i64,
    cb: HashMap<i32, &'static str>,
    item_type: i32,
    states: HashMap<i32, String>,
    sounds: HashMap<i32, String>,
    items: HashMap<i32, usize>,
    log: Vec<String>,
    window: i32,
}

impl Probe {
    fn sb(&self, n: &str) -> i32 {
        self.table.id(State::Play, Direction::Serverbound, n).unwrap()
    }

    async fn pump(&mut self, ms: u64) {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
        while let Ok(Ok(ev)) = tokio::time::timeout_at(deadline, self.c.next_event()).await {
            if let Event::GameTime(t) = ev {
                self.time = t;
            }
        }
        for (id, body) in std::mem::take(&mut self.c.captured_by) {
            if let Some(kind) = self.cb.get(&id) {
                self.read(kind, &body);
            }
        }
    }

    fn block(&mut self, p: [i32; 3], state: i32) {
        let Some(what) = label(p) else { return };
        let name = self.states.get(&state).cloned().unwrap_or_else(|| state.to_string());
        let line = format!("{what} {name}");
        if self.log.last() != Some(&line) {
            self.log.push(line);
        }
    }

    fn read(&mut self, kind: &str, b: &[u8]) {
        let mut r = Reader::new(b);
        match kind {
            "add_entity" => {
                let id = r.var_int().unwrap();
                r.bytes(16).unwrap();
                if r.var_int().unwrap() != self.item_type {
                    return;
                }
                let pos = [r.f64().unwrap(), r.f64().unwrap(), r.f64().unwrap()];
                if (pos[0] - 10.0).abs() > 3.0 || (pos[2] - 9.0).abs() > 3.0 || (pos[1] - 200.5).abs() > 2.0 {
                    return;
                }
                let n = self.items.len();
                self.items.insert(id, n);
                self.log.push(format!("add item#{n}"));
            }
            "set_entity_data" => {
                let id = r.var_int().unwrap();
                if let Some(n) = self.items.get(&id) {
                    self.log.push(format!("item#{n} data {}", hex(r.remaining())));
                }
            }
            "level_event" => {
                let ev = i32::from_be_bytes(r.bytes(4).unwrap().try_into().unwrap());
                let p = unpack(r.i64().unwrap());
                let data = i32::from_be_bytes(r.bytes(4).unwrap().try_into().unwrap());
                if let Some(what) = label(p) {
                    self.log.push(format!("event {ev} at {what} data {data}"));
                }
            }
            "sound" => {
                let id = r.var_int().unwrap();
                let name = if id == 0 {
                    let n = format!("inline {}", r.string(256).unwrap());
                    if r.bool().unwrap() {
                        r.f32().unwrap();
                    }
                    n
                } else {
                    self.sounds.get(&(id - 1)).cloned().unwrap_or_else(|| (id - 1).to_string())
                };
                let source = r.var_int().unwrap();
                let mut int = || i32::from_be_bytes(r.bytes(4).unwrap().try_into().unwrap());
                let at = [int(), int(), int()].map(|v| v as f64 / 8.0);
                let (vol, pitch) = (r.f32().unwrap(), r.f32().unwrap());
                if (at[0] - 10.0).abs() > 4.0 || (at[2] - 9.0).abs() > 4.0 {
                    return;
                }
                self.log.push(format!(
                    "sound {name} src {source} at {:.2} {:.2} {:.2} vol {vol} pitch {pitch:.3}",
                    at[0], at[1], at[2]
                ));
            }
            "level_particles" => {
                // Только число: точки брызг случайны.
                self.log.push("particles".into());
            }
            "block_update" => {
                let p = unpack(r.i64().unwrap());
                let s = r.var_int().unwrap();
                self.block(p, s);
            }
            "section_blocks_update" => {
                let sec = r.i64().unwrap();
                let (sx, sy, sz) = ((sec >> 42) as i32, (sec << 44 >> 44) as i32, (sec << 22 >> 42) as i32);
                let n = r.var_int().unwrap();
                for _ in 0..n {
                    let v = r.var_long().unwrap();
                    let (state, x, z, y) = ((v >> 12) as i32, (v >> 8 & 15) as i32, (v >> 4 & 15) as i32, (v & 15) as i32);
                    self.block([sx * 16 + x, sy * 16 + y, sz * 16 + z], state);
                }
            }
            "open_screen" => {
                self.window = r.var_int().unwrap();
            }
            "container_set_content" => {
                let w = r.var_int().unwrap();
                if w == self.window && w != 0 {
                    let _state = r.var_int().unwrap();
                    // Только девять слотов раздатчика; хвост — инвентарь игрока.
                    self.log.push(format!("content {}", hex(r.remaining())));
                }
            }
            "container_set_slot" => {
                let w = r.var_int().unwrap();
                let _state = r.var_int().unwrap();
                let slot = r.u16().unwrap();
                if w == 0 {
                    self.log.push(format!("inv slot {slot} {}", hex(r.remaining())));
                }
            }
            _ => {}
        }
    }

    async fn ticks(&mut self, n: i64) {
        let start = std::time::Instant::now();
        while self.time == 0 && start.elapsed().as_secs() < 30 {
            self.pump(200).await;
        }
        let until = self.time + n;
        while self.time < until && start.elapsed().as_secs() < 120 {
            self.pump(200).await;
        }
        self.pump(600).await;
    }

    async fn cmd(&mut self, s: &str) {
        let mut w = Writer::new();
        w.var_int(self.sb("chat_command")).string(s);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(200).await;
    }

    async fn use_on(&mut self, p: [i32; 3], face: i32) {
        self.seq += 1;
        let mut w = Writer::new();
        w.var_int(self.sb("use_item_on")).var_int(0).i64(pack(p)).var_int(face).f32(0.5).f32(1.0).f32(0.5).u8(0).u8(0).var_int(self.seq);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(600).await;
    }

    async fn close(&mut self) {
        let mut w = Writer::new();
        w.var_int(self.sb("container_close")).var_int(self.window);
        self.c.send_raw(&w.buf).await.unwrap();
        self.window = -1;
        self.pump(200).await;
    }

    /// Новый раздатчик с блоком `front` перед ним и стопкой `give` внутри.
    async fn setup(&mut self, front: &str, give: &str) {
        self.cmd("kill @e[type=minecraft:item]").await;
        self.cmd("setblock 10 200 8 minecraft:air").await;
        self.cmd("setblock 10 200 8 minecraft:dispenser[facing=east]").await;
        self.cmd(&format!("setblock 11 200 8 {front}")).await;
        self.cmd("clear framer").await;
        self.cmd(&format!("give framer {give}")).await;
        self.pump(300).await;
        // Окно: 0–8 раздатчик, 36 — первый слот хотбара; шифт-клик.
        let quiet = self.log.len();
        self.use_on(DISP, 1).await;
        let mut w = Writer::new();
        w.var_int(self.sb("container_click")).var_int(self.window).var_int(0).u16(36).u8(0).var_int(1).var_int(0).bool(false);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(300).await;
        self.close().await;
        self.pump(300).await;
        self.log.truncate(quiet);
    }

    async fn fire(&mut self) {
        self.cmd("setblock 9 200 8 minecraft:redstone_block").await;
        self.ticks(6).await;
        self.cmd("setblock 9 200 8 minecraft:air").await;
        self.ticks(4).await;
        self.use_on(DISP, 1).await;
        self.close().await;
    }

    async fn step(&mut self, title: &str, front: &str, give: &str) {
        self.setup(front, give).await;
        self.log.push(format!("== {title}"));
        self.fire().await;
    }

    /// Игрок с `give` в руке кликает по верху `TARGET` с блоком `block`.
    async fn player(&mut self, title: &str, block: &str, give: &str) {
        self.cmd("kill @e[type=minecraft:item]").await;
        self.cmd(&format!("setblock 9 200 10 {block}")).await;
        self.cmd("clear framer").await;
        self.cmd(&format!("give framer {give}")).await;
        self.pump(400).await;
        self.log.push(format!("== player: {title}"));
        self.use_on(TARGET, 1).await;
        self.ticks(4).await;
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let reg: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&a[3]).unwrap()).unwrap();
    let id_of = |r: &str, k: &str| reg[r]["entries"][k]["protocol_id"].as_i64().unwrap() as i32;
    let item_type = id_of("minecraft:entity_type", "minecraft:item");
    let sounds = reg["minecraft:sound_event"]["entries"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (v["protocol_id"].as_i64().unwrap() as i32, k.clone()))
        .collect();
    let blocks: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::path::Path::new(&a[3]).with_file_name("blocks.json")).unwrap(),
    )
    .unwrap();
    let mut states = HashMap::new();
    for (name, b) in blocks.as_object().unwrap() {
        for s in b["states"].as_array().unwrap() {
            let mut props: Vec<String> = s["properties"]
                .as_object()
                .map(|o| o.iter().map(|(k, v)| format!("{k}={}", v.as_str().unwrap())).collect())
                .unwrap_or_default();
            props.sort();
            states.insert(s["id"].as_i64().unwrap() as i32, format!("{name}[{}]", props.join(",")));
        }
    }
    let ids = Ids::load(&table).unwrap();
    let mut c = Client::connect(&a[1], "framer", ids, 777, "26.3").await.unwrap();
    let mut cb = HashMap::new();
    for n in [
        "add_entity",
        "set_entity_data",
        "level_event",
        "sound",
        "level_particles",
        "block_update",
        "section_blocks_update",
        "open_screen",
        "container_set_content",
        "container_set_slot",
    ] {
        let id = table.id(State::Play, Direction::Clientbound, n).unwrap();
        cb.insert(id, n);
        c.capture_also.push(id);
    }
    loop {
        if let Event::Position(_) = c.next_event().await.unwrap() {
            break;
        }
    }
    let mut p = Probe {
        c,
        table,
        seq: 0,
        time: 0,
        cb,
        item_type,
        states,
        sounds,
        items: HashMap::new(),
        log: Vec::new(),
        window: -1,
    };
    p.pump(2000).await;
    for s in [
        "gamemode survival",
        "fill 4 199 4 14 199 14 minecraft:stone",
        "fill 4 200 4 14 203 14 minecraft:air",
        "tp framer 7.5 200 8.5 -90 0",
    ] {
        p.cmd(s).await;
    }
    p.pump(1500).await;
    p.log.clear();

    let water = "minecraft:potion[potion_contents={potion:\"minecraft:water\"}]";
    let awkward = "minecraft:potion[potion_contents={potion:\"minecraft:awkward\"}]";
    let hive = "minecraft:beehive[facing=west,honey_level=5]";
    p.step("bottle at air: dropped with a fail click", "minecraft:air", "minecraft:glass_bottle 2").await;
    p.step("bottle at water: water bottle into the dispenser", "minecraft:water", "minecraft:glass_bottle 2").await;
    p.step("last bottle at a full hive: honey bottle", hive, "minecraft:glass_bottle 1").await;
    p.step("bottle at an empty hive: dropped", "minecraft:beehive[facing=west]", "minecraft:glass_bottle 1").await;
    p.step("glowstone charges the anchor", "minecraft:respawn_anchor", "minecraft:glowstone 2").await;
    p.step("glowstone at a full anchor: fail", "minecraft:respawn_anchor[charges=4]", "minecraft:glowstone 1").await;
    p.step("glowstone at air: dropped", "minecraft:air", "minecraft:glowstone 1").await;
    p.step("honeycomb waxes copper", "minecraft:copper_block", "minecraft:honeycomb 1").await;
    p.step("honeycomb at stone: dropped", "minecraft:stone", "minecraft:honeycomb 1").await;
    p.step("water bottle turns dirt to mud", "minecraft:dirt", water).await;
    p.step("awkward potion at dirt: dropped twice-clicked", "minecraft:dirt", awkward).await;
    p.step("shears at a full hive: honeycomb", hive, "minecraft:shears").await;
    p.step("powder snow bucket into air", "minecraft:air", "minecraft:powder_snow_bucket").await;
    p.step("powder snow bucket at stone: dropped", "minecraft:stone", "minecraft:powder_snow_bucket").await;
    p.step("bucket picks powder snow", "minecraft:powder_snow", "minecraft:bucket").await;
    p.step("bucket drains a waterlogged slab", "minecraft:oak_slab[waterlogged=true]", "minecraft:bucket").await;
    p.step("bucket of two at water: one filled into a free slot", "minecraft:water", "minecraft:bucket 2").await;
    p.step("bucket at stone: dropped", "minecraft:stone", "minecraft:bucket").await;

    p.player("glowstone charges the anchor", "minecraft:respawn_anchor", "minecraft:glowstone 2").await;
    p.player("shears at a full hive", "minecraft:beehive[facing=north,honey_level=5]", "minecraft:shears").await;
    p.player("bottle at a full hive", "minecraft:beehive[facing=north,honey_level=5]", "minecraft:glass_bottle 3").await;
    p.player("water bottle on dirt", "minecraft:dirt", water).await;
    for l in &p.log {
        println!("{l}");
    }
}
