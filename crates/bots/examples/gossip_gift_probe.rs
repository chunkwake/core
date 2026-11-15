//! Репутация жителей (`GossipContainer`) и подарок железного голема
//! (`OfferFlowerGoal`) медному голему.
//!
//! `gossip_gift_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

const X: i32 = 3104;
const Z: i32 = 8;
const Y: i32 = 100;
/// Арена подарка — вдали от жителей.
const GX: i32 = X + 40;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_interact: i32,
    sb_click: i32,
    sb_select_trade: i32,
    sb_close: i32,
    cb_chat: i32,
    cb_add: i32,
    cb_event: i32,
    cb_equip: i32,
    cb_offers: i32,
    chat: String,
    villager_type: i32,
    golem_type: i32,
    copper_type: i32,
    poppy: i32,
    window: Option<i32>,
    first_offer: Option<(i32, i32)>,
    /// Все жители: entity id -> позиция спавна.
    villagers: HashMap<i32, [f64; 3]>,
    golems: HashSet<i32>,
    copper: Option<i32>,
    offer_events: HashSet<i32>,
    gift_done: bool,
    poppy_on_copper: bool,
    seq: i32,
}

impl Probe {
    fn packet(&self, id: i32) -> Writer {
        let mut w = Writer::new();
        w.var_int(id);
        w
    }

    fn parse_offers(&mut self, body: &[u8]) {
        let mut r = Reader::new(body);
        let (Ok(_window), Ok(count)) = (r.var_int(), r.var_int()) else {
            return;
        };
        for i in 0..count {
            let (Ok(_buy_item), Ok(buy_count), Ok(_pred)) = (r.var_int(), r.var_int(), r.var_int())
            else {
                return;
            };
            if chunkwake_protocol::inventory::read_stack(&mut r).is_err() {
                return;
            }
            let Ok(has_b) = r.bool() else { return };
            if has_b && (r.var_int().is_err() || r.var_int().is_err() || r.var_int().is_err()) {
                return;
            }
            if r.bool().is_err() {
                return;
            }
            let Ok(_uses) = read_i32(&mut r) else { return };
            let Ok(_max) = read_i32(&mut r) else { return };
            let Ok(_xp) = read_i32(&mut r) else { return };
            let Ok(price_diff) = read_i32(&mut r) else {
                return;
            };
            if i == 0 {
                self.first_offer = Some((buy_count, price_diff));
            }
            if r.f32().is_err() || read_i32(&mut r).is_err() {
                return;
            }
        }
    }

    fn add_entity(&mut self, entity: i32, kind: i32, _pos: [f64; 3]) {
        if kind == self.golem_type {
            self.golems.insert(entity);
        } else if kind == self.copper_type {
            self.copper = Some(entity);
        }
    }

    fn apply(&mut self, ev: Event) {
        match ev {
            Event::EntitySpawned {
                entity_id,
                kind,
                pos,
            } => self.add_entity(entity_id, kind, pos),
            Event::OpenScreen { window, .. } => self.window = Some(window),
            _ => {}
        }
    }

