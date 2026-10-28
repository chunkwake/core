//! Блоки (`reports/blocks.json`) и типы измерений (`data/minecraft/dimension_type`).

use crate::DataError;
use std::collections::HashMap;
use std::path::Path;

/// `reports/light.csv` (выгрузка `ChunkwakeLight`): по id состояния
/// `(излучение, затухание, небесный свет проходит вниз)`.
pub fn load_light(root: &Path) -> Result<Vec<(u8, u8, bool)>, DataError> {
    let path = root.join("reports/light.csv");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| DataError::Io(format!("read {}: {e}", path.display())))?;
    text.lines()
        .enumerate()
        .map(|(i, line)| {
            let mut f = line.split(',').map(|v| v.trim().parse::<u8>().ok());
            match (f.next().flatten(), f.next().flatten(), f.next().flatten()) {
                (Some(e), Some(d), Some(s)) => Ok((e, d, s != 0)),
                _ => Err(DataError::Format {
                    file: path.display().to_string(),
                    msg: format!("line {}: {line:?}", i + 1),
                }),
            }
        })
        .collect()
}

/// Твёрдость состояний блоков (4-я колонка `reports/light.csv`); -1 — неразрушимый.
pub fn load_hardness(root: &Path) -> Result<Vec<f32>, DataError> {
    let path = root.join("reports/light.csv");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| DataError::Io(format!("read {}: {e}", path.display())))?;
    text.lines()
        .enumerate()
        .map(|(i, line)| {
            line.split(',')
                .nth(3)
                .and_then(|v| v.trim().parse::<f32>().ok())
                .ok_or_else(|| DataError::Format {
                    file: path.display().to_string(),
                    msg: format!("line {}: no hardness in {line:?}", i + 1),
                })
        })
        .collect()
}

/// Нужен ли инструмент для дропа (`requiresCorrectToolForDrops`, 5-я колонка `light.csv`).
pub fn load_requires_tool(root: &Path) -> Result<Vec<bool>, DataError> {
    let path = root.join("reports/light.csv");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| DataError::Io(format!("read {}: {e}", path.display())))?;
    text.lines()
        .enumerate()
        .map(|(i, line)| match line.split(',').nth(4).map(str::trim) {
            Some("0") => Ok(false),
            Some("1") => Ok(true),
            _ => Err(DataError::Format {
                file: path.display().to_string(),
                msg: format!("line {}: no requires-tool flag in {line:?}", i + 1),
            }),
        })
        .collect()
}

/// Еда: компонент `minecraft:food` и время поедания из `minecraft:consumable`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Food {
    pub nutrition: i32,
    pub saturation: f32,
    pub can_always_eat: bool,
    pub seconds: f32,
}

/// Свойства предмета, нужные выживанию.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemInfo {
    pub max_stack: u8,
    pub food: Option<Food>,
    /// Слот `minecraft:equippable`: `head`, `chest`, `legs`, `feet`, `offhand`…
    pub equip: Option<&'static str>,
    /// `minecraft:max_damage` (0 — предмет не изнашивается).
    pub max_damage: i32,
}

impl Default for ItemInfo {
    fn default() -> Self {
        Self {
            max_stack: 64,
            food: None,
            equip: None,
            max_damage: 0,
        }
    }
}

/// Время поедания по умолчанию (`Consumable.DEFAULT_CONSUME_SECONDS`).
const DEFAULT_CONSUME_SECONDS: f32 = 1.6;

