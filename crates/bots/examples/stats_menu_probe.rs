//! Статистика меню: interact_with_*, drop и play_record.
//!
//! `stats_menu_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::collections::HashMap;
use std::time::Duration;

const BX: i32 = 5400;
const BZ: i32 = 5400;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_select: i32,
    sb_use_on: i32,
    sb_action: i32,
    sb_close: i32,
    cb_chat: i32,
    chat: String,
    seq: i32,
    window: Option<i32>,
    stats: HashMap<(i32, i32), i32>,
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
                Event::Stats { stats } => {
                    for (key, n) in stats {
                        self.stats.insert(key, n);
                    }
                }
                Event::OpenScreen { window, .. } => self.window = Some(window),
                _ => {}
            }
        }
        self.drain();
    }

    fn stat(&self, kind: i32, value: i32) -> i32 {
        self.stats.get(&(kind, value)).copied().unwrap_or(0)
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
        self.pump(500).await;
    }

    async fn close(&mut self) {
        if let Some(win) = self.window.take() {
            let mut w = self.packet(self.sb_close);
            w.var_int(win);
            self.c.send_raw(&w.buf).await.unwrap();
            self.pump(250).await;
        }
    }

    async fn drop_one(&mut self) {
        self.seq += 1;
        let mut w = self.packet(self.sb_action);
        w.var_int(5)
            .i64(pack_pos([0, 0, 0]))
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
    let ids = Ids::load(&table).unwrap();
    let reg: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::path::Path::new(&a[2]).with_file_name("registries.json"))
            .unwrap(),
    )
    .unwrap();
    let cstat = |n: &str| {
        reg["minecraft:custom_stat"]["entries"][n]["protocol_id"]
            .as_i64()
            .unwrap() as i32
    };
    let custom = reg["minecraft:stat_type"]["entries"]["minecraft:custom"]["protocol_id"]
        .as_i64()
        .unwrap() as i32;
    let stone = reg["minecraft:item"]["entries"]["minecraft:stone"]["protocol_id"]
        .as_i64()
        .unwrap() as i32;
    let dropped_kind = reg["minecraft:stat_type"]["entries"]["minecraft:dropped"]["protocol_id"]
        .as_i64()
        .unwrap() as i32;
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
        sb_action: sb("player_action"),
        sb_close: sb("container_close"),
        cb_chat,
        chat: String::new(),
        seq: 0,
        window: None,
        stats: HashMap::new(),
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
    ] {
        p.cmd(&line).await;
    }

    let menus = [
        (
            "minecraft:crafting_table",
            "minecraft:interact_with_crafting_table",
        ),
        ("minecraft:furnace", "minecraft:interact_with_furnace"),
        (
            "minecraft:blast_furnace",
            "minecraft:interact_with_blast_furnace",
        ),
        ("minecraft:smoker", "minecraft:interact_with_smoker"),
        ("minecraft:anvil", "minecraft:interact_with_anvil"),
        ("minecraft:grindstone", "minecraft:interact_with_grindstone"),
        (
            "minecraft:stonecutter",
            "minecraft:interact_with_stonecutter",
        ),
        ("minecraft:loom", "minecraft:interact_with_loom"),
        (
            "minecraft:cartography_table",
            "minecraft:interact_with_cartography_table",
        ),
        (
            "minecraft:lectern[has_book=true]",
            "minecraft:interact_with_lectern",
        ),
        (
            "minecraft:brewing_stand",
            "minecraft:interact_with_brewingstand",
        ),
        ("minecraft:beacon", "minecraft:interact_with_beacon"),
        (
            "minecraft:smithing_table",
            "minecraft:interact_with_smithing_table",
        ),
    ];
    for (block, stat) in menus {
        p.cmd(&format!("setblock {} {} {} {block}", BX + 1, by, BZ))
            .await;
        let id = cstat(stat);
        let before = p.stat(custom, id);
        p.window = None;
        p.use_on([BX + 1, by, BZ], 1).await;
        assert!(p.window.is_some(), "{block}: меню не открылось");
        p.close().await;
        assert_eq!(p.stat(custom, id), before + 1, "{stat} не вырос на 1");
    }
    println!("interact_with_* ok");

    // drop: выбросить камень.
    p.cmd("clear boater").await;
    p.cmd("give boater minecraft:stone 1").await;
    p.select(0).await;
    let drop_stat = cstat("minecraft:drop");
    let before = p.stat(custom, drop_stat);
    let dropped_before = p.stat(dropped_kind, stone);
    p.drop_one().await;
    p.pump(600).await;
    assert_eq!(p.stat(custom, drop_stat), before + 1, "drop не вырос на 1");
    assert!(
        p.stat(dropped_kind, stone) > dropped_before,
        "dropped stone не вырос"
    );
    println!("drop ok");

    // play_record: вставить пластинку в проигрыватель.
    p.cmd("clear boater").await;
    p.cmd(&format!(
        "setblock {} {} {} minecraft:jukebox",
        BX + 1,
        by,
        BZ
    ))
    .await;
    p.cmd("give boater minecraft:music_disc_cat 1").await;
    p.select(0).await;
    let rec = cstat("minecraft:play_record");
    let before = p.stat(custom, rec);
    p.use_on([BX + 1, by, BZ], 1).await;
    p.pump(500).await;
    assert_eq!(p.stat(custom, rec), before + 1, "play_record не вырос на 1");
    println!("play_record ok");

    p.cmd("kill @e[type=minecraft:item]").await;
    println!("stats_menu: ok");
}
