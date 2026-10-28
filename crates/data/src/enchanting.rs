//! Данные для стола зачарования: чары (`data/minecraft/enchantment`), их
//! предметы и исключения по тегам, «зачаряемость» предметов
//! (`minecraft:enchantable`) и блоки-полки и проводники силы.

use crate::DataError;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Одни чары; `id` — сетевой номер (порядок реестра `enchantment`).
#[derive(Clone, Debug, Default)]
pub struct EnchantDef {
    pub id: i32,
    pub name: String,
    pub max_level: i32,
    pub weight: i32,
    pub min_base: i32,
    pub min_step: i32,
    pub max_base: i32,
    pub max_step: i32,
    /// Предметы, на которые чары вообще ложатся (`supported_items`).
    pub supported: HashSet<String>,
    /// Предметы для стола (`primary_items`, по умолчанию — `supported`).
    pub primary: HashSet<String>,
    /// Несовместимые чары (`exclusive_set`), имена.
    pub exclusive: HashSet<String>,
    /// Бывает на столе (тег `in_enchanting_table`).
    pub in_table: bool,
    /// Тег `#minecraft:curse`: точило такое не снимает.
    pub curse: bool,
    /// Группы слотов, где чары действуют (`slots`: mainhand, armor, …).
    pub slots: Vec<String>,
    /// Модификаторы атрибутов (`effects."minecraft:attributes"`).
    pub attributes: Vec<AttrEffect>,
    /// `effects."minecraft:damage_protection"`: условия по тегам типа урона
    /// (типы урона тега, ожидаемое) и прибавка защиты.
    pub protection: Vec<Protection>,
}

/// Условия по тегам типа урона (типы тега, ожидаемое) и прибавка защиты.
pub type Protection = (Vec<(HashSet<String>, bool)>, LevelValue);

/// `EnchantmentAttributeEffect`: атрибут, ID модификатора, операция, величина.
#[derive(Clone, Debug)]
pub struct AttrEffect {
    pub attribute: String,
    pub id: String,
    /// 0 add_value, 1 add_multiplied_base, 2 add_multiplied_total.
    pub op: i32,
    pub amount: LevelValue,
}

/// `LevelBasedValue`: величина от уровня чар.
#[derive(Clone, Debug)]
pub enum LevelValue {
    Constant(f32),
    Linear {
        base: f32,
        per: f32,
    },
    LevelsSquared {
        added: f32,
    },
    Fraction(Box<LevelValue>, Box<LevelValue>),
    Clamped {
        value: Box<LevelValue>,
        min: f32,
        max: f32,
    },
}

impl LevelValue {
    pub fn parse(v: &serde_json::Value) -> Option<LevelValue> {
        if let Some(n) = v.as_f64() {
            return Some(LevelValue::Constant(n as f32));
        }
        let f = |k: &str| v.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
        Some(
            match v.get("type")?.as_str()?.trim_start_matches("minecraft:") {
                "linear" => LevelValue::Linear {
                    base: f("base"),
                    per: f("per_level_above_first"),
                },
                "levels_squared" => LevelValue::LevelsSquared { added: f("added") },
                "fraction" => LevelValue::Fraction(
                    Box::new(LevelValue::parse(v.get("numerator")?)?),
                    Box::new(LevelValue::parse(v.get("denominator")?)?),
                ),
                "clamped" => LevelValue::Clamped {
                    value: Box::new(LevelValue::parse(v.get("value")?)?),
                    min: f("min"),
                    max: f("max"),
                },
                _ => return None,
            },
        )
    }

    pub fn calc(&self, level: i32) -> f32 {
        match self {
            LevelValue::Constant(c) => *c,
            LevelValue::Linear { base, per } => base + per * (level - 1) as f32,
            LevelValue::LevelsSquared { added } => (level * level) as f32 + added,
            LevelValue::Fraction(n, d) => {
                let d = d.calc(level);
                if d == 0.0 { 0.0 } else { n.calc(level) / d }
            }
            LevelValue::Clamped { value, min, max } => value.calc(level).clamp(*min, *max),
        }
    }
}

impl EnchantDef {
    /// `Enchantment.getMinCost`/`getMaxCost`: `base + шаг × (уровень − 1)`.
    pub fn min_cost(&self, level: i32) -> i32 {
        self.min_base + self.min_step * (level - 1)
    }

