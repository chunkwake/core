//! Достижения (`Advancement`): загрузка данных, прогресс, `update_advancements`
//! и триггеры (`villager_trade`, `fishing_rod_hooked`, `consume_item`).
//!
//! Определения читаются из `data/minecraft/advancement/**/*.json` при старте.
//! Прогресс хранится по игроку (advancement → критерий → время получения) и
//! пишется в `advancements/<uuid>.json`.

use super::*;
use std::path::Path;

/// `ItemPredicate` из предиката сущности: предметы и `components`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ItemPred {
    /// `items`: id или `#тег` (пусто - любой предмет).
    pub items: Vec<String>,
    /// `components.minecraft:damage` (точное значение; `None` - не проверять).
    pub damage: Option<i32>,
}

/// `EntityPredicate` из JSON (`conditions.player`/`conditions.entity`).
#[derive(Clone, Debug)]
pub enum EntityPred {
    Inverted(Box<EntityPred>),
    AnyOf(Vec<EntityPred>),
    AllOf(Vec<EntityPred>),
    /// `entity_properties` с поддержанными полями.
    Properties {
        equipment: Vec<(String, ItemPred)>,
        entities: Vec<String>,
        baby: Option<bool>,
    },
}

#[derive(Clone, Debug)]
pub struct AdvancementCriterion {
    pub name: String,
    pub trigger: String,
    /// `conditions.item.items`: какие предметы закрывают критерий.
    pub items: Vec<String>,
    /// `conditions.entity.predicate.minecraft:entity_type`.
    pub entities: Vec<String>,
    /// `conditions.recipes` (для `recipe_unlocked`).
    pub recipes: Vec<String>,
    /// `conditions.location.*.blocks` (для `placed_block`).
    pub blocks: Vec<String>,
    /// `conditions.from`/`to` (для `changed_dimension`).
    pub from_dims: Vec<String>,
    pub to_dims: Vec<String>,
    /// `conditions.potion` (для `brewed_potion`).
    pub potions: Vec<String>,
    /// `conditions.distance` (для `used_ender_eye`, по квадрату расстояния).
    pub distance: Option<(f64, f64)>,
    /// `conditions.levels` (для `enchanted_item`).
    pub levels: Option<(i32, i32)>,
    /// `conditions.signal_strength` (для `target_hit`).
    pub signal_strength: Option<(i32, i32)>,
    /// `conditions.projectile.predicate.minecraft:distance.horizontal`.
    pub proj_distance: Option<(f64, f64)>,
    /// `conditions.damage.type.direct_entity.minecraft:entity_type`.
    pub direct_entities: Vec<String>,
    /// `conditions.damage.type.tags` с `expected: true`.
    pub damage_tags_req: Vec<String>,
    /// `conditions.damage.blocked`.
    pub blocked: Option<bool>,
    /// `conditions.victims[].predicate.minecraft:entity_type` (`channeled_lightning`).
    pub victims: Vec<String>,
    /// `conditions.distance.horizontal` (`nether_travel`).
    pub travel_distance: Option<(f64, f64)>,
    /// `conditions.child`/`parent`/`partner` (`bred_animals`).
    pub child_entities: Vec<String>,
    pub parent_entities: Vec<String>,
    pub partner_entities: Vec<String>,
    /// `conditions.ingredients` (`recipe_crafted`): список предикатов.
    pub ingredients: Vec<Vec<String>>,
    /// `conditions.player.predicate.minecraft:location.biomes` (`location`).
    pub biomes: Vec<String>,
    /// `conditions.player.predicate.minecraft:location.structures`.
    pub structures: Vec<String>,
    /// `conditions.player.predicate.minecraft:location.position`: ось и
    /// границы (включительно), например `x` -> `(1.0, 2.0)`.
    pub position: Vec<(String, (f64, f64))>,
    /// `...location.dimension` (`minecraft:overworld` и т.п.).
    pub dimension: Option<String>,
    /// `...location.smokey`: дым от зажжённого костра под точкой.
    pub smokey: Option<bool>,
    /// `...location.light.light`: границы яркости (`getMaxLocalRawBrightness`).
    pub light: Option<(i32, i32)>,
    /// `...location.block.blocks`.
    pub block: Option<Vec<String>>,
    /// `...location.block` содержит `state`/`nbt`.
    pub block_unsupported: bool,
    /// `...location.fluid.fluid`.
    pub fluid: Option<Vec<String>>,
    /// `...location.fluid` содержит что-то кроме `fluid`.
    pub fluid_unsupported: bool,
    /// `...location.can_see_sky`.
    pub can_see_sky: Option<bool>,
    /// `conditions.player.predicate.minecraft:stepping_on.block.blocks`.
    pub stepping_on: Vec<String>,
    /// `stepping_on` содержит неподдержанные части предиката.
    pub stepping_on_unsupported: bool,
    /// `conditions.player.predicate.minecraft:equipment`: слот и предметы.
    pub equipment: Vec<(String, Vec<String>)>,
    /// `equipment` содержит неподдержанные части (`count`, `components`, ...).
    pub equipment_unsupported: bool,
    /// Предикат `location` содержит то, что мы не умеем проверять
    /// (`position`, `light`, ...).
    pub unsupported_location: bool,
    /// `conditions.effects` (`effects_changed`): имя и требуемый уровень.
    pub effects_req: Vec<(String, Option<i32>)>,
    /// `conditions.source.predicate.minecraft:entity_type`.
    pub source_entities: Vec<String>,
    /// В `conditions.effects`/`source` есть то, что мы не умеем проверять.
    pub unsupported_effects: bool,
    /// `conditions.loot_tables` (`player_generates_container_loot`).
    pub loot_tables: Vec<String>,
    /// `conditions.entity.predicate.minecraft:flags.is_baby`.
    pub entity_baby: Option<bool>,
    /// `conditions.entity.predicate.minecraft:distance.horizontal`
    /// (`player_killed_entity`): расстояние жертвы до игрока.
    pub victim_distance: Option<(f64, f64)>,
    /// В предикате жертвы есть неподдержанные поля.
    pub victim_unsupported: bool,
    /// `conditions.killing_blow.tags` (id без `#`, только `expected: true`).
    pub killing_blow_tags: Vec<String>,
    /// `conditions.killing_blow` содержит неподдержанные поля.
    pub killing_blow_unsupported: bool,
    /// `conditions.location` (`item_used_on_block`) содержит неподдержанные
    /// поля `location_check`.
    pub item_location_unsupported: bool,
    /// `conditions.entity.predicate.minecraft:equipment` (экипировка цели).
    pub target_equipment: Vec<(String, ItemPred)>,
    /// В предикате цели есть неподдержанные поля (компоненты, nbt, ...).
    pub target_unsupported: bool,
    /// `conditions.player` (`player_interacted_with_entity`).
    pub player_pred: Option<EntityPred>,
    /// `conditions.player` содержит неподдержанные узлы.
    pub player_pred_unsupported: bool,
}

#[derive(Clone, Debug)]
pub struct AdvancementDisplay {
    pub title: serde_json::Value,
    pub description: serde_json::Value,
    pub icon: String,
    /// `AdvancementType.id`: task=0, challenge=1, goal=2.
    pub frame: i32,
    pub background: Option<String>,
    pub show_toast: bool,
    pub announce: bool,
    pub hidden: bool,
}

#[derive(Clone, Debug)]
pub struct AdvancementDef {
    pub id: String,
    pub parent: Option<String>,
    pub display: Option<AdvancementDisplay>,
    pub requirements: Vec<Vec<String>>,
    pub criteria: Vec<AdvancementCriterion>,
    pub x: f32,
    pub y: f32,
}

fn short(id: &str) -> &str {
    id.trim_start_matches("minecraft:")
}

/// Разбор `EntityPredicate` (`player`/`entity`): `inverted`, `any_of`,
/// `all_of` и `entity_properties` с `entity_type`/`flags.is_baby`/`equipment`.
/// Неподдержанные узлы помечают `ok = false`.
/// `ItemPredicate`: `items` (id/тег) и `components.minecraft:damage`.
/// Прочие поля и компоненты - неподдержаны (`None`).
fn parse_item_pred(v: &serde_json::Value) -> Option<ItemPred> {
    let items = v.get("items").map(string_or_list)?;
    if items.is_empty() {
        return None;
    }
    let mut damage = None;
    if let Some(map) = v.as_object() {
        for (key, value) in map {
            match key.as_str() {
                "items" => {}
                "components" => {
                    let comps = value.as_object()?;
                    for (name, comp) in comps {
                        if name == "minecraft:damage" {
                            damage = Some(comp.as_i64()? as i32);
                        } else {
                            return None;
                        }
                    }
                }
                _ => return None,
            }
        }
    }
    Some(ItemPred { items, damage })
}

fn parse_entity_pred(v: &serde_json::Value, ok: &mut bool) -> Option<EntityPred> {
    let typ = v.get("type").and_then(|t| t.as_str())?;
    match typ {
        "minecraft:inverted" => {
            let term = parse_entity_pred(v.get("term")?, ok)?;
            Some(EntityPred::Inverted(Box::new(term)))
        }
        "minecraft:any_of" | "minecraft:all_of" => {
            let terms = v.get("terms")?.as_array()?;
            let mut list = Vec::with_capacity(terms.len());
            for term in terms {
                list.push(parse_entity_pred(term, ok)?);
            }
            Some(if typ == "minecraft:any_of" {
                EntityPred::AnyOf(list)
            } else {
                EntityPred::AllOf(list)
            })
        }
        "minecraft:entity_properties" => {
            if v.get("entity").and_then(|e| e.as_str()) != Some("this") {
                *ok = false;
            }
            let pred = v.get("predicate")?;
            let mut equipment = Vec::new();
            let mut entities = Vec::new();
            let mut baby = None;
            let Some(map) = pred.as_object() else {
                *ok = false;
                return None;
            };
            for (key, value) in map {
                match key.as_str() {
                    "minecraft:entity_type" => entities = string_or_list(value),
                    "minecraft:flags" => {
                        if let Some(flag) = value.get("is_baby") {
                            match flag.as_bool() {
                                Some(b) => baby = Some(b),
                                None => *ok = false,
                            }
                        }
                        if value
                            .as_object()
                            .is_some_and(|o| o.keys().any(|k| k != "is_baby"))
                        {
                            *ok = false;
                        }
                    }
                    "minecraft:equipment" => {
                        let Some(slots) = value.as_object() else {
                            *ok = false;
                            continue;
                        };
                        for (slot, item) in slots {
                            match parse_item_pred(item) {
                                Some(pred) if equipment_slot(slot, 0).is_some() => {
                                    equipment.push((slot.clone(), pred));
                                }
                                _ => *ok = false,
                            }
                        }
                    }
                    _ => *ok = false,
                }
            }
            Some(EntityPred::Properties {
                equipment,
                entities,
                baby,
            })
        }
        _ => {
            *ok = false;
            None
        }
    }
}

