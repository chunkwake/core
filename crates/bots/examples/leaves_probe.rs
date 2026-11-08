//! Листва живым ботом против `server.jar`: `distance` от бревна по ряду,
//! пересчёт после сноса и возврата бревна, распад на случайных тиках.
//!
//! `leaves_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer};
use std::collections::{BTreeMap, HashMap};
use std::time::Duration;


fn unpack(v: i64) -> [i32; 3] {
    [(v >> 38) as i32, (v << 52 >> 52) as i32, (v << 26 >> 38) as i32]
}

struct Probe {
    c: Client,
    table: PacketTable,
    time: i64,
    cb: HashMap<i32, &'static str>,
    states: HashMap<i32, String>,
    log: Vec<String>,
    seen: BTreeMap<[i32; 3], i32>,
}

impl Probe {
    fn sb(&self, n: &str) -> i32 {
        self.table.id(State::Play, Direction::Serverbound, n).unwrap()
    }

    fn name(&self, s: i32) -> String {
        self.states.get(&s).cloned().unwrap_or_else(|| s.to_string())
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
        self.seen.insert(p, state);
    }

    fn read(&mut self, kind: &str, b: &[u8]) {
        let mut r = Reader::new(b);
        match kind {
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
        self.pump(600).await;
    }

    async fn cmd(&mut self, s: &str) {
        let mut w = Writer::new();
        w.var_int(self.sb("chat_command")).string(s);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(250).await;
    }

    /// Состояния ряда листвы x = 5..=12 одной строкой.
    fn row(&mut self, title: &str) {
        let mut parts = Vec::new();
        for x in 5..=12 {
            let s = self.seen.get(&[x, 201, 5]).map_or("-".to_owned(), |&s| {
                let n = self.name(s);
                let short = n.trim_start_matches("minecraft:");
                short
                    .replace("oak_leaves[", "L[")
                    .replace(",waterlogged=false", "")
                    .replace("persistent=false", "p0")
                    .replace("persistent=true", "p1")
            });
            parts.push(s);
        }
        self.log.push(format!("{title}: {}", parts.join(" ")));
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
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
    for n in ["block_update", "section_blocks_update"] {
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
        time: 0,
        cb,
        states,
        log: Vec::new(),
        seen: BTreeMap::new(),
    };
    p.pump(2000).await;
    for s in [
        "gamerule random_tick_speed 0",
        "fill 2 199 2 14 199 14 minecraft:stone",
        "fill 2 200 2 14 206 14 minecraft:air",
        "tp framer 7.5 200 8.5 -90 0",
    ] {
        p.cmd(s).await;
    }
    p.pump(1500).await;
    // Бревно и ряд листвы с `distance` 7: расстояния должны разойтись 1..7.
    p.cmd("setblock 5 201 5 minecraft:oak_log").await;
    p.cmd("fill 6 201 5 12 201 5 minecraft:oak_leaves[distance=7,persistent=false]").await;
    p.ticks(20).await;
    p.row("after fill");
    p.cmd("setblock 5 201 5 minecraft:air").await;
    p.ticks(20).await;
    p.row("log removed");
    p.cmd("setblock 5 201 5 minecraft:oak_log").await;
    p.ticks(20).await;
    p.row("log back");
    p.cmd("setblock 5 201 5 minecraft:air").await;
    p.ticks(20).await;
    p.cmd("gamerule random_tick_speed 4096").await;
    p.ticks(100).await;
    p.cmd("gamerule random_tick_speed 0").await;
    p.pump(500).await;
    p.row("decayed");
    for l in &p.log {
        println!("{l}");
    }
}
