//! Карты исследователя у картографа: `exploration_map` создаёт карту с
//! `map_id` и значком структуры (`map_decoration_type`), при покупке
//! приходит `map_item_data` с этим значком.
//!
//! `explorer_map_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::collections::HashMap;
use std::time::Duration;

const X: i32 = 512;
const Z: i32 = 8;
const Y: i32 = 100;

struct OfferInfo {
    buy_count: i32,
    sell_item: i32,
    map_id: Option<i32>,
}

struct Probe {
    c: Client,
    sb_command: i32,
    sb_interact: i32,
    sb_click: i32,
    sb_select_trade: i32,
    sb_set_carried: i32,
    sb_close: i32,
    cb_chat: i32,
    cb_add: i32,
    cb_offers: i32,
    cb_map: i32,
    chat: String,
    villager_type: i32,
    villagers: HashMap<i32, [f64; 3]>,
    window: Option<i32>,
    offers: Vec<OfferInfo>,
    /// (map_id, тип значка, подпись) из `map_item_data`.
    maps: Vec<(i32, i32, String)>,
    seq: i32,
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
            let (Ok(_buy_item), Ok(buy_count), Ok(_pred)) = (r.var_int(), r.var_int(), r.var_int())
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
            let Ok(_max) = read_i32(&mut r) else { return };
            let Ok(_xp) = read_i32(&mut r) else { return };
            if read_i32(&mut r).is_err() || r.f32().is_err() || read_i32(&mut r).is_err() {
                return;
            }
            let map_id = chunkwake_protocol::components::get(sell.extra).map_id;
            self.offers.push(OfferInfo {
                buy_count,
                sell_item: sell.item,
                map_id,
            });
        }
    }

    fn parse_map(&mut self, body: &[u8]) {
        let mut r = Reader::new(body);
        let (Ok(map_id), Ok(scale), Ok(_locked)) = (r.var_int(), r.u8(), r.bool()) else {
            return;
        };
        let _ = scale;
        let Ok(has_deco) = r.bool() else { return };
        if !has_deco {
            return;
        }
        let Ok(count) = r.var_int() else { return };
        for _ in 0..count.clamp(0, 256) {
            let (Ok(kind), Ok(_x), Ok(_z), Ok(_rot)) = (r.var_int(), r.u8(), r.u8(), r.u8()) else {
                return;
            };
            let Ok(has_name) = r.bool() else { return };
            let mut name = String::new();
            if has_name {
                let Ok(component) = text::read_component(&mut r) else {
                    return;
                };
                flatten(&component, &mut name);
            }
            self.maps.push((map_id, kind, name));
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
            } else if id == self.cb_map {
                self.parse_map(&body);
            } else if id == self.cb_chat
                && let Ok(component) = text::read_component(&mut r)
            {
                flatten(&component, &mut self.chat);
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
            self.offers.clear();
            let mut w = self.packet(self.sb_interact);
            w.var_int(entity).var_int(0).u8(0).bool(false);
            self.c.send_raw(&w.buf).await.unwrap();
            self.pump(400).await;
            if let Some(window) = self.window {
                return window;
            }
        }
        panic!("окно торговли не открылось");
    }

    async fn close(&mut self, window: i32) {
        let mut w = self.packet(self.sb_close);
        w.var_int(window);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(200).await;
    }

    async fn select_trade(&mut self, idx: i32) {
        let mut w = self.packet(self.sb_select_trade);
        w.var_int(idx);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(200).await;
    }

    async fn select_slot(&mut self, slot: u16) {
        let mut w = self.packet(self.sb_set_carried);
        w.u16(slot);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(250).await;
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
    // `read_stack` должен узнавать компонент `map_id`, иначе чтение
    // `merchant_offers` обрывается на карте.
    if let Some(id) =
        reg["minecraft:data_component_type"]["entries"]["minecraft:map_id"]["protocol_id"].as_i64()
    {
        chunkwake_protocol::components::MAP_ID_COMPONENT
            .store(id as i32, std::sync::atomic::Ordering::Relaxed);
    }

    let ids = Ids::load(&table).unwrap();
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n| table.id(State::Play, Direction::Clientbound, n).unwrap();
    let cb_chat = cb("system_chat");
    let cb_add = cb("add_entity");
    let cb_offers = cb("merchant_offers");
    let cb_map = cb("map_item_data");
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    c.capture_also = vec![cb_chat, cb_add, cb_offers, cb_map];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_interact: sb("interact"),
        sb_click: sb("container_click"),
        sb_select_trade: sb("select_trade"),
        sb_set_carried: sb("set_carried_item"),
        sb_close: sb("container_close"),
        cb_chat,
        cb_add,
        cb_offers,
        cb_map,
        chat: String::new(),
        villager_type: pid("minecraft:entity_type", "minecraft:villager"),
        villagers: HashMap::new(),
        window: None,
        offers: Vec::new(),
        maps: Vec::new(),
        seq: 0,
    };
    p.pump(1500).await;

    for line in [
        "difficulty peaceful".to_string(),
        "gamemode survival".to_string(),
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

    // Проверяем, что в мире вообще есть структуры для карт.
    let mut located = false;
    for name in [
        "minecraft:village_plains",
        "minecraft:village_desert",
        "minecraft:jungle_pyramid",
        "minecraft:desert_pyramid",
    ] {
        p.chat.clear();
        p.cmd(&format!("locate structure {name}")).await;
        if !p.chat.trim().is_empty() {
            located = true;
            println!("locate {name} ok: {}", p.chat.trim());
            break;
        }
    }
    assert!(
        located,
        "в мире не нашлось структур для карт исследователя: {}",
        p.chat.trim()
    );

    // Картограф варианта desert: допустимы plains/savanna/jungle maps.
    let spot = [X + 2, Y, Z];
    let mut chosen: Option<(i32, i32, i32, i32)> = None;
    for attempt in 0..12 {
        p.villagers.clear();
        p.cmd(&format!(
            "kill @e[type=minecraft:villager,x={}.5,y={Y},z={}.5,distance=..3]",
            spot[0], spot[2]
        ))
        .await;
        p.cmd(&format!(
            "summon minecraft:villager {}.5 {Y} {}.5 {{NoAI:1b,VillagerData:{{type:\"minecraft:desert\",profession:\"minecraft:cartographer\",level:2}},Xp:0}}",
            spot[0], spot[2]
        ))
        .await;
        p.pump(700).await;
        let entity = p
            .villagers
            .iter()
            .min_by(|a, b| {
                let da =
                    (a.1[0] - f64::from(spot[0])).powi(2) + (a.1[2] - f64::from(spot[2])).powi(2);
                let db =
                    (b.1[0] - f64::from(spot[0])).powi(2) + (b.1[2] - f64::from(spot[2])).powi(2);
                da.total_cmp(&db)
            })
            .map(|(&id, _)| id)
            .expect("картограф не появился");
        p.cmd(&format!("tp @s {}.5 {Y} {}.5 180 20", spot[0], spot[2] + 2))
            .await;
        p.c.confirm_loaded().await.unwrap();
        let window = p.open_trade(entity).await;
        p.pump(250).await;
        let found = p
            .offers
            .iter()
            .enumerate()
            .find(|(_, o)| o.map_id.is_some());
        if let Some((index, offer)) = found {
            chosen = Some((entity, window, index as i32, offer.sell_item));
            let _ = offer.buy_count;
            println!(
                "explorer map offer ok (attempt {attempt}, index {index}, map_id {:?})",
                offer.map_id
            );
            break;
        }
        p.close(window).await;
    }
    let Some((entity, window, index, sell_item)) = chosen else {
        panic!("картограф не предложил карту исследователя");
    };
    let _ = entity;
    let map_id = p.offers[index as usize].map_id.unwrap();
    let expected = match sell_item {
        id if id == pid("minecraft:item", "minecraft:plains_village_map") => {
            pid("minecraft:map_decoration_type", "minecraft:village_plains")
        }
        id if id == pid("minecraft:item", "minecraft:savanna_village_map") => {
            pid("minecraft:map_decoration_type", "minecraft:village_savanna")
        }
        id if id == pid("minecraft:item", "minecraft:jungle_pyramid_map") => {
            pid("minecraft:map_decoration_type", "minecraft:jungle_temple")
        }
        id if id == pid("minecraft:item", "minecraft:desert_village_map") => {
            pid("minecraft:map_decoration_type", "minecraft:village_desert")
        }
        _ => panic!("неизвестная карта исследователя (item {sell_item})"),
    };

    // Покупка: 8 изумрудов + компас (цена из данных, без скидок).
    let cost = p.offers[index as usize].buy_count;
    p.cmd(&format!(
        "item replace entity @s hotbar.0 with minecraft:emerald {cost}"
    ))
    .await;
    p.cmd("item replace entity @s hotbar.1 with minecraft:compass 1")
        .await;
    p.select_trade(index).await;
    p.click(window, 30).await;
    p.click(window, 0).await;
    p.click(window, 31).await;
    p.click(window, 1).await;
    p.click(window, 2).await;
    p.click(window, 32).await;
    p.pump(400).await;
    let inv = p.data("@s Inventory").await;
    assert!(
        inv.contains("_map") || inv.contains("filled_map"),
        "карта не попала в инвентарь: {inv}"
    );
    p.close(window).await;

    // Держим карту в руке: `MapItem.inventoryTick` присылает `map_item_data`.
    // Слот после покупки заранее неизвестен — перебираем хотбар.
    'slots: for slot in 0..9u16 {
        p.select_slot(slot).await;
        for _ in 0..8 {
            p.pump(250).await;
            if p.maps.iter().any(|(id, _, _)| *id == map_id) {
                println!("map held in hotbar slot {slot}");
                break 'slots;
            }
        }
    }
    let deco = p
        .maps
        .iter()
        .find(|(id, kind, _)| *id == map_id && *kind == expected)
        .unwrap_or_else(|| {
            panic!(
                "в map_item_data нет значка {expected} у карты {map_id}: {:?}",
                p.maps
            )
        });
    assert_eq!(deco.2, "+", "подпись значка структуры не \"+\": {deco:?}");
    println!("explorer map decoration ok (map {map_id}, kind {expected}, name \"+\")");
    println!("explorer_map_probe: ok");
}