/// Пересечение AABB с положительным объёмом (`Shapes.joinIsNotEmpty`).
fn box_overlaps(a: [f64; 6], b: [f64; 6]) -> bool {
    a[0] < b[3] && a[3] > b[0] && a[1] < b[4] && a[4] > b[1] && a[2] < b[5] && a[5] > b[2]
}

/// Слот экипировки (`EquipmentSlot`) в `Inventory.slots`.
fn equipment_slot(slot: &str, selected: usize) -> Option<usize> {
    use crate::survival::{CHEST, FEET, HEAD, HOTBAR, LEGS, OFFHAND};
    match slot {
        "head" => Some(HEAD),
        "chest" => Some(CHEST),
        "legs" => Some(LEGS),
        "feet" => Some(FEET),
        "mainhand" => Some(HOTBAR + selected.min(8)),
        "offhand" => Some(OFFHAND),
        _ => None,
    }
}

fn json_string_list(v: &serde_json::Value) -> Vec<String> {
    match v {
        serde_json::Value::String(s) => vec![s.clone()],
        serde_json::Value::Array(a) => a
            .iter()
            .filter_map(|x| x.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}

/// Собрать `blocks` из дерева block predicate (`match_block`, `location_check`,
/// `all_of`/`any_of`).
fn collect_blocks(v: &serde_json::Value, out: &mut Vec<String>) {
    if let Some(b) = v.get("blocks") {
        out.extend(json_string_list(b));
    }
    if let Some(block) = v.get("predicate").and_then(|p| p.get("block")) {
        collect_blocks(block, out);
    }
    if let Some(terms) = v.get("terms").and_then(|t| t.as_array()) {
        for term in terms {
            collect_blocks(term, out);
        }
    }
}

/// `location_check` внутри `item_used_on_block`: поддержан `smokey` и
/// `block.blocks` (последний собирается отдельно). Остальное - unsupported.
fn collect_item_location(v: &serde_json::Value, smokey: &mut Option<bool>, ok: &mut bool) {
    if let Some(terms) = v.get("terms").and_then(|t| t.as_array()) {
        for term in terms {
            collect_item_location(term, smokey, ok);
        }
    }
    if v.get("type").and_then(|t| t.as_str()) != Some("minecraft:location_check") {
        return;
    }
    if v.as_object()
        .is_some_and(|o| o.keys().any(|k| k != "type" && k != "predicate"))
    {
        *ok = false;
        return;
    }
    let Some(pred) = v.get("predicate").and_then(|p| p.as_object()) else {
        return;
    };
    for (key, value) in pred {
        match key.as_str() {
            "smokey" => match value.as_bool() {
                Some(flag) => {
                    if smokey.is_some_and(|old| old != flag) {
                        *ok = false;
                    }
                    *smokey = Some(flag);
                }
                None => *ok = false,
            },
            "block" => {
                let simple = value
                    .as_object()
                    .is_some_and(|o| o.keys().all(|k| k == "blocks"));
                if !simple {
                    *ok = false;
                }
            }
            _ => *ok = false,
        }
    }
}

/// `match_tool` внутри дерева `location`: `predicate.items`.
fn collect_location_items(v: &serde_json::Value, out: &mut Vec<String>) {
    if let Some(items) = v.get("predicate").and_then(|p| p.get("items")) {
        out.extend(json_string_list(items));
    }
    if let Some(terms) = v.get("terms").and_then(|t| t.as_array()) {
        for term in terms {
            collect_location_items(term, out);
        }
    }
}

/// Строка или список строк в JSON.
fn string_or_list(v: &serde_json::Value) -> Vec<String> {
    match v {
        serde_json::Value::String(s) => vec![s.clone()],
        serde_json::Value::Array(a) => a
            .iter()
            .filter_map(|x| x.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}

fn bounds_f64(v: &serde_json::Value) -> Option<(f64, f64)> {
    match v {
        serde_json::Value::Number(n) => {
            let x = n.as_f64()?;
            Some((x, x))
        }
        serde_json::Value::Object(o) => Some((
            o.get("min")
                .and_then(|x| x.as_f64())
                .unwrap_or(f64::NEG_INFINITY),
            o.get("max")
                .and_then(|x| x.as_f64())
                .unwrap_or(f64::INFINITY),
        )),
        _ => None,
    }
}

fn bounds_i32(v: &serde_json::Value) -> Option<(i32, i32)> {
    match v {
        serde_json::Value::Number(n) => {
            let x = n.as_i64()? as i32;
            Some((x, x))
        }
        serde_json::Value::Object(o) => Some((
            o.get("min")
                .and_then(|x| x.as_i64())
                .map_or(i32::MIN, |x| x as i32),
            o.get("max")
                .and_then(|x| x.as_i64())
                .map_or(i32::MAX, |x| x as i32),
        )),
        _ => None,
    }
}

fn parse_display(v: &serde_json::Value) -> Option<AdvancementDisplay> {
    let d = v.get("display")?;
    let icon = d
        .get("icon")
        .and_then(|i| i.get("id").or_else(|| i.get("item")))
        .and_then(|i| i.as_str())?
        .to_owned();
    let frame = match d.get("frame").and_then(|f| f.as_str()) {
        Some("challenge") => 1,
        Some("goal") => 2,
        _ => 0,
    };
    Some(AdvancementDisplay {
        title: d
            .get("title")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({"text": ""})),
        description: d
            .get("description")
            .cloned()
            .unwrap_or(serde_json::Value::String(String::new())),
        icon,
        frame,
        background: d
            .get("background")
            .and_then(|b| b.as_str())
            .map(str::to_owned),
        show_toast: d
            .get("show_toast")
            .and_then(|b| b.as_bool())
            .unwrap_or(true),
        announce: d
            .get("announce_to_chat")
            .and_then(|b| b.as_bool())
            .unwrap_or(true),
        hidden: d.get("hidden").and_then(|b| b.as_bool()).unwrap_or(false),
    })
}

fn parse_advancement(id: &str, json: &serde_json::Value) -> Option<AdvancementDef> {
    let parent = json
        .get("parent")
        .and_then(|p| p.as_str())
        .map(short)
        .map(str::to_owned);
    let display = parse_display(json);
    let mut criteria = Vec::new();
    if let Some(map) = json.get("criteria").and_then(|c| c.as_object()) {
        for (name, c) in map {
            let trigger = c
                .get("trigger")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_owned();
            let mut items = c
                .get("conditions")
                .and_then(|c| c.get("item"))
                .and_then(|i| i.get("items"))
                .map(json_string_list)
                .unwrap_or_default();
            // `inventory_changed`: `conditions.items` — список предикатов.
            if let Some(list) = c
                .get("conditions")
                .and_then(|c| c.get("items"))
                .and_then(|i| i.as_array())
            {
                for pred in list {
                    let names = pred
                        .get("items")
                        .map(json_string_list)
                        .or_else(|| {
                            pred.get("tag")
                                .and_then(|t| t.as_str())
                                .map(|t| vec![format!("#{t}")])
                        })
                        .unwrap_or_default();
                    items.extend(names);
                }
            }
            let entities = c
                .get("conditions")
                .and_then(|c| c.get("entity"))
                .and_then(|e| e.get("predicate"))
                .and_then(|p| p.get("minecraft:entity_type"))
                .map(json_string_list)
                .unwrap_or_default();
            let entity_baby = c
                .get("conditions")
                .and_then(|c| c.get("entity"))
                .and_then(|e| e.get("predicate"))
                .and_then(|p| p.get("minecraft:flags"))
                .and_then(|f| f.get("is_baby"))
                .and_then(|b| b.as_bool());
            let recipes = c
                .get("conditions")
                .and_then(|c| c.get("recipes"))
                .map(json_string_list)
                .unwrap_or_default();
            // `player_killed_entity`: предикат жертвы и `killing_blow`.
            let mut victim_distance = None;
            let mut victim_unsupported = false;
            let mut killing_blow_tags = Vec::new();
            let mut killing_blow_unsupported = false;
            if let Some(pred) = c
                .get("conditions")
                .and_then(|c| c.get("entity"))
                .and_then(|e| e.get("predicate"))
            {
                if let Some(dist) = pred.get("minecraft:distance") {
                    match dist.get("horizontal").map(bounds_f64) {
                        Some(Some(range)) => victim_distance = Some(range),
                        Some(None) => victim_unsupported = true,
                        None => {}
                    }
                    if dist
                        .as_object()
                        .is_some_and(|o| o.keys().any(|k| k != "horizontal"))
                    {
                        victim_unsupported = true;
                    }
                }
                if pred.as_object().is_some_and(|o| {
                    o.keys().any(|k| {
                        !matches!(
                            k.as_str(),
                            "minecraft:entity_type" | "minecraft:flags" | "minecraft:distance"
                        )
                    })
                }) {
                    victim_unsupported = true;
                }
            }
            if let Some(kb) = c.get("conditions").and_then(|c| c.get("killing_blow")) {
                let mut ok = kb.is_object();
                if let Some(map) = kb.as_object() {
                    for (key, value) in map {
                        if key != "tags" {
                            ok = false;
                            continue;
                        }
                        for tag in value.as_array().into_iter().flatten() {
                            let expected = tag
                                .get("expected")
                                .and_then(|b| b.as_bool())
                                .unwrap_or(true);
                            match tag.get("id").and_then(|s| s.as_str()) {
                                Some(id) if expected => {
                                    killing_blow_tags.push(id.trim_start_matches('#').to_owned());
                                }
                                Some(_) => ok = false,
                                None => ok = false,
                            }
                        }
                    }
                }
                if !ok {
                    killing_blow_unsupported = true;
                }
            }
            // `conditions.entity.predicate.minecraft:equipment` (цель).
            let mut target_equipment = Vec::new();
            let mut target_unsupported = false;
            if let Some(pred) = c
                .get("conditions")
                .and_then(|c| c.get("entity"))
                .and_then(|e| e.get("predicate"))
                .and_then(|p| p.as_object())
            {
                for (key, value) in pred {
                    match key.as_str() {
                        // Уже разобраны выше либо поддержаны триггером.
                        "minecraft:entity_type" | "minecraft:flags" | "minecraft:distance" => {}
                        "minecraft:equipment" => {
                            let Some(slots) = value.as_object() else {
                                target_unsupported = true;
                                continue;
                            };
                            for (slot, item) in slots {
                                match parse_item_pred(item) {
                                    Some(pred) => target_equipment.push((slot.clone(), pred)),
                                    None => target_unsupported = true,
                                }
                            }
                        }
                        _ => target_unsupported = true,
                    }
                }
            }
            let mut blocks = Vec::new();
            // `item_used_on_block`: `smokey` из `location_check`.
            let mut smokey = None;
            let mut item_location_supported = true;
            // `enter_block`: `conditions.blocks` — блок или список блоков.
            if let Some(blocks_v) = c.get("conditions").and_then(|c| c.get("blocks")) {
                blocks.extend(string_or_list(blocks_v));
            }
            if let Some(location) = c.get("conditions").and_then(|c| c.get("location")) {
                collect_blocks(location, &mut blocks);
                // `match_tool.predicate.items` внутри location.
                collect_location_items(location, &mut items);
                collect_item_location(location, &mut smokey, &mut item_location_supported);
            }
            let item_location_unsupported = !item_location_supported;
            let cond = c.get("conditions");
            let dim = |key: &str| {
                cond.and_then(|c| c.get(key))
                    .map(string_or_list)
                    .unwrap_or_default()
            };
            let potions = cond
                .and_then(|c| c.get("potion"))
                .map(|v| match v {
                    serde_json::Value::Object(o) => {
                        o.get("potions").map(string_or_list).unwrap_or_default()
                    }
                    other => string_or_list(other),
                })
                .unwrap_or_default();
            let distance = cond.and_then(|c| c.get("distance")).and_then(bounds_f64);
            let levels = cond.and_then(|c| c.get("levels")).and_then(bounds_i32);
            let signal_strength = cond
                .and_then(|c| c.get("signal_strength"))
                .and_then(bounds_i32);
            let proj_distance = cond
                .and_then(|c| c.get("projectile"))
                .and_then(|p| p.get("predicate"))
                .and_then(|p| p.get("minecraft:distance"))
                .and_then(|d| d.get("horizontal"))
                .and_then(bounds_f64);
            let damage = cond.and_then(|c| c.get("damage"));
            let direct_entities = damage
                .and_then(|d| d.get("type"))
                .and_then(|t| t.get("direct_entity"))
                .and_then(|e| e.get("minecraft:entity_type"))
                .map(string_or_list)
                .unwrap_or_default();
            let mut damage_tags_req = Vec::new();
            if let Some(tags) = damage
                .and_then(|d| d.get("type"))
                .and_then(|t| t.get("tags"))
                .and_then(|t| t.as_array())
            {
                for tag in tags {
                    let expected = tag
                        .get("expected")
                        .and_then(|b| b.as_bool())
                        .unwrap_or(true);
                    if expected && let Some(id) = tag.get("id").and_then(|s| s.as_str()) {
                        damage_tags_req.push(id.trim_start_matches('#').to_owned());
                    }
                }
            }
            let blocked = damage
                .and_then(|d| d.get("blocked"))
                .and_then(|b| b.as_bool());
            let mut victims = Vec::new();
            if let Some(list) = cond
                .and_then(|c| c.get("victims"))
                .and_then(|v| v.as_array())
            {
                for victim in list {
                    let names = victim
                        .get("predicate")
                        .and_then(|p| p.get("minecraft:entity_type"))
                        .map(string_or_list)
                        .unwrap_or_default();
                    victims.extend(names);
                }
            }
            let travel_distance = cond
                .and_then(|c| c.get("distance"))
                .and_then(|d| d.get("horizontal"))
                .and_then(bounds_f64);
            let entity_type = |key: &str| {
                cond.and_then(|c| c.get(key))
                    .and_then(|e| e.get("predicate"))
                    .and_then(|p| p.get("minecraft:entity_type"))
                    .map(string_or_list)
                    .unwrap_or_default()
            };
            let (child_entities, parent_entities, partner_entities) = (
                entity_type("child"),
                entity_type("parent"),
                entity_type("partner"),
            );
            let player_pred = cond
                .and_then(|c| c.get("player"))
                .and_then(|p| p.get("predicate"));
            let mut biomes = Vec::new();
            let mut structures = Vec::new();
            let mut position = Vec::new();
            let mut dimension = None;
            let mut light = None;
            let mut block = None;
            let mut block_unsupported = false;
            let mut fluid = None;
            let mut fluid_unsupported = false;
            let mut can_see_sky = None;
            let mut stepping_on = Vec::new();
            let mut stepping_on_unsupported = false;
            let mut equipment = Vec::new();
            let mut equipment_unsupported = false;
            let mut unsupported_location = false;
            if let Some(pred) = player_pred {
                if let Some(loc) = pred.get("minecraft:location") {
                    biomes = loc.get("biomes").map(string_or_list).unwrap_or_default();
                    structures = loc
                        .get("structures")
                        .map(string_or_list)
                        .unwrap_or_default();
                    if let Some(bounds) = loc.get("position") {
                        let mut ok = bounds.is_object();
                        if let Some(map) = bounds.as_object() {
                            for (axis, bound) in map {
                                if !matches!(axis.as_str(), "x" | "y" | "z") {
                                    ok = false;
                                    continue;
                                }
                                // `min_exclusive`/`max_exclusive` пока не
                                // поддержаны: такой предикат не проверяем.
                                let simple = match bound {
                                    serde_json::Value::Object(o) => {
                                        o.keys().all(|k| k == "min" || k == "max")
                                    }
                                    _ => true,
                                };
                                match bounds_f64(bound).filter(|_| simple) {
                                    Some(range) => position.push((axis.clone(), range)),
                                    None => ok = false,
                                }
                            }
                        }
                        if !ok {
                            unsupported_location = true;
                        }
                    }
                    if let Some(v) = loc.get("dimension") {
                        match v.as_str() {
                            Some(name) => dimension = Some(short(name).to_owned()),
                            None => unsupported_location = true,
                        }
                    }
                    if let Some(v) = loc.get("smokey") {
                        match v.as_bool() {
                            Some(flag) => smokey = Some(flag),
                            None => unsupported_location = true,
                        }
                    }
                    if let Some(v) = loc.get("can_see_sky") {
                        match v.as_bool() {
                            Some(flag) => can_see_sky = Some(flag),
                            None => unsupported_location = true,
                        }
                    }
                    if let Some(v) = loc.get("light") {
                        let inner = v.get("light");
                        let simple = v
                            .as_object()
                            .is_some_and(|o| o.keys().all(|k| k == "light"));
                        // `min_exclusive`/`max_exclusive` не поддержаны.
                        let inner_simple = inner.is_none_or(|b| match b {
                            serde_json::Value::Object(o) => {
                                o.keys().all(|k| k == "min" || k == "max")
                            }
                            _ => true,
                        });
                        match inner.map(bounds_i32) {
                            Some(Some(range)) if simple && inner_simple => light = Some(range),
                            Some(None) => unsupported_location = true,
                            _ => {}
                        }
                        if !simple || !inner_simple {
                            unsupported_location = true;
                        }
                    }
                    if let Some(v) = loc.get("block") {
                        let blocks = v.get("blocks").map(string_or_list).unwrap_or_default();
                        let extra = v
                            .as_object()
                            .is_none_or(|o| o.keys().any(|k| k != "blocks"));
                        if blocks.is_empty() && !extra {
                            // `{}` - любой блок.
                        } else if blocks.is_empty() || extra {
                            block_unsupported = true;
                        } else {
                            block = Some(blocks);
                        }
                    }
                    if let Some(v) = loc.get("fluid") {
                        let fluids = v.get("fluid").map(string_or_list).unwrap_or_default();
                        let extra = v.as_object().is_none_or(|o| o.keys().any(|k| k != "fluid"));
                        if fluids.is_empty() && !extra {
                            // `{}` - любая жидкость.
                        } else if fluids.is_empty() || extra {
                            fluid_unsupported = true;
                        } else {
                            fluid = Some(fluids);
                        }
                    }
                    if loc.as_object().is_some_and(|o| {
                        o.keys().any(|k| {
                            !matches!(
                                k.as_str(),
                                "biomes"
                                    | "structures"
                                    | "position"
                                    | "dimension"
                                    | "smokey"
                                    | "light"
                                    | "block"
                                    | "fluid"
                                    | "can_see_sky"
                            )
                        })
                    }) {
                        unsupported_location = true;
                    }
                }
                if let Some(step) = pred.get("minecraft:stepping_on") {
                    let block = step.get("block");
                    stepping_on = block
                        .and_then(|b| b.get("blocks"))
                        .map(string_or_list)
                        .unwrap_or_default();
                    let extra = step
                        .as_object()
                        .is_some_and(|o| o.keys().any(|k| k != "block"));
                    let block_extra = block
                        .and_then(|b| b.as_object())
                        .is_some_and(|o| o.keys().any(|k| k != "blocks"));
                    if stepping_on.is_empty() || extra || block_extra {
                        stepping_on_unsupported = true;
                    }
                }
                if let Some(map) = pred.get("minecraft:equipment").and_then(|e| e.as_object()) {
                    for (slot, want) in map {
                        let items = want.get("items").map(string_or_list).unwrap_or_default();
                        let extra = want
                            .as_object()
                            .is_some_and(|o| o.keys().any(|k| k != "items"));
                        if items.is_empty() || extra || equipment_slot(slot, 0).is_none() {
                            equipment_unsupported = true;
                        } else {
                            equipment.push((slot.clone(), items));
                        }
                    }
                }
                if pred.as_object().is_some_and(|o| {
                    o.keys().any(|k| {
                        !matches!(
                            k.as_str(),
                            "minecraft:location" | "minecraft:equipment" | "minecraft:stepping_on"
                        )
                    })
                }) {
                    unsupported_location = true;
                }
            }
            let (entity_pred, entity_pred_bad) = match cond.and_then(|c| c.get("player")) {
                Some(v) => {
                    let mut ok = true;
                    let tree = parse_entity_pred(v, &mut ok);
                    if tree.is_none() {
                        ok = false;
                    }
                    (tree, !ok)
                }
                None => (None, false),
            };
            let mut effects_req = Vec::new();
            let mut unsupported_effects = false;
            if let Some(map) = cond
                .and_then(|c| c.get("effects"))
                .and_then(|e| e.as_object())
            {
                for (name, pred) in map {
                    if pred.as_object().is_some_and(|o| {
                        o.keys().any(|k| {
                            !matches!(k.as_str(), "amplifier" | "duration" | "ambient" | "visible")
                        })
                    }) {
                        unsupported_effects = true;
                    }
                    let amp = pred
                        .get("amplifier")
                        .and_then(|a| a.as_i64())
                        .map(|v| v as i32);
                    effects_req.push((name.clone(), amp));
                }
            }
            let source = cond.and_then(|c| c.get("source"));
            let source_entities = source
                .and_then(|s| s.get("predicate"))
                .and_then(|p| p.get("minecraft:entity_type"))
                .map(string_or_list)
                .unwrap_or_default();
            if source.is_some() && source_entities.is_empty() {
                unsupported_effects = true;
            }
            let loot_tables = cond
                .and_then(|c| c.get("loot_tables"))
                .map(string_or_list)
                .unwrap_or_default();
            let ingredients = cond
                .and_then(|c| c.get("ingredients"))
                .and_then(|i| i.as_array())
                .map(|list| {
                    list.iter()
                        .map(|pred| {
                            pred.get("items")
                                .map(string_or_list)
                                .or_else(|| {
                                    pred.get("tag")
                                        .and_then(|t| t.as_str())
                                        .map(|t| vec![format!("#{t}")])
                                })
                                .unwrap_or_default()
                        })
                        .collect()
                })
                .unwrap_or_default();
            criteria.push(AdvancementCriterion {
                name: name.clone(),
                trigger,
                items,
                entities,
                recipes,
                blocks,
                from_dims: dim("from"),
                to_dims: dim("to"),
                potions,
                distance,
                levels,
                signal_strength,
                proj_distance,
                direct_entities,
                damage_tags_req,
                blocked,
                victims,
                travel_distance,
                child_entities,
                parent_entities,
                partner_entities,
                ingredients,
                biomes,
                structures,
                position,
                dimension,
                smokey,
                light,
                block,
                block_unsupported,
                fluid,
                fluid_unsupported,
                can_see_sky,
                stepping_on,
                stepping_on_unsupported,
                equipment,
                equipment_unsupported,
                unsupported_location,
                effects_req,
                source_entities,
                unsupported_effects,
                loot_tables,
                entity_baby,
                victim_distance,
                victim_unsupported,
                killing_blow_tags,
                killing_blow_unsupported,
                item_location_unsupported,
                target_equipment,
                target_unsupported,
                player_pred: entity_pred,
                player_pred_unsupported: entity_pred_bad,
            });
        }
    }
    let requirements = json
        .get("requirements")
        .and_then(|r| r.as_array())
        .map(|groups| groups.iter().map(json_string_list).collect::<Vec<_>>())
        .filter(|g| !g.is_empty())
        .unwrap_or_else(|| vec![criteria.iter().map(|c| c.name.clone()).collect()]);
    Some(AdvancementDef {
        id: id.to_owned(),
        parent,
        display,
        requirements,
        criteria,
        x: 0.0,
        y: 0.0,
    })
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, serde_json::Value)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, out);
        } else if path.extension().is_some_and(|e| e == "json")
            && let Ok(bytes) = std::fs::read(&path)
            && let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes)
        {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .with_extension("")
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, json));
        }
    }
}

