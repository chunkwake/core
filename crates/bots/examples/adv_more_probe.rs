//! Триггеры достижений: item_used_on_block, changed_dimension,
//! brewed_potion и enchanted_item.
//!
//! `adv_more_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::collections::HashMap;
use std::time::Duration;

const BX: i32 = 5300;
const BZ: i32 = 5300;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_select: i32,
    sb_use_on: i32,
    sb_click: i32,
    sb_button: i32,
    sb_close: i32,
    cb_chat: i32,
    chat: String,
    seq: i32,
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

    async fn click(&mut self, window: i32, slot: u16) {
        self.seq += 1;
        let mut w = self.packet(self.sb_click);
        w.var_int(window)
            .var_int(self.seq)
            .u16(slot)
            .u8(0)
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
        self.pump(300).await;
    }

    async fn button(&mut self, window: i32, button: i32) {
        let mut w = self.packet(self.sb_button);
        w.var_int(window).var_int(button);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(800).await;
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
        sb_click: sb("container_click"),
        sb_button: sb("container_button_click"),
        sb_close: sb("container_close"),
        cb_chat,
        chat: String::new(),
        seq: 0,
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
        "advancement grant @s only minecraft:husbandry/root".to_string(),
        "advancement grant @s only minecraft:story/mine_diamond".to_string(),
        "advancement grant @s only minecraft:nether/obtain_blaze_rod".to_string(),
    ] {
        p.cmd(&line).await;
    }

    // 1. item_used_on_block: бутылка мёда из улья.
    p.cmd(&format!(
        "setblock {} {} {} minecraft:beehive[honey_level=5]",
        BX + 1,
        by,
        BZ
    ))
    .await;
    p.cmd("give boater minecraft:glass_bottle 1").await;
    p.select(0).await;
    p.use_on([BX + 1, by, BZ], 1).await;
    p.pump(500).await;
    assert!(
        p.done(
            "minecraft:husbandry/safely_harvest_honey",
            "safely_harvest_honey"
        ),
        "item_used_on_block не сработал: {:?}",
        p.adv.get("minecraft:husbandry/safely_harvest_honey")
    );
    println!("item_used_on_block ok");

    // 2. changed_dimension: переход в Нижний мир и обратно.
    p.cmd("execute in minecraft:the_nether run tp @s 0 64 0")
        .await;
    p.pump(600).await;
    assert!(
        p.done("minecraft:nether/root", "entered_nether"),
        "changed_dimension (nether) не сработал: {:?}",
        p.adv.get("minecraft:nether/root")
    );
    p.cmd(&format!(
        "execute in minecraft:overworld run tp @s {} {} {}",
        BX as f64 + 0.5,
        by,
        BZ as f64 + 0.5
    ))
    .await;
    p.pump(600).await;
    println!("changed_dimension ok");

    // 3. enchanted_item: стол зачарований.
    p.cmd("clear boater").await;
    p.cmd("give boater minecraft:diamond_sword 1").await;
    p.cmd("give boater minecraft:lapis_lazuli 3").await;
    p.cmd("xp add @s 30 levels").await;
    p.cmd(&format!(
        "setblock {} {} {} minecraft:enchanting_table",
        BX + 2,
        by,
        BZ
    ))
    .await;
    p.window = None;
    p.use_on([BX + 2, by, BZ], 1).await;
    let win = p.window.expect("стол зачарований не открылся");
    // Инвентарь начинается со слота 2; хотбар 0 -> 29, хотбар 1 -> 30.
    p.click(win, 29).await;
    p.click(win, 0).await;
    p.click(win, 30).await;
    p.click(win, 1).await;
    p.button(win, 0).await;
    p.pump(600).await;
    assert!(
        p.done("minecraft:story/enchant_item", "enchanted_item"),
        "enchanted_item не сработал: {:?}",
        p.adv.get("minecraft:story/enchant_item")
    );
    println!("enchanted_item ok");
    p.close(win).await;

    // 4. brewed_potion: варка и взятие зелья.
    p.cmd("clear boater").await;
    p.cmd(&format!(
        "setblock {} {} {} minecraft:brewing_stand",
        BX + 3,
        by,
        BZ
    ))
    .await;
    p.cmd("give boater minecraft:potion[potion_contents=\"minecraft:water\"] 1")
        .await;
    p.cmd("give boater minecraft:nether_wart 1").await;
    p.cmd("give boater minecraft:blaze_powder 1").await;
    p.window = None;
    p.use_on([BX + 3, by, BZ], 1).await;
    let win = p.window.expect("варильная стойка не открылась");
    // Инвентарь со слота 5; хотбар 0..2 -> 32..34.
    p.click(win, 32).await;
    p.click(win, 0).await;
    p.click(win, 33).await;
    p.click(win, 3).await;
    p.click(win, 34).await;
    p.click(win, 4).await;
    p.pump(22000).await;
    p.click(win, 0).await;
    p.pump(500).await;
    assert!(
        p.done("minecraft:nether/brew_potion", "potion"),
        "brewed_potion не сработал: {:?}",
        p.adv.get("minecraft:nether/brew_potion")
    );
    println!("brewed_potion ok");

    println!("adv_more: ok");
}
