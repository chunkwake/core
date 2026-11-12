//! Статистика: `award_stats` для использования предмета, торговли и рыбалки,
//! сохранение в `stats/<uuid>.json` и повторная отправка при входе.
//!
//! `stats_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::collections::HashMap;
use std::time::Duration;

const BX: i32 = 4100;
const BZ: i32 = 4100;
const CUSTOM: i32 = 8;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_interact: i32,
    sb_click: i32,
    sb_select_trade: i32,
    sb_use_item: i32,
    sb_set_carried: i32,
    cb_chat: i32,
    villager_ty: i32,
    chat: String,
    seq: i32,
    villager: Option<i32>,
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
                    if kind == self.villager_ty
                        && (pos[0] - BX as f64 - 0.5).abs() < 10.0
                        && (pos[2] - BZ as f64 - 0.5).abs() < 10.0
                    {
                        self.villager = Some(entity_id);
                    }
                }
                Event::OpenScreen { window, .. } => self.window = Some(window),
                Event::Stats { stats } => {
                    for (key, n) in stats {
                        let e = self.stats.entry(key).or_insert(0);
                        *e = (*e).max(n);
                    }
                }
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

    async fn interact(&mut self, entity: i32) {
        let mut w = self.packet(self.sb_interact);
        w.var_int(entity).var_int(0).u8(0).bool(false);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(500).await;
    }

    async fn use_item(&mut self, yaw: f32, pitch: f32) {
        self.seq += 1;
        let mut w = self.packet(self.sb_use_item);
        w.var_int(0).var_int(self.seq).f32(yaw).f32(pitch);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(300).await;
    }

    async fn trade_select(&mut self, idx: i32) {
        let mut w = self.packet(self.sb_select_trade);
        w.var_int(idx);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(200).await;
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
        self.pump(250).await;
    }
}

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
    let custom = |n: &str| {
        reg["minecraft:custom_stat"]["entries"][n]["protocol_id"]
            .as_i64()
            .unwrap() as i32
    };
    let item = |n: &str| {
        reg["minecraft:item"]["entries"][n]["protocol_id"]
            .as_i64()
            .unwrap() as i32
    };
    let (villager_ty, wheat) = (ty("minecraft:villager"), item("minecraft:wheat"));
    let (traded, talked, fished) = (
        custom("minecraft:traded_with_villager"),
        custom("minecraft:talked_to_villager"),
        custom("minecraft:fish_caught"),
    );
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n| table.id(State::Play, Direction::Clientbound, n).unwrap();
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    loop {
        if let Event::Position(_) = c.next_event().await.unwrap() {
            break;
        }
    }
    let cb_chat = cb("system_chat");
    c.capture_also = vec![cb_chat];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_interact: sb("interact"),
        sb_click: sb("container_click"),
        sb_select_trade: sb("select_trade"),
        sb_use_item: sb("use_item"),
        sb_set_carried: sb("set_carried_item"),
        cb_chat,
        villager_ty,
        chat: String::new(),
        seq: 0,
        villager: None,
        window: None,
        stats: HashMap::new(),
    };
    p.pump(1200).await;

    let by = 64;
    for line in [
        "difficulty peaceful".to_string(),
        "gamemode survival".to_string(),
        "kill @e[type=minecraft:villager]".to_string(),
        "kill @e[type=minecraft:fishing_bobber]".to_string(),
        format!(
            "fill {} {} {} {} {} {} minecraft:air",
            BX - 10,
            by,
            BZ - 10,
            BX + 16,
            by + 6,
            BZ + 10
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            BX - 10,
            by - 1,
            BZ - 10,
            BX + 16,
            by - 1,
            BZ + 10
        ),
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
        "give boater minecraft:wheat 20".to_string(),
    ] {
        p.cmd(&line).await;
    }
    p.select(0).await;

    // 1. `minecraft:used` по предмету в руке.
    p.use_item(0.0, 0.0).await;
    p.pump(400).await;
    assert!(
        p.stat(2, wheat) >= 1,
        "нет статистики used для пшеницы: {:?}",
        p.stats
    );
    println!("used stat ok");

    // 2. `talked_to_villager` и `traded_with_villager`.
    p.villager = None;
    p.window = None;
    p.cmd(&format!(
        "summon minecraft:villager {} {} {} {{VillagerData:{{profession:\"minecraft:farmer\",level:1}},Xp:0}}",
        BX as f64 + 4.5,
        by as f64,
        BZ as f64 + 0.5
    ))
    .await;
    p.pump(700).await;
    let villager = p.villager.expect("житель не появился");
    p.interact(villager).await;
    p.pump(300).await;
    assert!(
        p.stat(CUSTOM, talked) >= 1,
        "нет talked_to_villager: {:?}",
        p.stats
    );
    let window = p.window.expect("окно торговли не открылось");
    p.trade_select(0).await;
    p.click(window, 30).await;
    p.click(window, 0).await;
    p.click(window, 2).await;
    p.click(window, 31).await;
    p.pump(400).await;
    assert!(
        p.stat(CUSTOM, traded) >= 1,
        "нет traded_with_villager: {:?}",
        p.stats
    );
    println!("villager stats ok");

    // 3. `fish_caught`: заброс в пруд, поклёвка, вытаскивание.
    p.cmd("clear boater").await;
    p.cmd("give boater minecraft:fishing_rod 1").await;
    p.cmd("enchant boater minecraft:lure 3").await;
    p.select(0).await;
    p.use_item(-90.0, 8.0).await;
    p.pump(700).await;
    let mut biting = false;
    for _ in 0..100 {
        p.pump(250).await;
        let t = p
            .data("@e[type=minecraft:fishing_bobber,limit=1,sort=nearest] Biting")
            .await;
        if tail_number(&t) > 0.0 {
            biting = true;
            break;
        }
    }
    assert!(biting, "поклёвки не дождались");
    p.use_item(-90.0, 8.0).await;
    p.pump(1200).await;
    assert!(
        p.stat(CUSTOM, fished) >= 1,
        "нет fish_caught: {:?}",
        p.stats
    );
    println!("fishing stat ok");

    // 4. Сохранение и повторная отправка при входе.
    p.cmd("save-all").await;
    p.pump(500).await;
    let before: HashMap<(i32, i32), i32> = p.stats.clone();
    drop(p);
    tokio::time::sleep(Duration::from_millis(2000)).await;
    let mut c2 = Client::connect(&a[1], "boater", Ids::load(&table).unwrap(), 777, "26.3")
        .await
        .unwrap();
    let mut after: HashMap<(i32, i32), i32> = HashMap::new();
    let deadline = tokio::time::Instant::now() + Duration::from_millis(2500);
    while let Ok(Ok(ev)) = tokio::time::timeout_at(deadline, c2.next_event()).await {
        if let Event::Stats { stats } = ev {
            for (key, n) in stats {
                let e = after.entry(key).or_insert(0);
                *e = (*e).max(n);
            }
        }
    }
    assert!(
        after.get(&(CUSTOM, traded)).copied().unwrap_or(0) >= 1,
        "после входа статистика не пришла: before={before:?} after={after:?}"
    );
    assert!(
        after.get(&(CUSTOM, fished)).copied().unwrap_or(0) >= 1,
        "fish_caught не сохранился: {after:?}"
    );
    println!("stats persistence ok");

    println!("stats: ok");
}
