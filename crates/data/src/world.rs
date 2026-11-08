//! Блоки (`reports/blocks.json`) и типы измерений (`data/minecraft/dimension_type`).

use crate::DataError;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

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

/// Все четыре колонки `light.csv` за одно чтение файла: строки выгрузки
/// `ChunkwakeLight` нужны серверу вместе, а файл весит полмегабайта.
pub struct LightRows {
    /// `(излучение, затухание, небесный свет проходит вниз)`.
    pub light: Vec<(u8, u8, bool)>,
    /// Твёрдость; -1 — неразрушимый.
    pub hardness: Vec<f32>,
    /// Прочные грани по битам низ, верх, север, юг, запад, восток; бит 6 —
    /// `BlockState.isSolid` (7-я колонка).
    pub sturdy: Vec<u8>,
    /// `requiresCorrectToolForDrops`.
    pub requires_tool: Vec<bool>,
}

/// Строки `reports/light.csv` одним проходом.
pub fn load_light_rows(root: &Path) -> Result<LightRows, DataError> {
    let path = root.join("reports/light.csv");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| DataError::Io(format!("read {}: {e}", path.display())))?;
    let bad = |i: usize, line: &str, msg: &str| DataError::Format {
        file: path.display().to_string(),
        msg: format!("line {}: {msg} {line:?}", i + 1),
    };
    let n = text.lines().count();
    let mut rows = LightRows {
        light: Vec::with_capacity(n),
        hardness: Vec::with_capacity(n),
        sturdy: Vec::with_capacity(n),
        requires_tool: Vec::with_capacity(n),
    };
    for (i, line) in text.lines().enumerate() {
        let mut f = line.split(',').map(str::trim);
        let emission = f
            .next()
            .and_then(|v| v.parse::<u8>().ok())
            .ok_or_else(|| bad(i, line, "no emission"))?;
        let dampening = f
            .next()
            .and_then(|v| v.parse::<u8>().ok())
            .ok_or_else(|| bad(i, line, "no dampening"))?;
        let down = f
            .next()
            .and_then(|v| v.parse::<u8>().ok())
            .ok_or_else(|| bad(i, line, "no skylight flag"))?;
        let hardness = f
            .next()
            .and_then(|v| v.parse::<f32>().ok())
            .ok_or_else(|| bad(i, line, "no hardness"))?;
        let requires_tool = match f.next() {
            Some("0") => false,
            Some("1") => true,
            _ => return Err(bad(i, line, "no requires-tool flag")),
        };
        let sturdy = f.next().and_then(|v| v.parse::<u8>().ok()).unwrap_or(0) & 63;
        let solid = if f.next() == Some("1") { SOLID_BIT } else { 0 };
        let sturdy = sturdy | solid;
        rows.light.push((emission, dampening, down != 0));
        rows.hardness.push(hardness);
        rows.sturdy.push(sturdy);
        rows.requires_tool.push(requires_tool);
    }
    Ok(rows)
}

/// Бит `BlockState.isSolid` в байте прочных граней [`LightRows::sturdy`].
pub const SOLID_BIT: u8 = 1 << 6;

/// Твёрдость состояний блоков (4-я колонка `reports/light.csv`); -1 — неразрушимый.
pub fn load_hardness(root: &Path) -> Result<Vec<f32>, DataError> {
    Ok(load_light_rows(root)?.hardness)
}

/// Прочные грани состояний (6-я колонка `light.csv`): биты по граням низ, верх,
/// север, юг, запад, восток (`isFaceSturdy`), бит 6 — `isSolid` (7-я колонка).
pub fn load_sturdy(root: &Path) -> Result<Vec<u8>, DataError> {
    Ok(load_light_rows(root)?.sturdy)
}

/// Нужен ли инструмент для дропа (`requiresCorrectToolForDrops`, 5-я колонка `light.csv`).
pub fn load_requires_tool(root: &Path) -> Result<Vec<bool>, DataError> {
    Ok(load_light_rows(root)?.requires_tool)
}

/// Печи по порядку индексов в [`Fuel`]: обычная, доменная, коптильня.
pub const FURNACES: [&str; 3] = [
    "minecraft:furnace",
    "minecraft:blast_furnace",
    "minecraft:smoker",
];

/// Топливо (`minecraft:cooking_fuel`), вычисленное для каждой печи из [`FURNACES`]:
/// время горения в тиках и множитель скорости готовки.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fuel {
    pub burn: [i32; 3],
    pub speed: [f32; 3],
}

/// Вычисляет `context_int_provider`/`context_float_provider` (число, ссылка,
/// `div`, `conditional`) для блока печи `block`.
fn eval_provider(
    root: &Path,
    v: &serde_json::Value,
    float: bool,
    block: &str,
    depth: u32,
) -> Option<f64> {
    if depth > 16 {
        return None;
    }
    let dir = if float {
        "context_float_provider"
    } else {
        "context_int_provider"
    };
    match v {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(id) => {
            let (ns, path) = id.split_once(':').unwrap_or(("minecraft", id));
            let json = read_json(&root.join(format!("data/{ns}/{dir}/{path}.json"))).ok()?;
            eval_provider(root, &json, float, block, depth + 1)
        }
        serde_json::Value::Object(o) => match o.get("type")?.as_str()? {
            "minecraft:constant" => eval_provider(root, o.get("value")?, float, block, depth + 1),
            "minecraft:div" => {
                let l = eval_provider(root, o.get("left")?, float, block, depth + 1)?;
                let r = eval_provider(root, o.get("right")?, float, block, depth + 1)?;
                Some(match (float, r == 0.0) {
                    (_, true) => 0.0,
                    (true, _) => l / r,
                    (false, _) => (l as i64 / r as i64) as f64,
                })
            }
            "minecraft:conditional" => {
                let on = if block_predicate(root, o.get("condition")?, block)? {
                    "on_true"
                } else {
                    "on_false"
                };
                eval_provider(root, o.get(on)?, float, block, depth + 1)
            }
            _ => None,
        },
        _ => None,
    }
}