/// Прочитать все достижения из `data/minecraft/advancement`.
pub fn load_advancements(data_dir: &Path) -> Vec<AdvancementDef> {
    let mut raw = Vec::new();
    let root = data_dir.join("data/minecraft/advancement");
    walk(&root, &root, &mut raw);
    raw.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out: Vec<AdvancementDef> = raw
        .iter()
        .filter_map(|(id, json)| parse_advancement(id, json))
        .collect();
    // Раскладка дерева: корни в (0,0), дети справа со сдвигом по вертикали.
    let children: FastMap<String, Vec<String>> = {
        let mut map: FastMap<String, Vec<String>> = FastMap::default();
        for a in &out {
            if let Some(p) = &a.parent {
                map.entry(p.clone()).or_default().push(a.id.clone());
            }
        }
        map
    };
    let mut pos: FastMap<String, (f32, f32)> = FastMap::default();
    let roots: Vec<String> = out
        .iter()
        .filter(|a| a.parent.is_none())
        .map(|a| a.id.clone())
        .collect();
    let mut stack: Vec<(String, f32, f32, f32)> = roots
        .iter()
        .enumerate()
        .map(|(i, id)| (id.clone(), 0.0, i as f32 * 2.0, 1.0))
        .collect();
    while let Some((id, x, y, _)) = stack.pop() {
        pos.insert(id.clone(), (x, y));
        if let Some(kids) = children.get(&id) {
            let n = kids.len() as f32;
            for (i, kid) in kids.iter().enumerate() {
                stack.push((
                    kid.clone(),
                    x + 1.5,
                    y + (i as f32 - (n - 1.0) / 2.0) * 1.5,
                    0.0,
                ));
            }
        }
    }
    for a in &mut out {
        if let Some(&(x, y)) = pos.get(&a.id) {
            a.x = x;
            a.y = y;
        }
    }
    out
}

