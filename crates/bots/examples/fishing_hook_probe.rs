//! Зацеп сущностей поплавком: боббер цепляет моба, тянет к игроку,
//! при вытягивании снимается.
//!
//! `fishing_hook_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::collections::HashSet;
use std::time::Duration;

const BX: i32 = 4700;
const BZ: i32 = 4700;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_use: i32,
    sb_select: i32,
    cb_chat: i32,
    cow_ty: i32,
    bobber_ty: i32,
    chat: String,
    seq: i32,
    cow: Option<i32>,
    bobber: Option<i32>,
    removed: HashSet<i32>,
    cow_dx: f64,
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
                Event::EntitySpawned {
                    entity_id,
                    kind,
                    pos,
                } => {
                    if kind == self.cow_ty
                        && (pos[0] - BX as f64 - 2.5).abs() < 5.0
                        && (pos[2] - BZ as f64 - 0.5).abs() < 5.0
                    {
                        self.cow = Some(entity_id);
                    } else if kind == self.bobber_ty {
                        self.bobber = Some(entity_id);
                    }
                }
                Event::EntityMoved {
                    entity_id, delta, ..
                } => {
                    if Some(entity_id) == self.cow {
                        self.cow_dx += delta[0];
                    }
                }
                Event::EntityRemoved(ids) => self.removed.extend(ids),
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

    async fn select(&mut self, slot: u16) {
        let mut w = self.packet(self.sb_select);
        w.u16(slot);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(150).await;
    }

    async fn use_item(&mut self, yaw: f32, pitch: f32) {
        self.seq += 1;
        let mut w = self.packet(self.sb_use);
        w.var_int(0).var_int(self.seq).f32(yaw).f32(pitch);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(250).await;
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
        sb_use: sb("use_item"),
        sb_select: sb("set_carried_item"),
        cb_chat,
        cow_ty: ent("minecraft:cow"),
        bobber_ty: ent("minecraft:fishing_bobber"),
        chat: String::new(),
        seq: 0,
        cow: None,
        bobber: None,
        removed: HashSet::new(),
        cow_dx: 0.0,
    };
    p.pump(2000).await;

    let by = 64;
    for line in [
        "difficulty peaceful".to_string(),
        "gamemode survival".to_string(),
        "kill @e[type=minecraft:cow]".to_string(),
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
        format!("tp boater {} {} {}", BX as f64 + 0.5, by, BZ as f64 + 0.5),
        "clear boater".to_string(),
        "give boater minecraft:fishing_rod 1".to_string(),
    ] {
        p.cmd(&line).await;
    }
    p.select(0).await;

    p.cow = None;
    p.cmd(&format!(
        "summon minecraft:cow {} {} {} {{NoAI:1b}}",
        BX as f64 + 2.5,
        by as f64,
        BZ as f64 + 0.5
    ))
    .await;
    p.pump(500).await;
    let cow = p.cow.expect("корова не появилась");

    p.bobber = None;
    p.cow_dx = 0.0;
    p.use_item(-90.0, 0.0).await;
    p.pump(3000).await;
    let bobber = p.bobber.expect("поплавок не появился");
    assert!(
        p.cow_dx < -0.01,
        "корова не подтянулась к игроку: dx={}",
        p.cow_dx
    );
    println!("bobber hook pull ok (dx={:.2})", p.cow_dx);

    p.removed.clear();
    p.use_item(-90.0, 0.0).await;
    p.pump(500).await;
    assert!(
        p.removed.contains(&bobber),
        "поплавок не снят при вытягивании: removed={:?}",
        p.removed
    );
    println!("bobber reel ok (cow {cow})");

    p.cmd("kill @e[type=minecraft:cow]").await;
    println!("fishing_hook: ok");
}
