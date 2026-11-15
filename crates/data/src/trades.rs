//! Сделки жителей (`villager_trade` и `trade_set` 26.3).
//!
//! Данные лежат отдельными реестрами: `data/minecraft/villager_trade/**`
//! (описание одной сделки), `data/minecraft/trade_set/<profession>/level_N.json`
//! (сколько сделок уровня и тег-список) и `data/minecraft/tags/villager_trade/**`.
//! Теги резолвятся по именам, без числовых ID.

use crate::DataError;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Предмет сделки (`wants`/`additional_wants`/`gives`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TradeItem {
    pub id: String,
    pub count: i32,
    /// `minecraft:potion_contents.potion` (бутылка воды у странствующего).
    pub potion: Option<String>,
}

/// `given_item_modifier` с зачарованием (`enchant_randomly` /
/// `enchant_with_levels`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnchantSpec {
    /// `enchant_randomly` (книга) или `enchant_with_levels` (экипировка).
    pub random: bool,
    /// `levels {min, max}` для `enchant_with_levels`.
    pub levels: Option<(i32, i32)>,
    /// Разрешённые зачарования (тег раскрыт).
    pub options: Vec<String>,
    pub only_compatible: bool,
    pub include_cost: bool,
}

/// `given_item_modifier` `exploration_map`: карта к ближайшей структуре.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExplorationMapSpec {
    /// Тег структур (`#minecraft:on_jungle_pyramid_maps`) или id.
    pub destination: String,
    /// `map_decoration_type` (например, `minecraft:jungle_temple`).
    pub decoration: String,
    pub zoom: u8,
    pub search_radius: i32,
    pub skip_existing: bool,
}

/// Одна сделка (`VillagerTrade`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TradeDef {
    pub wants: TradeItem,
    pub additional: Option<TradeItem>,
    pub gives: TradeItem,
    pub max_uses: i32,
    pub xp: i32,
    /// `reputation_discount` — `MerchantOffer.priceMultiplier` (f32 в JSON).
    pub price_mul: f32,
    /// `merchant_predicate`: допустимые варианты жителя.
    pub variants: Option<Vec<String>>,
    pub enchant: Option<EnchantSpec>,
    /// `given_item_modifier` `exploration_map`.
    pub exploration: Option<ExplorationMapSpec>,
    /// `double_trade_price_enchantments` (имена зачарований).
    pub double_price: Vec<String>,
    /// Модификатор не поддержан (например, карты исследователя).
    pub unsupported: bool,
}

/// `TradeSet`: сколько сделок выбрать и из какого тега.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TradeSetDef {
    pub amount: i32,
    pub trades: Vec<String>,
    pub allow_duplicates: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Trades {
    /// `(профессия без "minecraft:", уровень) -> набор`.
    pub sets: HashMap<(String, i32), TradeSetDef>,
    /// `id` сделки (`minecraft:farmer/1/wheat_emerald`) -> описание.
    pub defs: HashMap<String, TradeDef>,
    /// Теги зачарований (`minecraft:tradeable` и т.п.).
    pub enchant_tags: HashMap<String, Vec<String>>,
}

impl Trades {
    pub fn set(&self, profession: &str, level: i32) -> Option<&TradeSetDef> {
        self.sets.get(&(profession.to_owned(), level))
    }
}

pub fn load(root: &Path) -> Result<Trades, DataError> {
    let enchant_tags = load_tags(root, "enchantment");
    let mut trades = Trades {
        sets: HashMap::new(),
        defs: HashMap::new(),
        enchant_tags,
    };
    // Сделки.
    for (path, json) in walk_json(&root.join("data/minecraft/villager_trade"))? {
        let id = format!("minecraft:{path}");
        if let Some(def) = parse_trade(&json, &trades.enchant_tags) {
            trades.defs.insert(id, def);
        }
    }
    // Наборы уровней.
    let tags = load_tags(root, "villager_trade");
    for (path, json) in walk_json(&root.join("data/minecraft/trade_set"))? {
        let (profession, level) = match path.split_once('/') {
            Some((prof, file)) => {
                let level = file
                    .strip_prefix("level_")
                    .and_then(|n| n.parse::<i32>().ok());
                match level {
                    Some(level) => (prof.to_owned(), level),
                    None => continue,
                }
            }
            None => continue,
        };
        let amount = json.get("amount").and_then(Value::as_i64).unwrap_or(1) as i32;
        let allow_duplicates = json
            .get("allow_duplicates")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let list = match json.get("trades") {
            Some(Value::String(tag)) => tags
                .get(tag.trim_start_matches('#'))
                .cloned()
                .unwrap_or_default(),
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
            _ => Vec::new(),
        };
        trades.sets.insert(
            (profession, level),
            TradeSetDef {
                amount: amount.max(0),
                trades: list,
                allow_duplicates,
            },
        );
    }
    Ok(trades)
}

