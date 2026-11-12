//! NoAI: моб с флагом не ходит и не атакует, но остаётся на месте.
//!
//! `noai_probe <addr> <packets.json> <registries.json>`
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

    async fn pos(&mut self) -> [f64; 3] {
        let text = self
            .data("@e[type=minecraft:zombie,limit=1,sort=nearest] Pos")
            .await;
        vec3(&text).unwrap_or_else(|| panic!("нет Pos у зомби: {text}"))
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
        .find_map(|t| t.trim_end_matches('f').trim_end_matches('b').parse().ok())
}

/// Координаты из `[1.5d, -60.0d, 3.5d]`.
fn vec3(text: &str) -> Option<[f64; 3]> {
    let start = text.find('[')?;
    let end = text[start..].find(']')? + start;
    let v: Vec<f64> = text[start + 1..end]
        .split(',')
        .filter_map(|t| {
            t.trim()
                .trim_end_matches('d')
                .trim_end_matches('f')
                .parse()
                .ok()
        })
        .collect();
    (v.len() >= 3).then(|| [v[0], v[1], v[2]])
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
    let (bx, bz) = (x + 80, z + 80);
    let by = -60;
    let base = [bx as f64 + 0.5, by as f64, bz as f64 + 0.5];
    let mob_at = [base[0], base[1], base[2] + 3.0];

    p.cmd("gamemode survival").await;
    p.cmd("difficulty normal").await;
    p.cmd("time set midnight").await;
    p.cmd("gamerule doMobSpawning false").await;
    p.cmd("clear @s").await;
    p.cmd("effect clear @s").await;
    p.cmd("kill @e[type=minecraft:zombie]").await;
    // Смерть занимает ~20 тиков: ждём, чтобы труп не попал в выборку.
    p.pump(1200).await;
    p.cmd(&format!(
        "fill {} {} {} {} {} {} minecraft:air",
        bx - 2,
        by,
        bz - 2,
        bx + 2,
        by + 3,
        bz + 6
    ))
    .await;
    p.cmd(&format!("tp @s {} {} {} 0 0", base[0], base[1], base[2]))
        .await;
    p.cmd("data merge entity @s {Health:20.0f}").await;
    p.pump(300).await;

    // 1. NoAI: зомби стоит на месте и не бьёт игрока.
    p.cmd(&format!(
        "summon minecraft:zombie {} {} {} {{NoAI:1b,PersistenceRequired:1b}}",
        mob_at[0], mob_at[1], mob_at[2]
    ))
    .await;
    p.pump(400).await;
    let before = p.pos().await;
    assert!(
        (before[0] - mob_at[0]).abs() < 1.5 && (before[2] - mob_at[2]).abs() < 1.5,
        "выбран не тот зомби: {before:?}, ожидали {mob_at:?}"
    );
    p.pump(2600).await;
    let after = p.pos().await;
    assert!(
        (after[0] - before[0]).abs() < 0.1 && (after[2] - before[2]).abs() < 0.1,
        "NoAI-моб сдвинулся: {before:?} -> {after:?}"
    );
    let h = p.health().await;
    assert!(h >= 19.5, "NoAI-моб ударил игрока: {h}");

    // 2. Обычный зомби с теми же условиями идёт к игроку или бьёт его.
    p.cmd("kill @e[type=minecraft:zombie]").await;
    // Смерть занимает ~20 тиков: ждём, чтобы труп не попал в выборку.
    p.pump(1200).await;
    p.cmd(&format!(
        "summon minecraft:zombie {} {} {} {{PersistenceRequired:1b}}",
        mob_at[0], mob_at[1], mob_at[2]
    ))
    .await;
    p.pump(400).await;
    let before = p.pos().await;
    p.pump(3200).await;
    let after = p.pos().await;
    let h = p.health().await;
    let moved = (after[0] - before[0]).abs() > 0.3 || (after[2] - before[2]).abs() > 0.3;
    assert!(
        moved || h < 19.5,
        "обычный зомби не пошёл к игроку: {before:?} -> {after:?}, HP {h}"
    );

    p.cmd("kill @e[type=minecraft:zombie]").await;
    // Смерть занимает ~20 тиков: ждём, чтобы труп не попал в выборку.
    p.pump(1200).await;
    p.cmd("clear @s").await;
    println!("noai: ok");
}
