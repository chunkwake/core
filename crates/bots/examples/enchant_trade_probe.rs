//! Точные зачарования в сделках: `enchant_with_levels` у бронника (цена —
//! `wants + additional_trade_cost` из `levels`, уровень чара по диапазонам
//! `min_cost`/`max_cost`) и `enchant_randomly` у библиотекаря (уровень
//! 1..=max, цена `2 + nextInt(5 + level*10) + 3*level`, x2 для
//! `#double_trade_price`).
//!
//! `enchant_trade_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer};
use std::collections::HashMap;
use std::time::Duration;

const X: i32 = 3392;
const Z: i32 = 8;
const Y: i32 = 100;

struct OfferInfo {
    buy_count: i32,
    buy_b: Option<i32>,
    sell_item: i32,
    enchantments: Vec<(i32, i32)>,
    stored: Vec<(i32, i32)>,
}

struct Probe {
    c: Client,
    sb_command: i32,
    sb_interact: i32,
    sb_close: i32,
    cb_add: i32,
    cb_offers: i32,
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
            let (Ok(_buy_item), Ok(buy_count), Ok(_pred)) = (r.var_int(), r.var_int(), r.var_int())
            else {
                return;
            };
            let Ok(sell) = chunkwake_protocol::inventory::read_stack(&mut r) else {
                return;
            };
            let Some(sell) = sell else { return };
            let Ok(has_b) = r.bool() else { return };
            let mut buy_b = None;
            if has_b {
                let (Ok(item), Ok(_count), Ok(_pred)) = (r.var_int(), r.var_int(), r.var_int())
                else {
                    return;
                };
                buy_b = Some(item);
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
            let extra = chunkwake_protocol::components::get(sell.extra);
            self.offers.push(OfferInfo {
                buy_count,
                buy_b,
                sell_item: sell.item,
                enchantments: extra.enchantments.clone(),
                stored: extra.stored.clone(),
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
}

fn read_i32(r: &mut Reader) -> Result<i32, chunkwake_protocol::ProtocolError> {
    let bytes = r.bytes(4)?;
    Ok(i32::from_be_bytes(bytes.try_into().unwrap()))
}

/// `max_level` каждого чара: ID — алфавитный порядок файлов
/// `data/minecraft/enchantment` (как `RegistryData::load`).
fn enchant_max_levels(root: &std::path::Path) -> HashMap<i32, (String, i32)> {
    let dir = root.join("data/minecraft/enchantment");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            (path.extension()? == "json").then(|| {
                path.file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default()
            })
        })
        .collect();
    names.sort();
    let mut out = HashMap::new();
    for (id, name) in names.iter().enumerate() {
        let Ok(text) = std::fs::read_to_string(dir.join(format!("{name}.json"))) else {
            continue;
        };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let max = json["max_level"].as_i64().unwrap_or(1) as i32;
        out.insert(id as i32, (format!("minecraft:{name}"), max));
    }
    out
}

/// Тег `#minecraft:double_trade_price` как имена чар.
fn double_trade_set(root: &std::path::Path) -> Vec<String> {
    let file = root.join("data/minecraft/tags/enchantment/double_trade_price.json");
    let Ok(text) = std::fs::read_to_string(file) else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    json["values"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect()
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let root = std::path::Path::new(&a[2])
        .parent()
        .and_then(std::path::Path::parent)
        .unwrap()
        .to_path_buf();
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
    // Компоненты предметов для `read_stack` (`merchant_offers`).
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
        set(&c::REPAIR_COST_COMPONENT, "minecraft:repair_cost");
        set(&c::TRIM_COMPONENT, "minecraft:trim");
        set(&c::BANNER_PATTERNS_COMPONENT, "minecraft:banner_patterns");
        set(&c::MAP_ID_COMPONENT, "minecraft:map_id");
    }
    let max_levels = enchant_max_levels(&root);
    let double_set = double_trade_set(&root);

    let ids = Ids::load(&table).unwrap();
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n| table.id(State::Play, Direction::Clientbound, n).unwrap();
    let cb_add = cb("add_entity");
    let cb_offers = cb("merchant_offers");
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    c.capture_also = vec![cb_add, cb_offers];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_interact: sb("interact"),
        sb_close: sb("container_close"),
        cb_add,
        cb_offers,
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
    ] {
        p.cmd(&line).await;
    }
    p.c.confirm_loaded().await.unwrap();
    p.villagers.clear();

    let armor_items: Vec<(i32, i32)> = [
        ("minecraft:diamond_boots", 8),
        ("minecraft:diamond_leggings", 14),
        ("minecraft:diamond_chestplate", 16),
        ("minecraft:diamond_helmet", 8),
    ]
    .iter()
    .map(|(name, count)| (pid("minecraft:item", name), *count))
    .collect();