impl PlayPlan {
    fn adv(&self, id: &str) -> Option<&AdvancementDef> {
        self.survival
            .advancement_ids
            .get(id)
            .and_then(|&i| self.survival.advancements.get(i))
    }

    pub(crate) fn advancement_index(&self, id: &str) -> Option<usize> {
        self.survival.advancement_ids.get(id).copied()
    }

    fn adv_done(progress: &FastMap<String, i64>, def: &AdvancementDef) -> bool {
        !def.requirements.is_empty()
            && def.requirements.iter().all(|group| {
                group
                    .iter()
                    .any(|name| progress.get(name).is_some_and(|&at| at > 0))
            })
    }

    fn adv_parent_done(&self, progress: &AdvProgress, parent: Option<&str>) -> bool {
        let Some(parent) = parent else {
            return true;
        };
        self.adv(parent).is_some_and(|def| {
            progress.get(parent).is_some_and(|p| Self::adv_done(p, def)) || def.criteria.is_empty()
        })
    }

    /// Добавить критерии по триггеру и отправить прогресс.
    pub(crate) fn advancement_trigger(&self, uuid: Uuid, trigger: &str, item: Option<&str>) {
        self.advancement_run(uuid, trigger, |c| {
            if !c.items.is_empty()
                && !item
                    .is_some_and(|name| c.items.iter().any(|want| self.item_matches(want, name)))
            {
                return false;
            }
            if !c.blocks.is_empty()
                && !item
                    .is_some_and(|name| c.blocks.iter().any(|want| self.block_matches(want, name)))
            {
                return false;
            }
            if !c.entities.is_empty()
                && !item.is_some_and(|name| c.entities.iter().any(|n| short(n) == short(name)))
            {
                return false;
            }
            if !c.recipes.is_empty()
                && !item.is_some_and(|name| c.recipes.iter().any(|n| short(n) == short(name)))
            {
                return false;
            }
            true
        });
    }

    /// `RecipeUnlockedTrigger` для набора рецептов: один проход вместо
    /// вызова `advancement_trigger` на каждый рецепт (`/recipe give *`).
    pub(crate) fn advancement_recipes_unlocked(&self, uuid: Uuid, ids: &[String]) {
        self.advancement_run(uuid, "minecraft:recipe_unlocked", |c| {
            c.recipes.is_empty()
                || c.recipes
                    .iter()
                    .any(|want| ids.iter().any(|id| short(want) == short(id)))
        });
    }

    /// `item_used_on_block`: предмет в руке и блок под прицелом.
    /// `FilledBucketTrigger`: игрок получил ведро с существом.
    /// `SummonedEntityTrigger`: игрок призвал сущность (голем из тыквы).
    pub(crate) fn advancement_summoned_entity(
        &self,
        uuid: Uuid,
        entity: &str,
        target_equipment: &dyn Fn(&str) -> Option<(String, i32)>,
        slots: &[Option<Stack>],
        selected: usize,
    ) {
        self.advancement_run(uuid, "minecraft:summoned_entity", |c| {
            if c.target_unsupported {
                return false;
            }
            let entity_ok =
                c.entities.is_empty() || c.entities.iter().any(|w| self.entity_matches(w, entity));
            let target_ok = c.target_equipment.iter().all(|(slot, pred)| {
                target_equipment(slot)
                    .is_some_and(|(name, damage)| self.item_pred_matches(pred, &name, damage))
            });
            entity_ok && target_ok && self.player_pred_ok(c, slots, selected)
        });
    }

    pub(crate) fn advancement_filled_bucket(&self, uuid: Uuid, item: &str) {
        self.advancement_run(uuid, "minecraft:filled_bucket", |c| {
            c.items.is_empty() || c.items.iter().any(|w| self.item_matches(w, item))
        });
    }

    pub(crate) fn advancement_item_used_on_block(
        &self,
        uuid: Uuid,
        item: &str,
        block: &str,
        pos: [i32; 3],
    ) {
        self.advancement_run(uuid, "minecraft:item_used_on_block", |c| {
            !c.item_location_unsupported
                && (c.items.is_empty() || c.items.iter().any(|w| self.item_matches(w, item)))
                && (c.blocks.is_empty() || c.blocks.iter().any(|w| self.block_matches(w, block)))
                && c.smokey.is_none_or(|want| want == self.is_smokey_at(pos))
        });
    }

    /// `used_ender_eye`: условие `distance` по квадрату расстояния.
    pub(crate) fn advancement_used_ender_eye(&self, uuid: Uuid, sqr_distance: f64) {
        self.advancement_run(uuid, "minecraft:used_ender_eye", |c| {
            c.distance
                .is_none_or(|(min, max)| sqr_distance >= min && sqr_distance <= max)
        });
    }

    /// `changed_dimension`: `from`/`to` измерий.
    pub(crate) fn advancement_changed_dimension(&self, uuid: Uuid, from: &str, to: &str) {
        self.advancement_run(uuid, "minecraft:changed_dimension", |c| {
            (c.from_dims.is_empty() || c.from_dims.iter().any(|w| short(w) == short(from)))
                && (c.to_dims.is_empty() || c.to_dims.iter().any(|w| short(w) == short(to)))
        });
    }

    /// `brewed_potion`: зелье из `PotionContents`.
    pub(crate) fn advancement_brewed_potion(&self, uuid: Uuid, potion: Option<&str>) {
        self.advancement_run(uuid, "minecraft:brewed_potion", |c| {
            c.potions.is_empty()
                || potion.is_some_and(|p| c.potions.iter().any(|w| short(w) == short(p)))
        });
    }

    /// `enchanted_item`: зачарованный предмет и стоимость в уровнях.
    pub(crate) fn advancement_enchanted_item(&self, uuid: Uuid, item: &str, levels: i32) {
        self.advancement_run(uuid, "minecraft:enchanted_item", |c| {
            (c.items.is_empty() || c.items.iter().any(|w| self.item_matches(w, item)))
                && c.levels
                    .is_none_or(|(min, max)| levels >= min && levels <= max)
        });
    }