    fn drain(&mut self) {
        // `entity_type` для распознавания жителей.
        for (id, body) in std::mem::take(&mut self.c.captured_by) {
            let mut r = Reader::new(&body);
            if id == self.cb_chat {
                if let Ok(component) = text::read_component(&mut r) {
                    flatten(&component, &mut self.chat);
                    self.chat.push(' ');
                }
            } else if id == self.cb_offers {
                self.parse_offers(&body);
            } else if id == self.cb_event {
                let (Ok(entity), Ok(event)) = (read_i32(&mut r), r.u8()) else {
                    continue;
                };
                if self.golems.contains(&entity) {
                    self.offer_events.insert(event as i32);
                    if event == 34 {
                        self.gift_done = true;
                    }
                }
            } else if id == self.cb_equip {
                let Ok(entity) = r.var_int() else { continue };
                while let Ok(slot) = r.u8() {
                    let Ok(stack) = chunkwake_protocol::inventory::read_stack(&mut r) else {
                        break;
                    };
                    if slot & 0x7F == 7
                        && Some(entity) == self.copper
                        && stack.is_some_and(|s| s.item == self.poppy)
                    {
                        self.poppy_on_copper = true;
                    }
                    if slot & 0x80 == 0 {
                        break;
                    }
                }
            } else if id == self.cb_add {
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
                self.add_entity(entity, kind, [x, y, z]);
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

    async fn cmd_quick(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(40).await;
    }

    async fn data(&mut self, query: &str) -> String {
        self.pump(120).await;
        self.chat.clear();
        self.cmd(&format!("data get entity {query}")).await;
        self.chat.clone()
    }

    async fn interact(&mut self, entity: i32) {
        let mut w = self.packet(self.sb_interact);
        w.var_int(entity).var_int(0).u8(0).bool(false);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(500).await;
    }

    /// ПКМ по жителю и ожидание `open_screen` (сервер может прислать его
    /// с задержкой на тике/автосейве).
    async fn open_trade(&mut self, entity: i32) -> i32 {
        for _ in 0..8 {
            self.window = None;
            self.interact(entity).await;
            if let Some(window) = self.window {
                return window;
            }
            self.pump(250).await;
        }
        panic!("окно торговли не открылось (entity {entity})");
    }

    async fn close(&mut self, window: i32) {
        let mut w = self.packet(self.sb_close);
        w.var_int(window);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(250).await;
    }

    async fn select_trade(&mut self, idx: i32) {
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
    let ids = Ids::load(&table).unwrap();
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n| table.id(State::Play, Direction::Clientbound, n).unwrap();
    let cb_chat = cb("system_chat");
    let cb_add = cb("add_entity");
    let cb_event = cb("entity_event");
    let cb_equip = cb("set_equipment");
    let cb_offers = cb("merchant_offers");
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    c.capture_also = vec![cb_chat, cb_add, cb_event, cb_equip, cb_offers];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_interact: sb("interact"),
        sb_click: sb("container_click"),
        sb_select_trade: sb("select_trade"),
        sb_close: sb("container_close"),
        cb_chat,
        cb_add,
        cb_event,
        cb_equip,
        cb_offers,
        chat: String::new(),
        villager_type: pid("minecraft:entity_type", "minecraft:villager"),
        golem_type: pid("minecraft:entity_type", "minecraft:iron_golem"),
        copper_type: pid("minecraft:entity_type", "minecraft:copper_golem"),
        poppy: pid("minecraft:item", "minecraft:poppy"),
        window: None,
        first_offer: None,
        villagers: HashMap::new(),
        golems: HashSet::new(),
        copper: None,
        offer_events: HashSet::new(),
        gift_done: false,
        poppy_on_copper: false,
        seq: 0,
    };
    p.pump(1500).await;

    for line in [
        "difficulty peaceful".to_string(),
        "gamemode survival".to_string(),
        "time set day".to_string(),
        "weather clear".to_string(),
        "effect clear @s".to_string(),
        "kill @e[type=!minecraft:player,distance=..96]".to_string(),
        format!("execute in minecraft:overworld run tp @s {X}.5 {Y} {Z}.5 180 20"),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            X - 8,
            Y - 1,
            Z - 8,
            X + 56,
            Y - 1,
            Z + 8
        ),
        format!(
            "fill {} {Y} {} {} {} {} minecraft:air",
            X - 8,
            Z - 8,
            X + 56,
            Y + 5,
            Z + 8
        ),
        "clear @s".to_string(),
    ] {
        p.cmd(&line).await;
    }
    p.c.confirm_loaded().await.unwrap();
    // `/kill` мог снести прежних жителей: их id больше не нужны.
    p.villagers.clear();
    p.golems.clear();
    p.copper = None;

    // Фермер (жертва/торговец) и свидетель в трёх блоках.
    for offset in [2, 5] {
        p.cmd(&format!(
            "summon minecraft:villager {}.5 {Y} {}.5 {{NoAI:1b,VillagerData:{{profession:\"minecraft:farmer\",level:1}},Xp:0}}",
            X + offset,
            Z
        ))
        .await;
    }
    p.pump(600).await;

    let farmer_id = p
        .nearest_villager([f64::from(X + 2), f64::from(Y), f64::from(Z)])
        .expect("фермер не появился");
    let farmer = format!(
        "@e[type=minecraft:villager,x={}.5,y={Y},z={}.5,distance=..3,sort=nearest,limit=1]",
        X + 2,
        Z
    );
    // Скидка/наценка репутации проверяется на фиксированной сделке
    // (20 пшеницы), независимо от случайного набора `TradeSet`.
    p.cmd(&format!(
        "data merge entity {farmer} {{Offers:{{Recipes:[{{buy:{{id:\"minecraft:wheat\",count:20}},sell:{{id:\"minecraft:emerald\",count:1}},uses:0,maxUses:16,xp:2,priceMultiplier:0.05f,demand:0}}]}}}}"
    ))
    .await;
    let witness = format!(
        "@e[type=minecraft:villager,x={}.5,y={Y},z={}.5,distance=..3,sort=nearest,limit=1]",
        X + 5,
        Z
    );

    // 1. Базовая цена 20.
    p.cmd(&format!("tp @s {}.5 {Y} {}.5 180 20", X + 2, Z + 2))
        .await;
    p.c.confirm_loaded().await.unwrap();
    p.first_offer = None;
    p.window = None;
    let window = p.open_trade(farmer_id).await;
    p.pump(250).await;
    let (base, _) = p.first_offer.expect("нет merchant_offers");
    assert_eq!(base, 20, "базовая цена фермера не 20: {base}");
    p.close(window).await;

    // 2. Трейд: `TRADE` +2 к `trading`.
    p.cmd("item replace entity @s hotbar.0 with minecraft:wheat 20")
        .await;
    p.window = None;
    p.first_offer = None;
    let window = p.open_trade(farmer_id).await;
    p.select_trade(0).await;
    p.click(window, 30).await;
    p.click(window, 0).await;
    p.click(window, 2).await;
    p.click(window, 31).await;
    p.pump(300).await;
    let inventory = p.data("@s Inventory").await;
    assert!(
        inventory.contains("emerald"),
        "трейд не прошёл: {inventory}"
    );
    p.close(window).await;

    let gossips = p.data(&farmer).await;
    assert!(
        gossips.contains("trading") && gossips.contains("Value:2"),
        "TRADE не добавил trading=2: {gossips}"
    );
    println!("villager trade gossip ok (trading 2)");

    // 3. `VILLAGER_HURT`: minor_negative 25 и цена +2 (20 -> 22).
    p.cmd(&format!("damage {farmer} 1 minecraft:player_attack by @s"))
        .await;
    p.pump(200).await;
    let gossips = p.data(&farmer).await;
    assert!(
        gossips.contains("minor_negative") && gossips.contains("Value:25"),
        "VILLAGER_HURT не добавил minor_negative=25: {gossips}"
    );
    p.window = None;
    p.first_offer = None;
    let window = p.open_trade(farmer_id).await;
    p.pump(250).await;
    let (price, diff) = p.first_offer.expect("нет merchant_offers после удара");
    assert_eq!(price, 22, "цена с репутацией -23 не 22: {price}");
    assert_eq!(diff, 2, "specialPriceDiff не +2: {diff}");
    p.close(window).await;
    println!("villager hurt gossip ok (minor_negative 25, 20 -> 22)");

    // 4. `VILLAGER_KILLED`: свидетель получает major_negative 25.
    p.cmd(&format!(
        "damage {witness} 30 minecraft:player_attack by @s"
    ))
    .await;
    p.pump(400).await;
    let gossips = p.data(&farmer).await;
    assert!(
        gossips.contains("major_negative") && gossips.contains("Value:25"),
        "VILLAGER_KILLED не добавил major_negative=25: {gossips}"
    );
    // Репутация: 2 - 25 - 125 = -148 -> floor(-7.4) = -8 -> +8 к цене.
    p.window = None;
    p.first_offer = None;
    let window = p.open_trade(farmer_id).await;
    p.pump(250).await;
    let (price, _) = p.first_offer.expect("нет merchant_offers после убийства");
    assert_eq!(price, 28, "цена с репутацией -148 не 28: {price}");
    p.close(window).await;
    println!("villager killed gossip ok (major_negative 25, price {price})");

    // 5. Подарок голема: медный голем получает мак в antenna-слот.
    p.cmd(&format!("tp @s {}.5 {Y} {}.5 180 20", GX, Z + 4))
        .await;
    p.c.confirm_loaded().await.unwrap();
    p.cmd("kill @e[type=!minecraft:player,distance=..96]").await;
    p.villagers.clear();
    p.golems.clear();
    p.copper = None;
    p.offer_events.clear();
    p.gift_done = false;
    p.poppy_on_copper = false;
    p.cmd(&format!(
        "summon minecraft:copper_golem {GX}.5 {Y} {Z}.5 {{NoAI:1b,PersistenceRequired:1b}}"
    ))
    .await;
    p.pump(500).await;
    let copper = p.copper.expect("медный голем не появился");
    let mut spawned = 0;
    'spawn: for dx in -4..=4 {
        for dz in -4..=4 {
            if dx * dx + dz * dz > 16 || (dx == 0 && dz == 0) {
                continue;
            }
            if spawned >= 80 {
                break 'spawn;
            }
            spawned += 1;
            p.cmd_quick(&format!(
                "summon minecraft:iron_golem {}.5 {Y} {}.5 {{PersistenceRequired:1b}}",
                GX + dx,
                Z + dz
            ))
            .await;
        }
    }
    p.pump(800).await;
    println!("iron golems spawned: {spawned}");

    // Шанс `nextInt(8000) == 0` на голема за тик; 80 големов — ~1% в тик.
    let mut waited = 0;
    while !p.offer_events.contains(&11) && waited < 800 {
        p.pump(250).await;
        waited += 1;
    }
    assert!(
        p.offer_events.contains(&11),
        "голем не предложил цветок (событие 11)"
    );
    println!("iron golem flower offer ok (entity event 11)");

    waited = 0;
    while !(p.gift_done && p.poppy_on_copper) && waited < 400 {
        p.pump(250).await;
        waited += 1;
    }
    assert!(p.gift_done, "нет события 34 (конец предложения)");
    assert!(
        p.poppy_on_copper,
        "медный голем не получил мак в antenna-слот 7 (copper {copper})"
    );
    println!("iron golem gift ok (poppy -> copper golem antenna)");
    println!("gossip_gift_probe: ok");
}
