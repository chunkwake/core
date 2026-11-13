//! Фактический крафт transmute/dye/imbue: книга рецептов + сетка.
//!
//! `craft_special_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

const BX: i32 = 4800;
const BZ: i32 = 4800;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_click: i32,
    sb_place: i32,
    sb_select: i32,
    sb_use_on: i32,
    cb_chat: i32,
    chat: String,
    seq: i32,
    book: Vec<(i32, u8)>,
    window: Option<i32>,
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
                Event::RecipeBookAdd { entries, .. } => self.book.extend(entries),
                Event::OpenScreen { window, .. } => self.window = Some(window),
                _ => {}
            }
        }
        self.drain();
    }

    async fn cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(350).await;
    }

    async fn data(&mut self, query: &str) -> String {
        self.pump(120).await;
        self.chat.clear();
        self.cmd(&format!("data get entity {query}")).await;
        if self.chat.trim().is_empty() {
            self.pump(250).await;
            self.cmd(&format!("data get entity {query}")).await;
        }
        self.chat.clone()
    }

    async fn select(&mut self, slot: u16) {
        let mut w = self.packet(self.sb_select);
        w.u16(slot);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(150).await;
    }

    async fn place(&mut self, window: i32, display: i32) {
        let mut w = self.packet(self.sb_place);
        w.var_int(window).var_int(display).bool(false);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(500).await;
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

    async fn give_recipe(&mut self, id: &str) -> i32 {
        self.cmd(&format!("recipe take @s {id}")).await;
        self.book.clear();
        self.cmd(&format!("recipe give @s {id}")).await;
        self.book.last().map(|e| e.0).expect("рецепт не выдан")
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
        sb_click: sb("container_click"),
        sb_place: sb("place_recipe"),
        sb_select: sb("set_carried_item"),
        sb_use_on: sb("use_item_on"),
        cb_chat,
        chat: String::new(),
        seq: 0,
        book: Vec::new(),
        window: None,
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
            by + 5,
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
        format!("tp boater {} {} {}", BX as f64 + 0.5, by, BZ as f64 + 0.5),
        "clear boater".to_string(),
    ] {
        p.cmd(&line).await;
    }
    p.select(0).await;

    // 1. Transmute: bundle + black_dye -> black_bundle.
    let display = p.give_recipe("minecraft:black_bundle").await;
    p.cmd("give boater minecraft:bundle 1").await;
    p.cmd("give boater minecraft:black_dye 1").await;
    p.select(0).await;
    p.place(0, display).await;
    p.click(0, 0).await;
    p.click(0, 40).await;
    let inv = p.data("@s Inventory").await;
    assert!(
        inv.contains("black_bundle"),
        "transmute не дал black_bundle: {inv}"
    );
    assert!(
        !inv.contains("black_dye"),
        "transmute не списал краситель: {inv}"
    );
    println!("transmute craft ok");

    // 2. Dye: leather_boots + white_dye -> окрашенные ботинки.
    p.cmd("clear boater").await;
    let display = p.give_recipe("minecraft:leather_boots_dyed").await;
    p.cmd("give boater minecraft:leather_boots 1").await;
    p.cmd("give boater minecraft:white_dye 1").await;
    p.select(0).await;
    p.place(0, display).await;
    p.click(0, 0).await;
    p.click(0, 40).await;
    let inv = p.data("@s Inventory").await;
    assert!(inv.contains("leather_boots"), "dye потерял ботинки: {inv}");
    assert!(!inv.contains("white_dye"), "dye не списал краситель: {inv}");
    println!("dye craft ok");

    // 3. Imbue: 8 стрел + зелье -> 8 tipped_arrow (нужен верстак 3×3).
    p.cmd("clear boater").await;
    p.cmd(&format!(
        "setblock {} {} {} minecraft:crafting_table",
        BX + 1,
        by,
        BZ
    ))
    .await;
    p.window = None;
    p.use_on([BX + 1, by, BZ], 1).await;
    let window = p.window.expect("верстак не открылся");
    let display = p.give_recipe("minecraft:tipped_arrow").await;
    p.cmd("give boater minecraft:arrow 8").await;
    p.cmd("give boater minecraft:lingering_potion 1").await;
    p.select(0).await;
    p.place(window, display).await;
    p.click(window, 0).await;
    p.click(window, 40).await;
    let inv = p.data("@s Inventory").await;
    assert!(
        inv.contains("tipped_arrow"),
        "imbue не дал tipped_arrow: {inv}"
    );
    println!("imbue craft ok");

    p.cmd("kill @e[type=minecraft:item]").await;
    println!("craft_special: ok");
}