/// Предикат блока: ссылка на `predicate/…` или `match_block`.
fn block_predicate(root: &Path, v: &serde_json::Value, block: &str) -> Option<bool> {
    match v {
        serde_json::Value::String(id) => {
            let (ns, path) = id.split_once(':').unwrap_or(("minecraft", id));
            let json = read_json(&root.join(format!("data/{ns}/predicate/{path}.json"))).ok()?;
            block_predicate(root, &json, block)
        }
        serde_json::Value::Object(o) if o.get("type")?.as_str()? == "minecraft:match_block" => {
            Some(
                o.get("blocks")?
                    .as_array()?
                    .iter()
                    .any(|b| b.as_str() == Some(block)),
            )
        }
        _ => None,
    }
}

fn parse_fuel(root: &Path, c: &serde_json::Value) -> Option<Fuel> {
    let f = c.get("minecraft:cooking_fuel")?;
    let mut fuel = Fuel {
        burn: [0; 3],
        speed: [1.0; 3],
    };
    for (i, block) in FURNACES.iter().enumerate() {
        fuel.burn[i] = eval_provider(root, f.get("burn_time")?, false, block, 0)? as i32;
        if let Some(s) = f.get("speed_multiplier") {
            fuel.speed[i] = eval_provider(root, s, true, block, 0)? as f32;
        }
    }
    Some(fuel)
}

/// Еда: компонент `minecraft:food` и время поедания из `minecraft:consumable`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Food {
    pub nutrition: i32,
    pub saturation: f32,
    pub can_always_eat: bool,
    pub seconds: f32,
}

/// Питьё без сытости (`minecraft:potion`): секунды и ID остатка (`-1` — ничего).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drink {
    pub seconds: f32,
    /// ID предмета `minecraft:use_remainder`; `-1` — остатка нет.
    pub remainder: i32,
}

/// Свойства предмета, нужные выживанию.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemInfo {
    pub max_stack: u8,
    pub food: Option<Food>,
    /// Питьё с `potion_contents` и без `food`.
    pub drink: Option<Drink>,
    /// Слот `minecraft:equippable`: `head`, `chest`, `legs`, `feet`, `offhand`…
    pub equip: Option<&'static str>,
    /// `minecraft:max_damage` (0 — предмет не изнашивается).
    pub max_damage: i32,
    pub fuel: Option<Fuel>,
    /// Сумма `minecraft:armor` (`add_value`).
    pub armor: f32,
    /// Сумма `minecraft:armor_toughness` (`add_value`).
    pub toughness: f32,
    /// Добавка к урону `minecraft:attack_damage` (к базовым 1 игрока).
    pub attack: f32,
    /// Добавка к `minecraft:attack_speed` (к базовым 4 игрока).
    pub attack_speed: f32,
    /// `minecraft:attack_animation`: вид (`none` 0, `whack` 1, `stab` 2) и
    /// длительность в тиках; без компонента — `SwingAnimation.DEFAULT`.
    pub swing: (u8, u8),
}

impl Default for ItemInfo {
    fn default() -> Self {
        Self {
            max_stack: 64,
            food: None,
            drink: None,
            equip: None,
            max_damage: 0,
            fuel: None,
            armor: 0.0,
            toughness: 0.0,
            attack: 0.0,
            attack_speed: 0.0,
            swing: DEFAULT_SWING,
        }
    }
}

/// `SwingAnimation.DEFAULT`: `whack` на 6 тиков.
pub const DEFAULT_SWING: (u8, u8) = (1, 6);

/// `minecraft:attack_animation` из текста отчёта предмета. Компонент есть у
/// каждого предмета, но непустой только у копий (`stab`), поэтому JSON
/// целиком не разбирается: берётся объект сразу за ключом.
fn parse_swing(text: &str) -> (u8, u8) {
    let Some((_, rest)) = text.split_once("\"minecraft:attack_animation\":") else {
        return DEFAULT_SWING;
    };
    let body = rest.split_once('}').map_or("", |(b, _)| b);
    let field = |key: &str| {
        body.split_once(key).map(|(_, v)| {
            v.trim_start_matches([':', ' ', '"'])
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect::<String>()
        })
    };
    let kind = match field("\"type\"").as_deref() {
        Some("none") => 0,
        Some("stab") => 2,
        _ => DEFAULT_SWING.0,
    };
    let duration = field("\"duration\"")
        .and_then(|d| d.parse().ok())
        .unwrap_or(DEFAULT_SWING.1);
    (kind, duration)
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
        if let Some(n) = text
            .split_once("\"minecraft:max_damage\":")
            .and_then(|(_, rest)| {
                let digits: String = rest
                    .trim_start()
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect();
                digits.parse::<i32>().ok()
            })
        {
            out[id as usize].max_damage = n;
        }
        out[id as usize].swing = parse_swing(&text);
        // Еды и зелий немного: такие файлы разбираем целиком.
        if text.contains("\"minecraft:food\"")
            || text.contains("\"minecraft:potion_contents\"")
            || text.contains("\"minecraft:equippable\"")
            || text.contains("\"minecraft:cooking_fuel\"")
            || text.contains("attack_damage")
        {
            let json: serde_json::Value =
                serde_json::from_str(&text).map_err(|e| DataError::Format {
                    file: file.display().to_string(),
                    msg: e.to_string(),
                })?;
            let c = &json["components"];
            out[id as usize].food = parse_food(c);
            out[id as usize].drink = parse_drink(c, items);
            out[id as usize].fuel = parse_fuel(root, c);
            out[id as usize].equip = c["minecraft:equippable"]["slot"]
                .as_str()
                .and_then(equip_slot);
            let (armor, toughness) = parse_armor(c);
            out[id as usize].armor = armor;
            out[id as usize].toughness = toughness;
            out[id as usize].attack = attr_add(c, "attack_damage");
            out[id as usize].attack_speed = attr_add(c, "attack_speed");
        }
    }
    Ok(out)
}

/// Зелье: есть `potion_contents` и `consumable`, нет `food` (еда идёт своим путём).
fn parse_drink(c: &serde_json::Value, items: &HashMap<String, i32>) -> Option<Drink> {
    if c.get("minecraft:food").is_some() || c.get("minecraft:potion_contents").is_none() {
        return None;
    }
    let cons = c.get("minecraft:consumable")?;
    let seconds = cons
        .get("consume_seconds")
        .and_then(|v| v.as_f64())
        .map_or(DEFAULT_CONSUME_SECONDS, |s| s as f32);
    let remainder = c
        .pointer("/minecraft:use_remainder/id")
        .and_then(|v| v.as_str())
        .and_then(|n| items.get(n).copied())
        .unwrap_or(-1);
    Some(Drink { seconds, remainder })
}

