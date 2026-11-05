//! Проигрыватель живым ботом: тот же сценарий на нашем сервере и на
//! `server.jar`, вывод — нормализованный журнал для сравнения.
//!
//! Пластинка `cat` в проигрыватель рукой (компаратор читает 2, провод рядом
//! получает 15), пустой рукой — наружу. Затем воронка сверху кладёт `13`,
//! воронка снизу забирает её обратно.
//!
//! `jukebox_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer};
use std::collections::HashMap;
use std::time::Duration;

const JUKEBOX: [i32; 3] = [10, 200, 8];
const COMPARATOR: [i32; 3] = [11, 200, 8];
const WIRE: [i32; 3] = [10, 200, 7];
/// Провод за компаратором: сила его выхода.
const OUT: [i32; 3] = [12, 200, 8];

fn pack(p: [i32; 3]) -> i64 {
    ((p[0] as i64 & 0x3FF_FFFF) << 38) | ((p[2] as i64 & 0x3FF_FFFF) << 12) | (p[1] as i64 & 0xFFF)
}

fn unpack(v: i64) -> [i32; 3] {
    [(v >> 38) as i32, (v << 52 >> 52) as i32, (v << 26 >> 38) as i32]
}

struct Probe {
    c: Client,
    table: PacketTable,
    seq: i32,
    time: i64,
    cb: HashMap<i32, &'static str>,
    item_type: i32,
    note: i32,
    states: HashMap<i32, String>,
    items: HashMap<i32, usize>,
    log: Vec<String>,
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
        let what = match p {
            JUKEBOX => "jukebox",
            COMPARATOR => "comparator",
            WIRE => "wire",
            OUT => "comparator out wire",
            _ => return,
        };
        let name = self.states.get(&state).cloned().unwrap_or_else(|| state.to_string());
        self.log.push(format!("{what} {name}"));
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
                if (pos[0] - 10.5).abs() > 0.5 || (pos[2] - 8.5).abs() > 0.5 || (pos[1] - 201.0).abs() > 2.0 {
                    return;
                }
                let n = self.items.len();
                self.items.insert(id, n);
                // x и z случайны (`offsetRandomXZ(0.7)`): только высота.
                self.log.push(format!("add item#{n} y {:.3}", pos[1]));
            }
            "remove_entities" => {
                let n = r.var_int().unwrap();
                for _ in 0..n {
                    let id = r.var_int().unwrap();
                    if let Some(i) = self.items.get(&id) {
                        self.log.push(format!("remove item#{i}"));
                    }
                }
            }
            "level_event" => {
                let ev = i32::from_be_bytes(r.bytes(4).unwrap().try_into().unwrap());
                let p = unpack(r.i64().unwrap());
                let data = i32::from_be_bytes(r.bytes(4).unwrap().try_into().unwrap());
                if p == JUKEBOX {
                    self.log.push(format!("event {ev} data {data}"));
                }
            }
            "level_particles" => {
                if r.var_int().unwrap() != self.note {
                    return;
                }
                let (force, _) = (r.bool().unwrap(), r.bool().unwrap());
                let pos = [r.f64().unwrap(), r.f64().unwrap(), r.f64().unwrap()];
                let d = [r.f32().unwrap(), r.f32().unwrap(), r.f32().unwrap()];
                let speed = r.f32().unwrap();
                r.f32().unwrap();
                r.f32().unwrap();
                let count = r.var_int().unwrap();
                // Цвет ноты случаен: только что он из четырёх.
                let color_ok = [0.0, 1.0, 2.0, 3.0].iter().any(|c| (d[0] * 24.0 - c).abs() < 1e-4);
                self.log.push(format!(
                    "note {:.2} {:.2} {:.2} force {force} color {color_ok} {} {} speed {speed} count {count}",
                    pos[0], pos[1], pos[2], d[1], d[2]
                ));
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
        // `set_time` идёт раз в секунду: дотянуть остаток реальным временем.
        self.pump(1100).await;
    }

    async fn cmd(&mut self, s: &str) {
        let mut w = Writer::new();
        w.var_int(self.sb("chat_command")).string(s);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(150).await;
    }

    async fn hold(&mut self, slot: i16) {
        let mut w = Writer::new();
        w.var_int(self.sb("set_carried_item")).u16(slot as u16);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(100).await;
    }

    async fn use_on(&mut self, p: [i32; 3], face: i32) {
        self.seq += 1;
        let mut w = Writer::new();
        w.var_int(self.sb("use_item_on")).var_int(0).i64(pack(p)).var_int(face).f32(0.5).f32(0.5).f32(0.5).u8(0).u8(0).var_int(self.seq);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(400).await;
    }

    /// `player_action` 4: выбросить один предмет из руки.
    async fn drop_one(&mut self) {
        self.seq += 1;
        let mut w = Writer::new();
        w.var_int(self.sb("player_action")).var_int(4).i64(0).u8(0).var_int(self.seq);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(200).await;
    }

    fn step(&mut self, title: &str) {
        self.log.push(format!("== {title}"));
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let reg: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&a[3]).unwrap()).unwrap();
    let id_of = |r: &str, k: &str| reg[r]["entries"][k]["protocol_id"].as_i64().unwrap() as i32;
    let item_type = id_of("minecraft:entity_type", "minecraft:item");
    let note = id_of("minecraft:particle_type", "minecraft:note");
    // Имена состояний проигрывателя, компаратора и провода по blocks.json.
    let blocks: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::path::Path::new(&a[3]).with_file_name("blocks.json")).unwrap(),
    )
    .unwrap();
    let mut states = HashMap::new();
    for b in ["minecraft:jukebox", "minecraft:comparator", "minecraft:redstone_wire"] {
        for s in blocks[b]["states"].as_array().unwrap() {
            let props = s["properties"].as_object().map(|o| {
                let mut v: Vec<String> = o
                    .iter()
                    .filter(|(k, _)| matches!(k.as_str(), "has_record" | "powered" | "power" | "mode"))
                    .map(|(k, v)| format!("{k}={}", v.as_str().unwrap()))
                    .collect();
                v.sort();
                v.join(",")
            });
            states.insert(s["id"].as_i64().unwrap() as i32, props.unwrap_or_default());
        }
    }
    let ids = Ids::load(&table).unwrap();
    let mut c = Client::connect(&a[1], "framer", ids, 777, "26.3").await.unwrap();
    let mut cb = HashMap::new();
    for n in ["add_entity", "remove_entities", "level_event", "level_particles", "block_update", "section_blocks_update"] {
        let id = table.id(State::Play, Direction::Clientbound, n).unwrap();
        cb.insert(id, n);
        c.capture_also.push(id);
    }
    loop {
        if let Event::Position(_) = c.next_event().await.unwrap() {
            break;
        }
    }
    let mut p = Probe { c, table, seq: 0, time: 0, cb, item_type, note, states, items: HashMap::new(), log: Vec::new() };
    p.pump(2000).await;
    for s in [
        "gamemode survival",
        "fill 4 199 4 14 199 14 minecraft:stone",
        "fill 4 200 4 14 203 14 minecraft:air",
        "tp framer 8.5 200 8.5 -90 0",
        "kill @e[type=minecraft:item]",
        "clear framer",
        "give framer minecraft:music_disc_cat",
        "give framer minecraft:music_disc_13",
        "setblock 10 200 8 minecraft:jukebox",
        "setblock 11 200 8 minecraft:comparator[facing=west]",
        "setblock 10 200 7 minecraft:redstone_wire",
        "setblock 12 200 8 minecraft:redstone_wire",
    ] {
        p.cmd(s).await;
    }
    p.pump(1500).await;
    p.log.clear();

    p.step("cat into the jukebox");
    p.hold(0).await;
    p.use_on(JUKEBOX, 1).await;
    p.ticks(45).await;
    p.step("empty hand pops it out");
    p.hold(5).await;
    p.use_on(JUKEBOX, 1).await;
    p.ticks(10).await;

    p.step("hopper above feeds 13, hopper below takes it");
    p.cmd("kill @e[type=minecraft:item]").await;
    p.cmd("setblock 10 199 8 minecraft:hopper[facing=east]").await;
    p.cmd("setblock 10 201 8 minecraft:hopper[facing=down]").await;
    p.cmd("tp framer 10.5 202 8.5 0 90").await;
    p.pump(500).await;
    p.hold(1).await;
    p.drop_one().await;
    p.ticks(60).await;
    for l in &p.log {
        println!("{l}");
    }
}