/// Свойства предметов по ID (`reports/minecraft/components/item`); без отчёта —
/// стопка 64 и не еда.
pub fn load_items(root: &Path, items: &HashMap<String, i32>) -> Result<Vec<ItemInfo>, DataError> {
    let dir = root.join("reports/minecraft/components/item");
    let mut out = vec![ItemInfo::default(); items.values().max().map_or(0, |&m| m as usize + 1)];
    for (name, &id) in items {
        let file = dir.join(format!("{}.json", name.trim_start_matches("minecraft:")));
        if !file.is_file() {
            continue;
        }
        // Файлы большие (атрибуты, еда, ...), а нужно одно число: ищем ключ в тексте.
        let text = std::fs::read_to_string(&file)
            .map_err(|e| DataError::Io(format!("read {}: {e}", file.display())))?;
        if let Some(n) = text
            .split_once("\"minecraft:max_stack_size\":")
            .and_then(|(_, rest)| {
                let digits: String = rest
                    .trim_start()
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect();
                digits.parse::<u64>().ok()
            })
        {
            out[id as usize].max_stack = n.clamp(1, 99) as u8;
        }
        if let Some(n) = text.split_once("\"minecraft:max_damage\":").and_then(|(_, rest)| {
            let digits: String = rest.trim_start().chars().take_while(char::is_ascii_digit).collect();
            digits.parse::<i32>().ok()
        }) {
            out[id as usize].max_damage = n;
        }
        // Еды немного: такие файлы разбираем целиком.
        if text.contains("\"minecraft:food\"") || text.contains("\"minecraft:equippable\"") {
            let json: serde_json::Value =
                serde_json::from_str(&text).map_err(|e| DataError::Format {
                    file: file.display().to_string(),
                    msg: e.to_string(),
                })?;
            let c = &json["components"];
            out[id as usize].food = parse_food(c);
            out[id as usize].equip = c["minecraft:equippable"]["slot"].as_str().and_then(equip_slot);
        }
    }
    Ok(out)
}

fn parse_food(c: &serde_json::Value) -> Option<Food> {
    let f = c.get("minecraft:food")?;
    Some(Food {
        nutrition: f.get("nutrition")?.as_i64()? as i32,
        saturation: f.get("saturation")?.as_f64()? as f32,
        can_always_eat: f
            .get("can_always_eat")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        seconds: c
            .pointer("/minecraft:consumable/consume_seconds")
            .and_then(|v| v.as_f64())
            .map_or(DEFAULT_CONSUME_SECONDS, |s| s as f32),
    })
}

/// Обычный дроп блока (без шёлкового касания, ножниц и случайностей) из
/// `data/minecraft/loot_table/blocks/<имя>.json`: блок → предмет.
pub fn load_block_drops(root: &Path) -> Result<HashMap<String, String>, DataError> {
    let dir = root.join("data/minecraft/loot_table/blocks");
    let rd = std::fs::read_dir(&dir)
        .map_err(|e| DataError::Io(format!("read {}: {e}", dir.display())))?;
    let mut out = HashMap::new();
    for entry in rd.flatten() {
        let path = entry.path();
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if let Some(item) = read_json(&path)?
            .get("pools")
            .and_then(|p| p.get(0))
            .and_then(pool_drop)
        {
            out.insert(format!("minecraft:{name}"), item);
        }
    }
    Ok(out)
}

fn pool_drop(pool: &serde_json::Value) -> Option<String> {
    if pool.get("conditions").is_some_and(|c| !plain_conditions(c)) {
        return None;
    }
    pool.get("entries")?.as_array()?.iter().find_map(entry_drop)
}

fn entry_drop(e: &serde_json::Value) -> Option<String> {
    for key in ["condition", "conditions"] {
        if e.get(key).is_some_and(|c| !plain_conditions(c)) {
            return None;
        }
    }
    match e.get("type")?.as_str()? {
        "minecraft:item" => Some(e.get("name")?.as_str()?.to_owned()),
        "minecraft:alternatives" | "minecraft:group" | "minecraft:sequence" => {
            e.get("children")?.as_array()?.iter().find_map(entry_drop)
        }
        _ => None,
    }
}

/// Условия, которые выполняются при ломке рукой: выживание при взрыве и т. п.
/// Шёлковое касание, ножницы, случайные шансы и свойства блока — нет.
fn plain_conditions(c: &serde_json::Value) -> bool {
    match c {
        serde_json::Value::Array(a) => a.iter().all(plain_conditions),
        serde_json::Value::String(s) => !s.contains("silk_touch") && !s.contains("shears"),
        serde_json::Value::Object(_) => matches!(
            c.get("type").and_then(|t| t.as_str()),
            Some("minecraft:survives_explosion")
        ),
        _ => false,
    }
}

