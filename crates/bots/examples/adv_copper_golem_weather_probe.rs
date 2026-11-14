//! Окисление медного голема: `weather_state`/`next_weather_age` в NBT,
//! воск сотами и снятие воска топором, смена стадии по `next_weather_age`.
//!
//! `adv_copper_golem_weather_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::collections::HashMap;
use std::time::Duration;

const X: i32 = 2112;
const Z: i32 = 8;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_interact: i32,
    sb_select: i32,
    cb_add_entity: i32,
    cb_chat: i32,
    golem_type: i32,
    golem: Option<i32>,
    chat: String,
    adv: HashMap<String, Vec<String>>,
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

    async fn interact(&mut self, entity: i32) {
        let mut w = self.packet(self.sb_interact);
        w.var_int(entity).var_int(0).u8(0).bool(false);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(400).await;
    }

    async fn golem_nbt(&mut self) -> String {
        self.chat.clear();
        self.cmd("data get entity @e[type=minecraft:copper_golem,limit=1]")
            .await;
        self.chat.clone()
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
        sb_interact: sb("interact"),
        sb_select: sb("set_carried_item"),
        cb_add_entity,
        cb_chat,
        golem_type,
        golem: None,
        chat: String::new(),
        adv: HashMap::new(),
    };
    p.pump(2000).await;

    let setup: Vec<String> = vec![
        "difficulty peaceful".into(),
        "gamemode survival".into(),
        "effect clear @s".into(),
        "clear @s".into(),
        "kill @e[type=!minecraft:player,distance=..48]".into(),
        "execute in minecraft:overworld run tp @s 2112.5 100 11.5 180 20".into(),
        format!("fill {} 99 4 {} 104 12 minecraft:stone", X - 4, X + 4),
        format!("fill {} 100 4 {} 104 12 minecraft:air", X - 4, X + 4),
        format!("summon minecraft:copper_golem {X}.5 100 {Z}.5"),
        "item replace entity @s hotbar.0 with minecraft:honeycomb 8".into(),
        "item replace entity @s hotbar.1 with minecraft:iron_axe".into(),
    ];
    for line in setup {
        p.cmd(&line).await;
    }
    p.select(0).await;
    p.pump(800).await;

    let golem = p.golem.expect("add_entity медного голема не пришёл");
    let nbt = p.golem_nbt().await;
    assert!(
        nbt.contains("weather_state:\"unaffected\""),
        "стартовая стадия не unaffected: {nbt}"
    );
    println!("fresh golem unaffected ok");

    // Воск сотами: `next_weather_age` = -2.
    p.interact(golem).await;
    let nbt = p.golem_nbt().await;
    assert!(
        nbt.contains("next_weather_age:-2L"),
        "воск не остановил окисление: {nbt}"
    );
    println!("waxed ok");

    // Топор снимает воск: `next_weather_age` = -1.
    p.select(1).await;
    p.interact(golem).await;
    let nbt = p.golem_nbt().await;
    // После снятия воска таймер пересчитывается на ближайшем тике.
    assert!(
        !nbt.contains("next_weather_age:-2L"),
        "топор не снял воск: {nbt}"
    );
    assert!(
        nbt.contains("weather_state:\"unaffected\""),
        "стадия изменилась без окисления: {nbt}"
    );
    println!("scraped ok");

    // Стадии: выставляем ближайший тик и ждём тик сервера.
    for stage in ["exposed", "weathered", "oxidized"] {
        p.cmd("data merge entity @e[type=minecraft:copper_golem,limit=1] {next_weather_age:1L}")
            .await;
        p.pump(400).await;
        let nbt = p.golem_nbt().await;
        assert!(
            nbt.contains(&format!("weather_state:\"{stage}\"")),
            "стадия {stage} не наступила: {nbt}"
        );
        println!("stage {stage} ok");
    }
    println!("adv_copper_golem_weather: ok");
}