/// Сумма модификаторов `add_value` атрибута `name` (`minecraft:<name>`).
fn attr_add(c: &serde_json::Value, name: &str) -> f32 {
    let raw = &c["minecraft:attribute_modifiers"];
    let Some(list) = raw
        .as_array()
        .or_else(|| raw.get("modifiers").and_then(|m| m.as_array()))
    else {
        return 0.0;
    };
    list.iter()
        .filter(|m| {
            let ty = m
                .get("type")
                .or_else(|| m.get("attribute"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let op = m
                .get("operation")
                .and_then(|v| v.as_str())
                .unwrap_or("add_value");
            ty.rsplit([':', '.']).next() == Some(name) && (op == "add_value" || op == "add")
        })
        .filter_map(|m| m.get("amount").and_then(|v| v.as_f64()))
        .sum::<f64>() as f32
}

/// Очки брони и прочность из `attribute_modifiers` (массив или `{modifiers: [...]}`).
fn parse_armor(c: &serde_json::Value) -> (f32, f32) {
    let raw = &c["minecraft:attribute_modifiers"];
    let Some(list) = raw
        .as_array()
        .or_else(|| raw.get("modifiers").and_then(|m| m.as_array()))
    else {
        return (0.0, 0.0);
    };
    let mut armor = 0.0;
    let mut toughness = 0.0;
    for m in list {
        let ty = m
            .get("type")
            .or_else(|| m.get("attribute"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let amount = m.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let op = m
            .get("operation")
            .and_then(|v| v.as_str())
            .unwrap_or("add_value");
        if op != "add_value" && op != "add" {
            continue;
        }
        if ty.ends_with("armor_toughness") || ty.ends_with("generic.armor_toughness") {
            toughness += amount;
        } else if ty.ends_with(":armor") || ty.ends_with("generic.armor") || ty == "armor" {
            armor += amount;
        }
    }
    (armor, toughness)
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

pub fn read_json(path: &Path) -> Result<serde_json::Value, DataError> {
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
    states: Arc<Vec<Option<(String, Properties)>>>,
    /// Обратный индекс: `name[k=v,...]` → ID. Строится при первом запросе.
    by_key: std::sync::OnceLock<HashMap<String, u32>>,
}

/// Обратный индекс — производное значение, поэтому сравнивается только содержимое.
impl PartialEq for Blocks {
    fn eq(&self, other: &Self) -> bool {
        self.max_id == other.max_id
            && self.defaults == other.defaults
            && self.states == other.states
    }
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
            states: Arc::new(states),
            by_key: std::sync::OnceLock::new(),
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
        self.by_key()?.get(&state_key(name, &props)).copied()
    }

    /// Обратный индекс строится при первом обращении: серверу он нужен
    /// для сохранённых чанков, а не для запуска.
    fn by_key(&self) -> Option<&HashMap<String, u32>> {
        if let Some(map) = self.by_key.get() {
            return Some(map);
        }
        let mut map = HashMap::with_capacity(self.states.len());
        for (id, s) in self.states.iter().enumerate() {
            if let Some((name, props)) = s {
                map.insert(state_key(name, props), id as u32);
            }
        }
        Some(self.by_key.get_or_init(|| map))
    }

    /// Устойчивый взгляд на содержимое для образа запуска.
    #[allow(clippy::type_complexity)]
    pub fn parts(&self) -> (&HashMap<String, i32>, i64, &[Option<(String, Properties)>]) {
        (&self.defaults, self.max_id, &self.states)
    }

    /// Общий список состояний: `Placement` берёт его без копии.
    pub fn states(&self) -> Arc<Vec<Option<(String, Properties)>>> {
        self.states.clone()
    }

    /// Обратно из [`Self::parts`]: обратный индекс строится при первом запросе.
    pub fn from_parts(
        defaults: HashMap<String, i32>,
        max_id: i64,
        states: Vec<Option<(String, Properties)>>,
    ) -> Self {
        Self {
            defaults,
            max_id,
            states: Arc::new(states),
            by_key: std::sync::OnceLock::new(),
        }
    }

    pub fn default_state(&self, name: &str) -> Result<i32, DataError> {
        self.defaults
            .get(name)
            .copied()
            .ok_or_else(|| DataError::Io(format!("unknown block {name}")))
    }

    /// Состояние по умолчанию для каждого имени блока (для `Block.setType`).
    pub fn defaults(&self) -> HashMap<String, u32> {
        self.defaults
            .iter()
            .map(|(name, id)| (name.clone(), *id as u32))
            .collect()
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
    write_atomic(path, text.as_bytes())
}

/// То же для двоичного образа.
pub(crate) fn write_derived_bytes(path: &Path, bytes: &[u8]) -> Result<(), DataError> {
    write_atomic(path, bytes)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), DataError> {
    let io = |e: std::io::Error| DataError::Io(format!("write {}: {e}", path.display()));
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).map_err(io)?;
    std::fs::rename(&tmp, path).map_err(io)
}

/// Таблицы лута (`loot_table/**`) и именованные условия (`predicate/**`,
/// на них ссылаются строкой, например `minecraft:tool/can_silk_touch`).
///
/// Ключ таблицы — путь от `loot_table` без расширения: `minecraft:blocks/stone`,
/// `minecraft:entities/zombie`, `minecraft:chests/abandoned_mineshaft`.
/// `/loot` берёт таблицу по этому же ключу.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct BlockLoot {
    pub tables: HashMap<String, serde_json::Value>,
    pub predicates: HashMap<String, serde_json::Value>,
}

impl BlockLoot {
    /// Таблица по ключу. Старые ключи блоков без `blocks/` тоже понимаются:
    /// так их видел дроп блоков до появления `/loot`.
    pub fn table(&self, name: &str) -> Option<&serde_json::Value> {
        let name = if name.contains(':') {
            name.to_owned()
        } else {
            format!("minecraft:{name}")
        };
        self.tables.get(&name).or_else(|| {
            let path = name.strip_prefix("minecraft:")?;
            self.tables.get(&format!("minecraft:blocks/{path}"))
        })
    }
}

/// Лут через сводку `loot.json`.
pub fn load_block_loot_cached(root: &Path) -> Result<BlockLoot, DataError> {
    // v3: ключом стала полная дорожка таблицы, а не одно имя файла.
    let path = derived(root, "loot.v3.json");
    if let Ok(text) = std::fs::read_to_string(&path)
        && let Ok(loot) = serde_json::from_str(&text)
    {
        return Ok(loot);
    }
    let mut loot = BlockLoot::default();
    let base = root.join("data/minecraft");
    let mut files = Vec::new();
    let tables = base.join("loot_table");
    collect_json(&tables, &mut files);
    for p in files.drain(..) {
        // Путь как он записан в моде и в `/loot`: `chests/village/...`,
        // `entities/zombie`, `blocks/stone`.
        let stem = p.with_extension("");
        let Ok(rel) = stem.strip_prefix(&tables) else {
            continue;
        };
        let rel = rel.to_string_lossy().replace('\\', "/");
        loot.tables
            .insert(format!("minecraft:{rel}"), read_json(&p)?);
    }
    let pred = base.join("predicate");
    collect_json(&pred, &mut files);
    for p in files {
        if let Ok(rel) = p.with_extension("").strip_prefix(&pred) {
            let id = rel.to_string_lossy().replace('\\', "/");
            loot.predicates
                .insert(format!("minecraft:{id}"), read_json(&p)?);
        }
    }
    let text = serde_json::to_string(&loot).map_err(|e| DataError::Io(e.to_string()))?;
    write_derived(&path, &text)?;
    Ok(loot)
}

fn collect_json(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_json(&p, out);
        } else if p.extension().is_some_and(|x| x == "json") {
            out.push(p);
        }
    }
}

/// Лут, разбираемый в фоне: сервер принимает игроков, не дожидаясь таблиц,
/// а первый дроп ждёт готовности.
pub struct LootSource {
    value: std::sync::OnceLock<BlockLoot>,
    ready: std::sync::Mutex<bool>,
    signal: std::sync::Condvar,
}

impl std::fmt::Debug for LootSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LootSource")
            .field("ready", &self.value.get().is_some())
            .finish()
    }
}

