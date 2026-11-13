//! `summoned_entity` для медного голема: тыква на блоке из `#minecraft:copper`
//! (`CarvedPumpkinBlock`), блок заменяется медным сундуком. В ванили
//! достижения на медного голема нет, поэтому проба использует временное
//! `custom/copper_probe` (`copper_probe.advancement.json`).
//!
//! `adv_copper_golem_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::collections::HashMap;
use std::time::Duration;

const ADV: &str = "minecraft:custom/copper_probe";
const CRITERION: &str = "summoned";
const X: i32 = 1560;
const Z: i32 = 8;

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
    cb_add_entity: i32,
    cb_chat: i32,
    chat: String,
    golem_type: i32,
    golem: Option<i32>,
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
            if id == self.cb_add_entity {
                let mut r = Reader::new(&body);
                let Ok(entity) = r.var_int() else { continue };
                if r.bytes(16).is_err() {
                    continue;
                }
                let Ok(ty) = r.var_int() else { continue };
                if ty == self.golem_type {
                    self.golem = Some(entity);
                }
            } else if id == self.cb_chat {
                let mut r = Reader::new(&body);
                if let Ok(component) = text::read_component(&mut r) {
                    self.chat.push_str(&flatten(&component));
                    self.chat.push('\n');
                }
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

    fn done(&self) -> bool {
        self.adv
            .get(ADV)
            .is_some_and(|v| v.iter().any(|c| c == CRITERION))
    }

    async fn cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(200).await;
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let reg: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::path::Path::new(&a[2]).with_file_name("registries.json"))
            .unwrap(),
    )
    .unwrap();
    let golem_type =
        reg["minecraft:entity_type"]["entries"]["minecraft:copper_golem"]["protocol_id"]
            .as_i64()
            .unwrap() as i32;
    let ids = Ids::load(&table).unwrap();
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n| table.id(State::Play, Direction::Clientbound, n).unwrap();
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    let cb_add_entity = cb("add_entity");
    let cb_chat = cb("system_chat");
    c.capture_also = vec![cb_add_entity, cb_chat];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        cb_add_entity,
        cb_chat,
        chat: String::new(),
        golem_type,
        golem: None,
        adv: HashMap::new(),
    };
    p.pump(2000).await;

    let setup: Vec<String> = vec![
        "difficulty peaceful".into(),
        "gamemode creative".into(),
        "effect clear @s".into(),
        format!("fill {} 99 4 {} 99 12 minecraft:stone", X - 4, X + 4),
        format!("fill {} 100 4 {} 103 12 minecraft:air", X - 4, X + 4),
        format!("tp @s {} 100 12.5", X as f64 + 0.5),
        "kill @e[type=!minecraft:player,distance=..48]".into(),
        format!("advancement revoke @s only {ADV}"),
        // Не медь: голем не появляется.
        format!("setblock {X} 100 {Z} minecraft:stone"),
        format!("setblock {X} 101 {Z} minecraft:carved_pumpkin"),
    ];
    for line in setup {
        p.cmd(&line).await;
    }
    p.pump(500).await;

    assert!(
        p.golem.is_none() && !p.done(),
        "медный голем появился на камне: {:?}",
        p.adv.get(ADV)
    );
    println!("non-copper skipped ok");

    // Медный блок и тыква заново.
    p.cmd(&format!("setblock {X} 100 {Z} minecraft:copper_block"))
        .await;
    p.cmd(&format!("setblock {X} 101 {Z} minecraft:air")).await;
    p.cmd(&format!("setblock {X} 101 {Z} minecraft:carved_pumpkin"))
        .await;
    p.pump(700).await;

    let golem = p.golem.expect("add_entity медного голема не пришёл");
    assert!(
        p.done(),
        "summoned_entity для медного голема не сработал: {:?}",
        p.adv.get(ADV)
    );
    println!("copper golem entity {golem}");

    // Основание заменено медным сундуком.
    p.chat.clear();
    p.cmd(&format!(
        "execute if block {X} 100 {Z} minecraft:copper_chest run data get entity @s"
    ))
    .await;
    let chat = p.chat.clone();
    // `execute if block` при совпадении выполняет `data get entity @s`,
    // при промахе молчит (`Outcome::Silent`).
    assert!(
        chat.contains("Health:"),
        "медный блок не заменён сундуком: {chat}"
    );
    p.chat.clear();
    p.cmd(&format!(
        "execute if block {X} 100 {Z} minecraft:copper_block run data get entity @s"
    ))
    .await;
    assert!(
        !p.chat.contains("Health:"),
        "на месте сундука всё ещё блок меди: {}",
        p.chat
    );
    println!("copper chest replaced ok");
    println!("adv_copper_golem: ok");
}
