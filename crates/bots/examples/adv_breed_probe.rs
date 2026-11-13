//! Триггеры достижений: bred_animals и enter_block.
//!
//! `adv_breed_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::collections::HashMap;
use std::time::Duration;

const BX: i32 = 5800;
const BZ: i32 = 5800;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_select: i32,
    sb_interact: i32,
    sb_move: i32,
    cb_chat: i32,
    cow_ty: i32,
    seq: i32,
    chat: String,
    cows: Vec<i32>,
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
                let line = flatten(&component);
                if !line.trim().is_empty() {
                    self.chat.push_str(line.trim());
                    self.chat.push(' ');
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
                Event::EntitySpawned {
                    entity_id, kind, ..
                } if kind == self.cow_ty && !self.cows.contains(&entity_id) => {
                    self.cows.push(entity_id);
                }
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

    async fn select(&mut self, slot: u16) {
        let mut w = self.packet(self.sb_select);
        w.u16(slot);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(120).await;
    }

    async fn pose(&mut self, pos: [f64; 3], yaw: f32, pitch: f32) {
        let mut w = self.packet(self.sb_move);
        w.f64(pos[0])
            .f64(pos[1])
            .f64(pos[2])
            .f32(yaw)
            .f32(pitch)
            .u8(1);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(150).await;
    }

    async fn interact(&mut self, entity: i32) {
        self.seq += 1;
        let mut w = self.packet(self.sb_interact);
        w.var_int(entity).var_int(0).u8(0).bool(false);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(300).await;
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let reg: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::path::Path::new(&a[2]).with_file_name("registries.json"))
            .unwrap(),
    )
    .unwrap();
    let ent = |n: &str| {
        reg["minecraft:entity_type"]["entries"][n]["protocol_id"]
            .as_i64()
            .unwrap() as i32
    };
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
        sb_interact: sb("interact"),
        sb_move: sb("move_player_pos_rot"),
        cb_chat,
        cow_ty: ent("minecraft:cow"),
        seq: 0,
        chat: String::new(),
        cows: Vec::new(),
        adv: HashMap::new(),
    };
    p.pump(2000).await;

    let by = 64;
    for line in [
        "difficulty peaceful".to_string(),
        "gamemode survival".to_string(),
        "weather clear".to_string(),
        format!(
            "fill {} {} {} {} {} {} minecraft:air",
            BX - 8,
            by,
            BZ - 8,
            BX + 8,
            by + 5,
            BZ + 8
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            BX - 8,
            by - 1,
            BZ - 8,
            BX + 8,
            by - 1,
            BZ + 8
        ),
        format!(
            "tp boater {} {} {} 0 0",
            BX as f64 + 0.5,
            by,
            BZ as f64 + 0.5
        ),
        "clear boater".to_string(),
        "effect clear @s".to_string(),
        "kill @e[type=minecraft:cow,limit=20]".to_string(),
        "advancement revoke @s only minecraft:husbandry/breed_an_animal".to_string(),
        "advancement revoke @s only minecraft:husbandry/bred_all_animals".to_string(),
        "advancement revoke @s only minecraft:recipes/transportation/oak_boat".to_string(),
        "advancement revoke @s only minecraft:end/enter_end_gateway".to_string(),
        "advancement grant @s only minecraft:husbandry/root".to_string(),
        "advancement grant @s only minecraft:recipes/root".to_string(),
        "advancement grant @s only minecraft:end/kill_dragon".to_string(),
    ] {
        p.cmd(&line).await;
    }

    // 1. bred_animals: две коровы и пшеница.
    p.cows.clear();
    for z in [2.5, 4.5] {
        p.cmd(&format!(
            "summon minecraft:cow {} {} {}",
            BX as f64 + 0.5,
            by as f64,
            BZ as f64 + z
        ))
        .await;
    }
    p.pump(600).await;
    assert!(p.cows.len() >= 2, "коровы не появились: {:?}", p.cows);
    p.cmd("give boater minecraft:wheat 8").await;
    p.select(0).await;
    p.pose([BX as f64 + 0.5, by as f64, BZ as f64 + 0.5], 0.0, 0.0)
        .await;
    p.interact(p.cows[0]).await;
    p.interact(p.cows[1]).await;
    for _ in 0..30 {
        p.pump(500).await;
        if p.done("minecraft:husbandry/breed_an_animal", "bred") {
            break;
        }
    }
    if !p.done("minecraft:husbandry/breed_an_animal", "bred") {
        p.chat.clear();
        p.cmd("data get entity @e[type=minecraft:cow,limit=1]")
            .await;
        println!("cow: {}", p.chat);
        println!("cows spawned: {}", p.cows.len());
    }
    assert!(
        p.done("minecraft:husbandry/breed_an_animal", "bred"),
        "bred_animals не сработал: {:?}",
        p.adv.get("minecraft:husbandry/breed_an_animal")
    );
    assert!(
        p.done("minecraft:husbandry/bred_all_animals", "minecraft:cow"),
        "bred_animals (criteria cow) не сработал: {:?}",
        p.adv.get("minecraft:husbandry/bred_all_animals")
    );
    println!("bred_animals ok");

    // 2. enter_block: вода и рецепт лодки.
    p.cmd("recipe take @s minecraft:oak_boat").await;
    p.cmd("recipe give @s minecraft:oak_boat").await;
    p.cmd(&format!(
        "fill {} {} {} {} {} {} minecraft:water",
        BX + 4,
        by,
        BZ,
        BX + 4,
        by + 1,
        BZ
    ))
    .await;
    p.pose([BX as f64 + 4.5, by as f64, BZ as f64 + 0.5], 0.0, 0.0)
        .await;
    p.pose([BX as f64 + 4.5, by as f64, BZ as f64 + 0.6], 0.0, 0.0)
        .await;
    p.pump(500).await;
    assert!(
        p.done("minecraft:recipes/transportation/oak_boat", "in_water"),
        "enter_block (вода) не сработал: {:?}",
        p.adv.get("minecraft:recipes/transportation/oak_boat")
    );
    println!("enter_block (water) ok");

    p.cmd("kill @e[type=minecraft:cow,limit=20]").await;
    p.cmd("effect clear @s").await;
    println!("adv_breed: ok");
}
