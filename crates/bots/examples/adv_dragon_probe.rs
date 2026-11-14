//! `summoned_entity` для дракона Края: убить дракона, поставить четыре
//! кристалла по сторонам портала и получить `end/respawn_dragon`.
//!
//! `adv_dragon_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::collections::HashMap;
use std::time::Duration;

const ADV: &str = "minecraft:end/respawn_dragon";
const CRITERION: &str = "summoned_dragon";

fn pack_pos([x, y, z]: [i32; 3]) -> i64 {
    ((x as i64 & 0x3ffffff) << 38) | ((z as i64 & 0x3ffffff) << 12) | (y as i64 & 0xfff)
}

struct Probe {
    c: Client,
    sb_command: i32,
    sb_use_on: i32,
    sb_select: i32,
    cb_add_entity: i32,
    cb_chat: i32,
    chat: String,
    dragon_type: i32,
    dragon: Option<i32>,
    seq: i32,
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
            if id == self.cb_chat {
                let mut r = Reader::new(&body);
                if let Ok(component) = text::read_component(&mut r) {
                    self.chat.push_str(&flatten(&component));
                    self.chat.push('\n');
                }
                continue;
            }
            if id != self.cb_add_entity {
                continue;
            }
            let mut r = Reader::new(&body);
            let Ok(entity) = r.var_int() else { continue };
            if r.bytes(16).is_err() {
                continue;
            }
            let Ok(ty) = r.var_int() else { continue };
            if ty == self.dragon_type {
                self.dragon = Some(entity);
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
        self.pump(180).await;
    }

    async fn select(&mut self, slot: u16) {
        let mut w = self.packet(self.sb_select);
        w.u16(slot);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(120).await;
    }

    async fn use_on(&mut self, pos: [i32; 3]) {
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
        self.pump(500).await;
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
    let dragon_type =
        reg["minecraft:entity_type"]["entries"]["minecraft:ender_dragon"]["protocol_id"]
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
        sb_use_on: sb("use_item_on"),
        sb_select: sb("set_carried_item"),
        cb_add_entity,
        cb_chat,
        chat: String::new(),
        dragon_type,
        dragon: None,
        seq: 0,
        adv: HashMap::new(),
    };
    p.pump(2000).await;

    let setup: Vec<String> = vec![
        "difficulty normal".into(),
        "gamemode creative".into(),
        "effect clear @s".into(),
        // Площадка в Крае и сам игрок.
        // Сначала переходим в Край (загружает чанки), только потом правим блоки.
        "execute in minecraft:the_end run tp @s 0.5 127 6.5 0 20".into(),
        "execute in minecraft:the_end run fill -6 126 -6 6 126 6 minecraft:stone".into(),
        "execute in minecraft:the_end run fill -6 127 -6 6 131 6 minecraft:air".into(),
        format!("advancement revoke @s only {ADV}"),
        "advancement grant @s only minecraft:end/kill_dragon".into(),
    ];
    for line in setup {
        p.cmd(&line).await;
    }
    p.pump(1500).await;

    // Убиваем дракона: без этого возрождение не начинается.
    p.cmd("execute in minecraft:the_end run kill @e[type=minecraft:ender_dragon]")
        .await;
    p.cmd("execute in minecraft:the_end run summon minecraft:ender_dragon 0.5 130 0.5 {NoAI:1b,Health:1.0f}")
        .await;
    p.pump(400).await;
    p.dragon = None;
    p.cmd("execute in minecraft:the_end run kill @e[type=minecraft:ender_dragon]")
        .await;
    p.pump(1200).await;
    p.chat.clear();
    p.cmd("execute in minecraft:the_end if entity @e[type=minecraft:ender_dragon] run data get entity @s")
        .await;
    assert!(!p.chat.contains("Health:"), "дракон не умер: {}", p.chat);
    println!("dragon killed ok");

    // Четыре обсидиана по сторонам центра и кристаллы на них.
    p.cmd("execute in minecraft:the_end run item replace entity @s hotbar.0 with minecraft:end_crystal 4")
        .await;
    // Предыдущие пробы могли оставить выбранным другой слот.
    p.select(0).await;
    for (x, z) in [(3, 0), (-3, 0), (0, 3), (0, -3)] {
        p.cmd(&format!(
            "execute in minecraft:the_end run setblock {x} 127 {z} minecraft:obsidian"
        ))
        .await;
        p.cmd(&format!(
            "execute in minecraft:the_end run setblock {x} 128 {z} minecraft:air"
        ))
        .await;
    }
    p.pump(400).await;

    for (x, z, yaw) in [(3, 0, -90.0), (-3, 0, 90.0), (0, 3, 0.0), (0, -3, 180.0)] {
        let (px, pz) = match (x, z) {
            (3, 0) => (6.5, 0.5),
            (-3, 0) => (-5.5, 0.5),
            (0, 3) => (0.5, 6.5),
            _ => (0.5, -5.5),
        };
        p.cmd(&format!(
            "execute in minecraft:the_end run tp @s {px} 127 {pz} {yaw} 20"
        ))
        .await;
        p.pump(250).await;
        p.use_on([x, 127, z]).await;
    }
    p.pump(1200).await;

    let dragon = p
        .dragon
        .expect("add_entity возрождённого дракона не пришёл");
    assert!(p.done(), "respawn_dragon не закрылся: {:?}", p.adv.get(ADV));
    println!("respawned dragon entity {dragon}");

    // Возвращаем игрока в Верхний мир: иначе следующие пробы работают в Крае
    // (там нет небесного света и другая местность).
    p.cmd("execute in minecraft:overworld run tp @s 0.5 100 0.5 0 0")
        .await;
    p.pump(600).await;
    println!("adv_dragon: ok");
}
