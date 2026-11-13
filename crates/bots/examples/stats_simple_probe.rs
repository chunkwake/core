//! Простые custom-статистики: bell_ring, play/tune_noteblock, pot_flower и
//! inspect_hopper/dispenser/dropper.
//!
//! `stats_simple_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::collections::HashMap;
use std::time::Duration;

const BX: i32 = 5500;
const BZ: i32 = 5500;

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
        self.pump(450).await;
    }

    async fn hit(&mut self, pos: [i32; 3]) {
        for action in [0, 3] {
            self.seq += 1;
            let mut w = self.packet(self.sb_action);
            w.var_int(action).i64(pack_pos(pos)).u8(1).var_int(self.seq);
            self.c.send_raw(&w.buf).await.unwrap();
            self.pump(350).await;
        }
    }

    async fn close(&mut self) {
        if let Some(win) = self.window.take() {
            let mut w = self.packet(self.sb_close);
            w.var_int(win);
            self.c.send_raw(&w.buf).await.unwrap();
            self.pump(250).await;
        }
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

    // 1. tune_noteblock: ПКМ по ноте.
    p.cmd(&format!(
        "setblock {} {} {} minecraft:note_block",
        BX + 1,
        by,
        BZ
    ))
    .await;
    let tune = cstat("minecraft:tune_noteblock");
    let before = p.stat(custom, tune);
    p.use_on([BX + 1, by, BZ], 1).await;
    assert_eq!(p.stat(custom, tune), before + 1, "tune_noteblock не вырос");
    println!("tune_noteblock ok");

    // 2. play_noteblock: ЛКМ по ноте.
    let play = cstat("minecraft:play_noteblock");
    let before = p.stat(custom, play);
    p.hit([BX + 1, by, BZ]).await;
    assert_eq!(p.stat(custom, play), before + 1, "play_noteblock не вырос");
    println!("play_noteblock ok");

    // 3. bell_ring: ПКМ по колоколу.
    p.cmd(&format!(
        "setblock {} {} {} minecraft:bell[attachment=floor,facing=north]",
        BX + 2,
        by,
        BZ
    ))
    .await;
    let bell = cstat("minecraft:bell_ring");
    let before = p.stat(custom, bell);
    p.use_on([BX + 2, by, BZ], 1).await;
    assert_eq!(p.stat(custom, bell), before + 1, "bell_ring не вырос");
    println!("bell_ring ok");

    // 4. pot_flower: цветок в горшок.
    p.cmd(&format!(
        "setblock {} {} {} minecraft:flower_pot",
        BX + 3,
        by,
        BZ
    ))
    .await;
    p.cmd("give boater minecraft:poppy 1").await;
    p.select(0).await;
    let pot = cstat("minecraft:pot_flower");
    let before = p.stat(custom, pot);
    p.use_on([BX + 3, by, BZ], 1).await;
    assert_eq!(p.stat(custom, pot), before + 1, "pot_flower не вырос");
    println!("pot_flower ok");

    // 5. inspect_hopper/dispenser/dropper: открыть меню.
    for (block, stat) in [
        ("minecraft:hopper", "minecraft:inspect_hopper"),
        ("minecraft:dispenser", "minecraft:inspect_dispenser"),
        ("minecraft:dropper", "minecraft:inspect_dropper"),
    ] {
        p.cmd(&format!("setblock {} {} {} {block}", BX + 4, by, BZ))
            .await;
        let id = cstat(stat);
        let before = p.stat(custom, id);
        p.window = None;
        p.use_on([BX + 4, by, BZ], 1).await;
        assert!(p.window.is_some(), "{block}: меню не открылось");
        p.close().await;
        assert_eq!(p.stat(custom, id), before + 1, "{stat} не вырос");
    }
    println!("inspect_* ok");

    p.cmd("kill @e[type=minecraft:item]").await;
    println!("stats_simple: ok");
}
