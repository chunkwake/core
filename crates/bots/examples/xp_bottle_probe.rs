//! Опыт из бутылки: бросок, разбитие и опыт игроку.
//!
//! `xp_bottle_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_use_item: i32,
    sb_carried: i32,
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

    async fn cmd(&mut self, line: &str) -> String {
        self.chat.clear();
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(350).await;
        self.chat.clone()
    }

    async fn select(&mut self, slot: u16) {
        let mut w = self.packet(self.sb_carried);
        w.u16(slot);
        self.c.send_raw(&w.buf).await.unwrap();
    }

    async fn use_item(&mut self, sequence: i32) {
        let mut w = self.packet(self.sb_use_item);
        w.var_int(0).var_int(sequence).f32(0.0).f32(10.0);
        self.c.send_raw(&w.buf).await.unwrap();
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
    text.split_whitespace().rev().find_map(|t| {
        t.trim_end_matches(['f', 'b', 's'])
            .trim_end_matches('.')
            .parse()
            .ok()
    })
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n| table.id(State::Play, Direction::Clientbound, n).unwrap();
    let mut c = Client::connect(&a[1], "framer", ids, 777, "26.3")
        .await
        .unwrap();
    let _spawn = loop {
        if let Event::Position(p) = c.next_event().await.unwrap() {
            break p;
        }
    };
    let cb_chat = cb("system_chat");
    c.capture_also = vec![cb_chat];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_use_item: sb("use_item"),
        sb_carried: sb("set_carried_item"),
        cb_chat,
        chat: String::new(),
    };
    p.pump(1200).await;
    let base = [1000.5, 64.0, 1000.5];
    for line in [
        "fill 997 64 997 1003 67 1003 minecraft:air".to_string(),
        "fill 997 63 997 1003 63 1003 minecraft:stone".to_string(),
        "difficulty peaceful".into(),
        "gamemode survival".into(),
        "clear framer".into(),
        "xp set framer 0 points".into(),
        format!("tp framer {} {} {}", base[0], base[1], base[2] + 3.0),
    ] {
        p.cmd(&line).await;
    }
    let before = last_number(&p.cmd("xp query framer points").await).unwrap_or(-1.0);
    assert_eq!(before, 0.0, "опыт не сбросился");

    p.cmd("give framer minecraft:experience_bottle 3").await;
    p.select(0).await;
    p.use_item(1).await;
    p.pump(2500).await;

    let after = last_number(&p.cmd("xp query framer points").await)
        .unwrap_or_else(|| panic!("нет ответа xp query: {}", p.chat));
    assert!(
        (3.0..=11.0).contains(&after),
        "бутылка опыта дала {after} очков, ждали 3..11"
    );
    println!("xp_bottle: ok ({after} points)");
}
