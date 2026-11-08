//! Крафтер живым ботом: тот же сценарий на нашем сервере и на `server.jar`,
//! вывод — нормализованный журнал для сравнения.
//!
//! Бревно через воронку в слот 0, импульс сверху — доски выпадают вперёд
//! (восток), бревно −1, компаратор сбоку читает число занятых слотов. Пустая
//! сетка — осечка `1050`. Окно: содержимое, данные, выключение слота 0.
//! Сундук впереди принимает результат.
//!
//! `crafter_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer};
use std::collections::HashMap;
use std::time::Duration;

const CRAFTER: [i32; 3] = [10, 200, 8];
const COMPARATOR: [i32; 3] = [10, 200, 9];
/// Провод за компаратором: сила его выхода.
const OUT: [i32; 3] = [10, 200, 10];
const POWER: [i32; 3] = [10, 201, 8];

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
    item_type: i32,
    states: HashMap<i32, String>,
    items: HashMap<i32, usize>,
    log: Vec<String>,
    window: i32,
}

impl Probe {
    fn sb(&self, n: &str) -> i32 {
        self.table
            .id(State::Play, Direction::Serverbound, n)
            .unwrap()
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
            CRAFTER => "crafter",
            OUT => "out wire",
            [11, 200, 6] => "chest out wire",
            _ => return,
        };
        let name = self
            .states
            .get(&state)
            .cloned()
            .unwrap_or_else(|| state.to_string());
        let line = format!("{what} {name}");
        // Повтор того же состояния (пакет чанка и правка) не пишется.
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
                if (pos[0] - 11.2).abs() > 1.0
                    || (pos[2] - 8.5).abs() > 1.0
                    || (pos[1] - 200.3).abs() > 1.5
                {
                    return;
                }
                let n = self.items.len();
                self.items.insert(id, n);
                self.log.push(format!(
                    "add item#{n} at {:.3} {:.3} {:.3}",
                    pos[0], pos[1], pos[2]
                ));
            }
            "set_entity_data" => {
                let id = r.var_int().unwrap();
                if let Some(n) = self.items.get(&id) {
                    // Предмет — слот в данных сущности: хвост как есть.
                    self.log
                        .push(format!("item#{n} data {}", hex(r.remaining())));
                }
            }
            "level_event" => {
                let ev = i32::from_be_bytes(r.bytes(4).unwrap().try_into().unwrap());
                let p = unpack(r.i64().unwrap());
                let data = i32::from_be_bytes(r.bytes(4).unwrap().try_into().unwrap());
                if p == CRAFTER {
                    self.log.push(format!("event {ev} data {data}"));
                }
            }
            "block_update" => {
                let p = unpack(r.i64().unwrap());
                let s = r.var_int().unwrap();
                self.block(p, s);
            }
            "section_blocks_update" => {
                let sec = r.i64().unwrap();
                let (sx, sy, sz) = (
                    (sec >> 42) as i32,
                    (sec << 44 >> 44) as i32,
                    (sec << 22 >> 42) as i32,
                );
                let n = r.var_int().unwrap();
                for _ in 0..n {
                    let v = r.var_long().unwrap();
                    let (state, x, z, y) = (
                        (v >> 12) as i32,
                        (v >> 8 & 15) as i32,
                        (v >> 4 & 15) as i32,
                        (v & 15) as i32,
                    );
                    self.block([sx * 16 + x, sy * 16 + y, sz * 16 + z], state);
                }
            }
            "open_screen" => {
                self.window = r.var_int().unwrap();
                let menu = r.var_int().unwrap();
                self.log
                    .push(format!("open menu {menu} title {}", hex(r.remaining())));
            }
            "container_set_content" => {
                let w = r.var_int().unwrap();
                if w == self.window && w != 0 {
                    let _state = r.var_int().unwrap();
                    self.log.push(format!("content {}", hex(r.remaining())));
                }
            }
            "container_set_slot" => {
                let w = r.var_int().unwrap();
                if w == self.window && w != 0 {
                    let _state = r.var_int().unwrap();
                    let slot = r.u16().unwrap();
                    self.log.push(format!("slot {slot} {}", hex(r.remaining())));
                }
            }
            "container_set_data" => {
                let w = r.var_int().unwrap();
                if w == self.window {
                    let (k, v) = (r.u16().unwrap(), r.u16().unwrap() as i16);
                    self.log.push(format!("data {k}={v}"));
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
        self.pump(1100).await;
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
        self.pump(600).await;
    }

    async fn slot_state(&mut self, slot: i32, enabled: bool) {
        let mut w = Writer::new();
        w.var_int(self.sb("container_slot_state_changed"))
            .var_int(slot)
            .var_int(self.window)
            .bool(enabled);
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

    async fn pulse(&mut self) {
        self.cmd("setblock 10 201 8 minecraft:redstone_block").await;
        self.ticks(12).await;
        self.cmd("setblock 10 201 8 minecraft:air").await;
        self.ticks(4).await;
    }

    /// Бросить `n` брёвен в воронку сверху и подождать, пока она отдаст.
    async fn feed(&mut self, n: i32) {
        self.cmd("clear framer").await;
        self.cmd(&format!("give framer minecraft:oak_log {n}"))
            .await;
        self.cmd("tp framer 9.5 201.2 8.5 0 90").await;
        // `DROP_ALL_ITEMS` (4): вся стопка из руки.
        self.seq += 1;
        let mut w = Writer::new();
        w.var_int(self.sb("player_action"))
            .var_int(4)
            .i64(0)
            .u8(0)
            .var_int(self.seq);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(150).await;
        self.cmd("tp framer 7.5 200 8.5 -90 0").await;
        self.ticks(40).await;
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
    let item_type = id_of("minecraft:entity_type", "minecraft:item");
    let blocks: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::path::Path::new(&a[3]).with_file_name("blocks.json"))
            .unwrap(),
    )
    .unwrap();
    let mut states = HashMap::new();
    for b in ["minecraft:crafter", "minecraft:redstone_wire"] {
        for s in blocks[b]["states"].as_array().unwrap() {
            let props = s["properties"].as_object().map(|o| {
                let mut v: Vec<String> = o
                    .iter()
                    .filter(|(k, _)| {
                        matches!(
                            k.as_str(),
                            "orientation" | "triggered" | "crafting" | "power"
                        )
                    })
                    .map(|(k, v)| format!("{k}={}", v.as_str().unwrap()))
                    .collect();
                v.sort();
                v.join(",")
            });
            states.insert(s["id"].as_i64().unwrap() as i32, props.unwrap_or_default());
        }
    }
    let ids = Ids::load(&table).unwrap();
    let mut c = Client::connect(&a[1], "framer", ids, 777, "26.3")
        .await
        .unwrap();
    let mut cb = HashMap::new();
    for n in [
        "add_entity",
        "set_entity_data",
        "level_event",
        "block_update",
        "section_blocks_update",
        "open_screen",
        "container_set_content",
        "container_set_slot",
        "container_set_data",
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
        "kill @e[type=minecraft:item]",
        "clear framer",
        "setblock 10 200 8 minecraft:crafter[orientation=east_up]",
        "setblock 10 200 9 minecraft:comparator[facing=north]",
        "setblock 10 200 10 minecraft:redstone_wire",
        "setblock 9 200 8 minecraft:hopper[facing=east]",
    ] {
        p.cmd(s).await;
    }
    p.pump(1500).await;
    p.log.clear();
    let _ = (COMPARATOR, POWER);

    p.step("hopper feeds one log");
    p.feed(1).await;
    p.step("pulse: planks out east");
    p.pulse().await;
    p.step("pulse: empty grid fails");
    p.pulse().await;
    p.step("chest in front takes the planks");
    p.feed(1).await;
    p.cmd("setblock 11 200 8 minecraft:chest").await;
    p.cmd("setblock 11 200 7 minecraft:comparator[facing=south]")
        .await;
    p.cmd("setblock 11 200 6 minecraft:redstone_wire").await;
    p.pulse().await;
    p.step("two logs spread over slots 0 and 1, no recipe");
    p.feed(2).await;
    p.pulse().await;
    p.step("open the crafter");
    p.use_on(CRAFTER, 4).await;
    p.step("disable slot 2, then enable");
    p.slot_state(2, false).await;
    p.slot_state(2, true).await;
    p.step("disable slot 0 (has a log): refused; slot 5 off");
    p.slot_state(0, false).await;
    p.slot_state(5, false).await;
    p.close().await;
    for l in &p.log {
        println!("{l}");
    }
}
