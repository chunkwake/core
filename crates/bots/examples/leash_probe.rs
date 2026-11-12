//! Поводок: привязь к игроку, натяжение, обрыв и узел на заборе.
//!
//! `leash_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

const BX: i32 = 3600;
const BZ: i32 = 3600;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_use_item_on: i32,
    sb_interact: i32,
    sb_set_carried: i32,
    cb_chat: i32,
    cow_ty: i32,
    knot_ty: i32,
    item_ty: i32,
    chat: String,
    seq: i32,
    cow: Option<i32>,
    knot: Option<i32>,
    links: Vec<(i32, i32)>,
    removed: Vec<i32>,
    items: u32,
}

impl Probe {
    fn packet(&self, id: i32) -> Writer {
        let mut w = Writer::new();
        w.var_int(id);
        w
    }

    fn near(&self, pos: [f64; 3]) -> bool {
        (pos[0] - BX as f64 - 0.5).abs() < 20.0 && (pos[2] - BZ as f64 - 0.5).abs() < 20.0
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
                    if !self.near(pos) {
                        continue;
                    }
                    if kind == self.cow_ty {
                        self.cow.get_or_insert(entity_id);
                    } else if kind == self.knot_ty {
                        self.knot = Some(entity_id);
                    } else if kind == self.item_ty {
                        // `minecraft:item`: выпавший поводок.
                        self.items += 1;
                    }
                }
                Event::EntityLink { source, dest } => self.links.push((source, dest)),
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
        self.pump(300).await;
    }

    async fn data(&mut self, query: &str) -> String {
        self.pump(120).await;
        self.chat.clear();
        self.cmd(&format!("data get entity {query}")).await;
        if self.chat.trim().is_empty() {
            self.pump(300).await;
            self.cmd(&format!("data get entity {query}")).await;
        }
        self.chat.clone()
    }

    async fn pos(&mut self, query: &str) -> [f64; 3] {
        let mut text = self.data(query).await;
        if vec3(&text).is_none() {
            text = self.data(query).await;
        }
        vec3(&text).unwrap_or_else(|| panic!("нет Pos у {query}: {text}"))
    }

    async fn select(&mut self, slot: u16) {
        let mut w = self.packet(self.sb_set_carried);
        w.u16(slot);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(150).await;
    }

    async fn interact(&mut self, entity: i32) {
        let mut w = self.packet(self.sb_interact);
        w.var_int(entity).var_int(0).u8(0).bool(false);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(450).await;
    }

    /// ПКМ по верхней грани блока.
    async fn use_on(&mut self, pos: [i32; 3]) {
        self.seq += 1;
        let [x, y, z] = pos;
        let packed =
            ((x as i64 & 0x3FF_FFFF) << 38) | ((z as i64 & 0x3FF_FFFF) << 12) | (y as i64 & 0xFFF);
        let mut w = self.packet(self.sb_use_item_on);
        w.var_int(0)
            .i64(packed)
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

fn numbers(text: &str) -> Vec<f64> {
    let body = match (text.find('['), text.rfind(']')) {
        (Some(a), Some(b)) if b > a => &text[a + 1..b],
        _ => return Vec::new(),
    };
    body.split(',')
        .filter_map(|t| {
            t.trim()
                .trim_end_matches('d')
                .trim_end_matches('f')
                .parse()
                .ok()
        })
        .collect()
}

fn vec3(text: &str) -> Option<[f64; 3]> {
    let v = numbers(text);
    (v.len() >= 3).then(|| [v[0], v[1], v[2]])
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
    let (cow_ty, knot_ty, item_ty) = (
        ty("minecraft:cow"),
        ty("minecraft:leash_knot"),
        ty("minecraft:item"),
    );
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
        sb_use_item_on: sb("use_item_on"),
        sb_interact: sb("interact"),
        sb_set_carried: sb("set_carried_item"),
        cb_chat,
        cow_ty,
        knot_ty,
        item_ty,
        chat: String::new(),
        seq: 0,
        cow: None,
        knot: None,
        links: Vec::new(),
        removed: Vec::new(),
        items: 0,
    };
    p.pump(1200).await;

    let by = 64;
    for line in [
        "difficulty peaceful".to_string(),
        "gamemode survival".to_string(),
        "kill @e[type=minecraft:cow]".to_string(),
        "kill @e[type=minecraft:leash_knot]".to_string(),
        format!(
            "fill {} {} {} {} {} {} minecraft:air",
            BX - 12,
            by,
            BZ - 12,
            BX + 12,
            by + 6,
            BZ + 12
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            BX - 12,
            by - 1,
            BZ - 12,
            BX + 12,
            by - 1,
            BZ + 12
        ),
        format!("tp boater {} {} {}", BX as f64 + 0.5, by, BZ as f64 + 0.5),
        "give boater minecraft:lead 4".to_string(),
    ] {
        p.cmd(&line).await;
    }
    p.select(0).await;

    // 1. Привязь к игроку.
    p.cmd(&format!(
        "summon minecraft:cow {} {} {} {{NoAI:1b}}",
        BX as f64 + 1.5,
        by as f64,
        BZ as f64 + 0.5
    ))
    .await;
    p.pump(700).await;
    let cow = p.cow.expect("корова не появилась");
    p.links.clear();
    p.interact(cow).await;
    assert!(
        p.links.iter().any(|&(s, d)| s == cow && d != 0),
        "нет поводка к игроку: {:?}",
        p.links
    );
    println!("lead attached to player");

    // 2. Натяжение: игрок в 8 блоках, корову тянет к нему.
    let start = p
        .pos("@e[type=minecraft:cow,limit=1,sort=nearest] Pos")
        .await;
    p.cmd(&format!(
        "tp boater {} {} {}",
        BX as f64 + 9.5,
        by,
        BZ as f64 + 0.5
    ))
    .await;
    p.pump(1500).await;
    let pulled = p
        .pos("@e[type=minecraft:cow,limit=1,sort=nearest] Pos")
        .await;
    assert!(
        pulled[0] - start[0] > 0.2,
        "поводок не тянет: {start:?} → {pulled:?}"
    );
    println!("leash pulls cow: x {:.2} → {:.2}", start[0], pulled[0]);

    // 3. Обрыв: игрок уходит за 12 блоков, поводок падает предметом.
    p.links.clear();
    p.items = 0;
    p.cmd(&format!(
        "tp boater {} {} {}",
        BX as f64 + 21.5,
        by,
        BZ as f64 + 0.5
    ))
    .await;
    p.pump(800).await;
    assert!(
        p.links.iter().any(|&(s, d)| s == cow && d == 0),
        "поводок не оборвался: {:?}",
        p.links
    );
    assert!(p.items > 0, "оборванный поводок не выпал предметом");
    println!("lead broke and dropped");

    // 4. Узел на заборе: снова привязать и кликнуть по забору поводком.
    let cur = p
        .pos("@e[type=minecraft:cow,limit=1,sort=nearest] Pos")
        .await;
    p.cmd(&format!("tp boater {} {} {}", cur[0] + 1.0, by, cur[2]))
        .await;
    p.cmd(&format!(
        "setblock {} {} {} minecraft:oak_fence",
        BX + 6,
        by,
        BZ
    ))
    .await;
    p.links.clear();
    p.interact(cow).await;
    assert!(
        p.links.iter().any(|&(s, d)| s == cow && d != 0),
        "поводок не привязался повторно: {:?}",
        p.links
    );
    p.cmd(&format!(
        "tp boater {} {} {}",
        BX as f64 + 5.0,
        by,
        BZ as f64 + 0.5
    ))
    .await;
    p.links.clear();
    p.knot = None;
    p.use_on([BX + 6, by, BZ]).await;
    let knot = p.knot.expect("узел на заборе не появился");
    assert!(
        p.links.iter().any(|&(s, d)| s == cow && d == knot),
        "корова не привязана к узлу: links={:?} knot={knot}",
        p.links
    );
    println!("cow tied to fence knot {knot}");

    p.cmd("kill @e[type=minecraft:cow]").await;
    p.cmd(&format!(
        "tp boater {} 4 {}",
        BX as f64 + 0.5,
        BZ as f64 + 0.5
    ))
    .await;
    p.pump(400).await;
    println!("leash: ok");
}