    pub fn max_cost(&self, level: i32) -> i32 {
        self.max_base + self.max_step * (level - 1)
    }

    /// `Enchantment.areCompatible`: разные и не исключают друг друга.
    pub fn compatible(&self, other: &EnchantDef) -> bool {
        self.id != other.id
            && !self.exclusive.contains(&other.name)
            && !other.exclusive.contains(&self.name)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Enchanting {
    pub defs: Vec<EnchantDef>,
    /// Предмет → `enchantable.value`.
    pub enchantable: HashMap<String, i32>,
    /// Блоки-полки (`enchantment_power_provider`).
    pub providers: HashSet<String>,
    /// Блоки, сквозь которые сила проходит (`enchantment_power_transmitter`).
    pub transmitters: HashSet<String>,
    /// Индексы `defs` чар стола в порядке тега `in_enchanting_table`.
    pub table: Vec<usize>,
}

/// Чары в сетевом порядке `names`.
pub fn load(root: &Path, names: &[String]) -> Result<Enchanting, DataError> {
    let base = root.join("data/minecraft");
    let mut tags = Tags {
        root,
        memo: HashMap::new(),
    };
    let table = tags.values("enchantment", "#minecraft:in_enchanting_table");
    let curses = tags.values("enchantment", "#minecraft:curse");
    let mut out = Enchanting::default();
    for (id, name) in names.iter().enumerate() {
        let path = name.strip_prefix("minecraft:").unwrap_or(name);
        let file = base.join(format!("enchantment/{path}.json"));
        let Ok(v) = crate::world::read_json(&file) else {
            continue;
        };
        let n = |k: &str| v.get(k).and_then(|x| x.as_i64()).unwrap_or(1) as i32;
        let cost = |k: &str, f: &str| {
            v.get(k)
                .and_then(|c| c.get(f))
                .and_then(|x| x.as_i64())
                .unwrap_or(0) as i32
        };
        let set = |tags: &mut Tags, kind: &str, k: &str| -> HashSet<String> {
            match v.get(k) {
                Some(serde_json::Value::String(s)) => tags.values(kind, s),
                Some(serde_json::Value::Array(a)) => a
                    .iter()
                    .filter_map(|e| e.as_str())
                    .flat_map(|s| tags.values(kind, s))
                    .collect(),
                _ => HashSet::new(),
            }
        };
        let supported = set(&mut tags, "item", "supported_items");
        let primary = if v.get("primary_items").is_some() {
            set(&mut tags, "item", "primary_items")
        } else {
            supported.clone()
        };
        out.defs.push(EnchantDef {
            id: id as i32,
            name: name.clone(),
            max_level: n("max_level"),
            weight: n("weight"),
            min_base: cost("min_cost", "base"),
            min_step: cost("min_cost", "per_level_above_first"),
            max_base: cost("max_cost", "base"),
            max_step: cost("max_cost", "per_level_above_first"),
            exclusive: set(&mut tags, "enchantment", "exclusive_set"),
            in_table: table.contains(name),
            curse: curses.contains(name),
            protection: v["effects"]["minecraft:damage_protection"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|p| {
                    let effect = &p["effect"];
                    if effect["type"].as_str()? != "minecraft:add" {
                        return None;
                    }
                    let value = LevelValue::parse(&effect["value"])?;
                    let req = &p["requirements"];
                    let mut tags_req = Vec::new();
                    if !req.is_null() {
                        if req["type"].as_str()? != "minecraft:damage_source_properties" {
                            return None;
                        }
                        for t in req["predicate"]["tags"].as_array()? {
                            tags_req.push((
                                tags.values("damage_type", t["id"].as_str()?),
                                t["expected"].as_bool()?,
                            ));
                        }
                    }
                    Some((tags_req, value))
                })
                .collect(),
            slots: v["slots"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|s| s.as_str().map(str::to_owned))
                .collect(),
            attributes: v["effects"]["minecraft:attributes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|a| {
                    Some(AttrEffect {
                        attribute: a["attribute"].as_str()?.to_owned(),
                        id: a["id"].as_str()?.to_owned(),
                        op: match a["operation"].as_str()? {
                            "add_value" => 0,
                            "add_multiplied_base" => 1,
                            _ => 2,
                        },
                        amount: LevelValue::parse(&a["amount"])?,
                    })
                })
                .collect(),
            supported,
            primary,
        });
    }
    // Зачаряемость читаем только у предметов, на которые хоть что-то ложится.
    let mut items: HashSet<&String> = out.defs.iter().flat_map(|d| d.supported.iter()).collect();
    let book = "minecraft:book".to_owned();
    items.insert(&book);
    let dir = root.join("reports/minecraft/components/item");
    for item in items {
        let path = item.strip_prefix("minecraft:").unwrap_or(item);
        let Ok(text) = std::fs::read_to_string(dir.join(format!("{path}.json"))) else {
            continue;
        };
        let value = text
            .split_once("\"minecraft:enchantable\"")
            .and_then(|(_, rest)| rest.split_once("\"value\":"))
            .and_then(|(_, rest)| {
                rest.trim_start()
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect::<String>()
                    .parse()
                    .ok()
            });
        if let Some(v) = value {
            out.enchantable.insert(item.clone(), v);
        }
    }
    let mut order = Vec::new();
    tags.ordered(
        "enchantment",
        "#minecraft:in_enchanting_table",
        &mut order,
        0,
    );
    for name in order {
        if let Some(i) = out.defs.iter().position(|d| d.name == name)
            && !out.table.contains(&i)
        {
            out.table.push(i);
        }
    }
    out.providers = tags.values("block", "#minecraft:enchantment_power_provider");
    out.transmitters = tags.values("block", "#minecraft:enchantment_power_transmitter");
    Ok(out)
}

/// Разворачивает теги `data/<ns>/tags/<kind>/<path>.json` (с вложенными) в имена.
struct Tags<'a> {
    root: &'a Path,
    memo: HashMap<(String, String), HashSet<String>>,
}

