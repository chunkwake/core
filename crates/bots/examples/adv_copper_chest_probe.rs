//! Склейка медных сундуков: два сундука подряд (left/right) и основание
//! медного голема, которое склеивается с соседним сундуком
//! (`CopperChestBlock.getFromCopperBlock`). Состояния проверяются через
//! `/execute if block <pos> <predicate> run data get entity @s`.
//!
//! `adv_copper_chest_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::collections::HashMap;
use std::time::Duration;

const X: i32 = 2048;
const Z: i32 = 8;

fn pack_pos([x, y, z]: [i32; 3]) -> i64 {
    ((x as i64 & 0x3ffffff) << 38) | ((z as i64 & 0x3ffffff) << 12) | (y as i64 & 0xfff)
}

fn flatten(v: &serde_json::Value) -> String {
    if let Some(s) = v.as_str() {
        return s.to_owned();
    }
    if let Some(n) = v.as_i64() {
        return n.to_string();
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

struct Probe {
    c: Client,
    sb_command: i32,
    sb_use_on: i32,
    sb_select: i32,
    cb_chat: i32,
    chat: String,
    seq: i32,
    adv: HashMap<String, Vec<String>>,
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
                self.chat.push('\n');
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
            if let Event::Advancements {
                reset, progress, ..
            } = ev
            {
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
        }
        self.drain();
    }

    async fn cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(180).await;
    }

    async fn select(&mut self, slot: u16) {
        let mut w = self.packet(self.sb_select);
        w.u16(slot);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(120).await;
    }

    async fn place_on_top(&mut self, pos: [i32; 3]) {
        self.seq += 1;
        let mut w = self.packet(self.sb_use_on);
        w.var_int(0)
            .i64(pack_pos(pos))
            .var_int(1)
            .f32(0.5)
            .f32(1.0)
            .f32(0.5)
            .u8(0)
            .u8(0)
            .var_int(self.seq);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(450).await;
    }

    /// `true`, если предикат блока с свойствами совпал.
    async fn block_is(&mut self, pos: [i32; 3], predicate: &str) -> bool {
        self.chat.clear();
        self.cmd(&format!(
            "execute if block {} {} {} {} run data get entity @s",
            pos[0], pos[1], pos[2], predicate
        ))
        .await;
        self.chat.contains("Health:")
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
        sb_select: sb("set_carried_item"),
        cb_chat,
        chat: String::new(),
        seq: 0,
        adv: HashMap::new(),
    };
    p.pump(2000).await;

    let setup: Vec<String> = vec![
        "difficulty peaceful".into(),
        "gamemode creative".into(),
        "effect clear @s".into(),
        "kill @e[type=!minecraft:player,distance=..48]".into(),
        // Игрок смотрит на север (yaw 180), сундуки будут смотреть на юг.
        format!(
            "execute in minecraft:overworld run tp @s {0}.5 100 11.5 180 20",
            X
        ),
        format!("fill {} 99 4 {} 104 12 minecraft:stone", X - 4, X + 12),
        format!("fill {} 100 4 {} 104 12 minecraft:air", X - 4, X + 12),
        "item replace entity @s hotbar.0 with minecraft:copper_chest 8".into(),
    ];
    for line in setup {
        p.cmd(&line).await;
    }
    p.select(0).await;
    p.pump(1500).await;

    // Два сундука подряд: западный становится right, восточный — left.
    p.place_on_top([X, 99, Z]).await;
    p.place_on_top([X + 1, 99, Z]).await;
    p.pump(400).await;
    assert!(
        p.block_is(
            [X, 100, Z],
            "minecraft:copper_chest[facing=south,type=right]"
        )
        .await,
        "западный сундук не стал right"
    );
    assert!(
        p.block_is(
            [X + 1, 100, Z],
            "minecraft:copper_chest[facing=south,type=left]"
        )
        .await,
        "восточный сундук не стал left"
    );
    println!("two copper chests linked ok");

    // Основание голема: тыква на медном блоке рядом с одиночным сундуком.
    p.cmd(&format!("setblock {X} 100 10 minecraft:copper_block"))
        .await;
    p.cmd(&format!(
        "setblock {} 100 10 minecraft:copper_chest[facing=south,type=single]",
        X + 1
    ))
    .await;
    p.cmd(&format!(
        "setblock {X} 101 10 minecraft:carved_pumpkin[facing=south]"
    ))
    .await;
    p.pump(700).await;
    assert!(
        p.block_is(
            [X, 100, 10],
            "minecraft:copper_chest[facing=south,type=right]"
        )
        .await,
        "основание голема не приклеилось (right)"
    );
    assert!(
        p.block_is(
            [X + 1, 100, 10],
            "minecraft:copper_chest[facing=south,type=left]"
        )
        .await,
        "сосед основания не стал left"
    );
    println!("copper golem foundation linked ok");
    println!("adv_copper_chest: ok");
}