    // Бронник 4/5: зачарованная броня, `levels` 5..19.
    let mut found = None;
    for attempt in 0..16 {
        p.villagers.clear();
        p.cmd(&format!(
            "kill @e[type=minecraft:villager,x={}.5,y={Y},z={}.5,distance=..3]",
            X + 2,
            Z
        ))
        .await;
        p.cmd(&format!(
            "summon minecraft:villager {}.5 {Y} {}.5 {{NoAI:1b,VillagerData:{{type:\"minecraft:plains\",profession:\"minecraft:armorer\",level:4}},Xp:0}}",
            X + 2,
            Z
        ))
        .await;
        p.pump(600).await;
        let Some(entity) = p.nearest_villager([f64::from(X + 2), f64::from(Y), f64::from(Z)])
        else {
            continue;
        };
        p.cmd(&format!("tp @s {}.5 {Y} {}.5 180 20", X + 2, Z + 2))
            .await;
        p.c.confirm_loaded().await.unwrap();
        let window = p.open_trade(entity).await;
        if let Some((index, offer)) = p.offers.iter().enumerate().find(|(_, o)| {
            armor_items.iter().any(|&(item, _)| item == o.sell_item) && !o.enchantments.is_empty()
        }) {
            found = Some((
                window,
                index,
                offer.buy_count,
                offer.sell_item,
                offer.enchantments.clone(),
            ));
            println!(
                "armorer level 4 enchanted armor ok (attempt {attempt}, buy {}, enchantments {:?})",
                p.offers[index].buy_count, p.offers[index].enchantments
            );
            break;
        }
        p.close(window).await;
    }
    let Some((window, _, buy_count, sell_item, enchantments)) = found else {
        panic!("бронник не предложил зачарованную броню");
    };
    let base = armor_items
        .iter()
        .find(|&&(item, _)| item == sell_item)
        .map(|&(_, count)| count)
        .unwrap();
    // `levels` = uniform 5..19; `additional_trade_cost` = ровно это число.
    assert!(
        (base + 5..=base + 19).contains(&buy_count),
        "цена зачарованной брони {buy_count}, ожидалось {}..{}",
        base + 5,
        base + 19
    );
    assert!(!enchantments.is_empty());
    for &(id, level) in &enchantments {
        let (_, max) = max_levels
            .get(&id)
            .unwrap_or_else(|| panic!("неизвестные чары {id}"));
        assert!(
            (1..=*max).contains(&level),
            "уровень чар {id} = {level} вне 1..={max}"
        );
    }
    println!("enchanted armor price ok ({} + 5..19 = {buy_count})", base);
    p.close(window).await;

    // Библиотекарь 1: `enchant_randomly` на книге.
    let book = pid("minecraft:item", "minecraft:enchanted_book");
    let paper_book = pid("minecraft:item", "minecraft:book");
    let mut book_offer = None;
    for attempt in 0..16 {
        p.villagers.clear();
        p.cmd(&format!(
            "kill @e[type=minecraft:villager,x={}.5,y={Y},z={}.5,distance=..3]",
            X + 6,
            Z
        ))
        .await;
        p.cmd(&format!(
            "summon minecraft:villager {}.5 {Y} {}.5 {{NoAI:1b,VillagerData:{{type:\"minecraft:plains\",profession:\"minecraft:librarian\",level:1}},Xp:0}}",
            X + 6,
            Z
        ))
        .await;
        p.pump(600).await;
        let Some(entity) = p.nearest_villager([f64::from(X + 6), f64::from(Y), f64::from(Z)])
        else {
            continue;
        };
        p.cmd(&format!("tp @s {}.5 {Y} {}.5 180 20", X + 6, Z + 2))
            .await;
        p.c.confirm_loaded().await.unwrap();
        let window = p.open_trade(entity).await;
        if let Some((index, offer)) = p
            .offers
            .iter()
            .enumerate()
            .find(|(_, o)| o.sell_item == book && !o.stored.is_empty())
        {
            book_offer = Some((
                window,
                index,
                offer.buy_count,
                offer.buy_b,
                offer.stored.clone(),
            ));
            println!(
                "librarian level 1 book ok (attempt {attempt}, buy {}, stored {:?})",
                offer.buy_count, offer.stored
            );
            break;
        }
        p.close(window).await;
    }
    let Some((window, _, buy_count, buy_b, stored)) = book_offer else {
        panic!("библиотекарь не предложил зачарованную книгу");
    };
    assert_eq!(buy_b, Some(paper_book), "книга не требует book");
    assert!(!stored.is_empty());
    for &(id, level) in &stored {
        let (_, max) = max_levels
            .get(&id)
            .unwrap_or_else(|| panic!("неизвестные чары {id}"));
        assert!(
            (1..=*max).contains(&level),
            "уровень книги {id} = {level} вне 1..={max}"
        );
    }
    let level = stored[0].1;
    let double = stored.iter().any(|&(id, _)| {
        max_levels
            .get(&id)
            .is_some_and(|(name, _)| double_set.iter().any(|n| n == name))
    });
    let mut low = 2 + 3 * level;
    let mut high = 6 + 13 * level;
    if double {
        low *= 2;
        high *= 2;
    }
    low = low.min(64);
    high = high.min(64);
    assert!(
        (low..=high).contains(&buy_count),
        "цена книги {buy_count} вне {low}..{high} (level {level}, double {double})"
    );
    println!(
        "enchanted book price ok (level {level}, x2 {double}, {low}..{high}, buy {buy_count})"
    );
    p.close(window).await;
    println!("enchant_trade_probe: ok");
}
