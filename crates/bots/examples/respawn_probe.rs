//! Якорь возрождения, кровать вне оверворлда и возрождение после смерти
//! живым ботом: один сценарий на нашем сервере и на `server.jar`.
//!
//! В журнале: смена блока якоря и кровати, звуки якоря и взрыва, сообщения
//! в чат, `game_event` 0 (точка пропала), пакет `respawn` (измерение) и
//! точка с углом после возрождения.
//!
//! `respawn_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer};
use std::collections::HashMap;
use std::time::Duration;

const TARGET: [i32; 3] = [9, 200, 10];
const HEAD: [i32; 3] = [9, 200, 11];

fn pack(p: [i32; 3]) -> i64 {
    ((p[0] as i64 & 0x3FF_FFFF) << 38) | ((p[2] as i64 & 0x3FF_FFFF) << 12) | (p[1] as i64 & 0xFFF)
}

fn unpack(v: i64) -> [i32; 3] {
    [(v >> 38) as i32, (v << 52 >> 52) as i32, (v << 26 >> 38) as i32]
}

/// Читаемые куски NBT-текста: ключи перевода и строки.
fn ascii(b: &[u8]) -> String {
    let mut out = Vec::new();
    let mut cur = String::new();
    for &c in b {
        if (0x20..0x7f).contains(&c) {
            cur.push(c as char);
        } else {
            if cur.len() >= 4 {
                out.push(std::mem::take(&mut cur));
            }
            cur.clear();
        }
    }
    if cur.len() >= 4 {
        out.push(cur);
    }
    out.join("|")
}

