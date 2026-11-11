//! Прыгучесть: безопасное падение игрока и моба, `safe_fall_distance`.
//!
//! `jump_boost_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_move_pos: i32,
    sb_tick_end: i32,
    cb_chat: i32,
    chat: String,
}

impl Probe {
    fn packet(&self, id: i32) -> Writer {
        let mut w = Writer::new();
        w.var_int(id);
        w
    }

    async fn pump(&mut self, ms: u64) {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
        while let Ok(Ok(_)) = tokio::time::timeout_at(deadline, self.c.next_event()).await {
            self.drain();
        }
    }

    fn drain(&mut self) {
        for (id, body) in std::mem::take(&mut self.c.captured_by) {
            if id != self.cb_chat {
                continue;
            }
            let mut r = Reader::new(&body);
            if let Ok(component) = text::read_component(&mut r) {
                self.chat.push_str(&flatten(&component));
                self.chat.push(' ');
            }
        }
    }

    async fn cmd(&mut self, line: &str) {
        self.chat.clear();
        self.send_cmd(line).await;
        self.pump(250).await;
    }

    async fn send_cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
    }

    async fn data(&mut self, query: &str) -> String {
        self.chat.clear();
        self.send_cmd(&format!("data get entity {query}")).await;
        self.pump(250).await;
        self.chat.clone()
    }

    async fn move_to(&mut self, pos: [f64; 3], on_ground: bool) {
        let mut w = self.packet(self.sb_move_pos);
        for v in pos {
            w.f64(v);
        }
        w.u8(u8::from(on_ground));
        self.c.send_raw(&w.buf).await.unwrap();
        let end = self.packet(self.sb_tick_end);
        self.c.send_raw(&end.buf).await.unwrap();
    }

    async fn health(&mut self) -> f64 {
        let text = self.data("@s Health").await;
        last_number(&text).unwrap_or_else(|| panic!("нет Health: {text}"))
    }
}

fn flatten(v: &serde_json::Value) -> String {
    if let Some(s) = v.as_str() {
        return s.to_owned();
    }
    if let Some(s) = v.get("text").and_then(serde_json::Value::as_str) {
        return s.to_owned();
    }
    let mut out = String::new();
    if let Some(args) = v.get("with").and_then(serde_json::Value::as_array) {
        for arg in args {
            out.push_str(&flatten(arg));
            out.push(' ');
        }
    }
    out
}

fn last_number(text: &str) -> Option<f64> {
    text.split_whitespace()
        .rev()
        .find_map(|word| word.trim_end_matches(['b', 'f', 'd']).parse().ok())
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let sb = |name| table.id(State::Play, Direction::Serverbound, name).unwrap();
    let cb = |name| table.id(State::Play, Direction::Clientbound, name).unwrap();
    let ids = Ids::load(&table).unwrap();
    let mut c = Client::connect(&a[1], "framer", ids, 777, "26.3")
        .await
        .unwrap();
    let spawn = loop {
        if let Event::Position(at) = c.next_event().await.unwrap() {
            break at;
        }
    };
    let cb_chat = cb("system_chat");
    c.capture_also = vec![cb_chat];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_move_pos: sb("move_player_pos"),
        sb_tick_end: sb("client_tick_end"),
        cb_chat,
        chat: String::new(),
    };
    let base = [spawn[0].floor() + 0.5, spawn[1], spawn[2].floor() + 0.5];
    let top = [base[0], base[1] + 10.0, base[2]];

    p.cmd("gamemode survival").await;
    p.cmd("effect clear @s").await;
    p.cmd(&format!("tp @s {} {} {}", base[0], base[1], base[2]))
        .await;
    p.move_to(base, true).await;
    p.cmd("data merge entity @s {Health:20.0f}").await;
    p.pump(300).await;
    let full = p.health().await;
    assert!((full - 20.0).abs() < 0.1, "здоровье не выставилось: {full}");

    // Падение 10 блоков без Прыгучести: безопасные 3 → 7 урона.
    p.cmd(&format!("tp @s {} {} {}", top[0], top[1], top[2]))
        .await;
    p.move_to(top, true).await;
    p.move_to(top, false).await;
    p.move_to(base, true).await;
    p.pump(600).await;
    let plain = 20.0 - p.health().await;
    assert!(plain > 0.0, "падение без Прыгучести не нанесло урона");

    // Прыгучесть IV: `safe_fall_distance` = 3 + 4, падение 10 → 3 урона.
    p.cmd("data merge entity @s {Health:20.0f}").await;
    p.cmd("effect give @s minecraft:jump_boost 60 3").await;
    p.pump(400).await;
    p.cmd("attribute @s minecraft:safe_fall_distance get").await;
    assert!(
        p.chat.contains("7.0"),
        "safe_fall_distance не 7: {}",
        p.chat
    );
    p.cmd(&format!("tp @s {} {} {}", top[0], top[1], top[2]))
        .await;
    p.move_to(top, true).await;
    p.move_to(top, false).await;
    p.move_to(base, true).await;
    p.pump(600).await;
    let boosted = 20.0 - p.health().await;
    // Броня ослабляет оба падения одинаково: отношение 7 / 3.
    assert!(
        boosted > 0.0 && (plain / boosted - 7.0 / 3.0).abs() < 0.3,
        "Прыгучесть не уменьшила урон: {plain} vs {boosted}"
    );

    p.cmd("effect clear @s").await;
    println!("jump_boost: ok");
}
