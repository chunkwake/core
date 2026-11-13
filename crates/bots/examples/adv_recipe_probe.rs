//! Триггер достижений recipe_crafted: улучшение брони шаблоном.
//!
//! `adv_recipe_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::collections::HashMap;
use std::time::Duration;

const BX: i32 = 5900;
const BZ: i32 = 5900;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_use_on: i32,
    sb_click: i32,
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
        self.pump(500).await;
    }

    async fn click(&mut self, window: i32, slot: u16) {
        self.click_button(window, slot, 0).await;
    }

    async fn click_button(&mut self, window: i32, slot: u16, button: u8) {
        self.seq += 1;
        let mut w = self.packet(self.sb_click);
        w.var_int(window)
            .var_int(self.seq)
            .u16(slot)
            .u8(button)
            .var_int(0)
            .var_int(0)
            .bool(false);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(300).await;
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
        sb_click: sb("container_click"),
        sb_close: sb("container_close"),
        cb_chat,
        seq: 0,
        chat: String::new(),
        window: None,
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
        "effect clear @s".to_string(),
        "advancement revoke @s only minecraft:adventure/trim_with_any_armor_pattern".to_string(),
        "advancement revoke @s only minecraft:adventure/craft_decorated_pot_using_only_sherds"
            .to_string(),
        "advancement grant @s only minecraft:adventure/root".to_string(),
        "advancement grant @s only minecraft:adventure/salvage_sherd".to_string(),
    ] {
        p.cmd(&line).await;
    }

    let criterion = "armor_trimmed_minecraft:wayfinder_armor_trim_smithing_template_smithing_trim";

    // 1. Кузнечный стол: шаблон + нагрудник + алмаз.
    p.cmd(&format!(
        "setblock {} {} {} minecraft:smithing_table",
        BX + 1,
        by,
        BZ
    ))
    .await;
    p.cmd("give boater minecraft:wayfinder_armor_trim_smithing_template 1")
        .await;
    p.cmd("give boater minecraft:iron_chestplate 1").await;
    p.cmd("give boater minecraft:diamond 1").await;
    p.pump(400).await;
    p.window = None;
    p.use_on([BX + 1, by, BZ], 1).await;
    let win = p.window.expect("кузнечный стол не открылся");
    // Контейнер 0-2, результат 3; инвентарь с 4, хотбар 0-2 -> 31-33.
    p.click(win, 31).await;
    p.click(win, 0).await;
    p.click(win, 32).await;
    p.click(win, 1).await;
    p.click(win, 33).await;
    p.click(win, 2).await;
    p.click(win, 3).await;
    p.pump(600).await;
    assert!(
        p.done("minecraft:adventure/trim_with_any_armor_pattern", criterion),
        "recipe_crafted (smithing trim) не сработал: {:?}",
        p.adv.get("minecraft:adventure/trim_with_any_armor_pattern")
    );
    println!("recipe_crafted (smithing trim) ok");
    p.close(win).await;

    // 2. Верстак: декорированный горшок из четырёх черепков.
    p.cmd("clear boater").await;
    for _ in 0..4 {
        p.cmd("give boater minecraft:angler_pottery_sherd 1").await;
    }
    p.cmd(&format!(
        "setblock {} {} {} minecraft:crafting_table",
        BX + 3,
        by,
        BZ
    ))
    .await;
    p.window = None;
    p.use_on([BX + 3, by, BZ], 1).await;
    let win = p.window.expect("верстак не открылся");
    // Сетка 1-9, инвентарь с 10, хотбар 0 -> 37. Берём стопку один раз и
    // раскладываем по одному черепку в крестик.
    p.click(win, 37).await;
    for slot in [2, 4, 6, 8] {
        p.click_button(win, slot, 1).await;
    }
    p.click(win, 0).await;
    p.pump(600).await;
    assert!(
        p.done(
            "minecraft:adventure/craft_decorated_pot_using_only_sherds",
            "pot_crafted_using_only_sherds"
        ),
        "recipe_crafted (decorated pot) не сработал: {:?}",
        p.adv
            .get("minecraft:adventure/craft_decorated_pot_using_only_sherds")
    );
    println!("recipe_crafted (decorated pot) ok");
    p.close(win).await;

    p.cmd("effect clear @s").await;
    println!("adv_recipe: ok");
}