    /// `CrafterRecipeCraftedTrigger`: крафтер выбросил результат рецепта.
    pub(crate) fn advancement_crafter_recipe_crafted(&self, uuid: Uuid, recipe: &str) {
        self.advancement_run(uuid, "minecraft:crafter_recipe_crafted", |c| {
            c.recipes.is_empty() || c.recipes.iter().any(|w| short(w) == short(recipe))
        });
    }

    /// `LootTableTrigger`: игрок открыл контейнер с таблицей добычи.
    pub(crate) fn advancement_container_loot(&self, uuid: Uuid, table: &str) {
        self.advancement_run(uuid, "minecraft:player_generates_container_loot", |c| {
            c.loot_tables.is_empty() || c.loot_tables.iter().any(|w| short(w) == short(table))
        });
    }

    /// `EffectsChangedTrigger`: активные эффекты и (необязательно) источник.
    pub(crate) fn advancement_effects_changed(
        &self,
        uuid: Uuid,
        active: &[(&str, i32)],
        source: Option<&str>,
    ) {
        self.advancement_run(uuid, "minecraft:effects_changed", |c| {
            if c.unsupported_effects {
                return false;
            }
            if !c.source_entities.is_empty() {
                let Some(source) = source else { return false };
                if !c
                    .source_entities
                    .iter()
                    .any(|w| self.entity_matches(w, source))
                {
                    return false;
                }
            }
            c.effects_req.iter().all(|(name, amp)| {
                active.iter().any(|(have, level)| {
                    short(have) == short(name) && amp.is_none_or(|want| *level == want)
                })
            })
        });
    }