impl Tags<'_> {
    fn values(&mut self, kind: &str, entry: &str) -> HashSet<String> {
        let key = (kind.to_owned(), entry.to_owned());
        if let Some(v) = self.memo.get(&key) {
            return v.clone();
        }
        let mut out = HashSet::new();
        self.add(kind, entry, &mut out, 0);
        self.memo.insert(key, out.clone());
        out
    }

    /// Как `add`, но в порядке файлов тегов.
    fn ordered(&self, kind: &str, entry: &str, out: &mut Vec<String>, depth: u32) {
        let ns = |s: &str| {
            if s.contains(':') {
                s.to_owned()
            } else {
                format!("minecraft:{s}")
            }
        };
        let Some(tag) = entry.strip_prefix('#') else {
            out.push(ns(entry));
            return;
        };
        if depth > 32 {
            return;
        }
        let tag = ns(tag);
        let (space, path) = tag.split_once(':').unwrap_or(("minecraft", &tag));
        let file = self
            .root
            .join(format!("data/{space}/tags/{kind}/{path}.json"));
        let Ok(json) = crate::world::read_json(&file) else {
            return;
        };
        for v in json["values"].as_array().into_iter().flatten() {
            if let Some(id) = v.as_str().or_else(|| v["id"].as_str()) {
                self.ordered(kind, id, out, depth + 1);
            }
        }
    }

    fn add(&self, kind: &str, entry: &str, out: &mut HashSet<String>, depth: u32) {
        let ns = |s: &str| {
            if s.contains(':') {
                s.to_owned()
            } else {
                format!("minecraft:{s}")
            }
        };
        let Some(tag) = entry.strip_prefix('#') else {
            out.insert(ns(entry));
            return;
        };
        if depth > 32 {
            return;
        }
        let tag = ns(tag);
        let (space, path) = tag.split_once(':').unwrap_or(("minecraft", &tag));
        let file = self
            .root
            .join(format!("data/{space}/tags/{kind}/{path}.json"));
        let Ok(json) = crate::world::read_json(&file) else {
            return;
        };
        for v in json["values"].as_array().into_iter().flatten() {
            if let Some(id) = v.as_str().or_else(|| v["id"].as_str()) {
                self.add(kind, id, out, depth + 1);
            }
        }
    }
}