fn parse_trade(json: &Value, enchant_tags: &HashMap<String, Vec<String>>) -> Option<TradeDef> {
    let wants = trade_item(json.get("wants")?)?;
    let gives = trade_item(json.get("gives")?)?;
    let mut def = TradeDef {
        wants,
        additional: json.get("additional_wants").and_then(trade_item),
        gives,
        max_uses: json.get("max_uses").and_then(Value::as_i64).unwrap_or(4) as i32,
        xp: json.get("xp").and_then(Value::as_i64).unwrap_or(1) as i32,
        price_mul: json
            .get("reputation_discount")
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32,
        variants: merchant_variants(json.get("merchant_predicate")),
        enchant: None,
        exploration: None,
        double_price: json
            .get("double_trade_price_enchantments")
            .and_then(Value::as_str)
            .and_then(|tag| enchant_tags.get(tag.trim_start_matches('#')))
            .cloned()
            .unwrap_or_default(),
        unsupported: false,
    };
    def.max_uses = def.max_uses.max(1);
    def.xp = def.xp.max(0);
    if let Some(list) = json.get("given_item_modifier").and_then(Value::as_array) {
        for function in list {
            match function.get("type").and_then(Value::as_str) {
                Some("minecraft:enchant_randomly") => {
                    def.enchant = Some(EnchantSpec {
                        random: true,
                        levels: None,
                        options: option_list(function.get("options"), enchant_tags),
                        only_compatible: function
                            .get("only_compatible")
                            .and_then(Value::as_bool)
                            .unwrap_or(true),
                        include_cost: function
                            .get("include_additional_cost_component")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    });
                }
                Some("minecraft:enchant_with_levels") => {
                    let levels = function.get("levels").and_then(|l| {
                        let min = l.get("min").and_then(Value::as_i64)? as i32;
                        let max = l.get("max").and_then(Value::as_i64)? as i32;
                        Some((min, max))
                    });
                    def.enchant = Some(EnchantSpec {
                        random: false,
                        levels,
                        options: option_list(function.get("options"), enchant_tags),
                        only_compatible: false,
                        include_cost: function
                            .get("include_additional_cost_component")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    });
                }
                // `filtered` лишь отбраковывает пустой результат.
                Some("minecraft:filtered") => {}
                Some("minecraft:exploration_map") => {
                    def.exploration = Some(ExplorationMapSpec {
                        destination: function
                            .get("destination")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        decoration: function
                            .get("decoration")
                            .and_then(Value::as_str)
                            .unwrap_or("minecraft:woodland_mansion")
                            .to_owned(),
                        zoom: function
                            .get("zoom")
                            .and_then(Value::as_i64)
                            .unwrap_or(2)
                            .clamp(0, 4) as u8,
                        search_radius: function
                            .get("search_radius")
                            .and_then(Value::as_i64)
                            .unwrap_or(50) as i32,
                        skip_existing: function
                            .get("skip_existing_chunks")
                            .and_then(Value::as_bool)
                            .unwrap_or(true),
                    });
                }
                // Прочие модификаторы не поддержаны.
                Some(_) => def.unsupported = true,
                None => {}
            }
        }
    }
    Some(def)
}