impl Default for LootSource {
    fn default() -> Self {
        Self::new()
    }
}

impl std::ops::Deref for LootSource {
    type Target = BlockLoot;

    /// Ждёт разбора, если он ещё идёт: несколько десятков миллисекунд.
    fn deref(&self) -> &BlockLoot {
        if let Some(v) = self.value.get() {
            return v;
        }
        let mut ready = self.ready.lock().unwrap();
        while !*ready {
            ready = self.signal.wait(ready).unwrap();
        }
        drop(ready);
        // Готовность выставлена уже после заполнения.
        self.value.get().expect("loot is ready")
    }
}

impl LootSource {
    pub fn new() -> Self {
        Self {
            value: std::sync::OnceLock::new(),
            ready: std::sync::Mutex::new(false),
            signal: std::sync::Condvar::new(),
        }
    }

    /// Загружает таблицы на своём потоке; ошибки уходят в лог, лут остаётся пустым.
    /// Готовность выставляется всегда: падение потока не должно подвесить дроп.
    pub fn load_in_background(root: &Path) -> Arc<Self> {
        let source = Arc::new(Self::new());
        let this = source.clone();
        let root = root.to_path_buf();
        let spawned = std::thread::Builder::new()
            .name("loot".into())
            .spawn(move || {
                // Отметка на случай паники: `Deref` ждёт именно её.
                let _ready = MarkReady(this.clone());
                let loot = load_block_loot_cached(&root).unwrap_or_else(|error| {
                    tracing::warn!("loot tables: {error}");
                    BlockLoot::default()
                });
                this.set(loot);
            });
        if let Err(error) = spawned {
            tracing::warn!("loot tables: cannot start loader: {error}");
            source.set(BlockLoot::default());
        }
        source
    }

    pub fn set(&self, loot: BlockLoot) {
        let _ = self.value.set(loot);
        *self.ready.lock().unwrap() = true;
        self.signal.notify_all();
    }
}

/// Отмечает готовность при выходе из области видимости, включая панику.
struct MarkReady(Arc<LootSource>);

impl Drop for MarkReady {
    fn drop(&mut self) {
        if self.0.value.get().is_none() {
            self.0.set(BlockLoot::default());
        }
    }
}

/// `load_items` через сводку `items.v4.tsv`: имя, стопка, питьё, броня, урон и для еды — сытость.
pub fn load_items_cached(
    root: &Path,
    items: &HashMap<String, i32>,
) -> Result<Vec<ItemInfo>, DataError> {
    let path = derived(root, "items.v4.tsv");
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
    [
        "mainhand", "offhand", "feet", "legs", "chest", "head", "body", "saddle",
    ]
    .into_iter()
    .find(|s| *s == name)
}

