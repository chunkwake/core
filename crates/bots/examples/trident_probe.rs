//! Трезубец: бросок, урон, возврат по Loyalty и износ.
//!
//! `trident_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

const BX: i32 = 3700;
const BZ: i32 = 3700;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_use_item: i32,
    sb_action: i32,
    sb_set_carried: i32,
    cb_chat: i32,
    trident_ty: i32,
    item_ty: i32,
    chat: String,
    seq: i32,
    trident: Option<i32>,
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
                    if (pos[0] - BX as f64 - 0.5).abs() > 12.0
                        || (pos[2] - BZ as f64 - 0.5).abs() > 12.0
                    {
                        continue;
                    }
                    if kind == self.trident_ty {
                        self.trident = Some(entity_id);
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
        self.pump(150).await;
        self.chat.clear();
        self.cmd(&format!("data get entity {query}")).await;
        if self.chat.trim().is_empty() {
            self.pump(300).await;
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
    }

    /// `player_action` RELEASE_USE_ITEM (6).
    async fn release(&mut self) {
        self.seq += 1;
        let packed =
            ((BX as i64 & 0x3FF_FFFF) << 38) | ((BZ as i64 & 0x3FF_FFFF) << 12) | (64i64 & 0xFFF);
        let mut w = self.packet(self.sb_action);
        w.var_int(6).i64(packed).u8(1).var_int(self.seq);
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

fn last_number(text: &str) -> Option<f64> {
    text.split_whitespace()
        .rev()
        .find_map(|t| t.trim_end_matches(['f', 'b', 's']).parse().ok())
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
    let (trident_ty, item_ty) = (ty("minecraft:trident"), ty("minecraft:item"));
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
        sb_action: sb("player_action"),
        sb_set_carried: sb("set_carried_item"),
        cb_chat,
        trident_ty,
        item_ty,
        chat: String::new(),
        seq: 0,
        trident: None,
        items: 0,
        removed: Vec::new(),
    };
    p.pump(1200).await;

    let by = 64;
    for line in [
        "difficulty peaceful".to_string(),
        "gamemode survival".to_string(),
        "kill @e[type=minecraft:cow]".to_string(),
        "kill @e[type=minecraft:trident]".to_string(),
        format!(
            "fill {} {} {} {} {} {} minecraft:air",
            BX - 10,
            by,
            BZ - 10,
            BX + 10,
            by + 6,
            BZ + 10
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            BX - 10,
            by - 1,
            BZ - 10,
            BX + 10,
            by - 1,
            BZ + 10
        ),
        format!("tp boater {} {} {}", BX as f64 + 0.5, by, BZ as f64 + 0.5),
        "clear boater".to_string(),
        "give boater minecraft:trident 1".to_string(),
        "enchant boater minecraft:loyalty 3".to_string(),
    ] {
        p.cmd(&line).await;
    }
    p.select(0).await;

    // 1. Бросок в пол: воткнулся, через 5 тиков вернулся по Loyalty.
    p.trident = None;
    p.items = 0;
    p.removed.clear();
    p.use_item(0.0, 70.0).await;
    p.pump(650).await;
    p.release().await;
    p.pump(400).await;
    let thrown = p.trident.expect("трезубец не появился");
    p.pump(2200).await;
    assert!(
        p.removed.contains(&thrown),
        "трезубец не вернулся: removed={:?}",
        p.removed
    );
    assert!(p.items > 0, "трезубец не выпал предметом после возврата");
    let inv = p.data("@s Inventory").await;
    assert!(
        inv.contains("trident"),
        "трезубца нет в инвентаре после возврата: {inv}"
    );
    println!("trident returns to owner");

    // 2. Урон: трезубец бьёт корову на 8 (10 → 2).
    p.cmd(&format!(
        "summon minecraft:cow {} {} {} {{NoAI:1b}}",
        BX as f64 + 0.5,
        by as f64,
        BZ as f64 + 3.5
    ))
    .await;
    p.pump(600).await;
    let health_query = "@e[type=minecraft:cow,limit=1,sort=nearest] Health";
    let before = last_number(&p.data(health_query).await).unwrap_or(0.0);
    p.use_item(0.0, 8.0).await;
    p.pump(650).await;
    p.release().await;
    p.pump(1200).await;
    let after = last_number(&p.data(health_query).await).unwrap_or(0.0);
    assert!(
        before - after >= 7.0,
        "урон трезубца мал: {before} → {after}"
    );
    println!("trident damage: {before} → {after}");

    p.cmd("kill @e[type=minecraft:cow]").await;
    // 3. NBT и выбор по типу: `/summon minecraft:trident` с `item`/`DealtDamage`.
    p.cmd(&format!(
        "summon minecraft:trident {} {} {} {{Motion:[0.0d,0.0d,0.0d],DealtDamage:0b,item:{{id:\"minecraft:trident\",count:1}}}}",
        BX + 8,
        by,
        BZ + 8
    ))
    .await;
    p.pump(500).await;
    let nbt = p
        .data("@e[type=minecraft:trident,limit=1,sort=nearest] item")
        .await;
    assert!(
        nbt.contains("trident"),
        "у /summon нет item трезубца: {nbt}"
    );
    let dealt = p
        .data("@e[type=minecraft:trident,limit=1,sort=nearest] DealtDamage")
        .await;
    assert!(dealt.contains('0'), "DealtDamage не прочитан: {dealt}");
    p.cmd("kill @e[type=minecraft:trident]").await;
    println!("trident nbt ok");

    p.cmd(&format!(
        "tp boater {} 4 {}",
        BX as f64 + 0.5,
        BZ as f64 + 0.5
    ))
    .await;
    p.pump(400).await;
    println!("trident: ok");
}
