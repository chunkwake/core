//! Триггеры достижений: inventory_changed (с тегом), recipe_unlocked и
//! placed_block.
//!
//! `adv_triggers_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::collections::HashMap;
use std::time::Duration;

const BX: i32 = 5100;
const BZ: i32 = 5100;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_select: i32,
    sb_use_on: i32,
    cb_chat: i32,
    chat: String,
    seq: i32,
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
                self.chat.push_str(&flatten(&component));
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

    fn done(&self, id: &str, criterion: &str) -> bool {
        self.adv
            .get(id)
            .is_some_and(|v| v.iter().any(|c| c == criterion))
    }

    async fn cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(350).await;
    }

    async fn select(&mut self, slot: u16) {
        let mut w = self.packet(self.sb_select);
        w.u16(slot);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(150).await;
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
        sb_select: sb("set_carried_item"),
        sb_use_on: sb("use_item_on"),
        cb_chat,
        chat: String::new(),
        seq: 0,
        adv: HashMap::new(),
    };
    p.pump(2000).await;

    let by = 64;
    for line in [
        "difficulty peaceful".to_string(),
        "gamemode survival".to_string(),
        format!(
            "fill {} {} {} {} {} {} minecraft:air",
            BX - 6,
            by,
            BZ - 6,
            BX + 6,
            by + 4,
            BZ + 6
        ),
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
        "advancement grant @s only minecraft:recipes/root".to_string(),
        "advancement grant @s only minecraft:husbandry/root".to_string(),
        "recipe take @s minecraft:oak_planks".to_string(),
    ] {
        p.cmd(&line).await;
    }

    // 1. recipe_unlocked: выдача рецепта.
    p.cmd("recipe give @s minecraft:oak_planks").await;
    p.pump(400).await;
    assert!(
        p.done(
            "minecraft:recipes/building_blocks/oak_planks",
            "has_the_recipe"
        ),
        "recipe_unlocked не сработал: {:?}",
        p.adv.get("minecraft:recipes/building_blocks/oak_planks")
    );
    println!("recipe_unlocked ok");

    // 2. inventory_changed с тегом `#minecraft:oak_logs`.
    p.cmd("give boater minecraft:oak_log 1").await;
    p.pump(500).await;
    assert!(
        p.done("minecraft:recipes/building_blocks/oak_planks", "has_logs"),
        "inventory_changed по тегу не сработал: {:?}",
        p.adv.get("minecraft:recipes/building_blocks/oak_planks")
    );
    println!("inventory_changed ok");

    // 3. placed_block: посадка пшеницы на грядку.
    p.cmd(&format!(
        "setblock {} {} {} minecraft:farmland",
        BX + 1,
        by - 1,
        BZ
    ))
    .await;
    p.cmd("clear boater").await;
    p.cmd("give boater minecraft:wheat_seeds 1").await;
    p.select(0).await;
    p.use_on([BX + 1, by - 1, BZ], 1).await;
    p.pump(500).await;
    assert!(
        p.done("minecraft:husbandry/plant_seed", "wheat"),
        "placed_block не сработал: {:?}",
        p.adv.get("minecraft:husbandry/plant_seed")
    );
    println!("placed_block ok");

    p.cmd("kill @e[type=minecraft:item]").await;
    println!("adv_triggers: ok");
}
