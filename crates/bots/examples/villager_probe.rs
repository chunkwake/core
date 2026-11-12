//! Житель и торговля: окно `merchant`, выбор сделки и обмен.
//!
//! `villager_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

const BX: i32 = 3900;
const BZ: i32 = 3900;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_interact: i32,
    sb_click: i32,
    sb_select_trade: i32,
    sb_set_carried: i32,
    cb_chat: i32,
    villager_ty: i32,
    chat: String,
    seq: i32,
    villager: Option<i32>,
    window: Option<i32>,
    menu: i32,
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
                    if (pos[0] - BX as f64 - 0.5).abs() > 10.0
                        || (pos[2] - BZ as f64 - 0.5).abs() > 10.0
                    {
                        continue;
                    }
                    if kind == self.villager_ty {
                        self.villager = Some(entity_id);
                    }
                }
                Event::OpenScreen { window, menu } => {
                    self.window = Some(window);
                    self.menu = menu;
                }
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

    /// Клик по слоту окна: `PICKUP`, левая кнопка.
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
        .find_map(|t| t.trim_end_matches(['d', 'f', 'b', 's']).parse().ok())
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
    let villager_ty = ty("minecraft:villager");
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
    let menu_merchant = 0;
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_interact: sb("interact"),
        sb_click: sb("container_click"),
        sb_select_trade: sb("select_trade"),
        sb_set_carried: sb("set_carried_item"),
        cb_chat,
        villager_ty,
        chat: String::new(),
        seq: 0,
        villager: None,
        window: None,
        menu: menu_merchant,
    };
    p.pump(1200).await;

    let by = 64;
    for line in [
        "difficulty peaceful".to_string(),
        "gamemode survival".to_string(),
        "kill @e[type=minecraft:villager]".to_string(),
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
        "give boater minecraft:wheat 20".to_string(),
    ] {
        p.cmd(&line).await;
    }
    p.select(0).await;

    // 1. Житель и открытие окна торговли.
    p.villager = None;
    p.window = None;
    p.cmd(&format!(
        "summon minecraft:villager {} {} {} {{VillagerData:{{profession:\"minecraft:farmer\",level:1}},Xp:0}}",
        BX as f64 + 2.5,
        by as f64,
        BZ as f64 + 0.5
    ))
    .await;
    p.pump(700).await;
    let villager = p.villager.expect("житель не появился");
    p.interact(villager).await;
    let window = p.window.expect("окно торговли не открылось");
    println!(
        "merchant window {window} (menu {}) for villager {villager}",
        p.menu
    );

    // 2. Первая сделка: 20 пшеницы → 1 изумруд.
    p.trade_select(0).await;
    p.click(window, 30).await; // пшеница из хотбара в курсор
    p.click(window, 0).await; // в слот покупки
    p.click(window, 2).await; // забрать результат
    p.click(window, 31).await; // изумруд в хотбар
    p.pump(300).await;

    let inv = p.data("@s Inventory").await;
    assert!(
        inv.contains("emerald"),
        "изумруда нет в инвентаре после сделки: {inv}"
    );
    let xp = p
        .data("@e[type=minecraft:villager,limit=1,sort=nearest] Xp")
        .await;
    assert_eq!(last_number(&xp), Some(2.0), "опыт жителя не вырос: {xp}");
    let offers = p
        .data("@e[type=minecraft:villager,limit=1,sort=nearest] Offers")
        .await;
    assert!(
        offers.contains("uses:1"),
        "сделка не отмечена использованной: {offers}"
    );
    println!("trade: 20 wheat -> emerald, xp=2, uses=1");

    // 3. Выбор другой сделки (изумруд → хлеб) и повторный обмен.
    p.trade_select(2).await;
    p.click(window, 31).await; // изумруд в курсор
    p.click(window, 0).await;
    p.click(window, 2).await; // забрать хлеб
    p.click(window, 32).await;
    p.pump(300).await;
    let inv = p.data("@s Inventory").await;
    assert!(
        inv.contains("bread"),
        "хлеба нет после второй сделки: {inv}"
    );
    println!("second trade: emerald -> bread");

    p.cmd("kill @e[type=minecraft:villager]").await;
    p.cmd(&format!(
        "tp boater {} 4 {}",
        BX as f64 + 0.5,
        BZ as f64 + 0.5
    ))
    .await;
    p.pump(400).await;
    println!("villager: ok");
}
