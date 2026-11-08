//! Рамки для предметов живым ботом: тот же сценарий на нашем сервере и на
//! `server.jar`, вывод — нормализованный журнал пакетов для сравнения.
//!
//! Поставить рамку на камень, вложить алмаз, повернуть (компаратор за камнем
//! читает 1 и 2), снять алмаз ударом, сломать рамку; светящаяся рамка с
//! картой — значок рамки на карте.
//!
//! `frame_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer};
use std::collections::HashMap;
use std::time::Duration;

fn pack(p: [i32; 3]) -> i64 {
    ((p[0] as i64 & 0x3FF_FFFF) << 38) | ((p[2] as i64 & 0x3FF_FFFF) << 12) | (p[1] as i64 & 0xFFF)
}

fn unpack(v: i64) -> [i32; 3] {
    [
        (v >> 38) as i32,
        (v << 52 >> 52) as i32,
        (v << 26 >> 38) as i32,
    ]
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

struct Probe {
    c: Client,
    table: PacketTable,
    seq: i32,
    time: i64,
    cb: HashMap<i32, &'static str>,
    all: HashMap<i32, String>,
    /// Типы сущностей, за которыми следим: рамка, светящаяся рамка, предмет.
    types: HashMap<i32, &'static str>,
    sounds: HashMap<i32, String>,
    /// Наши сущности: настоящий ID → (порядковый, тип).
    seen: HashMap<i32, (usize, &'static str)>,
    frames: Vec<i32>,
    log: Vec<String>,
}

impl Probe {
    fn sb(&self, n: &str) -> i32 {
        self.table
            .id(State::Play, Direction::Serverbound, n)
            .unwrap()
    }

    fn name(&self, id: i32) -> String {
        match self.seen.get(&id) {
            Some((n, t)) => format!("{t}#{n}"),
            None => "?".into(),
        }
    }

    async fn pump(&mut self, ms: u64) {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
        while let Ok(Ok(ev)) = tokio::time::timeout_at(deadline, self.c.next_event()).await {
            if let Event::GameTime(t) = ev {
                self.time = t;
            }
        }
        for (id, body) in std::mem::take(&mut self.c.captured_by) {
            match self.cb.get(&id) {
                Some(kind) => self.read(kind, &body),
                None => {
                    let n = self.all.get(&id).cloned().unwrap_or_default();
                    if !matches!(
                        n.as_str(),
                        "minecraft:keep_alive"
                            | "minecraft:set_time"
                            | "minecraft:level_chunk_with_light"
                            | "minecraft:move_entity_pos"
                            | "minecraft:set_entity_motion"
                            | "minecraft:rotate_head"
                            | "minecraft:move_entity_pos_rot"
                            | "minecraft:entity_position_sync"
                            | "minecraft:move_entity_rot"
                    ) {
                        self.log.push(format!("pkt {n} {}", body.len()));
                    }
                }
            }
        }
    }

    fn read(&mut self, kind: &str, b: &[u8]) {
        let mut r = Reader::new(b);
        match kind {
            "add_entity" => {
                let id = r.var_int().unwrap();
                r.bytes(16).unwrap();
                let ty = r.var_int().unwrap();
                let Some(&t) = self.types.get(&ty) else {
                    if std::env::var("FP_DEBUG").is_ok() {
                        self.log.push(format!("other add_entity type {ty}"));
                    }
                    return;
                };
                let pos = [r.f64().unwrap(), r.f64().unwrap(), r.f64().unwrap()];
                chunkwake_protocol::play::read_lp_vec3(&mut r).unwrap();
                let (pitch, yaw, _head) = (r.u8().unwrap(), r.u8().unwrap(), r.u8().unwrap());
                let data = r.var_int().unwrap();
                // Чужие предметы мира (далеко от стенда) — мимо.
                if (pos[0] - 9.0).abs() > 16.0
                    || (pos[2] - 8.0).abs() > 16.0
                    || (pos[1] - 200.0).abs() > 16.0
                {
                    return;
                }
                let n = self.seen.len();
                self.seen.insert(id, (n, t));
                if t != "item" {
                    self.frames.push(id);
                }
                // У предмета скорость случайная: только место.
                self.log.push(format!(
                    "add {t}#{n} pos {:.3} {:.3} {:.3} yaw {yaw} pitch {pitch} data {data}",
                    pos[0], pos[1], pos[2]
                ));
            }
            "set_entity_data" => {
                let id = r.var_int().unwrap();
                if self.seen.contains_key(&id) {
                    let name = self.name(id);
                    self.log.push(format!("data {name} {}", hex(r.remaining())));
                }
            }
            "remove_entities" => {
                let n = r.var_int().unwrap();
                let names: Vec<String> = (0..n)
                    .map(|_| r.var_int().unwrap())
                    .filter(|id| self.seen.contains_key(id))
                    .map(|id| self.name(id))
                    .collect();
                if !names.is_empty() {
                    self.log.push(format!("remove {}", names.join(" ")));
                }
            }
            "sound" => {
                let holder = r.var_int().unwrap();
                if let Some(s) = self.sounds.get(&(holder - 1)) {
                    let rest = r.remaining();
                    // Без сида (последние 8 байт).
                    self.log
                        .push(format!("sound {s} {}", hex(&rest[..rest.len() - 8])));
                }
            }
            "block_update" => {
                let p = unpack(r.i64().unwrap());
                if p == [11, 200, 8] {
                    self.log
                        .push(format!("comparator state {}", r.var_int().unwrap()));
                }
            }
            "teleport_entity" | "entity_position_sync" => {
                let id = r.var_int().unwrap();
                if self.seen.contains_key(&id) && std::env::var("FP_DEBUG").is_ok() {
                    let name = self.name(id);
                    let p = [r.f64().unwrap(), r.f64().unwrap(), r.f64().unwrap()];
                    self.log
                        .push(format!("moved {name} {:.3} {:.3} {:.3}", p[0], p[1], p[2]));
                }
            }
            "map_item_data" => {
                self.log.push(format!("map {}", hex(b)));
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

    async fn use_item(&mut self) {
        self.seq += 1;
        let mut w = Writer::new();
        w.var_int(self.sb("use_item"))
            .var_int(0)
            .var_int(self.seq)
            .f32(0.0)
            .f32(0.0);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(400).await;
    }

    async fn use_on(&mut self, p: [i32; 3], face: i32) {
        self.seq += 1;
        let mut w = Writer::new();
        w.var_int(self.sb("use_item_on"))
            .var_int(0)
            .i64(pack(p))
            .var_int(face)
            .f32(0.5)
            .f32(0.5)
            .f32(0.5)
            .u8(0)
            .u8(0)
            .var_int(self.seq);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(400).await;
    }

    async fn interact(&mut self, entity: i32) {
        let mut w = Writer::new();
        w.var_int(self.sb("interact")).var_int(entity).var_int(0);
        chunkwake_protocol::play::lp_vec3(&mut w, [0.0, 0.0, 0.0]);
        w.bool(false);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(400).await;
    }

    async fn attack(&mut self, entity: i32) {
        let mut w = Writer::new();
        w.var_int(self.sb("attack")).var_int(entity);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(400).await;
    }

    fn step(&mut self, title: &str) {
        self.log.push(format!("== {title}"));
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let reg: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&a[3]).unwrap()).unwrap();
    let id_of = |r: &str, k: &str| reg[r]["entries"][k]["protocol_id"].as_i64().unwrap() as i32;
    let types: HashMap<i32, &'static str> = [
        ("minecraft:item_frame", "frame"),
        ("minecraft:glow_item_frame", "glow"),
        ("minecraft:item", "item"),
    ]
    .into_iter()
    .map(|(k, v)| (id_of("minecraft:entity_type", k), v))
    .collect();
    let sounds: HashMap<i32, String> = reg["minecraft:sound_event"]["entries"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(k, _)| k.contains("item_frame"))
        .map(|(k, v)| {
            (
                v["protocol_id"].as_i64().unwrap() as i32,
                k.replace("minecraft:entity.", ""),
            )
        })
        .collect();
    let ids = Ids::load(&table).unwrap();
    let mut c = Client::connect(&a[1], "framer", ids, 777, "26.3")
        .await
        .unwrap();
    let mut cb = HashMap::new();
    for n in [
        "add_entity",
        "set_entity_data",
        "remove_entities",
        "sound",
        "block_update",
        "map_item_data",
        "teleport_entity",
        "entity_position_sync",
    ] {
        let id = table.id(State::Play, Direction::Clientbound, n).unwrap();
        cb.insert(id, n);
        c.capture_also.push(id);
    }
    let pj: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&a[2]).unwrap()).unwrap();
    let all: HashMap<i32, String> = pj["play"]["clientbound"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (v["protocol_id"].as_i64().unwrap() as i32, k.clone()))
        .collect();
    if std::env::var("FP_DEBUG").is_ok() {
        c.capture_also
            .extend(all.keys().copied().filter(|k| !cb.contains_key(k)));
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
        all,
        types,
        sounds,
        seen: HashMap::new(),
        frames: Vec::new(),
        log: Vec::new(),
    };
    p.pump(2000).await;
    for s in [
        "gamemode survival",
        "fill 4 199 4 14 199 14 minecraft:stone",
        "fill 4 200 4 14 203 14 minecraft:air",
        "tp framer 8.5 200 8.5 -90 0",
        "kill @e[type=minecraft:item]",
        "clear framer",
        "give framer minecraft:item_frame 2",
        "give framer minecraft:glow_item_frame",
        "give framer minecraft:diamond",
        "give framer minecraft:map",
        "setblock 10 200 8 minecraft:stone",
        "setblock 11 200 8 minecraft:comparator[facing=west]",
    ] {
        p.cmd(s).await;
    }
    p.pump(1500).await;
    p.log.clear();
    p.seen.clear();
    p.frames.clear();

    p.step("place frame on the west face of the stone");
    p.hold(0).await;
    p.use_on([10, 200, 8], 4).await;
    p.ticks(5).await;
    let Some(&frame) = p.frames.first() else {
        p.log.push("no frame".into());
        for l in &p.log {
            println!("{l}");
        }
        return;
    };
    p.step("diamond into the frame");
    p.hold(2).await;
    p.interact(frame).await;
    p.ticks(5).await;
    p.step("rotate");
    p.interact(frame).await;
    p.ticks(5).await;
    p.step("hit: diamond pops out");
    p.attack(frame).await;
    p.ticks(5).await;
    p.step("hit again: frame breaks");
    p.attack(frame).await;
    p.ticks(5).await;

    p.step("glow frame with a map on the north face");
    p.hold(3).await;
    p.use_item().await;
    p.hold(1).await;
    p.use_on([10, 200, 8], 2).await;
    p.ticks(3).await;
    if let Some(&glow) = p.frames.get(1) {
        p.hold(3).await;
        p.interact(glow).await;
        p.ticks(40).await;
        p.pump(3000).await;
    }
    for l in &p.log {
        println!("{l}");
    }
}
