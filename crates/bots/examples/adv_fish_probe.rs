//! Триггер `filled_bucket`: ведро воды + треска → `husbandry/tactical_fishing`.
//!
//! `adv_fish_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::collections::HashMap;
use std::time::Duration;

const ADV: &str = "minecraft:husbandry/tactical_fishing";
const COD: [f64; 3] = [80.5, 100.0, 8.5];

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
    sb_interact: i32,
    sb_select: i32,
    cb_add_entity: i32,
    cb_chat: i32,
    cod_type: i32,
    cod: Option<i32>,
    chat: String,
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
                if ty == self.cod_type {
                    self.cod = Some(entity);
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

    fn done(&self, criterion: &str) -> bool {
        self.adv
            .get(ADV)
            .is_some_and(|v| v.iter().any(|c| c == criterion))
    }

    async fn cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(200).await;
    }

    async fn select(&mut self, slot: u16) {
        let mut w = self.packet(self.sb_select);
        w.u16(slot);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(120).await;
    }

    async fn interact(&mut self, entity: i32) {
        let mut w = self.packet(self.sb_interact);
        w.var_int(entity).var_int(0).u8(0).bool(false);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(600).await;
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
    let cod_type = reg["minecraft:entity_type"]["entries"]["minecraft:cod"]["protocol_id"]
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
        sb_interact: sb("interact"),
        sb_select: sb("set_carried_item"),
        cb_add_entity,
        cb_chat,
        cod_type,
        cod: None,
        chat: String::new(),
        adv: HashMap::new(),
    };
    p.pump(2000).await;

    let setup: Vec<String> = vec![
        "difficulty peaceful".into(),
        "gamemode survival".into(),
        "effect clear @s".into(),
        "clear @s".into(),
        "time set noon".into(),
        // Бассейн: каменная чаша 5x5 со стенками, внутри вода 3x3x2.
        "fill 78 99 6 82 102 10 minecraft:stone".into(),
        "fill 79 100 7 81 101 9 minecraft:water".into(),
        "tp @s 80.5 100 12.5 0 20".into(),
        format!("summon minecraft:cod {} {} {}", COD[0], COD[1], COD[2]),
        "item replace entity @s hotbar.0 with minecraft:bucket".into(),
        format!("advancement revoke @s only {ADV}"),
        "advancement grant @s only minecraft:husbandry/fishy_business".into(),
    ];
    for line in setup {
        p.cmd(&line).await;
    }
    p.select(0).await;
    p.pump(800).await;
    let cod = p.cod.expect("add_entity трески не пришёл");
    println!("cod entity {cod}");

    // Пустое ведро не даёт ведра с рыбой.
    p.interact(cod).await;
    assert!(
        !p.done("cod_bucket")
            && !p.done("salmon_bucket")
            && !p.done("pufferfish_bucket")
            && !p.done("tropical_fish_bucket"),
        "tactical_fishing сработал с пустым ведром: {:?}",
        p.adv.get(ADV)
    );
    println!("empty bucket skipped ok");

    // Ведро воды превращается в ведро с треской.
    p.cmd("item replace entity @s hotbar.0 with minecraft:water_bucket")
        .await;
    p.select(0).await;
    p.interact(cod).await;
    p.pump(400).await;
    assert!(
        p.done("cod_bucket"),
        "filled_bucket не сработал: {:?}",
        p.adv.get(ADV)
    );
    println!("filled_bucket (cod) ok");

    // В руке ведро с треской.
    p.chat.clear();
    p.cmd("data get entity @s Inventory").await;
    let inv = p.chat.clone();
    assert!(
        inv.contains("minecraft:cod_bucket"),
        "в инвентаре нет cod_bucket: {inv}"
    );
    println!("cod_bucket in hand ok");
    println!("adv_fish: ok");
}
