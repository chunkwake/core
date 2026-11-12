//! Поглощение: жёлтые сердца принимают урон раньше здоровья.
//!
//! `absorption_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

struct Probe {
    c: Client,
    sb_command: i32,
    cb_chat: i32,
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

    async fn pump(&mut self, ms: u64) {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
        while let Ok(Ok(_)) = tokio::time::timeout_at(deadline, self.c.next_event()).await {}
        self.drain();
    }

    async fn cmd(&mut self, line: &str) {
        self.send_cmd(line).await;
        self.pump(350).await;
    }

    async fn send_cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
    }

    async fn data(&mut self, query: &str) -> String {
        self.pump(150).await;
        self.chat.clear();
        self.cmd(&format!("data get entity {query}")).await;
        self.chat.clone()
    }

    async fn health(&mut self) -> f64 {
        let text = self.data("@s Health").await;
        last_number(&text).unwrap_or_else(|| panic!("нет Health: {text}"))
    }

    async fn damage_at(&mut self, amount: f32, cause: &str, at: [f64; 3]) -> String {
        self.chat.clear();
        self.send_cmd(&format!(
            "damage @s {amount} minecraft:{cause} at {} {} {}",
            at[0], at[1], at[2]
        ))
        .await;
        self.pump(400).await;
        self.chat.clone()
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
        .find_map(|t| t.trim_end_matches('f').trim_end_matches('b').parse().ok())
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let sb = |name| table.id(State::Play, Direction::Serverbound, name).unwrap();
    let cb = |name| table.id(State::Play, Direction::Clientbound, name).unwrap();
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
        cb_chat,
        chat: String::new(),
    };
    let (x, z) = (spawn[0].floor() as i32, spawn[2].floor() as i32);
    let (bx, bz) = (x + 60, z + 60);
    let by = -60;
    let base = [bx as f64 + 0.5, by as f64, bz as f64 + 0.5];

    p.cmd("gamemode survival").await;
    p.cmd("difficulty easy").await;
    p.cmd("clear @s").await;
    p.cmd("effect clear @s").await;
    p.cmd(&format!(
        "fill {} {} {} {} {} {} minecraft:air",
        bx - 1,
        by,
        bz - 1,
        bx + 1,
        by + 3,
        bz + 1
    ))
    .await;
    p.cmd(&format!("tp @s {} {} {} 0 0", base[0], base[1], base[2]))
        .await;
    p.cmd("data merge entity @s {Health:20.0f}").await;
    p.pump(300).await;

    // Поглощение II: (1 + 1) * 4 = 8 жёлтых сердец.
    p.cmd("effect give @s minecraft:absorption 120 1").await;
    p.pump(400).await;

    // 5 урона уходят в поглощение, здоровье не меняется.
    let reply = p.damage_at(5.0, "generic", base).await;
    assert!(!reply.contains("error"), "damage не выполнился: {reply}");
    let h = p.health().await;
    assert!(h >= 19.5, "поглощение не приняло урон: {h}");

    // Ещё 5: 3 добирает остаток поглощения, 2 идут в здоровье.
    tokio::time::sleep(Duration::from_millis(1200)).await;
    p.damage_at(5.0, "generic", base).await;
    let h = p.health().await;
    // Регенерация от полной сытости может добавить до ~1 HP.
    assert!(
        (17.0..=18.5).contains(&h),
        "после поглощения должно остаться 18 HP: {h}"
    );

    p.cmd("effect clear @s").await;
    p.cmd("clear @s").await;
    println!("absorption: ok");
}
