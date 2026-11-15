//! Сделки жителей по `TradeSet` для всех профессий/уровней: броня
//! (armorer, уровень 1), свечи (librarian, уровень 5) и зачарованная книга
//! (librarian, уровни 1-4, `enchant_randomly`).
//!
//! `trade_set_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::collections::HashMap;
use std::time::Duration;

const X: i32 = 3264;
const Z: i32 = 8;
const Y: i32 = 100;

struct OfferInfo {
    buy_item: i32,
    buy_count: i32,
    sell_item: i32,
    sell_count: i32,
    max_uses: i32,
    xp: i32,
}

struct Probe {
    c: Client,
    sb_command: i32,
    sb_interact: i32,
    sb_close: i32,
    cb_chat: i32,
    cb_add: i32,
    cb_offers: i32,
    chat: String,
    villager_type: i32,
    villagers: HashMap<i32, [f64; 3]>,
    window: Option<i32>,
    offers: Vec<OfferInfo>,
}

impl Probe {
    fn packet(&self, id: i32) -> Writer {
        let mut w = Writer::new();
        w.var_int(id);
        w
    }

    fn parse_offers(&mut self, body: &[u8]) {
        self.offers.clear();
        let mut r = Reader::new(body);
        let (Ok(_window), Ok(count)) = (r.var_int(), r.var_int()) else {
            return;
        };
        for _ in 0..count {
            let (Ok(buy_item), Ok(buy_count), Ok(_pred)) = (r.var_int(), r.var_int(), r.var_int())
            else {
                return;
            };
            let Ok(sell) = chunkwake_protocol::inventory::read_stack(&mut r) else {
                return;
            };
            let Some(sell) = sell else { return };
            let Ok(has_b) = r.bool() else { return };
            if has_b && (r.var_int().is_err() || r.var_int().is_err() || r.var_int().is_err()) {
                return;
            }
            if r.bool().is_err() {
                return;
            }
            let Ok(_uses) = read_i32(&mut r) else { return };
            let Ok(max_uses) = read_i32(&mut r) else {
                return;
            };
            let Ok(xp) = read_i32(&mut r) else { return };
            if read_i32(&mut r).is_err() || r.f32().is_err() || read_i32(&mut r).is_err() {
                return;
            }
            self.offers.push(OfferInfo {
                buy_item,
                buy_count,
                sell_item: sell.item,
                sell_count: sell.count,
                max_uses,
                xp,
            });
        }
    }

    fn apply(&mut self, ev: Event) {
        match ev {
            Event::EntitySpawned {
                entity_id,
                kind,
                pos,
            } => {
                if kind == self.villager_type {
                    self.villagers.insert(entity_id, pos);
                }
            }
            Event::OpenScreen { window, .. } => self.window = Some(window),
            _ => {}
        }
    }

    fn drain(&mut self) {
        for (id, body) in std::mem::take(&mut self.c.captured_by) {
            let mut r = Reader::new(&body);
            if id == self.cb_add {
                let (Ok(entity), Ok(_uuid)) = (r.var_int(), r.bytes(16)) else {
                    continue;
                };
                let Ok(kind) = r.var_int() else { continue };
                let (Ok(x), Ok(y), Ok(z)) = (r.f64(), r.f64(), r.f64()) else {
                    continue;
                };
                if kind == self.villager_type {
                    self.villagers.insert(entity, [x, y, z]);
                }
            } else if id == self.cb_offers {
                self.parse_offers(&body);
            } else if id == self.cb_chat
                && let Ok(component) = text::read_component(&mut r)
            {
                flatten(&component, &mut self.chat);
                self.chat.push(' ');
            }
        }
    }