struct Probe {
    c: Client,
    table: PacketTable,
    seq: i32,
    time: i64,
    cb: HashMap<i32, &'static str>,
    states: HashMap<i32, String>,
    sounds: HashMap<i32, String>,
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
            TARGET => "target",
            HEAD => "head",
            _ => return,
        };
        let name = self.states.get(&state).cloned().unwrap_or_else(|| state.to_string());
        let line = format!("{what} {name}");
        if self.log.last() != Some(&line) {
            self.log.push(line);
        }
    }

    fn read(&mut self, kind: &str, b: &[u8]) {
        let mut r = Reader::new(b);
        match kind {
            "sound" => {
                let id = r.var_int().unwrap();
                let name = self.sounds.get(&(id - 1)).cloned().unwrap_or_else(|| (id - 1).to_string());
                if !(name.contains("anchor") || name.contains("explode") || name.contains("bed")) {
                    return;
                }
                let source = r.var_int().unwrap();
                let mut int = || i32::from_be_bytes(r.bytes(4).unwrap().try_into().unwrap());
                let at = [int(), int(), int()].map(|v| v as f64 / 8.0);
                let (vol, pitch) = (r.f32().unwrap(), r.f32().unwrap());
                let pitch = if name.contains("explode") { "*".to_owned() } else { format!("{pitch:.3}") };
                self.log.push(format!(
                    "sound {name} src {source} at {:.2} {:.2} {:.2} vol {vol} pitch {pitch}",
                    at[0], at[1], at[2]
                ));
            }
            "system_chat" => {
                let text = ascii(b);
                if !text.contains("joined") && !text.contains("left the game") {
                    self.log.push(format!("chat {text}"));
                }
            }
            "game_event" => {
                let ev = r.u8().unwrap();
                if ev == 0 || ev == 4 {
                    self.log.push(format!("game_event {ev}"));
                }
            }
            "respawn" => {
                let _ty = r.var_int().unwrap();
                let dim = r.string(256).unwrap();
                self.log.push(format!("respawn {dim}"));
            }
            "player_position" => {
                let _id = r.var_int().unwrap();
                let p = [r.f64().unwrap(), r.f64().unwrap(), r.f64().unwrap()];
                for _ in 0..3 {
                    r.f64().unwrap();
                }
                let (yaw, pitch) = (r.f32().unwrap(), r.f32().unwrap());
                self.log.push(format!(
                    "position {:.2} {:.2} {:.2} yaw {yaw:.1} pitch {pitch:.1}",
                    p[0], p[1], p[2]
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
        self.pump(600).await;
    }

    async fn cmd(&mut self, s: &str) {
        let mut w = Writer::new();
        w.var_int(self.sb("chat_command")).string(s);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(250).await;
    }

    /// Команда без следа в журнале (ответы команд — шум).
    async fn quiet(&mut self, s: &str) {
        let n = self.log.len();
        self.cmd(s).await;
        self.pump(200).await;
        self.log.truncate(n);
    }

    async fn use_on(&mut self, p: [i32; 3]) {
        self.seq += 1;
        let mut w = Writer::new();
        w.var_int(self.sb("use_item_on")).var_int(0).i64(pack(p)).var_int(1).f32(0.5).f32(1.0).f32(0.5).u8(0).u8(0).var_int(self.seq);
        self.c.send_raw(&w.buf).await.unwrap();
        self.ticks(4).await;
    }

    async fn die_and_respawn(&mut self) {
        self.quiet("kill framer").await;
        self.ticks(4).await;
        let mut w = Writer::new();
        w.var_int(self.sb("client_command")).var_int(0);
        self.c.send_raw(&w.buf).await.unwrap();
        self.ticks(4).await;
        // Без `player_loaded` ваниль держит возрождённого неуязвимым.
        let mut w = Writer::new();
        w.var_int(self.sb("player_loaded"));
        self.c.send_raw(&w.buf).await.unwrap();
        self.ticks(6).await;
    }

    /// Площадка и бот в измерении `dim`.
    async fn go(&mut self, dim: &str) {
        let n = self.log.len();
        self.cmd(&format!("execute in {dim} run tp framer 7.5 200 8.5 -90 0")).await;
        self.pump(2500).await;
        self.cmd(&format!("execute in {dim} run fill 4 199 4 14 199 14 minecraft:stone")).await;
        self.cmd(&format!("execute in {dim} run fill 4 200 4 14 203 14 minecraft:air")).await;
        self.cmd(&format!("execute in {dim} run tp framer 7.5 200 8.5 -90 0")).await;
        self.pump(1500).await;
        self.log.truncate(n);
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let reg: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&a[3]).unwrap()).unwrap();
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
        "sound",
        "system_chat",
        "game_event",
        "respawn",
        "player_position",
        "block_update",
        "section_blocks_update",
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
        states,
        sounds,
        log: Vec::new(),
    };
    p.pump(2000).await;
    p.quiet("gamemode creative").await;
    p.quiet("gamerule spawn_radius 0").await;
    p.quiet("setworldspawn 2 200 2").await;
    p.go("minecraft:overworld").await;
    p.quiet("clear framer").await;

    p.log.push("== overworld: charged anchor explodes".into());
    p.quiet("setblock 9 200 10 minecraft:respawn_anchor[charges=1]").await;
    p.use_on(TARGET).await;

    p.log.push("== overworld: empty anchor does nothing".into());
    p.quiet("setblock 9 200 10 minecraft:respawn_anchor").await;
    p.use_on(TARGET).await;

    p.log.push("== spawnpoint with angle, then death".into());
    p.cmd("spawnpoint framer 3 200 5 45 10").await;
    p.die_and_respawn().await;

    p.log.push("== nether: bed explodes".into());
    p.go("minecraft:the_nether").await;
    p.quiet("setblock 9 200 10 minecraft:red_bed[part=foot,facing=south]").await;
    p.quiet("setblock 9 200 11 minecraft:red_bed[part=head,facing=south]").await;
    p.use_on(TARGET).await;

    p.log.push("== nether: anchor sets spawn".into());
    p.quiet("fill 4 200 4 14 203 14 minecraft:air").await;
    p.quiet("setblock 9 199 11 minecraft:stone").await;
    p.quiet("setblock 9 200 10 minecraft:respawn_anchor[charges=2]").await;
    p.use_on(TARGET).await;
    p.log.push("== nether: same anchor again".into());
    p.use_on(TARGET).await;

    p.log.push("== nether: death respawns at the anchor".into());
    p.die_and_respawn().await;

    p.log.push("== nether: empty anchor, respawn falls back".into());
    p.quiet("execute in minecraft:the_nether run setblock 9 200 10 minecraft:respawn_anchor").await;
    p.die_and_respawn().await;

    p.log.push("== again: point is gone".into());
    p.die_and_respawn().await;
    for l in &p.log {
        println!("{l}");
    }
}