pub(crate) fn read_json(path: &Path) -> Result<serde_json::Value, DataError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| DataError::Io(format!("read {}: {e}", path.display())))?;
    serde_json::from_str(&text).map_err(|e| DataError::Format {
        file: path.display().to_string(),
        msg: e.to_string(),
    })
}

/// Свойства состояния блока: пары `(ключ, значение)`, отсортированные по ключу.
pub type Properties = Vec<(String, String)>;

/// ID состояний блоков по умолчанию: `minecraft:stone` → сетевой ID.
#[derive(Debug, Default)]
pub struct Blocks {
    defaults: HashMap<String, i32>,
    max_id: i64,
    /// ID состояния → имя блока и свойства (отсортированы по ключу).
    states: Vec<Option<(String, Properties)>>,
    /// Обратный индекс: `name[k=v,...]` → ID.
    by_key: HashMap<String, u32>,
}

impl Blocks {
    pub fn load(root: &Path) -> Result<Self, DataError> {
        let path = root.join("reports/blocks.json");
        Self::parse(&read_json(&path)?).map_err(|msg| DataError::Format {
            file: path.display().to_string(),
            msg,
        })
    }

    pub fn parse(json: &serde_json::Value) -> Result<Self, String> {
        let obj = json.as_object().ok_or("expected object")?;
        let mut defaults = HashMap::with_capacity(obj.len());
        let mut max_id = 0;
        let mut states = Vec::new();
        let mut by_key = HashMap::new();
        for (name, block) in obj {
            for s in block["states"].as_array().into_iter().flatten() {
                let id = s["id"]
                    .as_i64()
                    .ok_or_else(|| format!("{name}: state without id"))?;
                max_id = max_id.max(id);
                let mut props: Vec<(String, String)> = s["properties"]
                    .as_object()
                    .into_iter()
                    .flatten()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_owned()))
                    .collect();
                props.sort();
                by_key.insert(state_key(name, &props), id as u32);
                if states.len() <= id as usize {
                    states.resize(id as usize + 1, None);
                }
                states[id as usize] = Some((name.clone(), props));
            }
            let state = block["states"]
                .as_array()
                .and_then(|s| s.iter().find(|s| s["default"] == true))
                .ok_or_else(|| format!("{name}: no default state"))?;
            let id = state["id"]
                .as_i64()
                .ok_or_else(|| format!("{name}: no id"))?;
            defaults.insert(name.clone(), id as i32);
        }
        Ok(Self {
            defaults,
            max_id,
            states,
            by_key,
        })
    }

    /// Общее число состояний (бит прямой палитры = ceil(log2(n))).
    pub fn state_count(&self) -> usize {
        self.max_id as usize + 1
    }

    /// Имя блока и его свойства для ID состояния.
    pub fn state(&self, id: u32) -> Option<(&str, &[(String, String)])> {
        self.states
            .get(id as usize)?
            .as_ref()
            .map(|(n, p)| (n.as_str(), p.as_slice()))
    }

    /// ID состояния по имени и свойствам (порядок свойств не важен).
    /// Недостающие свойства не подставляются: нужен полный набор.
    pub fn state_id(&self, name: &str, props: &[(String, String)]) -> Option<u32> {
        let mut props = props.to_vec();
        props.sort();
        self.by_key.get(&state_key(name, &props)).copied()
    }

    pub fn default_state(&self, name: &str) -> Result<i32, DataError> {
        self.defaults
            .get(name)
            .copied()
            .ok_or_else(|| DataError::Io(format!("unknown block {name}")))
    }
}

fn state_key(name: &str, props: &[(String, String)]) -> String {
    let mut key = name.to_owned();
    key.push('[');
    for (i, (k, v)) in props.iter().enumerate() {
        if i > 0 {
            key.push(',');
        }
        key.push_str(k);
        key.push('=');
        key.push_str(v);
    }
    key.push(']');
    key
}