    /// `PlayerTrigger` для `minecraft:location`: биом, структуры, позиция,
    /// блок под ногами (`stepping_on`) и экипировка в точке игрока.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn advancement_location(
        &self,
        uuid: Uuid,
        pos: [f64; 3],
        biome: &str,
        on_block: Option<&str>,
        slots: &[Option<Stack>],
        selected: usize,
        inside: &dyn Fn(&str) -> bool,
    ) {
        self.advancement_run(uuid, "minecraft:location", |c| {
            if c.unsupported_location {
                return false;
            }
            let biome_ok = c.biomes.is_empty()
                || c.biomes
                    .iter()
                    .any(|w| w.strip_prefix('#').is_none() && short(w) == short(biome));
            let structure_ok = c.structures.is_empty()
                || c.structures
                    .iter()
                    .any(|w| w.strip_prefix('#').is_none() && inside(w));
            let position_ok = c.position.iter().all(|(axis, (min, max))| {
                let v = match axis.as_str() {
                    "x" => pos[0],
                    "y" => pos[1],
                    _ => pos[2],
                };
                v >= *min && v <= *max
            });
            let feet = [
                pos[0].floor() as i32,
                pos[1].floor() as i32,
                pos[2].floor() as i32,
            ];
            let dimension_ok = c
                .dimension
                .as_ref()
                .is_none_or(|want| short(want) == short(self.active_dim().key()));
            let block_ok = match &c.block {
                None => true,
                Some(want) => {
                    !c.block_unsupported
                        && self
                            .block_name_at(feet)
                            .is_some_and(|name| want.iter().any(|w| self.block_matches(w, &name)))
                }
            };
            let fluid_ok = match &c.fluid {
                None => true,
                Some(want) => {
                    !c.fluid_unsupported && {
                        let kind = self
                            .block(feet)
                            .and_then(|s| crate::fluids::state_fluid(&self.survival.placement, s))
                            .map(|f| f.kind);
                        want.iter().any(|w| self.fluid_matches(w, kind))
                    }
                }
            };
            let light_ok = match c.light {
                None => true,
                Some((min, max)) => self
                    .raw_brightness_at(feet)
                    .is_some_and(|v| v >= min && v <= max),
            };
            let smokey_ok = c.smokey.is_none_or(|want| want == self.is_smokey_at(feet));
            let sky_ok = c.can_see_sky.is_none_or(|want| {
                want == self.with_world(|w| w.sky_light_at(feet).is_some_and(|l| l >= 15))
            });
            let step_ok = c.stepping_on.is_empty()
                || (!c.stepping_on_unsupported
                    && on_block.is_some_and(|block| {
                        c.stepping_on
                            .iter()
                            .any(|want| self.block_matches(want, block))
                    }));
            let equipment_ok = c.equipment.is_empty()
                || (!c.equipment_unsupported
                    && c.equipment.iter().all(|(slot, items)| {
                        let Some(index) = equipment_slot(slot, selected) else {
                            return false;
                        };
                        slots.get(index).copied().flatten().is_some_and(|s| {
                            items
                                .iter()
                                .any(|want| self.item_matches(want, self.item_name(s.item)))
                        })
                    }));
            biome_ok
                && structure_ok
                && position_ok
                && dimension_ok
                && block_ok
                && fluid_ok
                && light_ok
                && smokey_ok
                && sky_ok
                && step_ok
                && equipment_ok
        });
    }

    /// `Level.getMaxLocalRawBrightness`: свет с учётом времени и погоды.
    fn raw_brightness_at(&self, pos: [i32; 3]) -> Option<i32> {
        let (sky, block) =
            self.with_world(|w| Some((w.sky_light_at(pos)?, w.block_light_at(pos)?)))?;
        let weather = self.vars.weather();
        let darken = self.sky_darken_now(self.active_dim(), weather.rain, weather.thunder);
        Some(i32::from(crate::mobs::raw_brightness(sky, block, darken)))
    }

    /// `FluidPredicate`: id или тег (`water`/`lava`).
    fn fluid_matches(&self, want: &str, kind: Option<crate::fluids::Kind>) -> bool {
        use crate::fluids::Kind;
        let Some(kind) = kind else {
            return false;
        };
        match short(want.trim_start_matches('#')) {
            "water" | "flowing_water" => kind == Kind::Water,
            "lava" | "flowing_lava" => kind == Kind::Lava,
            _ => false,
        }
    }

    /// `CampfireBlock.isLitCampfire`.
    fn is_lit_campfire(&self, state: u32) -> bool {
        matches!(
            self.state_name(state),
            "minecraft:campfire" | "minecraft:soul_campfire"
        ) && self.survival.placement.prop(state, "lit") != Some("false")
    }

    /// `CampfireBlock.isSmokeyPos`: зажжённый костёр под точкой, дым не
    /// перекрыт коллизией (виртуальный столб `Block.column(4, 0, 16)`).
    fn is_smokey_at(&self, pos: [i32; 3]) -> bool {
        const POST: [f64; 6] = [0.375, 0.0, 0.375, 0.625, 1.0, 0.625];
        for i in 1..=5 {
            let below = [pos[0], pos[1] - i, pos[2]];
            let Some(state) = self.block(below) else {
                return false;
            };
            if self.is_lit_campfire(state) {
                return true;
            }
            if self
                .collision_boxes(state)
                .iter()
                .any(|b| box_overlaps(*b, POST))
            {
                return self
                    .block([below[0], below[1] - 1, below[2]])
                    .is_some_and(|s| self.is_lit_campfire(s));
            }
        }
        false
    }

    /// `recipe_crafted`: id рецепта и использованные ингредиенты.
    /// `PlayerInteractTrigger`: игрок взаимодействовал с сущностью.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn advancement_player_interacted_with_entity(
        &self,
        uuid: Uuid,
        entity: &str,
        baby: bool,
        item: Option<&str>,
        slots: &[Option<Stack>],
        selected: usize,
        target_equipment: &dyn Fn(&str) -> Option<(String, i32)>,
    ) {
        self.advancement_run(uuid, "minecraft:player_interacted_with_entity", |c| {
            if c.player_pred_unsupported {
                return false;
            }
            let entity_ok =
                c.entities.is_empty() || c.entities.iter().any(|w| self.entity_matches(w, entity));
            let baby_ok = c.entity_baby.is_none_or(|want| want == baby);
            let item_ok = c.items.is_empty()
                || item.is_some_and(|name| c.items.iter().any(|w| self.item_matches(w, name)));
            let target_ok = !c.target_unsupported
                && c.target_equipment.iter().all(|(slot, pred)| {
                    target_equipment(slot)
                        .is_some_and(|(name, damage)| self.item_pred_matches(pred, &name, damage))
                });
            entity_ok && baby_ok && item_ok && target_ok && self.player_pred_ok(c, slots, selected)
        });
    }

    /// `ItemPredicate`: предметы и точное значение `minecraft:damage`.
    fn item_pred_matches(&self, pred: &ItemPred, name: &str, damage: i32) -> bool {
        (pred.items.is_empty() || pred.items.iter().any(|w| self.item_matches(w, name)))
            && pred.damage.is_none_or(|want| want == damage)
    }

    /// `conditions.player` критерия (общая часть триггеров взаимодействия).
    fn player_pred_ok(
        &self,
        c: &AdvancementCriterion,
        slots: &[Option<Stack>],
        selected: usize,
    ) -> bool {
        if c.player_pred_unsupported {
            return false;
        }
        c.player_pred.as_ref().is_none_or(|pred| {
            let equipment = |slot: &str| -> Option<(String, i32)> {
                let index = equipment_slot(slot, selected)?;
                slots
                    .get(index)
                    .copied()
                    .flatten()
                    .map(|s| (self.item_name(s.item).to_owned(), s.damage))
            };
            self.entity_pred_matches(pred, &equipment, "minecraft:player", false)
        })
    }

    /// `TameAnimalTrigger`: животное приручено.
    pub(crate) fn advancement_tame_animal(
        &self,
        uuid: Uuid,
        entity: &str,
        baby: bool,
        slots: &[Option<Stack>],
        selected: usize,
    ) {
        self.advancement_run(uuid, "minecraft:tame_animal", |c| {
            let entity_ok =
                c.entities.is_empty() || c.entities.iter().any(|w| self.entity_matches(w, entity));
            let baby_ok = c.entity_baby.is_none_or(|want| want == baby);
            entity_ok && baby_ok && !c.target_unsupported && self.player_pred_ok(c, slots, selected)
        });
    }

    /// Проверка `EntityPredicate` (только поддержанные поля).
    fn entity_pred_matches(
        &self,
        pred: &EntityPred,
        equipment: &dyn Fn(&str) -> Option<(String, i32)>,
        entity: &str,
        baby: bool,
    ) -> bool {
        match pred {
            EntityPred::Inverted(term) => !self.entity_pred_matches(term, equipment, entity, baby),
            EntityPred::AnyOf(list) => list
                .iter()
                .any(|term| self.entity_pred_matches(term, equipment, entity, baby)),
            EntityPred::AllOf(list) => list
                .iter()
                .all(|term| self.entity_pred_matches(term, equipment, entity, baby)),
            EntityPred::Properties {
                equipment: eq,
                entities,
                baby: want_baby,
            } => {
                (entities.is_empty()
                    || entities
                        .iter()
                        .any(|want| self.entity_matches(want, entity)))
                    && want_baby.is_none_or(|want| want == baby)
                    && eq.iter().all(|(slot, pred)| {
                        equipment(slot).is_some_and(|(name, damage)| {
                            self.item_pred_matches(pred, &name, damage)
                        })
                    })
            }
        }
    }

    pub(crate) fn advancement_recipe_crafted(&self, uuid: Uuid, recipe: &str, used: &[String]) {
        self.advancement_run(uuid, "minecraft:recipe_crafted", |c| {
            if !c.recipes.is_empty() && !c.recipes.iter().any(|w| short(w) == short(recipe)) {
                return false;
            }
            // `RecipeCraftedTrigger.TriggerInstance.matches`: каждый предикат
            // съедает один подходящий ингредиент (мультимножество).
            let mut remaining: Vec<&String> = used.iter().collect();
            for pred in &c.ingredients {
                let Some(pos) = remaining
                    .iter()
                    .position(|name| pred.iter().any(|w| self.item_matches(w, name)))
                else {
                    return false;
                };
                remaining.remove(pos);
            }
            true
        });
    }

    /// `bred_animals`: ребёнок, родитель и партнёр.
    pub(crate) fn advancement_bred_animals(
        &self,
        uuid: Uuid,
        child: &str,
        parent: &str,
        partner: &str,
    ) {
        self.advancement_run(uuid, "minecraft:bred_animals", |c| {
            let ok = |want: &[String], name: &str| {
                want.is_empty() || want.iter().any(|w| self.entity_matches(w, name))
            };
            ok(&c.child_entities, child)
                && ok(&c.parent_entities, parent)
                && ok(&c.partner_entities, partner)
        });
    }

    /// `enter_block`: блок, в который вошёл игрок (`conditions.blocks`).
    pub(crate) fn advancement_enter_block(&self, uuid: Uuid, block: &str) {
        self.advancement_run(uuid, "minecraft:enter_block", |c| {
            c.blocks.is_empty() || c.blocks.iter().any(|w| self.block_matches(w, block))
        });
    }

    /// `shot_crossbow`: предмет, из которого стреляли (`conditions.item`).
    pub(crate) fn advancement_shot_crossbow(&self, uuid: Uuid, item: &str) {
        self.advancement_run(uuid, "minecraft:shot_crossbow", |c| {
            c.items.is_empty() || c.items.iter().any(|w| self.item_matches(w, item))
        });
    }

    /// `used_totem`: предмет тотема (`conditions.item`).
    pub(crate) fn advancement_used_totem(&self, uuid: Uuid, item: &str) {
        self.advancement_run(uuid, "minecraft:used_totem", |c| {
            c.items.is_empty() || c.items.iter().any(|w| self.item_matches(w, item))
        });
    }

    /// `nether_travel`: горизонталь от входа в Нижний мир.
    pub(crate) fn advancement_nether_travel(&self, uuid: Uuid, horizontal: f64) {
        self.advancement_run(uuid, "minecraft:nether_travel", |c| {
            c.travel_distance
                .is_none_or(|(min, max)| horizontal >= min && horizontal <= max)
        });
    }

    /// `channeled_lightning`: каждый `victims` должен найтись среди поражённых.
    pub(crate) fn advancement_channeled_lightning(&self, uuid: Uuid, struck: &[&str]) {
        self.advancement_run(uuid, "minecraft:channeled_lightning", |c| {
            c.victims
                .iter()
                .all(|want| struck.iter().any(|name| self.entity_matches(want, name)))
        });
    }

    /// `target_hit`: сила сигнала и горизонталь до точки попадания.
    pub(crate) fn advancement_target_hit(&self, uuid: Uuid, signal: i32, horizontal: f64) {
        self.advancement_run(uuid, "minecraft:target_hit", |c| {
            c.signal_strength
                .is_none_or(|(min, max)| signal >= min && signal <= max)
                && c.proj_distance
                    .is_none_or(|(min, max)| horizontal >= min && horizontal <= max)
        });
    }

    /// `player_hurt_entity`: цель, прямой источник и тип урона.
    /// `PlayerKillEntityTrigger`: игрок убил сущность.
    pub(crate) fn advancement_player_killed_entity(
        &self,
        uuid: Uuid,
        victim: &str,
        victim_pos: [f64; 3],
        killer_pos: Option<[f64; 3]>,
        damage: Option<&str>,
    ) {
        self.advancement_run(uuid, "minecraft:player_killed_entity", |c| {
            if c.victim_unsupported || c.killing_blow_unsupported {
                return false;
            }
            let entity_ok =
                c.entities.is_empty() || c.entities.iter().any(|w| self.entity_matches(w, victim));
            let distance_ok = c.victim_distance.is_none_or(|(min, max)| {
                killer_pos.is_some_and(|k| {
                    let dx = victim_pos[0] - k[0];
                    let dz = victim_pos[2] - k[2];
                    let d = (dx * dx + dz * dz).sqrt();
                    d >= min && d <= max
                })
            });
            let blow_ok = c.killing_blow_tags.iter().all(|tag| {
                damage.is_some_and(|damage| {
                    self.survival
                        .damage_tags
                        .get(tag)
                        .is_some_and(|set| set.contains(damage))
                })
            });
            entity_ok && distance_ok && blow_ok
        });
    }

    pub(crate) fn advancement_player_hurt_entity(
        &self,
        uuid: Uuid,
        victim: &str,
        direct: &str,
        damage_type: &str,
    ) {
        self.advancement_run(uuid, "minecraft:player_hurt_entity", |c| {
            (c.entities.is_empty() || c.entities.iter().any(|w| self.entity_matches(w, victim)))
                && self.damage_matches(c, direct, damage_type)
        });
    }

    /// `entity_hurt_player`: игрок получил урон (и был ли он заблокирован).
    pub(crate) fn advancement_entity_hurt_player(
        &self,
        uuid: Uuid,
        direct: &str,
        damage_type: &str,
        blocked: bool,
    ) {
        self.advancement_run(uuid, "minecraft:entity_hurt_player", |c| {
            c.blocked.is_none_or(|b| b == blocked) && self.damage_matches(c, direct, damage_type)
        });
    }

    /// `DamagePredicate`: прямой источник и теги типа урона.
    fn damage_matches(&self, c: &AdvancementCriterion, direct: &str, damage_type: &str) -> bool {
        (c.direct_entities.is_empty()
            || c.direct_entities
                .iter()
                .any(|w| self.entity_matches(w, direct)))
            && c.damage_tags_req.iter().all(|tag| {
                self.survival
                    .damage_tags
                    .get(tag)
                    .is_some_and(|set| set.contains(damage_type))
            })
    }

    /// Тип сущности или `#тег` (`entity_type_tags`).
    fn entity_matches(&self, want: &str, name: &str) -> bool {
        if let Some(tag) = want.strip_prefix('#') {
            let key = if tag.contains(':') {
                tag.to_owned()
            } else {
                format!("minecraft:{tag}")
            };
            return self
                .survival
                .entity_type_tags
                .get(&key)
                .is_some_and(|set| set.contains(name));
        }
        short(want) == short(name)
    }

    fn advancement_run(
        &self,
        uuid: Uuid,
        trigger: &str,
        accept: impl Fn(&AdvancementCriterion) -> bool,
    ) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64);
        let mut changed = Vec::new();
        let mut completed = Vec::new();
        {
            let mut all = self.advancement_progress.lock().unwrap();
            let progress = all.entry(uuid).or_default();
            // Событие может закрыть и родителя, и ребёнка (первое размножение:
            // `breed_an_animal` + критерий `minecraft:cow` в `bred_all_animals`).
            // Поэтому проходим, пока появляются новые критерии: только что
            // выполненный родитель открывает своих детей в этом же событии.
            loop {
                let mut pass = false;
                for def in &self.survival.advancements {
                    if !self.adv_parent_done(progress, def.parent.as_deref()) {
                        continue;
                    }
                    let was = progress
                        .get(&def.id)
                        .is_some_and(|p| Self::adv_done(p, def));
                    let mut touched = false;
                    {
                        let entry = progress.entry(def.id.clone()).or_default();
                        for c in &def.criteria {
                            if c.trigger != trigger || !accept(c) {
                                continue;
                            }
                            if !entry.contains_key(&c.name) {
                                entry.insert(c.name.clone(), now);
                                touched = true;
                            }
                        }
                    }
                    if touched {
                        pass = true;
                        changed.push(def.id.clone());
                        let entry = &progress[&def.id];
                        if !was && Self::adv_done(entry, def) {
                            completed.push(def.id.clone());
                        }
                    }
                }
                if !pass {
                    break;
                }
            }
        }
        changed.sort();
        changed.dedup();
        completed.sort();
        completed.dedup();
        if !changed.is_empty() {
            self.send_advancement_progress(uuid, &changed);
        }
        for id in completed {
            self.announce_advancement(uuid, &id);
        }
    }

    /// `ItemPredicate.test` по имени: предмет или `#тег`.
    fn item_matches(&self, want: &str, name: &str) -> bool {
        if let Some(tag) = want.strip_prefix('#') {
            let key = if tag.contains(':') {
                tag.to_owned()
            } else {
                format!("minecraft:{tag}")
            };
            return self
                .survival
                .item_tags
                .get(&key)
                .is_some_and(|set| set.contains(name));
        }
        short(want) == short(name)
    }

    /// `BlockPredicate` по имени блока: блок или `#тег`.
    fn block_matches(&self, want: &str, name: &str) -> bool {
        if let Some(tag) = want.strip_prefix('#') {
            let key = if tag.contains(':') {
                tag.to_owned()
            } else {
                format!("minecraft:{tag}")
            };
            return self
                .survival
                .block_tags
                .get(&key)
                .is_some_and(|set| set.contains(name));
        }
        short(want) == short(name)
    }

    fn announce_advancement(&self, uuid: Uuid, id: &str) {
        let Some(def) = self.adv(id) else {
            return;
        };
        let Some(display) = &def.display else {
            return;
        };
        if !display.announce || display.hidden {
            return;
        }
        let name = self
            .hub
            .players
            .lock()
            .unwrap()
            .get(&uuid)
            .map(|p| p.entry.name.clone())
            .unwrap_or_default();
        let kind = match display.frame {
            1 => "challenge",
            2 => "goal",
            _ => "task",
        };
        let message = serde_json::json!({
            "translate": format!("chat.type.advancement.{kind}"),
            "with": [{"text": name}, display.title],
        });
        self.hub.send(
            None,
            play::system_chat(self.ids.system_chat, &message.to_string(), false),
        );
    }

    fn adv_body(w: &mut Writer, def: &AdvancementDef, icon_id: Option<i32>) {
        // parent: optional identifier.
        w.bool(def.parent.is_some());
        if let Some(parent) = &def.parent {
            w.string(&format!("minecraft:{parent}"));
        }
        // display: optional.
        w.bool(def.display.is_some());
        if let Some(d) = &def.display {
            text::write_component(w, &d.title);
            text::write_component(w, &d.description);
            // icon: ItemStackTemplate (item, count, пустой patch).
            w.var_int(icon_id.unwrap_or(0))
                .var_int(1)
                .var_int(0)
                .var_int(0);
            w.var_int(d.frame);
            let mut flags = 0i32;
            if d.background.is_some() {
                flags |= 1;
            }
            if d.show_toast {
                flags |= 2;
            }
            if d.hidden {
                flags |= 4;
            }
            w.i32(flags);
            if let Some(bg) = &d.background {
                w.string(bg);
            }
        }
        // requirements: list<list<string>>.
        w.var_int(def.requirements.len() as i32);
        for group in &def.requirements {
            w.var_int(group.len() as i32);
            for name in group {
                w.string(name);
            }
        }
        w.bool(true);
    }

    fn advancement_packets(
        &self,
        reset: bool,
        added: &[&AdvancementDef],
        progress: &[(String, FastMap<String, i64>)],
    ) -> Vec<u8> {
        let mut w = Writer::new();
        w.var_int(self.ids.update_advancements).bool(reset);
        w.var_int(added.len() as i32);
        for def in added {
            w.string(&format!("minecraft:{}", def.id));
            let icon = def.display.as_ref().and_then(|d| self.item_id(&d.icon));
            Self::adv_body(&mut w, def, icon);
            w.f32(def.x).f32(def.y);
        }
        w.var_int(0); // removed
        w.var_int(progress.len() as i32);
        for (id, criteria) in progress {
            w.string(&format!("minecraft:{id}"));
            w.var_int(criteria.len() as i32);
            for (name, at) in criteria {
                w.string(name).bool(*at > 0).i64(*at);
            }
        }
        w.bool(true);
        w.buf
    }

    /// Полный пакет при входе: все определения и накопленный прогресс.
    pub(crate) fn full_advancement_packet(&self, uuid: Uuid) -> Option<Vec<u8>> {
        if self.survival.advancements.is_empty() {
            return None;
        }
        let all: Vec<&AdvancementDef> = self.survival.advancements.iter().collect();
        let progress = self
            .advancement_progress
            .lock()
            .unwrap()
            .get(&uuid)
            .map(|p| {
                p.iter()
                    .map(|(id, c)| (id.clone(), c.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Some(self.advancement_packets(true, &all, &progress))
    }

    fn send_advancement_progress(&self, uuid: Uuid, ids: &[String]) {
        if ids.is_empty() {
            return;
        }
        let all = self.advancement_progress.lock().unwrap();
        let progress: Vec<(String, FastMap<String, i64>)> = ids
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    all.get(&uuid)
                        .and_then(|p| p.get(id))
                        .cloned()
                        .unwrap_or_default(),
                )
            })
            .collect();
        drop(all);
        let packet = self.advancement_packets(false, &[], &progress);
        self.send_to(uuid, packet);
    }

    /// Загруженный с диска прогресс.
    pub(crate) fn load_advancement_progress(
        &self,
        uuid: Uuid,
        data: Vec<(String, Vec<(String, i64)>)>,
    ) {
        let map: AdvProgress = data
            .into_iter()
            .map(|(id, criteria)| (short(&id).to_owned(), criteria.into_iter().collect()))
            .collect();
        self.advancement_progress.lock().unwrap().insert(uuid, map);
    }

    /// Прогресс для `advancements/<uuid>.json`.
    pub(crate) fn collect_advancement_progress(
        &self,
        uuid: Uuid,
    ) -> Vec<(String, Vec<(String, i64)>)> {
        self.advancement_progress
            .lock()
            .unwrap()
            .get(&uuid)
            .map(|p| {
                p.iter()
                    .filter(|(_, c)| !c.is_empty())
                    .map(|(id, c)| (id.clone(), c.iter().map(|(k, &v)| (k.clone(), v)).collect()))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(crate) fn forget_advancements(&self, uuid: Uuid) {
        self.advancement_progress.lock().unwrap().remove(&uuid);
    }

    /// `/advancement grant|revoke`: список id для `everything` и потомков.
    pub(crate) fn advancement_ids(&self) -> Vec<String> {
        self.survival
            .advancements
            .iter()
            .map(|a| a.id.clone())
            .collect()
    }

    fn adv_descendants(&self, root: &str) -> Vec<String> {
        let mut out = vec![root.to_owned()];
        let mut i = 0;
        while i < out.len() {
            let parent = out[i].clone();
            for def in &self.survival.advancements {
                if def.parent.as_deref() == Some(parent.as_str()) {
                    out.push(def.id.clone());
                }
            }
            i += 1;
        }
        out
    }

    fn adv_ancestors(&self, id: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut cur = self.adv(id).and_then(|d| d.parent.clone());
        while let Some(p) = cur {
            cur = self.adv(&p).and_then(|d| d.parent.clone());
            out.push(p);
        }
        out
    }

    /// Выдать критерии достижения (`criterion = None` — все).
    pub(crate) fn advancement_grant(&self, uuid: Uuid, id: &str, criterion: Option<&str>) {
        let Some(def) = self.adv(id) else {
            return;
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64);
        let mut changed = vec![id.to_owned()];
        {
            let mut all = self.advancement_progress.lock().unwrap();
            let entry = all
                .entry(uuid)
                .or_default()
                .entry(id.to_owned())
                .or_default();
            let names: Vec<String> = match criterion {
                Some(name) => def
                    .criteria
                    .iter()
                    .filter(|c| c.name == name)
                    .map(|c| c.name.clone())
                    .collect(),
                None => def.criteria.iter().map(|c| c.name.clone()).collect(),
            };
            for name in names {
                entry.entry(name).or_insert(now);
            }
        }
        // Родительские достижения тоже нужны для видимости.
        for p in self.adv_ancestors(id) {
            changed.push(p.clone());
        }
        self.send_advancement_progress(uuid, &changed);
    }

    pub(crate) fn advancement_revoke(&self, uuid: Uuid, id: &str, criterion: Option<&str>) {
        let mut changed = vec![id.to_owned()];
        {
            let mut all = self.advancement_progress.lock().unwrap();
            let progress = all.entry(uuid).or_default();
            match criterion {
                Some(name) => {
                    if let Some(entry) = progress.get_mut(id) {
                        entry.remove(name);
                        if entry.is_empty() {
                            progress.remove(id);
                        }
                    }
                }
                None => {
                    progress.remove(id);
                }
            }
        }
        for p in self.adv_ancestors(id) {
            changed.push(p);
        }
        self.send_advancement_progress(uuid, &changed);
    }

    /// `everything`: id всех достижений.
    pub(crate) fn advancement_all(&self) -> Vec<String> {
        self.advancement_ids()
    }

    /// `from`: достижение и потомки; `through`/`until`: предки.
    pub(crate) fn advancement_from(&self, id: &str) -> Vec<String> {
        self.adv_descendants(id)
    }

    pub(crate) fn advancement_through(&self, id: &str, include: bool) -> Vec<String> {
        let mut out = self.adv_ancestors(id);
        if include {
            out.insert(0, id.to_owned());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn location_advancement(location: serde_json::Value) -> AdvancementCriterion {
        let json = serde_json::json!({
            "criteria": {
                "here": {
                    "trigger": "minecraft:location",
                    "conditions": {
                        "player": {
                            "type": "minecraft:entity_properties",
                            "entity": "this",
                            "predicate": {"minecraft:location": location}
                        }
                    }
                }
            },
            "requirements": [["here"]]
        });
        let def = parse_advancement("test/pos", &json).expect("advancement");
        def.criteria.into_iter().next().expect("criterion")
    }

    #[test]
    fn location_position_parses_axes() {
        let c = location_advancement(serde_json::json!({
            "position": {
                "x": {"min": 1.0, "max": 2.0},
                "y": 3.0,
                "z": {"max": 4.5}
            }
        }));
        assert!(!c.unsupported_location);
        assert_eq!(
            c.position,
            vec![
                ("x".to_owned(), (1.0, 2.0)),
                ("y".to_owned(), (3.0, 3.0)),
                ("z".to_owned(), (f64::NEG_INFINITY, 4.5)),
            ]
        );
    }

    #[test]
    fn location_position_exclusive_is_unsupported() {
        let c = location_advancement(serde_json::json!({
            "position": {"x": {"min_exclusive": 0.0}}
        }));
        assert!(c.unsupported_location);
    }

    #[test]
    fn location_extra_fields_parse() {
        let c = location_advancement(serde_json::json!({
            "dimension": "minecraft:the_nether",
            "smokey": true,
            "light": {"light": {"min": 0, "max": 7}},
            "block": {"blocks": ["minecraft:beehive", "#minecraft:campfires"]},
            "fluid": {"fluid": "#minecraft:water"},
            "can_see_sky": false
        }));
        assert!(!c.unsupported_location);
        assert_eq!(c.dimension.as_deref(), Some("the_nether"));
        assert_eq!(c.smokey, Some(true));
        assert_eq!(c.light, Some((0, 7)));
        assert_eq!(
            c.block,
            Some(vec![
                "minecraft:beehive".to_owned(),
                "#minecraft:campfires".to_owned()
            ])
        );
        assert_eq!(c.fluid, Some(vec!["#minecraft:water".to_owned()]));
        assert!(!c.fluid_unsupported);
        assert_eq!(c.can_see_sky, Some(false));
    }

    #[test]
    fn location_block_state_and_fluid_nbt_are_unsupported() {
        let c = location_advancement(serde_json::json!({
            "block": {"blocks": "minecraft:stone", "state": {"lit": "true"}}
        }));
        assert!(c.block_unsupported);
        assert!(!c.unsupported_location);

        let c = location_advancement(serde_json::json!({
            "fluid": {"fluid": "minecraft:water", "nbt": 1}
        }));
        assert!(c.fluid_unsupported);
        assert!(!c.unsupported_location);

        let c = location_advancement(serde_json::json!({
            "light": {"light": {"min_exclusive": 1}}
        }));
        assert!(c.unsupported_location);
    }

    #[test]
    fn entity_pred_parses_piglin_player_tree() {
        let json = serde_json::json!({
            "criteria": {
                "c": {
                    "trigger": "minecraft:player_interacted_with_entity",
                    "conditions": {
                        "item": {"items": "#minecraft:piglin_loved"},
                        "entity": {
                            "type": "minecraft:entity_properties",
                            "entity": "this",
                            "predicate": {
                                "minecraft:entity_type": "minecraft:piglin",
                                "minecraft:flags": {"is_baby": false}
                            }
                        },
                        "player": {
                            "type": "minecraft:inverted",
                            "term": {
                                "type": "minecraft:any_of",
                                "terms": [
                                    {
                                        "type": "minecraft:entity_properties",
                                        "entity": "this",
                                        "predicate": {
                                            "minecraft:equipment": {
                                                "head": {"items": "#minecraft:piglin_safe_armor"}
                                            }
                                        }
                                    },
                                    {
                                        "type": "minecraft:entity_properties",
                                        "entity": "this",
                                        "predicate": {
                                            "minecraft:equipment": {
                                                "feet": {"items": "#minecraft:piglin_safe_armor"}
                                            }
                                        }
                                    }
                                ]
                            }
                        }
                    }
                }
            },
            "requirements": [["c"]]
        });
        let def = parse_advancement("test/interact", &json).expect("advancement");
        let c = &def.criteria[0];
        assert!(!c.player_pred_unsupported);
        assert_eq!(c.entity_baby, Some(false));
        assert_eq!(c.items, vec!["#minecraft:piglin_loved".to_owned()]);
        assert_eq!(c.entities, vec!["minecraft:piglin".to_owned()]);
        match &c.player_pred {
            Some(EntityPred::Inverted(term)) => match term.as_ref() {
                EntityPred::AnyOf(list) => assert_eq!(list.len(), 2),
                _ => panic!("ожидался any_of"),
            },
            _ => panic!("ожидался inverted"),
        }
    }

    #[test]
    fn entity_pred_equipment_unknown_component_is_unsupported() {
        let json = serde_json::json!({
            "criteria": {
                "c": {
                    "trigger": "minecraft:player_interacted_with_entity",
                    "conditions": {
                        "player": {
                            "type": "minecraft:entity_properties",
                            "entity": "this",
                            "predicate": {
                                "minecraft:equipment": {
                                    "head": {
                                        "items": "minecraft:golden_helmet",
                                        "components": {"minecraft:enchantments": {}}
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "requirements": [["c"]]
        });
        let def = parse_advancement("test/interact2", &json).expect("advancement");
        assert!(def.criteria[0].player_pred_unsupported);
    }

    #[test]
    fn target_equipment_body_damage_parses() {
        let json = serde_json::json!({
            "criteria": {
                "repair_wolf_armor": {
                    "trigger": "minecraft:player_interacted_with_entity",
                    "conditions": {
                        "entity": {
                            "type": "minecraft:entity_properties",
                            "entity": "this",
                            "predicate": {
                                "minecraft:entity_type": "minecraft:wolf",
                                "minecraft:equipment": {
                                    "body": {
                                        "items": "minecraft:wolf_armor",
                                        "components": {"minecraft:damage": 0}
                                    }
                                }
                            }
                        },
                        "item": {"items": "minecraft:armadillo_scute"}
                    }
                }
            },
            "requirements": [["repair_wolf_armor"]]
        });
        let def = parse_advancement("test/repair_wolf", &json).expect("advancement");
        let c = &def.criteria[0];
        assert!(!c.target_unsupported);
        assert_eq!(c.entities, vec!["minecraft:wolf".to_owned()]);
        assert_eq!(
            c.target_equipment,
            vec![(
                "body".to_owned(),
                ItemPred {
                    items: vec!["minecraft:wolf_armor".to_owned()],
                    damage: Some(0)
                }
            )]
        );
        assert_eq!(c.items, vec!["minecraft:armadillo_scute".to_owned()]);
    }

    #[test]
    fn target_equipment_unknown_field_is_unsupported() {
        let json = serde_json::json!({
            "criteria": {
                "c": {
                    "trigger": "minecraft:player_interacted_with_entity",
                    "conditions": {
                        "entity": {
                            "type": "minecraft:entity_properties",
                            "entity": "this",
                            "predicate": {
                                "minecraft:components": {
                                    "minecraft:frog/variant": "minecraft:cold"
                                }
                            }
                        }
                    }
                }
            },
            "requirements": [["c"]]
        });
        let def = parse_advancement("test/frog", &json).expect("advancement");
        assert!(def.criteria[0].target_unsupported);
    }

    #[test]
    fn tame_animal_without_conditions_matches_any() {
        let json = serde_json::json!({
            "criteria": {
                "tamed_animal": {"trigger": "minecraft:tame_animal"}
            },
            "requirements": [["tamed_animal"]]
        });
        let def = parse_advancement("test/tame", &json).expect("advancement");
        let c = &def.criteria[0];
        assert_eq!(c.trigger, "minecraft:tame_animal");
        assert!(c.entities.is_empty());
        assert!(c.items.is_empty());
        assert!(c.entity_baby.is_none());
        assert!(c.player_pred.is_none());
        assert!(!c.player_pred_unsupported);
    }

    #[test]
    fn killed_entity_distance_and_killing_blow_parse() {
        let json = serde_json::json!({
            "criteria": {
                "c": {
                    "trigger": "minecraft:player_killed_entity",
                    "conditions": {
                        "entity": {
                            "type": "minecraft:entity_properties",
                            "entity": "this",
                            "predicate": {
                                "minecraft:distance": {"horizontal": {"min": 50.0}},
                                "minecraft:entity_type": "minecraft:skeleton"
                            }
                        },
                        "killing_blow": {
                            "tags": [{"expected": true, "id": "#minecraft:is_projectile"}]
                        }
                    }
                }
            },
            "requirements": [["c"]]
        });
        let def = parse_advancement("test/sniper", &json).expect("advancement");
        let c = &def.criteria[0];
        assert!(!c.victim_unsupported);
        assert!(!c.killing_blow_unsupported);
        assert_eq!(c.victim_distance, Some((50.0, f64::INFINITY)));
        assert_eq!(
            c.killing_blow_tags,
            vec!["minecraft:is_projectile".to_owned()]
        );
        assert_eq!(c.entities, vec!["minecraft:skeleton".to_owned()]);
    }

    #[test]
    fn killed_entity_unsupported_fields() {
        let json = serde_json::json!({
            "criteria": {
                "c": {
                    "trigger": "minecraft:player_killed_entity",
                    "conditions": {
                        "entity": {
                            "type": "minecraft:entity_properties",
                            "entity": "this",
                            "predicate": {
                                "minecraft:equipment": {"head": {"items": "minecraft:white_banner"}}
                            }
                        },
                        "killing_blow": {
                            "direct_entity": {"minecraft:entity_type": "minecraft:wind_charge"}
                        }
                    }
                }
            },
            "requirements": [["c"]]
        });
        let def = parse_advancement("test/sniper2", &json).expect("advancement");
        let c = &def.criteria[0];
        assert!(c.victim_unsupported);
        assert!(c.killing_blow_unsupported);
    }

    #[test]
    fn item_used_on_block_smokey_parses() {
        let json = serde_json::json!({
            "criteria": {
                "c": {
                    "trigger": "minecraft:item_used_on_block",
                    "conditions": {
                        "location": {
                            "type": "minecraft:all_of",
                            "terms": [
                                {
                                    "type": "minecraft:location_check",
                                    "predicate": {
                                        "block": {"blocks": "#minecraft:beehives"},
                                        "smokey": true
                                    }
                                },
                                {
                                    "type": "minecraft:match_tool",
                                    "predicate": {"items": "minecraft:glass_bottle"}
                                }
                            ]
                        }
                    }
                }
            },
            "requirements": [["c"]]
        });
        let def = parse_advancement("test/honey", &json).expect("advancement");
        let c = &def.criteria[0];
        assert!(!c.item_location_unsupported);
        assert_eq!(c.smokey, Some(true));
        assert_eq!(c.blocks, vec!["#minecraft:beehives".to_owned()]);
        assert_eq!(c.items, vec!["minecraft:glass_bottle".to_owned()]);
    }

    #[test]
    fn item_used_on_block_unknown_location_is_unsupported() {
        let json = serde_json::json!({
            "criteria": {
                "c": {
                    "trigger": "minecraft:item_used_on_block",
                    "conditions": {
                        "location": {
                            "type": "minecraft:location_check",
                            "predicate": {
                                "block": {
                                    "blocks": "minecraft:beehive",
                                    "state": {"honey_level": "5"}
                                },
                                "smokey": true
                            }
                        }
                    }
                }
            },
            "requirements": [["c"]]
        });
        let def = parse_advancement("test/honey2", &json).expect("advancement");
        assert!(def.criteria[0].item_location_unsupported);
    }

    #[test]
    fn filled_bucket_cod_parses() {
        let json = serde_json::json!({
            "criteria": {
                "cod_bucket": {
                    "trigger": "minecraft:filled_bucket",
                    "conditions": {"item": {"items": "minecraft:cod_bucket"}}
                }
            },
            "requirements": [["cod_bucket"]]
        });
        let def = parse_advancement("test/fish", &json).expect("advancement");
        let c = &def.criteria[0];
        assert_eq!(c.trigger, "minecraft:filled_bucket");
        assert_eq!(c.items, vec!["minecraft:cod_bucket".to_owned()]);
    }

    #[test]
    fn summoned_entity_parses_entity_type() {
        let json = serde_json::json!({
            "criteria": {
                "summoned_golem": {
                    "trigger": "minecraft:summoned_entity",
                    "conditions": {
                        "entity": {
                            "type": "minecraft:entity_properties",
                            "entity": "this",
                            "predicate": {"minecraft:entity_type": "minecraft:iron_golem"}
                        }
                    }
                }
            },
            "requirements": [["summoned_golem"]]
        });
        let def = parse_advancement("test/golem", &json).expect("advancement");
        let c = &def.criteria[0];
        assert_eq!(c.trigger, "minecraft:summoned_entity");
        assert_eq!(c.entities, vec!["minecraft:iron_golem".to_owned()]);
        assert!(!c.target_unsupported);
        assert!(c.player_pred.is_none());
    }
}