/// `name, max_stack, equip (-), max_damage, fuel, секунды питья (-), остаток (-),
/// броня, прочность брони, урон, скорость атаки, взмах (вид,тики)`
/// и для еды — сытость, насыщение, можно ли всегда, секунды. Поля через табуляцию.
fn item_line(name: &str, i: &ItemInfo) -> String {
    let fuel = i.fuel.map_or("-".to_owned(), |f| {
        format!(
            "{},{},{};{},{},{}",
            f.burn[0], f.burn[1], f.burn[2], f.speed[0], f.speed[1], f.speed[2]
        )
    });
    let drink = match i.drink {
        Some(d) => format!(
            "{}\t{}",
            d.seconds,
            if d.remainder < 0 {
                "-".to_owned()
            } else {
                d.remainder.to_string()
            }
        ),
        None => "-\t-".to_owned(),
    };
    let head = format!(
        "{name}\t{}\t{}\t{}\t{fuel}\t{drink}\t{}\t{}\t{}\t{}\t{},{}",
        i.max_stack,
        i.equip.unwrap_or("-"),
        i.max_damage,
        i.armor,
        i.toughness,
        i.attack,
        i.attack_speed,
        i.swing.0,
        i.swing.1
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
    let fuel = match f.next()? {
        "-" => None,
        s => {
            let (b, sp) = s.split_once(';')?;
            let b: Vec<i32> = b
                .split(',')
                .map(|x| x.parse().ok())
                .collect::<Option<_>>()?;
            let sp: Vec<f32> = sp
                .split(',')
                .map(|x| x.parse().ok())
                .collect::<Option<_>>()?;
            Some(Fuel {
                burn: b.try_into().ok()?,
                speed: sp.try_into().ok()?,
            })
        }
    };
    let drink_seconds = f.next()?;
    let drink_rest = f.next()?;
    let drink = if drink_seconds == "-" {
        None
    } else {
        Some(Drink {
            seconds: drink_seconds.parse().ok()?,
            remainder: if drink_rest == "-" {
                -1
            } else {
                drink_rest.parse().ok()?
            },
        })
    };
    let armor = f.next()?.parse().ok()?;
    let toughness = f.next()?.parse().ok()?;
    let attack = f.next()?.parse().ok()?;
    let attack_speed = f.next()?.parse().ok()?;
    let (kind, duration) = f.next()?.split_once(',')?;
    let swing = (kind.parse().ok()?, duration.parse().ok()?);
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
            drink,
            equip,
            max_damage,
            fuel,
            armor,
            toughness,
            attack,
            attack_speed,
            swing,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loot_source_waits_for_background_parse() {
        let dir = std::env::temp_dir().join(format!("chunkwake-loot-{}", std::process::id()));
        let base = dir.join("data/minecraft");
        std::fs::create_dir_all(base.join("loot_table/blocks")).unwrap();
        std::fs::write(
            base.join("loot_table/blocks/stone.json"),
            r#"{"pools":[{"rolls":1,"entries":[{"type":"minecraft:item","name":"minecraft:stone"}]}]}"#,
        )
        .unwrap();
        let source = LootSource::load_in_background(&dir);
        // Первое обращение ждёт разбора, а не отдаёт пустые таблицы.
        assert!(source.tables.contains_key("minecraft:blocks/stone"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn light_rows_are_one_pass() {
        let dir = std::env::temp_dir().join(format!("chunkwake-light-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("reports")).unwrap();
        std::fs::write(
            dir.join("reports/light.csv"),
            "0,15,1,-1.0,0,0\n15,0,0,1.5,1,63\n",
        )
        .unwrap();
        let rows = load_light_rows(&dir).unwrap();
        assert_eq!(rows.light, vec![(0, 15, true), (15, 0, false)]);
        assert_eq!(rows.hardness, vec![-1.0, 1.5]);
        assert_eq!(rows.sturdy, vec![0, 63]);
        assert_eq!(rows.requires_tool, vec![false, true]);
        std::fs::remove_dir_all(dir).unwrap();
    }

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
    fn potion_is_a_drink() {
        let items = HashMap::from([("minecraft:glass_bottle".to_string(), 4)]);
        let c = serde_json::json!({
            "minecraft:consumable": {"animation": "drink"},
            "minecraft:potion_contents": {},
            "minecraft:use_remainder": {"id": "minecraft:glass_bottle"}
        });
        let d = parse_drink(&c, &items).unwrap();
        assert_eq!((d.seconds, d.remainder), (1.6, 4));
        let food = serde_json::json!({
            "minecraft:food": {"nutrition": 1, "saturation": 1.0},
            "minecraft:potion_contents": {}
        });
        assert!(parse_drink(&food, &items).is_none());
    }

    #[test]
    fn armor_modifiers_sum() {
        let list = serde_json::json!({
            "minecraft:attribute_modifiers": {
                "modifiers": [
                    {"type": "minecraft:armor", "amount": 8.0, "operation": "add_value"},
                    {"type": "minecraft:armor_toughness", "amount": 2.0, "operation": "add_value"},
                    {"type": "minecraft:armor", "amount": 1.0, "operation": "add_multiplied_base"}
                ]
            }
        });
        assert_eq!(parse_armor(&list), (8.0, 2.0));
        let flat = serde_json::json!({
            "minecraft:attribute_modifiers": [
                {"attribute": "minecraft:generic.armor", "amount": 3, "operation": "add"}
            ]
        });
        assert_eq!(parse_armor(&flat), (3.0, 0.0));
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
                drink: Some(Drink {
                    seconds: 1.6,
                    remainder: 12,
                }),
                equip: Some("head"),
                max_damage: 165,
                fuel: Some(Fuel {
                    burn: [1600, 800, 800],
                    speed: [1.0, 2.0, 2.0],
                }),
                armor: 8.0,
                toughness: 2.0,
                attack: 5.0,
                attack_speed: -2.4,
                swing: (2, 13),
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

    /// Взмах копья из отчёта 26.3 и умолчание у пустого компонента.
    #[test]
    fn swing_from_report() {
        let spear = r#"{"components":{"minecraft:attack_animation":{"type":"stab","duration":13},"minecraft:attack_range":{}}}"#;
        assert_eq!(parse_swing(spear), (2, 13));
        let pretty =
            "\"minecraft:attack_animation\": {\n  \"type\": \"stab\",\n  \"duration\": 21\n},";
        assert_eq!(parse_swing(pretty), (2, 21));
        assert_eq!(
            parse_swing(r#"{"minecraft:attack_animation":{},"x":{"duration":9}}"#),
            DEFAULT_SWING
        );
        assert_eq!(parse_swing("{}"), DEFAULT_SWING);
    }
}

/// Цвет на карте (`MapColor.id`) по id состояния из `reports/map_colors.csv`
/// (выгрузка `ChunkwakeMapColors`): строки `имя;цвет` или `имя[k=v,...];цвет`.
///
/// Файла может не быть (кэш до выгрузки): тогда список пуст и карты не рисуют.
pub fn load_map_colors(root: &Path, blocks: &Blocks) -> Result<Vec<u8>, DataError> {
    let path = root.join("reports/map_colors.csv");
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(DataError::Io(format!("read {}: {e}", path.display()))),
    };
    let mut by_key: HashMap<&str, u8> = HashMap::new();
    for (i, line) in text.lines().enumerate() {
        let parsed = line
            .rsplit_once(';')
            .and_then(|(k, c)| Some((k, c.trim().parse::<u8>().ok()?)));
        let Some((key, color)) = parsed else {
            return Err(DataError::Format {
                file: path.display().to_string(),
                msg: format!("line {}: {line:?}", i + 1),
            });
        };
        by_key.insert(key, color);
    }
    let states = blocks.states();
    Ok(states
        .iter()
        .map(|s| {
            s.as_ref().map_or(0, |(name, props)| {
                by_key
                    .get(state_key(name, props).as_str())
                    .or_else(|| by_key.get(name.as_str()))
                    .copied()
                    .unwrap_or(0)
            })
        })
        .collect())
}

/// Физика состояний блоков из `reports/shapes.csv` (выгрузка `ChunkwakeShapes`).
#[derive(Debug, Clone, Default)]
pub struct BlockPhysics {
    /// `Block.getFriction` по id состояния.
    pub friction: Vec<f32>,
    /// `getSpeedFactor` (душевой песок, мёд).
    pub speed: Vec<f32>,
    /// `getJumpFactor` (мёд).
    pub jump: Vec<f32>,
    /// Индекс формы в `boxes` по id состояния.
    pub shape: Vec<u32>,
    /// Разные формы коллизии: коробки `x0 y0 z0 x1 y1 z1` в долях блока.
    pub boxes: Vec<Vec<[f64; 6]>>,
}

impl BlockPhysics {
    /// Коробки коллизии состояния; неизвестное — полный куб.
    pub fn boxes_of(&self, state: u32) -> &[[f64; 6]] {
        const FULL: &[[f64; 6]] = &[[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]];
        self.shape
            .get(state as usize)
            .map_or(FULL, |&i| self.boxes[i as usize].as_slice())
    }
}

/// Строки `reports/shapes.csv`: `трение,скорость,прыжок,коробки через |`.
pub fn load_physics(root: &Path) -> Result<BlockPhysics, DataError> {
    let path = root.join("reports/shapes.csv");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| DataError::Io(format!("read {}: {e}", path.display())))?;
    let bad = |i: usize, line: &str| DataError::Format {
        file: path.display().to_string(),
        msg: format!("line {}: {line:?}", i + 1),
    };
    let mut out = BlockPhysics::default();
    let mut seen: HashMap<String, u32> = HashMap::new();
    for (i, line) in text.lines().enumerate() {
        let mut f = line.splitn(4, ',');
        let mut num = || f.next().and_then(|v| v.trim().parse::<f32>().ok());
        let (Some(fr), Some(sp), Some(ju)) = (num(), num(), num()) else {
            return Err(bad(i, line));
        };
        let shape = f.next().unwrap_or("").trim();
        let next = seen.len() as u32;
        let idx = match seen.get(shape) {
            Some(&k) => k,
            None => {
                let mut boxes = Vec::new();
                for b in shape.split('|').filter(|b| !b.is_empty()) {
                    let v: Vec<f64> = b.split(' ').filter_map(|x| x.parse().ok()).collect();
                    let a: [f64; 6] = v.try_into().map_err(|_| bad(i, line))?;
                    boxes.push(a);
                }
                out.boxes.push(boxes);
                seen.insert(shape.to_string(), next);
                next
            }
        };
        out.friction.push(fr);
        out.speed.push(sp);
        out.jump.push(ju);
        out.shape.push(idx);
    }
    Ok(out)
}

/// Инструмент нотного блока по имени блока (`reports/instruments.csv`).
///
/// Инструмент задаётся свойством блока (`instrument(...)` в
/// `BlockBehaviour.Properties`), поэтому одно значение на блок, а не на
/// состояние. Строки есть только для нестандартных: остальные — `harp`.
#[derive(Debug, Clone, Default)]
pub struct Instruments {
    kinds: HashMap<String, NoteInstrument>,
}

/// Инструменты `NoteBlockInstrument` в порядке, в котором их знает ванила.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteInstrument {
    Harp,
    BaseDrum,
    Snare,
    Hat,
    Bass,
    Flute,
    Bell,
    Guitar,
    Chime,
    Xylophone,
    IronXylophone,
    CowBell,
    Didgeridoo,
    Bit,
    Banjo,
    Pling,
    Trumpet,
    TrumpetExposed,
    TrumpetOxidized,
    TrumpetWeathered,
    Zombie,
    Skeleton,
    Creeper,
    Dragon,
    WitherSkeleton,
    Piglin,
    CustomHead,
}

impl NoteInstrument {
    /// `getSerializedName()` — то, что лежит в отчёте и в свойстве блока.
    pub fn name(self) -> &'static str {
        match self {
            Self::Harp => "harp",
            Self::BaseDrum => "basedrum",
            Self::Snare => "snare",
            Self::Hat => "hat",
            Self::Bass => "bass",
            Self::Flute => "flute",
            Self::Bell => "bell",
            Self::Guitar => "guitar",
            Self::Chime => "chime",
            Self::Xylophone => "xylophone",
            Self::IronXylophone => "iron_xylophone",
            Self::CowBell => "cow_bell",
            Self::Didgeridoo => "didgeridoo",
            Self::Bit => "bit",
            Self::Banjo => "banjo",
            Self::Pling => "pling",
            Self::Trumpet => "trumpet",
            Self::TrumpetExposed => "trumpet_exposed",
            Self::TrumpetOxidized => "trumpet_oxidized",
            Self::TrumpetWeathered => "trumpet_weathered",
            Self::Zombie => "zombie",
            Self::Skeleton => "skeleton",
            Self::Creeper => "creeper",
            Self::Dragon => "dragon",
            Self::WitherSkeleton => "wither_skeleton",
            Self::Piglin => "piglin",
            Self::CustomHead => "custom_head",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Some(match name.trim() {
            "harp" => Self::Harp,
            "basedrum" => Self::BaseDrum,
            "snare" => Self::Snare,
            "hat" => Self::Hat,
            "bass" => Self::Bass,
            "flute" => Self::Flute,
            "bell" => Self::Bell,
            "guitar" => Self::Guitar,
            "chime" => Self::Chime,
            "xylophone" => Self::Xylophone,
            "iron_xylophone" => Self::IronXylophone,
            "cow_bell" => Self::CowBell,
            "didgeridoo" => Self::Didgeridoo,
            "bit" => Self::Bit,
            "banjo" => Self::Banjo,
            "pling" => Self::Pling,
            "trumpet" => Self::Trumpet,
            "trumpet_exposed" => Self::TrumpetExposed,
            "trumpet_oxidized" => Self::TrumpetOxidized,
            "trumpet_weathered" => Self::TrumpetWeathered,
            "zombie" => Self::Zombie,
            "skeleton" => Self::Skeleton,
            "creeper" => Self::Creeper,
            "dragon" => Self::Dragon,
            "wither_skeleton" => Self::WitherSkeleton,
            "piglin" => Self::Piglin,
            "custom_head" => Self::CustomHead,
            _ => return None,
        })
    }

    /// `isTunable()`: `BASE_BLOCK` — звук зависит от `note`.
    pub fn tunable(self) -> bool {
        !matches!(
            self,
            Self::Zombie
                | Self::Skeleton
                | Self::Creeper
                | Self::Dragon
                | Self::WitherSkeleton
                | Self::Piglin
                | Self::CustomHead
        )
    }

    /// `worksAboveNoteBlock()`: блок над нотным блоком задаёт инструмент.
    pub fn works_above(self) -> bool {
        !self.tunable()
    }

    /// `hasCustomSound()`: у головы игрока звук берётся из её блок-сущности.
    pub fn custom_sound(self) -> bool {
        self == Self::CustomHead
    }

    /// `instrument.getSoundEvent()` — идентификатор звука.
    pub fn sound(self) -> &'static str {
        match self {
            Self::Harp => "minecraft:block.note_block.harp",
            Self::BaseDrum => "minecraft:block.note_block.basedrum",
            Self::Snare => "minecraft:block.note_block.snare",
            Self::Hat => "minecraft:block.note_block.hat",
            Self::Bass => "minecraft:block.note_block.bass",
            Self::Flute => "minecraft:block.note_block.flute",
            Self::Bell => "minecraft:block.note_block.bell",
            Self::Guitar => "minecraft:block.note_block.guitar",
            Self::Chime => "minecraft:block.note_block.chime",
            Self::Xylophone => "minecraft:block.note_block.xylophone",
            Self::IronXylophone => "minecraft:block.note_block.iron_xylophone",
            Self::CowBell => "minecraft:block.note_block.cow_bell",
            Self::Didgeridoo => "minecraft:block.note_block.didgeridoo",
            Self::Bit => "minecraft:block.note_block.bit",
            Self::Banjo => "minecraft:block.note_block.banjo",
            Self::Pling => "minecraft:block.note_block.pling",
            Self::Trumpet => "minecraft:block.note_block.trumpet",
            Self::TrumpetExposed => "minecraft:block.note_block.trumpet_exposed",
            Self::TrumpetOxidized => "minecraft:block.note_block.trumpet_oxidized",
            Self::TrumpetWeathered => "minecraft:block.note_block.trumpet_weathered",
            Self::Zombie => "minecraft:block.note_block.imitate.zombie",
            Self::Skeleton => "minecraft:block.note_block.imitate.skeleton",
            Self::Creeper => "minecraft:block.note_block.imitate.creeper",
            Self::Dragon => "minecraft:block.note_block.imitate.ender_dragon",
            Self::WitherSkeleton => "minecraft:block.note_block.imitate.wither_skeleton",
            Self::Piglin => "minecraft:block.note_block.imitate.piglin",
            Self::CustomHead => "minecraft:ui.button.click",
        }
    }
}

impl Instruments {
    /// Инструмент блока по имени; неизвестное — `harp`.
    pub fn of(&self, block: &str) -> NoteInstrument {
        self.kinds
            .get(block)
            .copied()
            .unwrap_or(NoteInstrument::Harp)
    }
}

/// Строки `reports/instruments.csv`: `имя блока,инструмент`.
///
/// Файла может не быть: кэш, собранный до появления выгрузки, остаётся
/// годным — тогда все блоки звучат как `harp`.
pub fn load_instruments(root: &Path) -> Result<Instruments, DataError> {
    let path = root.join("reports/instruments.csv");
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Instruments::default()),
        Err(e) => return Err(DataError::Io(format!("read {}: {e}", path.display()))),
    };
    let mut out = Instruments::default();
    for (i, line) in text.lines().enumerate() {
        let Some((name, kind)) = line.split_once(',') else {
            return Err(DataError::Format {
                file: path.display().to_string(),
                msg: format!("line {}: {line:?}", i + 1),
            });
        };
        let kind = NoteInstrument::parse(kind).ok_or_else(|| DataError::Format {
            file: path.display().to_string(),
            msg: format!("line {}: неизвестный инструмент {kind:?}", i + 1),
        })?;
        out.kinds.insert(name.trim().to_owned(), kind);
    }
    Ok(out)
}

/// Песня проигрывателя (`JukeboxSong`) из `data/minecraft/jukebox_song`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JukeboxSong {
    /// Сетевой ID в реестре `jukebox_song`: данные события 1010.
    pub id: i32,
    /// `comparator_output`.
    pub comparator: i32,
    /// `lengthInTicks`: `ceil(length_in_seconds · 20)`.
    pub ticks: i64,
}

/// Пластинки: ID предмета → песня (`minecraft:jukebox_playable`).
///
/// Для песни `<имя>` смотрим компоненты предмета `music_disc_<имя>` и берём
/// его, только если `jukebox_playable` действительно указывает на эту песню:
/// так не нужно читать отчёты всех предметов на старте.
pub fn load_jukebox_songs(
    root: &Path,
    items: &HashMap<String, i32>,
    registries: &crate::registries::RegistryData,
) -> Result<HashMap<i32, JukeboxSong>, DataError> {
    let dir = root.join("data/minecraft/jukebox_song");
    let mut out = HashMap::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok(out);
    };
    for e in entries.flatten() {
        let path = e.path();
        let Some(name) = path.file_stem().and_then(|s| s.to_str()).map(str::to_owned) else {
            continue;
        };
        let key = format!("minecraft:{name}");
        let Ok(id) = registries.network_id("jukebox_song", &key) else {
            continue;
        };
        let song = read_json(&path)?;
        let item = format!("minecraft:music_disc_{name}");
        let Some(&item_id) = items.get(&item) else {
            continue;
        };
        let report = root.join(format!(
            "reports/minecraft/components/item/music_disc_{name}.json"
        ));
        let playable = std::fs::read_to_string(&report)
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|j| {
                j["components"]["minecraft:jukebox_playable"]
                    .as_str()
                    .map(str::to_owned)
            });
        if playable.as_deref() != Some(key.as_str()) {
            continue;
        }
        let seconds = song["length_in_seconds"].as_f64().unwrap_or(0.0) as f32;
        out.insert(
            item_id,
            JukeboxSong {
                id,
                comparator: song["comparator_output"].as_i64().unwrap_or(0) as i32,
                ticks: (seconds * 20.0).ceil() as i64,
            },
        );
    }
    Ok(out)
}

