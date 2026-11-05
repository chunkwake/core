//! Карты и стол картографа живым ботом.
//!
//! `place`: пустая карта → заполненная (номер 0), держать в руке — сервер
//! рисует местность и шлёт заплатки; стол: копия (карта + пустая), замок
//! (+ стекло → номер 1, `locked`), увеличение (+ бумага → номер 2, масштаб 1).
//! `check` (после перезапуска): запертая карта в руке приходит с диска с теми
//! же пикселями.
//!
//! `map_probe <addr> <packets.json> place|check`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer};
use std::collections::BTreeMap;
use std::time::Duration;

fn pack(p: [i32; 3]) -> i64 {
    ((p[0] as i64 & 0x3FF_FFFF) << 38) | ((p[2] as i64 & 0x3FF_FFFF) << 12) | (p[1] as i64 & 0xFFF)
}

/// Сводка по карте из пакетов: масштаб, замок, пиксели, значки.
#[derive(Default, Debug)]
struct Seen {
    scale: u8,
    locked: bool,
    packets: usize,
    colors: BTreeMap<(u8, u8), u8>,
    decorations: Option<usize>,
}

struct Probe {
    c: Client,
    table: PacketTable,
    seq: i32,
    time: i64,
    map_packet: i32,
    maps: BTreeMap<i32, Seen>,
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
            if id == self.map_packet {
                self.read_map(&body);
            }
        }
    }

    fn read_map(&mut self, b: &[u8]) {
        let mut r = Reader::new(b);
        let id = r.var_int().unwrap();
        let e = self.maps.entry(id).or_default();
        e.scale = r.u8().unwrap();
        e.locked = r.bool().unwrap();
        e.packets += 1;
        if r.bool().unwrap() {
            let n = r.var_int().unwrap();
            for _ in 0..n {
                r.var_int().unwrap();
                r.u8().unwrap();
                r.u8().unwrap();
                r.u8().unwrap();
                assert!(!r.bool().unwrap(), "имя значка не ждём");
            }
            e.decorations = Some(n as usize);
        }
        let w = r.u8().unwrap();
        if w > 0 {
            let h = r.u8().unwrap();
            let x = r.u8().unwrap();
            let z = r.u8().unwrap();
            let len = r.var_int().unwrap() as usize;
            let data = r.bytes(len).unwrap();
            for dz in 0..h {
                for dx in 0..w {
                    e.colors.insert((x + dx, z + dz), data[dx as usize + dz as usize * w as usize]);
                }
            }
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

    async fn cmd(&mut self, s: String) {
        let mut w = Writer::new();
        w.var_int(self.sb("chat_command")).string(&s);
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
        w.var_int(self.sb("use_item")).var_int(0).var_int(self.seq).f32(0.0).f32(0.0);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(400).await;
    }

    async fn use_on(&mut self, p: [i32; 3]) {
        self.seq += 1;
        let mut w = Writer::new();
        w.var_int(self.sb("use_item_on")).var_int(0).i64(pack(p)).var_int(1).f32(0.5).f32(1.0).f32(0.5).u8(0).u8(0).var_int(self.seq);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(400).await;
    }

    async fn shift(&mut self, window: i32, slot: i16) {
        let mut w = Writer::new();
        w.var_int(self.sb("container_click")).var_int(window).var_int(0).u16(slot as u16).u8(0).var_int(1).var_int(0).bool(false);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(300).await;
    }

    fn report(&self, title: &str) {
        println!("== {title}");
        for (id, s) in &self.maps {
            let painted = s.colors.values().filter(|&&c| c != 0).count();
            println!(
                "map {id}: scale {} locked {} packets {} painted {painted} decorations {:?}",
                s.scale, s.locked, s.packets, s.decorations
            );
        }
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let mut c = Client::connect(&a[1], "mapper", ids, 777, "26.3").await.unwrap();
    let map_packet = table.id(State::Play, Direction::Clientbound, "map_item_data").unwrap();
    c.capture_also = vec![map_packet];
    let spawn = loop {
        if let Event::Position(p) = c.next_event().await.unwrap() {
            break p;
        }
    };
    println!("spawn {spawn:?}");
    let mut p = Probe { c, table, seq: 0, time: 0, map_packet, maps: BTreeMap::new() };
    p.pump(2000).await;

    if a[3] == "check" {
        // Слот 8 — запертая копия: пиксели с диска.
        p.hold(8).await;
        p.ticks(10).await;
        p.report("after restart, holding locked copy");
        return;
    }
    // Одна точка для сверки с ванилью: карта с центром у (8, 8).
    let (x, y, z) = (8, 200, 8);
    p.cmd(format!("tp mapper {x}.5 {y} {z}.5")).await;
    p.pump(1500).await;
    for s in [
        "clear mapper".to_string(),
        "gamemode survival".into(),
        "give mapper minecraft:map 3".into(),
        "give mapper minecraft:paper".into(),
        "give mapper minecraft:glass_pane".into(),
        format!("setblock {} {y} {z} minecraft:cartography_table", x + 2),
    ] {
        p.cmd(s).await;
    }
    p.hold(0).await;
    p.use_item().await;
    // Заполненная карта — в слоте 3 хотбара; держим её, сервер рисует.
    p.hold(3).await;
    let hold = std::env::var("MAP_HOLD").ok().and_then(|v| v.parse().ok()).unwrap_or(80);
    p.ticks(hold).await;
    p.report("after holding the new map 80 ticks");
    // Стол: окно 1. Хотбар — слоты 30..38.
    p.use_on([x + 2, y, z]).await;
    p.shift(1, 33).await; // карта 0 → вход
    p.shift(1, 30).await; // пустые карты ×2 → второй вход
    p.shift(1, 2).await; // копия: 2 × карта 0 → слот 38
    p.shift(1, 1).await; // оставшуюся пустую — обратно
    p.shift(1, 38).await; // 2 × карта 0 → вход
    p.shift(1, 32).await; // стекло
    p.shift(1, 2).await; // запертая (номер 1) → слот 38
    p.shift(1, 31).await; // бумага
    p.shift(1, 2).await; // увеличенная (номер 2) → слот 37
    let mut w = Writer::new();
    w.var_int(p.sb("container_close")).var_int(1);
    p.c.send_raw(&w.buf).await.unwrap();
    p.ticks(10).await;
    p.report("after cartography table");
    p.cmd("save-all".into()).await;
    p.pump(1500).await;
}
