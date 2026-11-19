//! Нагрузочная проба: спавнит N коров на площадке и читает `/mspt` и `/tps`.
//!
//! `perf_probe <addr> <packets.json> <registries.json> [count]`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer};
use std::time::Duration;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_chat: i32,
    cb_chat: i32,
    cb_player_chat: i32,
    chat: String,
}

impl Probe {
    fn packet(&self, id: i32) -> Writer {
        let mut w = Writer::new();
        w.var_int(id);
        w
    }

    fn drain(&mut self) {
        for (id, body) in std::mem::take(&mut self.c.captured_by) {
            if id == self.cb_chat {
                let mut r = Reader::new(&body);
                if let Ok(component) = chunkwake_protocol::text::read_component(&mut r) {
                    let mut out = String::new();
                    flatten(&component, &mut out);
                    self.chat.push_str(&out);
                    self.chat.push('\n');
                }
            } else if id == self.cb_player_chat
                && let Some(text) = read_player_chat_unsigned(&body)
            {
                println!("player_chat: {text}");
                self.chat.push_str(&text);
                self.chat.push('\n');
            }
        }
    }

    async fn pump(&mut self, ms: u64) {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
        loop {
            let ev = match tokio::time::timeout_at(deadline, self.c.next_event()).await {
                Ok(Ok(ev)) => ev,
                Ok(Err(_)) => break,
                Err(_) => break,
            };
            if let Event::Position(_) = ev {
                continue;
            }
        }
        self.drain();
    }

    async fn say(&mut self, text: &str, ms: u64) {
        let mut w = self.packet(self.sb_chat);
        w.string(text);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(ms).await;
    }

    async fn cmd(&mut self, line: &str, ms: u64) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(ms).await;
    }
}

fn flatten(v: &serde_json::Value, out: &mut String) {
    if let Some(s) = v.as_str() {
        out.push_str(s);
        return;
    }
    if let Some(s) = v.get("text").and_then(serde_json::Value::as_str) {
        out.push_str(s);
    }
    if let Some(key) = v.get("translate").and_then(serde_json::Value::as_str) {
        out.push_str(key);
    }
    for key in ["with", "extra"] {
        if let Some(args) = v.get(key).and_then(serde_json::Value::as_array) {
            for arg in args {
                flatten(arg, out);
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let count: i32 = a.get(4).and_then(|v| v.parse().ok()).unwrap_or(1000);
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n| table.id(State::Play, Direction::Clientbound, n).unwrap();
    let cb_chat = cb("system_chat");
    let cb_player_chat = cb("player_chat");
    let name = std::env::var("PERF_NAME").unwrap_or_else(|_| "boater".into());
    let mut c = Client::connect(&a[1], &name, ids, 777, "26.3")
        .await
        .unwrap();
    c.capture_also = vec![cb_chat, cb_player_chat];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_chat: sb("chat"),
        cb_chat,
        cb_player_chat,
        chat: String::new(),
    };
    // Точка спавна игрока: спавним мобов рядом с ней (иначе чанки
    // далеко от игрока выгружаются и сущности удаляются).
    let mut base = [0.0f64; 3];
    {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            match tokio::time::timeout_at(deadline, p.c.next_event()).await {
                Ok(Ok(Event::Position(pos))) => {
                    base = pos;
                    break;
                }
                Ok(Ok(_)) => continue,
                _ => break,
            }
        }
    }
    p.pump(500).await;

    p.say("hello-from-bot", 500).await;
    for line in [
        "gamerule doMobSpawning false",
        "gamerule doFireTick false",
        "gamerule randomTickSpeed 0",
        "kill @e[type=minecraft:cow]",
        "difficulty peaceful",
        "gamerule sendCommandFeedback false",
    ] {
        p.cmd(line, 300).await;
    }
    // Площадка 48x32 на y=100 (под ней камень).
    let y: i32 = std::env::var("PERF_Y")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(-59);
    if std::env::var("PERF_FLOOR").as_deref() == Ok("1") {
        let bx = base[0].floor() as i32;
        let bz = base[2].floor() as i32;
        p.cmd(
            &format!(
                "fill {} {} {} {} {} {} minecraft:stone",
                bx - 24,
                y - 1,
                bz - 16,
                bx + 23,
                y - 1,
                bz + 15
            ),
            300,
        )
        .await;
        p.cmd(
            &format!(
                "fill {} {} {} {} {} {} minecraft:air",
                bx - 24,
                y,
                bz - 16,
                bx + 23,
                y + 3,
                bz + 15
            ),
            300,
        )
        .await;
    }

    let cols = 40;
    let mut spawned = 0;
    if let Ok(function) = std::env::var("PERF_FN") {
        // Датапак Paper: одна команда спавнит всех (обходит антиспам чата).
        p.cmd(&format!("function {function}"), 500).await;
        spawned = count;
    } else {
        for i in 0..count {
            let x = -20 + i % cols;
            let z = -12 + i / cols;
            p.cmd(
                &format!(
                    "summon minecraft:cow {} {} {}",
                    base[0].floor() as i32 + x,
                    base[1].floor() as i32,
                    base[2].floor() as i32 + z
                ),
                10,
            )
            .await;
            spawned += 1;
        }
    }
    p.cmd(&format!("tp @s {} {} {}", base[0], base[1], base[2]), 500)
        .await;
    let wait: u64 = std::env::var("PERF_WAIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);
    p.pump(wait * 1000).await;
    // Вернуть вывод команд (выключали, чтобы не спамить чат при спавне).
    p.cmd("gamerule sendCommandFeedback true", 300).await;
    p.cmd("say PERF_DONE_SPAWN", 300).await;
    p.chat.clear();
    p.cmd("data get entity @e[type=minecraft:cow,limit=1] Pos", 500)
        .await;
    let cow = p.chat.clone();
    p.chat.clear();
    p.cmd("mspt", 800).await;
    let mspt = p.chat.clone();
    p.chat.clear();
    p.cmd("tps", 800).await;
    let tps = p.chat.clone();
    p.chat.clear();
    p.cmd("tick query", 800).await;
    let tick = p.chat.clone();
    p.chat.clear();
    p.cmd(
        "execute if entity @e[type=minecraft:cow] run say COWS_ALIVE",
        800,
    )
    .await;
    let alive = p.chat.contains("COWS_ALIVE");

    println!("spawned={spawned} alive={alive}");
    println!("--- chat history ---\n{}", p.chat.trim());
    println!("--- cow probe ---\n{}", cow.trim());
    println!("--- mspt ---\n{}", mspt.trim());
    println!("--- tps ---\n{}", tps.trim());
    println!("--- tick query ---\n{}", tick.trim());
}

/// Разбор `player_chat` до unsigned-компонента (наш сетевой формат).
fn read_player_chat_unsigned(body: &[u8]) -> Option<String> {
    let mut r = Reader::new(body);
    let _ = r.uuid().ok()?;
    let _ = r.var_int().ok()?;
    if r.bool().ok()? {
        let _ = r.bytes(256).ok()?;
    }
    let _ = r.string(256).ok()?;
    let _ = r.i64().ok()?;
    let _ = r.i64().ok()?;
    let seen = r.var_int().ok()?;
    for _ in 0..seen {
        let _ = r.uuid().ok()?;
        let _ = r.bytes(256).ok()?;
    }
    if r.bool().ok()? {
        let component = chunkwake_protocol::text::read_component(&mut r).ok()?;
        let mut out = String::new();
        flatten(&component, &mut out);
        return Some(out);
    }
    None
}