/// Тип сущности из `reports/entities.csv` (выгрузка `ChunkwakeEntities`).
#[derive(Clone, Debug, PartialEq)]
pub struct EntityTypeInfo {
    pub name: String,
    /// `EntityType.clientTrackingRange`, в чанках: слежение — ×16 блоков.
    pub tracking_range: i32,
    /// `EntityType.updateInterval`: тики между рассылками `ServerEntity`.
    pub update_interval: i32,
    pub width: f32,
    pub height: f32,
    /// `MobCategory.getName()`: `monster`, `creature`, `misc`…
    pub category: String,
    pub fire_immune: bool,
    /// `EntityDimensions.eyeHeight`.
    pub eye_height: f32,
    /// Базовые атрибуты (`DefaultAttributes`); `None` — тип не живой.
    pub attributes: Option<DefaultAttributes>,
}

/// Базовые значения атрибутов живого типа.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DefaultAttributes {
    pub max_health: f64,
    pub movement_speed: f64,
    pub follow_range: f64,
    /// Есть не у всех: у животных атрибута `attack_damage` нет.
    pub attack_damage: Option<f64>,
    pub armor: f64,
    pub knockback_resistance: f64,
    pub step_height: f64,
    pub safe_fall_distance: f64,
}

/// Строки `имя;дальность;интервал;ширина;высота;категория;fireImmune;глаза;`
/// и восемь атрибутов (пустые у неживых): `max_health;movement_speed;`
/// `follow_range;attack_damage;armor;knockback_resistance;step_height;`
/// `safe_fall_distance`.
fn parse_entity_line(line: &str) -> Option<EntityTypeInfo> {
    let f: Vec<&str> = line.split(';').collect();
    if f.len() != 16 {
        return None;
    }
    let opt = |s: &str| -> Option<Option<f64>> {
        if s.is_empty() {
            Some(None)
        } else {
            s.parse().ok().map(Some)
        }
    };
    let a: Vec<Option<f64>> = f[8..].iter().map(|s| opt(s)).collect::<Option<_>>()?;
    let attributes = match a[0] {
        None => None,
        Some(max_health) => Some(DefaultAttributes {
            max_health,
            movement_speed: a[1]?,
            follow_range: a[2]?,
            attack_damage: a[3],
            armor: a[4]?,
            knockback_resistance: a[5]?,
            step_height: a[6]?,
            safe_fall_distance: a[7]?,
        }),
    };
    Some(EntityTypeInfo {
        name: f[0].to_owned(),
        tracking_range: f[1].parse().ok()?,
        update_interval: f[2].parse().ok()?,
        width: f[3].parse().ok()?,
        height: f[4].parse().ok()?,
        category: f[5].to_owned(),
        fire_immune: f[6].parse().ok()?,
        eye_height: f[7].parse().ok()?,
        attributes,
    })
}