fn option_list(value: Option<&Value>, tags: &HashMap<String, Vec<String>>) -> Vec<String> {
    match value {
        Some(Value::String(tag)) => tags
            .get(tag.trim_start_matches('#'))
            .cloned()
            .unwrap_or_default(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

fn merchant_variants(predicate: Option<&Value>) -> Option<Vec<String>> {
    let predicate = predicate?;
    let predicates = predicate.get("predicate")?.get("minecraft:predicates")?;
    let list = predicates.get("minecraft:villager/variant")?.as_array()?;
    Some(
        list.iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
    )
}

fn trade_item(value: &Value) -> Option<TradeItem> {
    Some(TradeItem {
        id: value.get("id")?.as_str()?.to_owned(),
        count: value
            .get("count")
            .and_then(Value::as_i64)
            .unwrap_or(1)
            .max(0) as i32,
        potion: value
            .get("components")
            .and_then(|c| c.get("minecraft:potion_contents"))
            .and_then(|p| p.get("potion"))
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}

fn load_tags(root: &Path, registry: &str) -> HashMap<String, Vec<String>> {
    let Ok(files) = walk_json(&root.join("data/minecraft/tags").join(registry)) else {
        return HashMap::new();
    };
    let raw: HashMap<String, Value> = files
        .into_iter()
        .map(|(path, json)| (format!("minecraft:{path}"), json))
        .collect();
    let mut out = HashMap::new();
    for id in raw.keys() {
        let mut seen = HashSet::new();
        out.insert(id.clone(), resolve_tag(id, &raw, &mut seen));
    }
    out
}

fn resolve_tag(key: &str, raw: &HashMap<String, Value>, seen: &mut HashSet<String>) -> Vec<String> {
    let key = key.trim_start_matches('#');
    if !seen.insert(key.to_owned()) {
        return Vec::new();
    }
    let Some(value) = raw.get(key) else {
        return Vec::new();
    };
    let Some(values) = value.get("values").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in values {
        let (name, required) = match entry {
            Value::String(s) => (s.as_str(), true),
            Value::Object(o) => match o.get("id").and_then(Value::as_str) {
                Some(id) => (
                    id,
                    o.get("required").and_then(Value::as_bool).unwrap_or(true),
                ),
                None => continue,
            },
            _ => continue,
        };
        if name.starts_with('#') {
            out.extend(resolve_tag(name, raw, seen));
        } else if required || raw.contains_key(name) {
            out.push(name.to_owned());
        }
    }
    out
}

fn walk_json(dir: &Path) -> Result<Vec<(String, Value)>, DataError> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<(String, Value)>) -> Result<(), DataError> {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(DataError::Io(format!("read {}: {e}", dir.display()))),
        };
        for entry in entries {
            let path = entry
                .map_err(|e| DataError::Io(format!("read {}: {e}", dir.display())))?
                .path();
            if path.is_dir() {
                walk(base, &path, out)?;
            } else if path.extension().is_some_and(|e| e == "json") {
                let rel = path.strip_prefix(base).unwrap().with_extension("");
                let id: Vec<_> = rel.iter().map(|c| c.to_string_lossy()).collect();
                let text = std::fs::read_to_string(&path)
                    .map_err(|e| DataError::Io(format!("read {}: {e}", path.display())))?;
                let json: Value = serde_json::from_str(&text).map_err(|e| DataError::Format {
                    file: path.display().to_string(),
                    msg: e.to_string(),
                })?;
                out.push((id.join("/"), json));
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trade_tags_resolve_nested() {
        let mut raw = HashMap::new();
        raw.insert(
            "minecraft:a".to_owned(),
            serde_json::json!({"values": ["#minecraft:b", "minecraft:c"]}),
        );
        raw.insert(
            "minecraft:b".to_owned(),
            serde_json::json!({"values": ["minecraft:d"]}),
        );
        let mut seen = HashSet::new();
        let list = resolve_tag("#minecraft:a", &raw, &mut seen);
        assert_eq!(
            list,
            vec!["minecraft:d".to_owned(), "minecraft:c".to_owned()]
        );
    }

    #[test]
    fn trade_item_defaults_and_potion() {
        let item = trade_item(&serde_json::json!({
            "id": "minecraft:potion",
            "components": {"minecraft:potion_contents": {"potion": "minecraft:water"}}
        }))
        .unwrap();
        assert_eq!(item.count, 1);
        assert_eq!(item.potion.as_deref(), Some("minecraft:water"));
        let emerald =
            trade_item(&serde_json::json!({"id": "minecraft:emerald", "count": 3})).unwrap();
        assert_eq!(emerald.count, 3);
        assert!(emerald.potion.is_none());
    }

    #[test]
    fn exploration_map_parses() {
        let json = serde_json::json!({
            "wants": {"id": "minecraft:emerald", "count": 8},
            "additional_wants": {"id": "minecraft:compass"},
            "gives": {"id": "minecraft:jungle_pyramid_map"},
            "max_uses": 12,
            "xp": 5,
            "reputation_discount": 0.2,
            "given_item_modifier": [
                {
                    "type": "minecraft:exploration_map",
                    "decoration": "minecraft:jungle_temple",
                    "destination": "#minecraft:on_jungle_pyramid_maps",
                    "search_radius": 100
                },
                {"type": "minecraft:filtered"}
            ]
        });
        let def = parse_trade(&json, &HashMap::new()).unwrap();
        let map = def.exploration.unwrap();
        assert_eq!(map.destination, "#minecraft:on_jungle_pyramid_maps");
        assert_eq!(map.decoration, "minecraft:jungle_temple");
        assert_eq!(map.zoom, 2);
        assert_eq!(map.search_radius, 100);
        assert!(!def.unsupported);
        assert_eq!(def.additional.unwrap().id, "minecraft:compass");
    }

    #[test]
    fn merchant_variants_parse() {
        let predicate = serde_json::json!({
            "type": "minecraft:entity_properties",
            "entity": "this",
            "predicate": {"minecraft:predicates": {"minecraft:villager/variant": ["minecraft:desert"]}}
        });
        assert_eq!(
            merchant_variants(Some(&predicate)).unwrap(),
            vec!["minecraft:desert".to_owned()]
        );
        assert!(merchant_variants(None).is_none());
    }
}