/// Параметры измерения, нужные для кодирования чанков.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DimensionType {
    pub min_y: i32,
    pub height: i32,
    pub has_skylight: bool,
}

impl DimensionType {
    /// `name` — путь без namespace, например `overworld`.
    pub fn load(root: &Path, name: &str) -> Result<Self, DataError> {
        let path = root.join(format!("data/minecraft/dimension_type/{name}.json"));
        let json = read_json(&path)?;
        let int = |k: &str| {
            json[k]
                .as_i64()
                .map(|v| v as i32)
                .ok_or_else(|| DataError::Format {
                    file: path.display().to_string(),
                    msg: format!("no {k}"),
                })
        };
        Ok(Self {
            min_y: int("min_y")?,
            height: int("height")?,
            has_skylight: json["has_skylight"].as_bool().unwrap_or(false),
        })
    }

    pub fn sections(&self) -> usize {
        (self.height / 16) as usize
    }
}

/// Сводка из тысяч мелких файлов (лут, компоненты предметов) в одном файле
/// рядом с данными: при первом запуске собирается, дальше читается только она.
/// На холодном диске под антивирусом это секунды вместо десятков.
const DERIVED_DIR: &str = "chunkwake-derived-v1";

pub(crate) fn derived(root: &Path, file: &str) -> std::path::PathBuf {
    root.join(DERIVED_DIR).join(file)
}

/// Пишет файл через временный: оборванная запись не оставит битую сводку.
pub(crate) fn write_derived(path: &Path, text: &str) -> Result<(), DataError> {
    let io = |e: std::io::Error| DataError::Io(format!("write {}: {e}", path.display()));
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text).map_err(io)?;
    std::fs::rename(&tmp, path).map_err(io)
}

/// `load_block_drops` через сводку `drops.tsv` (блок, предмет).
pub fn load_block_drops_cached(root: &Path) -> Result<HashMap<String, String>, DataError> {
    let path = derived(root, "drops.tsv");
    if let Ok(text) = std::fs::read_to_string(&path) {
        return Ok(text
            .lines()
            .filter_map(|l| l.split_once('\t'))
            .map(|(b, i)| (b.to_owned(), i.to_owned()))
            .collect());
    }
    let drops = load_block_drops(root)?;
    let mut text: Vec<String> = drops.iter().map(|(b, i)| format!("{b}\t{i}")).collect();
    text.sort();
    write_derived(&path, &(text.join("\n") + "\n"))?;
    Ok(drops)
}

/// `load_items` через сводку `items.tsv`: имя, стопка и для еды — сытость,
/// насыщение, можно ли всегда, секунды.
pub fn load_items_cached(
    root: &Path,
    items: &HashMap<String, i32>,
) -> Result<Vec<ItemInfo>, DataError> {
    let path = derived(root, "items.tsv");
    let len = items.values().max().map_or(0, |&m| m as usize + 1);
    if let Ok(text) = std::fs::read_to_string(&path) {
        let mut out = vec![ItemInfo::default(); len];
        for line in text.lines() {
            if let Some((id, info)) = parse_item_line(line, items) {
                out[id] = info;
            }
        }
        return Ok(out);
    }
    let out = load_items(root, items)?;
    let mut lines: Vec<String> = items
        .iter()
        .map(|(name, &id)| item_line(name, &out[id as usize]))
        .collect();
    lines.sort();
    write_derived(&path, &(lines.join("\n") + "\n"))?;
    Ok(out)
}

/// Слот экипировки (`EquipmentSlot`) по имени.
fn equip_slot(name: &str) -> Option<&'static str> {
    ["mainhand", "offhand", "feet", "legs", "chest", "head", "body", "saddle"]
        .into_iter()
        .find(|s| *s == name)
}

/// `name, max_stack, equip (-), max_damage [, nutrition saturation always seconds]` через табуляцию.
fn item_line(name: &str, i: &ItemInfo) -> String {
    let head = format!(
        "{name}\t{}\t{}\t{}",
        i.max_stack,
        i.equip.unwrap_or("-"),
        i.max_damage
    );
    match i.food {
        Some(f) => format!(
            "{head}\t{}\t{}\t{}\t{}",
            f.nutrition, f.saturation, f.can_always_eat, f.seconds
        ),
        None => head,
    }
}

