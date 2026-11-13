//! Триггер достижений player_generates_container_loot.
//!
//! `adv_loot_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::text;
use chunkwake_protocol::{Reader, Writer};
use std::collections::HashMap;
use std::time::Duration;

const BX: i32 = 6200;
const BZ: i32 = 6200;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_use_on: i32,
    sb_close: i32,
    cb_chat: i32,
    seq: i32,
    chat: String,
    window: Option<i32>,
    adv: HashMap<String, Vec<String>>,
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

fn pack_pos([x, y, z]: [i32; 3]) -> i64 {
    ((x as i64 & 0x3ffffff) << 38) | ((z as i64 & 0x3ffffff) << 12) | (y as i64 & 0xfff)
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
                let line = flatten(&component);
                self.chat.push_str(line.trim());
                self.chat.push(' ');
            }
        }
    }

    async fn pump(&mut self, ms: u64) {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
        loop {
            let ev = match tokio::time::timeout_at(deadline, self.c.next_event()).await {
                Ok(Ok(ev)) => ev,
                Ok(Err(e)) => {
                    eprintln!("event error: {e}");
                    break;
                }
                Err(_) => break,
            };
            match ev {
                Event::Advancements {
                    reset, progress, ..
                } => {
                    if reset {
                        self.adv.clear();
                    }
                    for (id, criteria) in progress {
                        if criteria.is_empty() {
                            self.adv.remove(&id);
                        } else {
                            self.adv.entry(id).or_default().extend(criteria);
                        }
                    }
                }
                Event::OpenScreen { window, .. } => self.window = Some(window),
                _ => {}
            }
        }
        self.drain();
    }

    fn done(&self, id: &str, criterion: &str) -> bool {
        self.adv
            .get(id)
            .is_some_and(|v| v.iter().any(|c| c == criterion))
    }

    async fn cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(300).await;
    }

    async fn use_on(&mut self, pos: [i32; 3], face: i32) {
        self.seq += 1;
        let mut w = self.packet(self.sb_use_on);
        w.var_int(0)
            .i64(pack_pos(pos))
            .var_int(face)
            .f32(0.5)
            .f32(0.5)
            .f32(0.5)
            .u8(0)
            .u8(0)
            .var_int(self.seq);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(600).await;
    }

    async fn close(&mut self, window: i32) {
        let mut w = self.packet(self.sb_close);
        w.var_int(window);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(250).await;
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n| table.id(State::Play, Direction::Clientbound, n).unwrap();
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    let cb_chat = cb("system_chat");
    c.capture_also = vec![cb_chat];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_use_on: sb("use_item_on"),
        sb_close: sb("container_close"),
        cb_chat,
        seq: 0,
        chat: String::new(),
        window: None,
        adv: HashMap::new(),
    };
    p.pump(2000).await;

    let by = 64;
    let chest = [BX + 1, by, BZ];
    for line in [
        "difficulty peaceful".to_string(),
        "gamemode survival".to_string(),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            BX - 6,
            by - 1,
            BZ - 6,
            BX + 6,
            by - 1,
            BZ + 6
        ),
        format!(
            "tp boater {} {} {} 0 0",
            BX as f64 + 0.5,
            by,
            BZ as f64 + 0.5
        ),
        "clear boater".to_string(),
        "effect clear @s".to_string(),
        format!(
            "setblock {} {} {} minecraft:chest",
            chest[0], chest[1], chest[2]
        ),
        format!(
            "data merge block {} {} {} {{LootTable:\"minecraft:archaeology/desert_pyramid\"}}",
            chest[0], chest[1], chest[2]
        ),
        "advancement revoke @s only minecraft:adventure/salvage_sherd".to_string(),
        "advancement grant @s only minecraft:adventure/root".to_string(),
    ] {
        p.cmd(&line).await;
    }

    p.window = None;
    p.use_on(chest, 1).await;
    let win = p.window.expect("сундук не открылся");
    p.pump(600).await;

    assert!(
        p.done("minecraft:adventure/salvage_sherd", "desert_pyramid"),
        "player_generates_container_loot не сработал: {:?}",
        p.adv.get("minecraft:adventure/salvage_sherd")
    );
    println!("player_generates_container_loot ok");

    // Добыча должна быть в сундуке, а LootTable — снят.
    p.chat.clear();
    p.cmd(&format!(
        "data get block {} {} {}",
        chest[0], chest[1], chest[2]
    ))
    .await;
    let data = p.chat.clone();
    assert!(data.contains("Items"), "в сундуке нет предметов: {data}");
    assert!(!data.contains("LootTable"), "LootTable не снят: {data}");
    println!("container loot filled ok");

    p.close(win).await;
    println!("adv_loot: ok");
}
