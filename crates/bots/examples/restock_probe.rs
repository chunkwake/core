//! Пополнение запасов у рабочего блока и странствующий торговец.
//!
//! `restock_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

const BX: i32 = 4000;
const BZ: i32 = 4000;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_interact: i32,
    sb_click: i32,
    sb_select_trade: i32,
    sb_close: i32,
    sb_set_carried: i32,
    cb_chat: i32,
    villager_ty: i32,
    trader_ty: i32,
    chat: String,
    seq: i32,
    villager: Option<i32>,
    trader: Option<i32>,
    window: Option<i32>,
    removed: Vec<i32>,
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

fn near(pos: [f64; 3]) -> bool {
    (pos[0] - BX as f64 - 0.5).abs() < 12.0 && (pos[2] - BZ as f64 - 0.5).abs() < 12.0
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
                    if !near(pos) {
                        continue;
                    }
                    if kind == self.villager_ty {
                        self.villager = Some(entity_id);
                    } else if kind == self.trader_ty {
                        self.trader = Some(entity_id);
                    }
                }
                Event::EntityRemoved(ids) => self.removed.extend(ids),
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

    async fn trade_select(&mut self, idx: i32) {
        let mut w = self.packet(self.sb_select_trade);
        w.var_int(idx);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(200).await;
    }

    async fn close(&mut self, window: i32) {
        let mut w = self.packet(self.sb_close);
        w.var_int(window);
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
    let (villager_ty, trader_ty) = (ty("minecraft:villager"), ty("minecraft:wandering_trader"));
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
        sb_close: sb("container_close"),
        sb_set_carried: sb("set_carried_item"),
        cb_chat,
        villager_ty,
        trader_ty,
        chat: String::new(),
        seq: 0,
        villager: None,
        trader: None,
        window: None,
        removed: Vec::new(),
    };
    p.pump(1200).await;

    let by = 64;
    for line in [
        "difficulty peaceful".to_string(),
        "gamemode survival".to_string(),
        "kill @e[type=minecraft:villager]".to_string(),
        "kill @e[type=minecraft:wandering_trader]".to_string(),
        format!(
            "fill {} {} {} {} {} {} minecraft:air",
            BX - 10,
            by,
            BZ - 10,
            BX + 10,
            by + 5,
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
        "give boater minecraft:wheat 20".to_string(),
        format!("setblock {} {} {} minecraft:composter", BX + 2, by - 1, BZ),
    ] {
        p.cmd(&line).await;
    }
    p.select(0).await;

    // 1. Фермер у компостера: сделка тратит запас.
    p.cmd(&format!(
        "summon minecraft:villager {} {} {} {{VillagerData:{{profession:\"minecraft:farmer\",level:1}},Xp:0}}",
        BX as f64 + 1.2,
        by as f64,
        BZ as f64 + 0.5
    ))
    .await;
    p.pump(700).await;
    let villager = p.villager.expect("житель не появился");
    p.interact(villager).await;
    let window = p.window.expect("окно торговли не открылось");
    p.trade_select(0).await;
    p.click(window, 30).await;
    p.click(window, 0).await;
    p.click(window, 2).await;
    p.click(window, 31).await;
    p.pump(300).await;
    p.close(window).await;
    let offers = p
        .data("@e[type=minecraft:villager,limit=1,sort=nearest] Offers")
        .await;
    assert!(
        offers.contains("uses:1"),
        "сделка не использована: {offers}"
    );
    println!("trade used one stock");

    // 2. Ждём рабочую проверку (300 тиков) и пополнение.
    let mut restocked = false;
    for _ in 0..45 {
        p.pump(1000).await;
        let offers = p
            .data("@e[type=minecraft:villager,limit=1,sort=nearest] Offers")
            .await;
        if offers.contains("uses:0") && !offers.contains("uses:1") {
            restocked = true;
            break;
        }
    }
    assert!(restocked, "запасы не пополнились у компостера");
    let today = p
        .data("@e[type=minecraft:villager,limit=1,sort=nearest] RestocksToday")
        .await;
    let restocks = today
        .split_whitespace()
        .rev()
        .find_map(|t| t.trim_end_matches(['d', 'f', 'b', 's']).parse::<f64>().ok())
        .unwrap_or(0.0);
    assert!(
        (restocks - 1.0).abs() < f64::EPSILON,
        "RestocksToday не вырос: {today}"
    );
    println!("restocked at composter, RestocksToday=1");

    // 3. Странствующий торговец: окно, DespawnDelay и сделка из NBT.
    p.trader = None;
    p.window = None;
    p.cmd(&format!(
        "summon minecraft:wandering_trader {} {} {} {{DespawnDelay:0,Offers:{{Recipes:[{{buy:{{id:\"minecraft:emerald\",count:1}},sell:{{id:\"minecraft:bread\",count:2}},maxUses:4,xp:0}}]}}}}",
        BX as f64 - 1.2,
        by as f64,
        BZ as f64 + 0.5
    ))
    .await;
    p.pump(700).await;
    let trader = p.trader.expect("торговец не появился");
    let before = p
        .data("@e[type=minecraft:wandering_trader,limit=1,sort=nearest] Offers")
        .await;
    println!("trader offers before: {before}");
    p.interact(trader).await;
    let window = p.window.expect("окно торговца не открылось");
    p.cmd("clear boater").await;
    p.cmd("give boater minecraft:emerald 5").await;
    p.select(0).await;
    p.trade_select(0).await;
    p.click(window, 30).await;
    p.click(window, 0).await;
    p.click(window, 2).await;
    p.click(window, 31).await;
    p.pump(400).await;
    let inv = p.data("@s Inventory").await;
    assert!(inv.contains("bread"), "у торговца не куплен хлеб: {inv}");
    let off = p
        .data("@e[type=minecraft:wandering_trader,limit=1,sort=nearest] Offers")
        .await;
    assert!(off.contains("uses:1"), "сделка торговца не учтена: {off}");
    println!("wandering trader trade ok");
    p.close(window).await;

    // 4. Исчезновение по DespawnDelay.
    p.trader = None;
    p.removed.clear();
    p.cmd(&format!(
        "summon minecraft:wandering_trader {} {} {} {{DespawnDelay:30}}",
        BX as f64 + 3.5,
        by as f64,
        BZ as f64 + 2.5
    ))
    .await;
    p.pump(500).await;
    let doomed = p.trader.expect("второй торговец не появился");
    p.pump(2500).await;
    assert!(
        p.removed.contains(&doomed),
        "торговец не исчез по DespawnDelay: removed={:?}",
        p.removed
    );
    println!("wandering trader despawn ok");

    p.cmd("kill @e[type=minecraft:villager]").await;
    p.cmd("kill @e[type=minecraft:wandering_trader]").await;
    p.cmd(&format!(
        "tp boater {} 4 {}",
        BX as f64 + 0.5,
        BZ as f64 + 0.5
    ))
    .await;
    p.pump(400).await;
    println!("restock: ok");
}