fn parse_item_line(line: &str, items: &HashMap<String, i32>) -> Option<(usize, ItemInfo)> {
    let mut f = line.split('\t');
    let id = *items.get(f.next()?)? as usize;
    let max_stack = f.next()?.parse().ok()?;
    let equip = equip_slot(f.next()?);
    let max_damage = f.next()?.parse().ok()?;
    let food = match (f.next(), f.next(), f.next(), f.next()) {
        (Some(n), Some(s), Some(a), Some(sec)) => Some(Food {
            nutrition: n.parse().ok()?,
            saturation: s.parse().ok()?,
            can_always_eat: a.parse().ok()?,
            seconds: sec.parse().ok()?,
        }),
        _ => None,
    };
    Some((
        id,
        ItemInfo {
            max_stack,
            food,
            equip,
            max_damage,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_states() {
        let json = serde_json::json!({
            "minecraft:grass_block": { "states": [
                { "id": 8, "properties": { "snowy": "true" } },
                { "default": true, "id": 9, "properties": { "snowy": "false" } }
            ]}
        });
        let b = Blocks::parse(&json).unwrap();
        assert_eq!(b.default_state("minecraft:grass_block").unwrap(), 9);
        assert_eq!(
            b.state(8),
            Some((
                "minecraft:grass_block",
                &[("snowy".to_owned(), "true".to_owned())][..]
            ))
        );
        assert_eq!(
            b.state_id("minecraft:grass_block", &[("snowy".into(), "false".into())]),
            Some(9)
        );
        assert!(b.default_state("minecraft:nope").is_err());
    }
}

#[cfg(test)]
mod loot_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn stone_drops_cobblestone() {
        let pool = json!({"entries": [{"type": "minecraft:alternatives", "children": [
            {"type": "minecraft:item", "condition": "minecraft:tool/can_silk_touch", "name": "minecraft:stone"},
            {"type": "minecraft:item", "condition": {"type": "minecraft:survives_explosion"}, "name": "minecraft:cobblestone"}
        ]}], "rolls": 1});
        assert_eq!(pool_drop(&pool).as_deref(), Some("minecraft:cobblestone"));
    }

    #[test]
    fn chance_only_drops_nothing() {
        let pool = json!({"entries": [{"type": "minecraft:item",
            "conditions": [{"type": "minecraft:random_chance", "chance": 0.1}], "name": "minecraft:apple"}]});
        assert_eq!(pool_drop(&pool), None);
    }

    #[test]
    fn food_component() {
        let c = serde_json::json!({"minecraft:food": {"nutrition": 5, "saturation": 6.0}, "minecraft:consumable": {}});
        let f = parse_food(&c).unwrap();
        assert_eq!(
            (f.nutrition, f.saturation, f.can_always_eat, f.seconds),
            (5, 6.0, false, 1.6)
        );
        let c = serde_json::json!({"minecraft:food": {"nutrition": 6, "saturation": 1.2, "can_always_eat": true},
            "minecraft:consumable": {"consume_seconds": 2.0}});
        let f = parse_food(&c).unwrap();
        assert!(f.can_always_eat);
        assert_eq!(f.seconds, 2.0);
        assert_eq!(parse_food(&serde_json::json!({})), None);
    }

    #[test]
    fn item_line_round_trip() {
        let items = HashMap::from([("minecraft:bread".to_string(), 3)]);
        let food = Food {
            nutrition: 5,
            saturation: 6.0,
            can_always_eat: false,
            seconds: 1.6,
        };
        for info in [
            ItemInfo {
                max_stack: 64,
                food: Some(food),
                equip: Some("head"),
                max_damage: 165,
            },
            ItemInfo {
                max_stack: 16,
                food: None,
                ..Default::default()
            },
        ] {
            let line = item_line("minecraft:bread", &info);
            assert_eq!(parse_item_line(&line, &items), Some((3, info)));
        }
    }
}
