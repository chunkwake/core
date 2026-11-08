//! Редстоун-руда и будящий аметист живым ботом: один сценарий на нашем
//! сервере и на `server.jar`.
//!
//! Руда: клик пустой рукой, клик с камнем (руда горит и камень ставится),
//! удар в выживании, шаг на руду. Аметист: при `random_tick_speed` 4096
//! все шесть граней дорастают до друз — в воздухе и у воды-источника.
//!
//! `minerals_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer};
use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

const ORE: [i32; 3] = [9, 200, 10];
const BUD: [i32; 3] = [5, 202, 12];

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

struct Probe {
    c: Client,
    table: PacketTable,
    seq: i32,
    time: i64,
    cb: HashMap<i32, &'static str>,
    states: HashMap<i32, String>,
    log: Vec<String>,
    seen: BTreeMap<[i32; 3], i32>,
}

impl Probe {
    fn sb(&self, n: &str) -> i32 {
        self.table
            .id(State::Play, Direction::Serverbound, n)
            .unwrap()
    }

    fn name(&self, s: i32) -> String {
        self.states
            .get(&s)
            .cloned()
            .unwrap_or_else(|| s.to_string())
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
        let what = if p == ORE {
            "ore"
        } else if p == [ORE[0], ORE[1] + 1, ORE[2]] {
            "above"
        } else {
            return;
        };
        let line = format!("{what} {}", self.name(state));
        if self.log.last() != Some(&line) {
            self.log.push(line);
        }
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

    async fn use_on(&mut self, p: [i32; 3]) {
        self.seq += 1;
        let mut w = Writer::new();
        w.var_int(self.sb("use_item_on"))
            .var_int(0)
            .i64(pack(p))
            .var_int(1)
            .f32(0.5)
            .f32(1.0)
            .f32(0.5)
            .u8(0)
            .u8(0)
            .var_int(self.seq);
        self.c.send_raw(&w.buf).await.unwrap();
        self.ticks(4).await;
    }

    async fn dig(&mut self, status: i32, p: [i32; 3]) {
        self.seq += 1;
        let mut w = Writer::new();
        w.var_int(self.sb("player_action"))
            .var_int(status)
            .i64(pack(p))
            .u8(1)
            .var_int(self.seq);
        self.c.send_raw(&w.buf).await.unwrap();
        self.ticks(2).await;
    }

    /// Шаги на месте: позиция с флагом «на земле».
    async fn stand(&mut self, at: [f64; 3]) {
        for _ in 0..6 {
            // С `client_tick_end`: две позиции за клиентский тик ваниль не примет.
            self.c.move_to(at).await.unwrap();
            self.pump(60).await;
        }
        self.ticks(2).await;
    }

    async fn ore(&mut self, title: &str) {
        self.cmd("setblock 9 201 10 minecraft:air").await;
        self.cmd("setblock 9 200 10 minecraft:redstone_ore").await;
        self.pump(300).await;
        self.log.push(format!("== {title}"));
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let blocks: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::path::Path::new(&a[3]).with_file_name("blocks.json"))
            .unwrap(),
    )
    .unwrap();
    let mut states = HashMap::new();
    for (name, b) in blocks.as_object().unwrap() {
        for s in b["states"].as_array().unwrap() {
            let mut props: Vec<String> = s["properties"]
                .as_object()
                .map(|o| {
                    o.iter()
                        .map(|(k, v)| format!("{k}={}", v.as_str().unwrap()))
                        .collect()
                })
                .unwrap_or_default();
            props.sort();
            states.insert(
                s["id"].as_i64().unwrap() as i32,
                format!("{name}[{}]", props.join(",")),
            );
        }
    }
    let ids = Ids::load(&table).unwrap();
    let mut c = Client::connect(&a[1], "framer", ids, 777, "26.3")
        .await
        .unwrap();
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
        seq: 0,
        time: 0,
        cb,
        states,
        log: Vec::new(),
        seen: BTreeMap::new(),
    };
    p.pump(2000).await;
    for s in [
        "gamerule random_tick_speed 0",
        "gamemode survival",
        "fill 2 199 2 14 199 14 minecraft:stone",
        "fill 2 200 2 14 206 14 minecraft:air",
        "tp framer 7.5 200 8.5 -90 0",
        "clear framer",
    ] {
        p.cmd(s).await;
    }
    p.pump(1500).await;
    p.log.clear();

    p.ore("click with an empty hand lights the ore").await;
    p.use_on(ORE).await;
    p.ore("click with stone: lit and stone placed").await;
    p.cmd("give framer minecraft:stone 4").await;
    p.pump(400).await;
    p.use_on(ORE).await;
    p.cmd("clear framer").await;
    p.ore("attack in survival").await;
    p.dig(0, ORE).await;
    p.dig(1, ORE).await;
    p.ore("step onto the ore").await;
    p.cmd("tp framer 9.5 201 10.5").await;
    p.pump(500).await;
    p.stand([9.5, 201.0, 10.5]).await;
    p.cmd("tp framer 7.5 200 8.5").await;
    p.stand([7.5, 200.0, 8.5]).await;
    p.log.push("== random ticks put the lit ore out".into());
    p.cmd("gamerule random_tick_speed 4096").await;
    p.ticks(20).await;
    p.cmd("gamerule random_tick_speed 0").await;

    // Будящий аметист: снизу камень, с запада вода-источник.
    p.cmd("setblock 5 202 12 minecraft:budding_amethyst").await;
    p.cmd("setblock 4 202 12 minecraft:water").await;
    p.pump(500).await;
    p.cmd("gamerule random_tick_speed 4096").await;
    // Время в stderr: замер скорости случайных тиков, в сравнение не идёт.
    let started = std::time::Instant::now();
    p.ticks(400).await;
    eprintln!(
        "400 ticks at speed 4096: {:.1} s",
        started.elapsed().as_secs_f64()
    );
    p.cmd("gamerule random_tick_speed 0").await;
    p.pump(500).await;
    p.log.push("== budding amethyst after 400 ticks".into());
    for d in [
        [0, -1, 0],
        [0, 1, 0],
        [0, 0, -1],
        [0, 0, 1],
        [-1, 0, 0],
        [1, 0, 0],
    ] {
        let at = [BUD[0] + d[0], BUD[1] + d[1], BUD[2] + d[2]];
        let s = p
            .seen
            .get(&at)
            .copied()
            .map_or("(no update)".to_owned(), |s| p.name(s));
        p.log.push(format!("{d:?} {s}"));
    }
    for l in &p.log {
        println!("{l}");
    }
}
