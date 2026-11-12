//! Рыбалка: заброс, поклёвка, вытаскивание добычи.
//!
//! `fishing_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

const BX: i32 = 3800;
const BZ: i32 = 3800;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_use_item: i32,
    sb_set_carried: i32,
    cb_chat: i32,
    bobber_ty: i32,
    item_ty: i32,
    chat: String,
    seq: i32,
    bobber: Option<i32>,
    items: u32,
    removed: Vec<i32>,
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
        while let Ok(Ok(ev)) = tokio::time::timeout_at(deadline, self.c.next_event()).await {
            match ev {
                Event::EntitySpawned {
                    entity_id,
                    kind,
                    pos,
                } => {
                    if (pos[0] - BX as f64 - 0.5).abs() > 20.0
                        || (pos[2] - BZ as f64 - 0.5).abs() > 20.0
                    {
                        continue;
                    }
                    if kind == self.bobber_ty {
                        self.bobber = Some(entity_id);
                    } else if kind == self.item_ty {
                        self.items += 1;
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
        let mut w = self.packet(self.sb_set_carried);
        w.u16(slot);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(150).await;
    }

    async fn use_item(&mut self, yaw: f32, pitch: f32) {
        self.seq += 1;
        let mut w = self.packet(self.sb_use_item);
        w.var_int(0).var_int(self.seq).f32(yaw).f32(pitch);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(250).await;
    }
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

/// Последнее число в ответе `/data get` (`Biting` байтом печатается как 1.0).
fn tail_number(text: &str) -> f64 {
    text.split_whitespace()
        .rev()
        .find_map(|t| {
            t.trim_end_matches(['d', 'f', 'b', 's'])
                .trim_end_matches('.')
                .parse()
                .ok()
        })
        .unwrap_or(0.0)
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
    let ty = |n: &str| {
        reg["minecraft:entity_type"]["entries"][n]["protocol_id"]
            .as_i64()
            .unwrap() as i32
    };
    let (bobber_ty, item_ty) = (ty("minecraft:fishing_bobber"), ty("minecraft:item"));
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n| table.id(State::Play, Direction::Clientbound, n).unwrap();
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    let _spawn = loop {
        if let Event::Position(p) = c.next_event().await.unwrap() {
            break p;
        }
    };
    let cb_chat = cb("system_chat");
    c.capture_also = vec![cb_chat];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_use_item: sb("use_item"),
        sb_set_carried: sb("set_carried_item"),
        cb_chat,
        bobber_ty,
        item_ty,
        chat: String::new(),
        seq: 0,
        bobber: None,
        items: 0,
        removed: Vec::new(),
    };
    p.pump(1200).await;

    let by = 64;
    for line in [
        "difficulty peaceful".to_string(),
        "gamemode survival".to_string(),
        "kill @e[type=minecraft:fishing_bobber]".to_string(),
        "kill @e[type=minecraft:item]".to_string(),
        format!(
            "fill {} {} {} {} {} {} minecraft:air",
            BX - 14,
            by,
            BZ - 14,
            BX + 14,
            by + 6,
            BZ + 14
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            BX - 14,
            by - 1,
            BZ - 14,
            BX + 14,
            by - 1,
            BZ + 14
        ),
        // Пруд: один слой воды, верх 64.888.
        format!(
            "fill {} {} {} {} {} {} minecraft:water",
            BX + 2,
            by,
            BZ - 6,
            BX + 16,
            by,
            BZ + 6
        ),
        format!("tp boater {} {} {}", BX as f64 + 0.5, by, BZ as f64 + 0.5),
        "clear boater".to_string(),
        "give boater minecraft:fishing_rod 1".to_string(),
        "enchant boater minecraft:lure 3".to_string(),
    ] {
        p.cmd(&line).await;
    }
    p.select(0).await;

    // Заброс в пруд (на восток, чуть вниз).
    p.bobber = None;
    p.use_item(-90.0, 8.0).await;
    p.pump(700).await;
    let bobber = p.bobber.expect("поплавок не появился");
    let bp = p
        .data("@e[type=minecraft:fishing_bobber,limit=1,sort=nearest] Pos")
        .await;
    let state = p
        .data("@e[type=minecraft:fishing_bobber,limit=1,sort=nearest] Biting")
        .await;
    println!("bobber cast: {bobber} at {bp}; {state}");

    // Ждём поклёвку (`Biting`/`Nibble`), до ~25 секунд.
    let mut biting = false;
    for _ in 0..100 {
        p.pump(250).await;
        let text = p
            .data("@e[type=minecraft:fishing_bobber,limit=1,sort=nearest] Biting")
            .await;
        if tail_number(&text) > 0.0 {
            biting = true;
            break;
        }
    }
    assert!(biting, "поклёвки не дождались");
    println!("bobber bites");

    // Вытаскиваем: поплавок исчезает, добыча падает предметом.
    p.items = 0;
    p.removed.clear();
    p.use_item(-90.0, 8.0).await;
    p.pump(1200).await;
    assert!(
        p.removed.contains(&bobber),
        "поплавок не исчез: removed={:?}",
        p.removed
    );
    assert!(p.items > 0, "добыча не выпала предметом");
    let inv = p.data("@s Inventory").await;
    assert!(
        inv.contains("fishing_rod"),
        "удочка пропала после вытаскивания: {inv}"
    );
    println!("fishing catch: {} item(s)", p.items);

    p.cmd(&format!(
        "tp boater {} 4 {}",
        BX as f64 + 0.5,
        BZ as f64 + 0.5
    ))
    .await;
    p.pump(300).await;
    println!("fishing: ok");
}
