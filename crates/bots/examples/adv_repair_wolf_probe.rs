//! `entity.equipment` в предикате цели и `husbandry/repair_wolf_armor`:
//! волк с повреждённой бронёй, ПКМ щитком броненосца.
//!
//! `adv_repair_wolf_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::collections::HashMap;
use std::time::Duration;

const ADV: &str = "minecraft:husbandry/repair_wolf_armor";
const CRITERION: &str = "repair_wolf_armor";
const WOLF: [f64; 3] = [1200.5, 100.0, 8.5];

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
    wolf_type: i32,
    wolf: Option<i32>,
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
                if ty == self.wolf_type {
                    self.wolf = Some(entity);
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

    async fn wolf_nbt(&mut self) -> String {
        self.chat.clear();
        self.cmd("data get entity @e[type=minecraft:wolf,limit=1]")
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
    let wolf_type = reg["minecraft:entity_type"]["entries"]["minecraft:wolf"]["protocol_id"]
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
        wolf_type,
        wolf: None,
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
        // Площадка и место для волка.
        "fill 1196 99 4 1204 99 12 minecraft:stone".into(),
        "fill 1196 100 4 1204 102 12 minecraft:air".into(),
        "tp @s 1202.5 100 8.5".into(),
    ];
    for line in setup {
        p.cmd(&line).await;
    }
    // Даём чанку загрузиться, только потом убираем старых волков.
    p.pump(1500).await;
    p.cmd("kill @e[type=minecraft:wolf,distance=..64]").await;
    p.pump(400).await;
    for line in [
        format!("advancement revoke @s only {ADV}"),
        "advancement grant @s only minecraft:husbandry/tame_an_animal".to_owned(),
        "item replace entity @s hotbar.0 with minecraft:bone 64".to_owned(),
        format!("summon minecraft:wolf {} {} {}", WOLF[0], WOLF[1], WOLF[2]),
    ] {
        p.cmd(&line).await;
    }
    p.select(0).await;
    p.pump(1200).await;
    let wolf = p.wolf.expect("add_entity волка не пришёл");
    println!("wolf entity {wolf}");

    // Приручение (шанс 1/3 на кость).
    let mut tames = 0;
    while tames < 32 {
        p.interact(wolf).await;
        tames += 1;
        let nbt = p.wolf_nbt().await;
        if nbt.contains("Tame:1b") {
            break;
        }
    }
    let nbt = p.wolf_nbt().await;
    assert!(nbt.contains("Tame:1b"), "волк не приручён: {nbt}");
    println!("tamed after {tames} bones");

    // Надеваем повреждённую броню (64 прочности, чинится на 8 за щиток).
    p.cmd(r#"data merge entity @e[type=minecraft:wolf,limit=1] {equipment:{body:{id:"minecraft:wolf_armor",count:1,components:{"minecraft:damage":8}}}}"#).await;
    let nbt = p.wolf_nbt().await;
    assert!(
        nbt.contains("minecraft:wolf_armor"),
        "броня не надета через NBT: {nbt}"
    );
    assert!(
        nbt.contains("Sitting:1b"),
        "волк не сидит после приручения: {nbt}"
    );
    println!("damaged wolf armor equipped");

    // Не сидя: щиток не чинит.
    p.cmd(&format!("advancement revoke @s only {ADV}")).await;
    p.select(8).await; // пустой слот
    p.interact(wolf).await;
    let nbt = p.wolf_nbt().await;
    assert!(nbt.contains("Sitting:0b"), "волк остался сидеть: {nbt}");
    p.cmd("item replace entity @s hotbar.1 with minecraft:armadillo_scute 16")
        .await;
    p.select(1).await;
    p.interact(wolf).await;
    assert!(
        !p.done(),
        "repair_wolf_armor сработал, когда волк стоял: {:?}",
        p.adv.get(ADV)
    );
    println!("standing wolf skipped ok");

    // Сидя: щиток чинит до нуля и закрывает достижение.
    p.select(8).await;
    p.interact(wolf).await;
    let nbt = p.wolf_nbt().await;
    assert!(nbt.contains("Sitting:1b"), "волк не сел: {nbt}");
    p.select(1).await;
    p.interact(wolf).await;
    p.pump(400).await;
    assert!(
        p.done(),
        "repair_wolf_armor не сработал: {:?}",
        p.adv.get(ADV)
    );
    let nbt = p.wolf_nbt().await;
    assert!(
        !nbt.contains("\"minecraft:damage\"") || nbt.contains("\"minecraft:damage\":0"),
        "броня не починена: {nbt}"
    );
    println!("wolf armor repaired ok");
    println!("adv_repair_wolf: ok");
}