    fn nearest_villager(&self, at: [f64; 3]) -> Option<i32> {
        self.villagers
            .iter()
            .min_by(|a, b| {
                let da = (a.1[0] - at[0]).powi(2) + (a.1[2] - at[2]).powi(2);
                let db = (b.1[0] - at[0]).powi(2) + (b.1[2] - at[2]).powi(2);
                da.total_cmp(&db)
            })
            .map(|(&id, _)| id)
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
            self.apply(ev);
        }
        self.drain();
    }

    async fn cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(200).await;
    }

    async fn data(&mut self, query: &str) -> String {
        self.pump(120).await;
        self.chat.clear();
        self.cmd(&format!("data get entity {query}")).await;
        self.chat.clone()
    }

    async fn open_trade(&mut self, entity: i32) -> i32 {
        for _ in 0..8 {
            self.window = None;
            let mut w = self.packet(self.sb_interact);
            w.var_int(entity).var_int(0).u8(0).bool(false);
            self.c.send_raw(&w.buf).await.unwrap();
            self.pump(400).await;
            if let Some(window) = self.window {
                return window;
            }
        }
        panic!("окно торговли не открылось (entity {entity})");
    }

    async fn close(&mut self, window: i32) {
        let mut w = self.packet(self.sb_close);
        w.var_int(window);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(200).await;
    }
}

fn read_i32(r: &mut Reader) -> Result<i32, chunkwake_protocol::ProtocolError> {
    let bytes = r.bytes(4)?;
    Ok(i32::from_be_bytes(bytes.try_into().unwrap()))
}