/// Типы сущностей в порядке реестра. Файла может не быть (кэш до выгрузки):
/// тогда список пуст и дальности берутся по умолчанию.
pub fn load_entity_types(root: &Path) -> Result<Vec<EntityTypeInfo>, DataError> {
    let path = root.join("reports/entities.csv");
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(DataError::Io(format!("read {}: {e}", path.display()))),
    };
    text.lines()
        .enumerate()
        .map(|(i, line)| {
            parse_entity_line(line).ok_or_else(|| DataError::Format {
                file: path.display().to_string(),
                msg: format!("line {}: {line:?}", i + 1),
            })
        })
        .collect()
}

#[cfg(test)]
mod entity_tests {
    use super::*;

    #[test]
    fn entity_line() {
        let z = parse_entity_line(
            "minecraft:zombie;8;3;0.6;1.95;monster;false;1.74;20.0;0.23000000417232513;35.0;3.0;2.0;0.0;0.6;3.0",
        )
        .unwrap();
        assert_eq!(z.name, "minecraft:zombie");
        assert_eq!((z.tracking_range, z.update_interval), (8, 3));
        assert_eq!((z.width, z.height), (0.6, 1.95));
        assert_eq!(z.category, "monster");
        assert!(!z.fire_immune);
        assert_eq!(z.eye_height, 1.74);
        let a = z.attributes.unwrap();
        assert_eq!((a.max_health, a.follow_range, a.armor), (20.0, 35.0, 2.0));
        assert_eq!(a.attack_damage, Some(3.0));
        assert_eq!(a.movement_speed as f32, 0.23);
        // Корова: без `attack_damage`; предмет: не живой.
        let cow = parse_entity_line(
            "minecraft:cow;10;3;0.9;1.4;creature;false;1.3;10.0;0.2;16.0;;0.0;0.0;0.6;3.0",
        )
        .unwrap();
        assert_eq!(cow.attributes.unwrap().attack_damage, None);
        let item =
            parse_entity_line("minecraft:item;6;20;0.25;0.25;misc;false;0.2125;;;;;;;;").unwrap();
        assert!(item.attributes.is_none());
        assert!(parse_entity_line("minecraft:zombie;8;3").is_none());
        assert!(
            parse_entity_line("minecraft:zombie;8;3;0.6;1.95;monster;false;x;;;;;;;;").is_none()
        );
    }
}
