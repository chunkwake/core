//! Триггер достижений crafter_recipe_crafted: крафтер выбрасывает результат.
//!
//! `adv_crafter_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use chunkwake_protocol::{Reader, text};
use std::collections::HashMap;
use std::time::Duration;

const BX: i32 = 6300;
const BZ: i32 = 6300;

struct Probe {
    c: Client,
    sb_command: i32,
    cb_chat: i32,
    chat: String,
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
        self.pump(120).await;
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
        cb_chat,
        chat: String::new(),
        adv: HashMap::new(),
    };
    p.pump(2000).await;

    let by = 64;
    let crafter = [BX + 2, by, BZ];
    for line in [
        "difficulty peaceful".to_string(),
        "gamemode survival".to_string(),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            BX - 4,
            by - 1,
            BZ - 4,
            BX + 6,
            by - 1,
            BZ + 4
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:air",
            BX - 4,
            by,
            BZ - 4,
            BX + 6,
            by + 3,
            BZ + 4
        ),
        format!(
            "tp boater {} {} {} 0 0",
            BX as f64 + 0.5,
            by,
            BZ as f64 + 0.5
        ),
        "clear boater".to_string(),
        "effect clear @s".to_string(),
        "kill @e[type=minecraft:item]".to_string(),
        format!(
            "setblock {} {} {} minecraft:crafter[orientation=north_up,crafting=false,triggered=false]",
            crafter[0], crafter[1], crafter[2]
        ),
        "advancement revoke @s only minecraft:adventure/crafters_crafting_crafters".to_string(),
        "advancement grant @s only minecraft:adventure/root".to_string(),
    ] {
        p.cmd(&line).await;
    }
    p.pump(400).await;

    // Рецепт крафтера (###/#C#/RDR): 5 железа, верстак, раздатчик, 2 редстоуна.
    let grid = [
        "minecraft:iron_ingot",
        "minecraft:iron_ingot",
        "minecraft:iron_ingot",
        "minecraft:iron_ingot",
        "minecraft:crafting_table",
        "minecraft:iron_ingot",
        "minecraft:redstone",
        "minecraft:dropper",
        "minecraft:redstone",
    ];
    for (slot, item) in grid.iter().enumerate() {
        p.cmd(&format!(
            "item replace block {} {} {} container.{slot} with {item} 1",
            crafter[0], crafter[1], crafter[2]
        ))
        .await;
    }
    p.pump(300).await;

    // Импульс: редстоун-блок сверху, затем убираем.
    p.cmd(&format!(
        "setblock {} {} {} minecraft:redstone_block",
        crafter[0],
        crafter[1] + 1,
        crafter[2]
    ))
    .await;
    p.pump(1200).await;
    p.cmd(&format!(
        "setblock {} {} {} minecraft:air",
        crafter[0],
        crafter[1] + 1,
        crafter[2]
    ))
    .await;
    p.pump(600).await;

    assert!(
        p.done(
            "minecraft:adventure/crafters_crafting_crafters",
            "crafter_crafted_crafter"
        ),
        "crafter_recipe_crafted не сработал: {:?}",
        p.adv.get("minecraft:adventure/crafters_crafting_crafters")
    );
    println!("crafter_recipe_crafted ok");

    // Результат должен быть выброшен перед крафтером.
    p.chat.clear();
    p.cmd("data get entity @e[type=minecraft:item,limit=1]")
        .await;
    let items = p.chat.clone();
    assert!(
        items.contains("minecraft:crafter"),
        "крафтер не выбросил результат: {items}"
    );
    println!("crafter dropped ok");

    p.cmd("kill @e[type=minecraft:item]").await;
    println!("adv_crafter: ok");
}
