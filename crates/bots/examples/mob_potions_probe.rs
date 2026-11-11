//! Зелья на мобах: instant-урон, яд и `active_effects`.
//!
//! `mob_potions_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_use_item: i32,
    sb_set_carried: i32,
    cb_chat: i32,
    chat: String,
    seq: i32,
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
        self.pump(300).await;
    }

    async fn send_cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
    }

    fn reset_chat(&mut self) {
        self.drain();
        self.chat.clear();
    }

    async fn data(&mut self, query: &str) -> String {
        self.reset_chat();
        self.pump(120).await;
        self.cmd(&format!("data get entity {query}")).await;
        self.chat.clone()
    }

    async fn health(&mut self) -> f64 {
        let text = self
            .data("@e[type=minecraft:cow,limit=1,sort=nearest] Health")
            .await;
        last_number(&text).expect("нет Health в ответе")
    }

    async fn spawn_cow(&mut self, at: [f64; 3]) {
        self.send_cmd(&format!(
            "summon minecraft:cow {} {} {}",
            at[0], at[1], at[2]
        ))
        .await;
        let deadline = tokio::time::Instant::now() + Duration::from_millis(800);
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout_at(deadline, self.c.next_event()).await {
                Ok(Ok(Event::EntitySpawned { kind: 30, .. })) => break,
                Ok(Ok(_)) => {}
                _ => break,
            }
        }
    }

    async fn clear_cows(&mut self) {
        self.cmd("kill @e[type=minecraft:cow]").await;
        self.pump(1300).await;
    }

    async fn throw(&mut self, yaw: f32, pitch: f32) {
        self.seq += 1;
        let mut w = self.packet(self.sb_use_item);
        w.var_int(0).var_int(self.seq).f32(yaw).f32(pitch);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(1200).await;
    }

    async fn hold_first(&mut self) {
        let mut s = self.packet(self.sb_set_carried);
        s.u16(0);
        self.c.send_raw(&s.buf).await.unwrap();
        self.pump(100).await;
    }
}

/// Плоский текст компонента: `text`, строки и `with`.
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
        sb_use_item: sb("use_item"),
        sb_set_carried: sb("set_carried_item"),
        cb_chat,
        chat: String::new(),
        seq: 0,
    };
    let (x, z) = (spawn[0].floor() as i32, spawn[2].floor() as i32);
    let (bx, bz) = (x + 40, z + 40);
    let by = -60;
    let cow = [bx as f64 + 2.5, by as f64, bz as f64 + 0.5];

    p.cmd("gamemode creative").await;
    p.cmd("difficulty easy").await;
    p.cmd("time set day").await;
    p.cmd("clear @s").await;
    p.cmd(&format!(
        "fill {} {} {} {} {} {} minecraft:air",
        bx - 1,
        by,
        bz - 1,
        bx + 6,
        by + 2,
        bz + 1
    ))
    .await;
    p.cmd(&format!(
        "tp @s {} {} {}",
        bx as f64 + 0.5,
        by as f64,
        bz as f64 + 0.5
    ))
    .await;
    // `/tp` ставит on_ground=false; клиент в игре присылает обычное движение.
    let mut w = Writer::new();
    w.var_int(sb("move_player_pos"));
    for v in [bx as f64 + 0.5, by as f64, bz as f64 + 0.5] {
        w.f64(v);
    }
    w.u8(1);
    p.c.send_raw(&w.buf).await.unwrap();
    p.clear_cows().await;

    // Instant damage: зелье вреда снимает часть здоровья в зависимости от радиуса.
    p.spawn_cow(cow).await;
    p.pump(200).await;
    p.cmd(
        "item replace entity @s hotbar.0 with minecraft:splash_potion[minecraft:potion_contents={potion:\"minecraft:harming\"}] 1",
    )
    .await;
    p.pump(200).await;
    p.hold_first().await;
    p.throw(-90.0, 75.0).await;
    let h1 = p.health().await;
    assert!(h1 < 9.5, "зелье вреда должно ранить корову, осталось {h1}");

    // Яд: эффект появляется в active_effects и тикает уроном.
    p.clear_cows().await;
    p.spawn_cow(cow).await;
    p.pump(200).await;
    p.cmd(
        "item replace entity @s hotbar.0 with minecraft:splash_potion[minecraft:potion_contents={potion:\"minecraft:poison\"}] 1",
    )
    .await;
    p.pump(200).await;
    p.hold_first().await;
    p.throw(-90.0, 75.0).await;
    let effects = p
        .data("@e[type=minecraft:cow,limit=1,sort=nearest] active_effects")
        .await;
    assert!(
        effects.contains("minecraft:poison"),
        "яд не попал в active_effects: {effects}"
    );
    let h2 = p.health().await;
    p.pump(1500).await;
    let h3 = p.health().await;
    assert!(h3 < h2, "яд должен тикать уроном: было {h2}, стало {h3}");

    println!("mob_potions: ok");
}