fn flatten(v: &serde_json::Value, out: &mut String) {
    if let Some(s) = v.as_str() {
        out.push_str(s);
        return;
    }
    if let Some(s) = v.get("text").and_then(serde_json::Value::as_str) {
        out.push_str(s);
    }
    for key in ["with", "extra"] {
        if let Some(args) = v.get(key).and_then(serde_json::Value::as_array) {
            for arg in args {
                flatten(arg, out);
                out.push(' ');
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let reg: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::path::Path::new(&a[2]).with_file_name("registries.json"))
            .unwrap(),
    )
    .unwrap();
    let pid = |reg_name: &str, name: &str| {
        reg[reg_name]["entries"][name]["protocol_id"]
            .as_i64()
            .unwrap() as i32
    };
    // Как `world_setup`: без ID компонентов `read_stack` не разбирает
    // зачарованные книги и останавливает чтение `merchant_offers`.
    {
        use chunkwake_protocol::components as c;
        use std::sync::atomic::{AtomicI32, Ordering};
        let set = |atom: &AtomicI32, name: &str| {
            if let Some(id) =
                reg["minecraft:data_component_type"]["entries"][name]["protocol_id"].as_i64()
            {
                atom.store(id as i32, Ordering::Relaxed);
            }
        };
        let order = Ordering::Relaxed;
        if let Some(id) =
            reg["minecraft:data_component_type"]["entries"]["minecraft:damage"]["protocol_id"]
                .as_i64()
        {
            chunkwake_protocol::inventory::DAMAGE_COMPONENT.store(id as i32, order);
        }
        set(&c::DYED_COLOR_COMPONENT, "minecraft:dyed_color");
        set(&c::ENCHANTMENTS_COMPONENT, "minecraft:enchantments");
        set(&c::CUSTOM_NAME_COMPONENT, "minecraft:custom_name");
        set(&c::LORE_COMPONENT, "minecraft:lore");
        set(
            &c::STORED_ENCHANTMENTS_COMPONENT,
            "minecraft:stored_enchantments",
        );
        set(&c::CUSTOM_DATA_COMPONENT, "minecraft:custom_data");
        set(
            &c::CHARGED_PROJECTILES_COMPONENT,
            "minecraft:charged_projectiles",
        );
        set(&c::CONTAINER_COMPONENT, "minecraft:container");
        set(&c::POT_DECORATIONS_COMPONENT, "minecraft:pot_decorations");
        set(&c::REPAIR_COST_COMPONENT, "minecraft:repair_cost");
        set(&c::TRIM_COMPONENT, "minecraft:trim");
        set(&c::BANNER_PATTERNS_COMPONENT, "minecraft:banner_patterns");
        set(
            &c::WRITTEN_BOOK_CONTENT_COMPONENT,
            "minecraft:written_book_content",
        );
        set(
            &c::WRITABLE_BOOK_CONTENT_COMPONENT,
            "minecraft:writable_book_content",
        );
        set(&c::FIREWORKS_COMPONENT, "minecraft:fireworks");
        set(&c::MAP_ID_COMPONENT, "minecraft:map_id");
        set(
            &c::MAP_POST_PROCESSING_COMPONENT,
            "minecraft:map_post_processing",
        );
    }
    let mut names: HashMap<i32, &str> = HashMap::new();
    for name in [
        "minecraft:coal",
        "minecraft:emerald",
        "minecraft:iron_boots",
        "minecraft:iron_helmet",
        "minecraft:iron_leggings",
        "minecraft:iron_chestplate",
        "minecraft:red_candle",
        "minecraft:yellow_candle",
        "minecraft:enchanted_book",
    ] {
        names.insert(pid("minecraft:item", name), name);
    }

    let ids = Ids::load(&table).unwrap();
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n| table.id(State::Play, Direction::Clientbound, n).unwrap();
    let cb_chat = cb("system_chat");
    let cb_add = cb("add_entity");
    let cb_offers = cb("merchant_offers");
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    c.capture_also = vec![cb_chat, cb_add, cb_offers];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_interact: sb("interact"),
        sb_close: sb("container_close"),
        cb_chat,
        cb_add,
        cb_offers,
        chat: String::new(),
        villager_type: pid("minecraft:entity_type", "minecraft:villager"),
        villagers: HashMap::new(),
        window: None,
        offers: Vec::new(),
    };
    p.pump(1500).await;

    for line in [
        "difficulty peaceful".to_string(),
        "gamemode survival".to_string(),
        "time set day".to_string(),
        "effect clear @s".to_string(),
        "kill @e[type=!minecraft:player,distance=..96]".to_string(),
        format!("execute in minecraft:overworld run tp @s {X}.5 {Y} {Z}.5 180 20"),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            X - 8,
            Y - 1,
            Z - 8,
            X + 16,
            Y - 1,
            Z + 8
        ),
        format!(
            "fill {} {Y} {} {} {} {} minecraft:air",
            X - 8,
            Z - 8,
            X + 16,
            Y + 5,
            Z + 8
        ),
        "clear @s".to_string(),
    ] {
        p.cmd(&line).await;
    }
    p.c.confirm_loaded().await.unwrap();
    p.villagers.clear();

    // Armorer уровня 1 (плоский мир, вариант plains).
    p.cmd(&format!(
        "summon minecraft:villager {}.5 {Y} {}.5 {{NoAI:1b,VillagerData:{{type:\"minecraft:plains\",profession:\"minecraft:armorer\",level:1}},Xp:0}}",
        X + 2,
        Z
    ))
    .await;
    // Librarian уровня 5: сделки свечей.
    p.cmd(&format!(
        "summon minecraft:villager {}.5 {Y} {}.5 {{NoAI:1b,VillagerData:{{type:\"minecraft:plains\",profession:\"minecraft:librarian\",level:5}},Xp:0}}",
        X + 5,
        Z
    ))
    .await;
    p.pump(600).await;

    // 1. Armorer 1: ровно 2 сделки из {coal→emerald, emerald→iron armor}.
    let armorer = p
        .nearest_villager([f64::from(X + 2), f64::from(Y), f64::from(Z)])
        .expect("armorer не появился");
    p.cmd(&format!("tp @s {}.5 {Y} {}.5 180 20", X + 2, Z + 2))
        .await;
    p.c.confirm_loaded().await.unwrap();
    let window = p.open_trade(armorer).await;
    p.pump(250).await;
    assert_eq!(p.offers.len(), 2, "у armorer не 2 сделки");
    for offer in &p.offers {
        let buy = names.get(&offer.buy_item).copied().unwrap_or("?");
        let sell = names.get(&offer.sell_item).copied().unwrap_or("?");
        let ok = matches!(
            (buy, offer.buy_count, sell, offer.sell_count, offer.max_uses),
            ("minecraft:coal", 15, "minecraft:emerald", 1, 16)
                | ("minecraft:emerald", 4, "minecraft:iron_boots", 1, 12)
                | ("minecraft:emerald", 5, "minecraft:iron_helmet", 1, 12)
                | ("minecraft:emerald", 7, "minecraft:iron_leggings", 1, 12)
                | ("minecraft:emerald", 9, "minecraft:iron_chestplate", 1, 12)
        );
        assert!(
            ok,
            "сделка armorer не из набора: {buy} x{} -> {sell} x{} (max {})",
            offer.buy_count, offer.sell_count, offer.max_uses
        );
    }
    println!("armorer level 1 trade set ok (2 offers)");
    p.close(window).await;

    // 2. Librarian 5: уровень 5 даёт две сделки со свечами (amount 3, в теге 2).
    let librarian = p
        .nearest_villager([f64::from(X + 5), f64::from(Y), f64::from(Z)])
        .expect("librarian не появился");
    p.cmd(&format!("tp @s {}.5 {Y} {}.5 180 20", X + 5, Z + 2))
        .await;
    p.c.confirm_loaded().await.unwrap();
    let window = p.open_trade(librarian).await;
    p.pump(250).await;
    let candles: Vec<&OfferInfo> = p
        .offers
        .iter()
        .filter(|o| {
            matches!(
                names.get(&o.sell_item).copied(),
                Some("minecraft:red_candle") | Some("minecraft:yellow_candle")
            )
        })
        .collect();
    assert_eq!(candles.len(), 2, "у librarian 5 не две сделки со свечами");
    for offer in candles {
        assert_eq!(offer.buy_count, 3, "свечи не за 3 изумруда");
        assert_eq!(offer.max_uses, 12, "maxUses свечей не 12");
        assert_eq!(offer.xp, 30, "xp свечей не 30");
        assert_eq!(offer.sell_count, 1, "свечей не 1");
    }
    assert!(
        p.offers.len() >= 8,
        "у librarian 5 не все уровни: {} сделок",
        p.offers.len()
    );
    println!(
        "librarian level 5 trade set ok ({} offers total, 2 candles)",
        p.offers.len()
    );
    p.close(window).await;

    // 3. Librarian 4: enchant_randomly даёт зачарованную книгу
    // (stored_enchantments) и цену из additional_trade_cost.
    let book_spot = format!(
        "@e[type=minecraft:villager,x={}.5,y={Y},z={}.5,distance=..2,sort=nearest,limit=1]",
        X + 9,
        Z
    );
    let mut book_nbt = String::new();
    for attempt in 0..6 {
        p.cmd(&format!(
            "kill @e[type=minecraft:villager,x={}.5,y={Y},z={}.5,distance=..2]",
            X + 9,
            Z
        ))
        .await;
        p.cmd(&format!(
            "summon minecraft:villager {}.5 {Y} {}.5 {{NoAI:1b,VillagerData:{{type:\"minecraft:plains\",profession:\"minecraft:librarian\",level:4}},Xp:0}}",
            X + 9,
            Z
        ))
        .await;
        p.pump(400).await;
        p.villagers.clear();
        p.pump(300).await;
        book_nbt = p.data(&format!("{book_spot} Offers")).await;
        if book_nbt.contains("stored_enchantments") {
            break;
        }
        eprintln!("librarian 4 attempt {attempt}: no enchanted book yet");
    }
    assert!(
        book_nbt.contains("stored_enchantments"),
        "в сделках librarian 4 нет зачарованной книги: {book_nbt}"
    );
    assert!(
        book_nbt.contains("enchanted_book"),
        "в сделках librarian 4 нет enchanted_book: {book_nbt}"
    );
    println!("librarian level 4 enchanted book trade ok (stored_enchantments)");
    println!("trade_set_probe: ok");
}
