//! Превращения блоков предметами: реестр `block_transformer` 26.3 (топор
//! снимает кору, счищает окисление и воск; мотыга вспахивает; лопата делает
//! тропинку), воск соты (`HoneycombItem.WAXABLES`) и окисление меди
//! (`WeatheringCopper.NEXT_BY_BLOCK`).
//!
//! `BlockTransformer.transformBlock`: перебор превращений по порядку; грань,
//! по которой кликнули, может быть запрещена; первое правило `rule_based`,
//! давшее состояние, побеждает.

use crate::placement::Placement;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// `LevelEvent.PARTICLES_SCRAPE` / `PARTICLES_AND_SOUND_WAX_ON` / `PARTICLES_WAX_OFF`.
pub const EVENT_SCRAPE: i32 = 3005;
pub const EVENT_WAX_ON: i32 = 3003;
pub const EVENT_WAX_OFF: i32 = 3004;

/// Семейства `WeatheringCopperCollection` из `WeatheringCopper.NEXT_BY_BLOCK`.
const COPPER_FAMILIES: [&str; 15] = [
    "copper_block",
    "cut_copper",
    "chiseled_copper",
    "cut_copper_slab",
    "cut_copper_stairs",
    "copper_door",
    "copper_trapdoor",
    "copper_bars",
    "copper_grate",
    "copper_bulb",
    "copper_lantern",
    "copper_chest",
    "copper_golem_statue",
    "lightning_rod",
    "copper_chain",
];

/// Четыре стадии семейства: `copper_block` → `exposed_copper`, остальные —
/// с приставкой стадии.
fn stages(base: &str) -> [String; 4] {
    let root = if base == "copper_block" { "copper" } else { base };
    [
        format!("minecraft:{base}"),
        format!("minecraft:exposed_{root}"),
        format!("minecraft:weathered_{root}"),
        format!("minecraft:oxidized_{root}"),
    ]
}

#[derive(Debug, Clone)]
enum Pred {
    /// `matching_blocks` / `matching_block_tag` со смещением.
    Blocks(HashSet<String>, [i32; 3]),
    All(Vec<Pred>),
    Any(Vec<Pred>),
    Not(Box<Pred>),
    True,
}

impl Pred {
    fn test(&self, pos: [i32; 3], name_at: &dyn Fn([i32; 3]) -> String) -> bool {
        match self {
            Pred::Blocks(set, o) => set.contains(&name_at([pos[0] + o[0], pos[1] + o[1], pos[2] + o[2]])),
            Pred::All(list) => list.iter().all(|p| p.test(pos, name_at)),
            Pred::Any(list) => list.iter().any(|p| p.test(pos, name_at)),
            Pred::Not(p) => !p.test(pos, name_at),
            Pred::True => true,
        }
    }
}

#[derive(Debug, Clone)]
enum Prov {
    /// Состояние по умолчанию и заданные свойства.
    State(String, Vec<(String, String)>),
    /// `copy_properties`: источник, на который переносятся общие свойства
    /// блока в клетке (`withPropertiesOf`).
    Copy(Box<Prov>),
}

impl Prov {
    fn get(&self, here: u32, pl: &Placement, defaults: &HashMap<String, u32>) -> Option<u32> {
        match self {
            Prov::State(id, props) => {
                let mut s = *defaults.get(id)?;
                for (k, v) in props {
                    s = pl.with_prop(s, k, v).unwrap_or(s);
                }
                Some(s)
            }
            Prov::Copy(source) => {
                let mut s = source.get(here, pl, defaults)?;
                for (k, v) in pl.props(here) {
                    s = pl.with_prop(s, k, v).unwrap_or(s);
                }
                Some(s)
            }
        }
    }
}

/// `BlockTransformer.BlockTransformData`.
#[derive(Debug, Clone)]
pub struct Transform {
    rules: Vec<(Pred, Prov)>,
    pub sound: Option<String>,
    /// Событие уровня частиц (0 — `none`).
    pub event: i32,
    /// Запрещённые грани в нумерации `Direction` (0 низ, 1 верх, 2..5 бока).
    pub disallowed_faces: Vec<u8>,
    pub loot: Option<String>,
    /// `drop_strategy: clicked_face`; иначе `from_middle`.
    pub drop_from_face: bool,
    pub update_from_neighbors: bool,
    pub copper_chest: bool,
    pub consume_on_use: bool,
    pub item_damage: i32,
}

impl Transform {
    /// `getOptionalState` правил по порядку.
    pub fn result(
        &self,
        pos: [i32; 3],
        here: u32,
        name_at: &dyn Fn([i32; 3]) -> String,
        pl: &Placement,
        defaults: &HashMap<String, u32>,
    ) -> Option<u32> {
        self.rules
            .iter()
            .find(|(p, _)| p.test(pos, name_at))
            .and_then(|(_, prov)| prov.get(here, pl, defaults))
    }
}

#[derive(Debug, Clone, Default)]
pub struct Transforms {
    /// Ключ реестра → список превращений.
    sets: HashMap<String, Vec<Transform>>,
    /// Предмет → ключ его компонента `block_transformer`.
    items: HashMap<String, String>,
    /// `HoneycombItem.WAXABLES`: блок → вощёный.
    pub waxed: HashMap<String, String>,
    /// Окисляемая медь: стадия 1..4 и следующая стадия.
    pub weathering: HashMap<String, (u8, Option<String>)>,
}

impl Transforms {
    pub fn for_item(&self, item: &str) -> Option<&[Transform]> {
        self.sets.get(self.items.get(item)?).map(Vec::as_slice)
    }

    /// Стадия окисляемого блока (`WeatheringCopper.getAge`), вощёные — нет.
    pub fn copper_age(&self, name: &str) -> Option<u8> {
        self.weathering.get(name).map(|w| w.0)
    }
}

fn face_index(name: &str) -> Option<u8> {
    Some(match name {
        "down" => 0,
        "up" => 1,
        "north" => 2,
        "south" => 3,
        "west" => 4,
        "east" => 5,
        _ => return None,
    })
}

fn ns(id: &str) -> String {
    if id.contains(':') {
        id.to_owned()
    } else {
        format!("minecraft:{id}")
    }
}

fn holder_set(v: &Value, tags: &HashMap<String, HashSet<String>>) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut add = |s: &str| {
        if let Some(tag) = s.strip_prefix('#') {
            out.extend(tags.get(&ns(tag)).into_iter().flatten().cloned());
        } else {
            out.insert(ns(s));
        }
    };
    match v {
        Value::String(s) => add(s),
        Value::Array(list) => list.iter().filter_map(Value::as_str).for_each(add),
        _ => {}
    }
    out
}

fn offset(v: &Value) -> [i32; 3] {
    let o: Vec<i32> = v["offset"]
        .as_array()
        .map(|a| a.iter().map(|x| x.as_i64().unwrap_or(0) as i32).collect())
        .unwrap_or_default();
    [
        o.first().copied().unwrap_or(0),
        o.get(1).copied().unwrap_or(0),
        o.get(2).copied().unwrap_or(0),
    ]
}

fn parse_pred(v: &Value, tags: &HashMap<String, HashSet<String>>) -> Result<Pred, String> {
    let list = |key: &str| -> Result<Vec<Pred>, String> {
        v[key]
            .as_array()
            .into_iter()
            .flatten()
            .map(|p| parse_pred(p, tags))
            .collect()
    };
    Ok(match v["type"].as_str().unwrap_or("") {
        "minecraft:matching_blocks" => Pred::Blocks(holder_set(&v["blocks"], tags), offset(v)),
        "minecraft:matching_block_tag" => {
            let tag = ns(v["tag"].as_str().unwrap_or(""));
            Pred::Blocks(tags.get(&tag).cloned().unwrap_or_default(), offset(v))
        }
        "minecraft:all_of" => Pred::All(list("predicates")?),
        "minecraft:any_of" => Pred::Any(list("predicates")?),
        "minecraft:not" => Pred::Not(Box::new(parse_pred(&v["predicate"], tags)?)),
        "minecraft:true" => Pred::True,
        other => return Err(format!("предикат {other}")),
    })
}

fn parse_prov(v: &Value) -> Result<Prov, String> {
    match v["type"].as_str() {
        None => {
            let id = ns(v["id"].as_str().ok_or("состояние без id")?);
            let props = v["properties"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(k, x)| (k.clone(), x.as_str().unwrap_or("").to_owned()))
                .collect();
            Ok(Prov::State(id, props))
        }
        Some("minecraft:simple_state_provider") => parse_prov(&v["state"]),
        Some("minecraft:copy_properties") => Ok(Prov::Copy(Box::new(parse_prov(&v["source"])?))),
        Some(other) => Err(format!("провайдер {other}")),
    }
}

fn parse_transform(v: &Value, tags: &HashMap<String, HashSet<String>>) -> Result<Transform, String> {
    let p = &v["block_state_provider"];
    let rules = match p["type"].as_str() {
        Some("minecraft:rule_based") => p["rules"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|r| Ok((parse_pred(&r["if_true"], tags)?, parse_prov(&r["then"])?)))
            .collect::<Result<Vec<_>, String>>()?,
        _ => vec![(Pred::True, parse_prov(p)?)],
    };
    Ok(Transform {
        rules,
        sound: v["sound"].as_str().map(ns),
        event: match v["particle"].as_str().unwrap_or("none") {
            "scrape" => EVENT_SCRAPE,
            "wax_on" => EVENT_WAX_ON,
            "wax_off" => EVENT_WAX_OFF,
            _ => 0,
        },
        disallowed_faces: v["disallowed_faces"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|f| face_index(f.as_str()?))
            .collect(),
        loot: v["loot"].as_str().map(ns),
        drop_from_face: v["drop_strategy"].as_str() == Some("clicked_face"),
        update_from_neighbors: v["update_from_neighbors"].as_bool().unwrap_or(true),
        copper_chest: v["transform_type"].as_str() == Some("copper_chest"),
        consume_on_use: v["consume_on_use"].as_bool().unwrap_or(true),
        item_damage: v["item_damage_per_use"].as_i64().unwrap_or(0) as i32,
    })
}

/// Реестр `block_transformer`, компоненты предметов и таблицы меди.
/// Чего нет в данных — пустые таблицы: сервер встаёт, предметы просто не
/// превращают блоки.
pub fn load(
    data_dir: &Path,
    block_tags: &HashMap<String, HashSet<String>>,
    blocks: &HashMap<String, u32>,
) -> Result<Transforms, String> {
    let mut out = Transforms::default();
    let dir = data_dir.join("data/minecraft/block_transformer");
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            let path = e.path();
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
            let v: Value = serde_json::from_str(&text).map_err(|e| format!("{stem}: {e}"))?;
            let list = v
                .as_array()
                .into_iter()
                .flatten()
                .map(|t| parse_transform(t, block_tags))
                .collect::<Result<Vec<_>, String>>()
                .map_err(|e| format!("block_transformer/{stem}: {e}"))?;
            out.sets.insert(format!("minecraft:{stem}"), list);
        }
    }
    let comp = data_dir.join("reports/minecraft/components/item");
    if let Ok(entries) = std::fs::read_dir(&comp) {
        for e in entries.flatten() {
            let path = e.path();
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if !text.contains("minecraft:block_transformer") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let Ok(v) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            if let Some(key) = v["components"]["minecraft:block_transformer"].as_str() {
                out.items.insert(format!("minecraft:{stem}"), ns(key));
            }
        }
    }
    for base in COPPER_FAMILIES {
        let names = stages(base);
        if !names.iter().all(|n| blocks.contains_key(n)) {
            continue;
        }
        for (i, name) in names.iter().enumerate() {
            let next = names.get(i + 1).cloned();
            out.weathering.insert(name.clone(), (i as u8 + 1, next));
            let waxed = format!("minecraft:waxed_{}", name.trim_start_matches("minecraft:"));
            if blocks.contains_key(&waxed) {
                out.waxed.insert(name.clone(), waxed);
            }
        }
    }
    Ok(out)
}

/// `Block.popResourceFromFace`: точка и скорость предмета, выпавшего из
/// грани `face` (предмет 0.25×0.25). `r` — шесть случайных чисел `[0, 1)`.
pub fn pop_from_face(pos: [i32; 3], face: u8, r: [f64; 6]) -> ([f64; 3], [f64; 3]) {
    let step: [i32; 3] = match face {
        0 => [0, -1, 0],
        1 => [0, 1, 0],
        2 => [0, 0, -1],
        3 => [0, 0, 1],
        4 => [-1, 0, 0],
        _ => [1, 0, 0],
    };
    let half = 0.125;
    let lerp = |t: f64, a: f64, b: f64| a + t * (b - a);
    let coord = |i: usize, sx: i32, t: f64| {
        pos[i] as f64 + 0.5
            + if sx == 0 {
                lerp(t, -0.25, 0.25)
            } else {
                sx as f64 * (0.5 + half)
            }
    };
    let at = [
        coord(0, step[0], r[0]),
        coord(1, step[1], r[1]) - half,
        coord(2, step[2], r[2]),
    ];
    let vel = [
        if step[0] == 0 { lerp(r[3], -0.1, 0.1) } else { step[0] as f64 * 0.1 },
        if step[1] == 0 { lerp(r[4], 0.0, 0.1) } else { step[1] as f64 * 0.1 + 0.1 },
        if step[2] == 0 { lerp(r[5], -0.1, 0.1) } else { step[2] as f64 * 0.1 },
    ];
    (at, vel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copper_stage_names() {
        assert_eq!(stages("copper_block")[1], "minecraft:exposed_copper");
        assert_eq!(stages("lightning_rod")[3], "minecraft:oxidized_lightning_rod");
        assert_eq!(stages("cut_copper_slab")[2], "minecraft:weathered_cut_copper_slab");
    }

    #[test]
    fn face_drop_leaves_the_clicked_side() {
        let (at, vel) = pop_from_face([0, 64, 0], 1, [0.5; 6]);
        assert_eq!(at, [0.5, 64.0 + 0.5 + 0.625 - 0.125, 0.5]);
        assert!((vel[1] - 0.2).abs() < 1e-9);
        let (at, _) = pop_from_face([0, 64, 0], 4, [0.0; 6]);
        assert_eq!(at[0], 0.5 - 0.625);
    }

    /// Все превращения ванили разбираются; топор снимает кору с дуба и
    /// счищает окисление, мотыга не пашет снизу.
    #[test]
    fn vanilla_transformers_parse() {
        let Ok(data) = std::env::var("CHUNKWAKE_DATA") else {
            return;
        };
        let data = Path::new(&data);
        let tags = crate::play::load_registry_tags(data, "block");
        let blocks = chunkwake_data::world::Blocks::load(data).unwrap().defaults();
        let t = load(data, &tags, &blocks).unwrap();
        // Все 15 семейств по 4 стадии, у каждой вощёная пара.
        assert_eq!(t.weathering.len(), 60);
        assert_eq!(t.waxed.len(), 60);
        assert_eq!(t.copper_age("minecraft:copper_chest"), Some(1));
        assert_eq!(t.copper_age("minecraft:exposed_copper"), Some(2));
        assert_eq!(t.copper_age("minecraft:weathered_cut_copper"), Some(3));
        assert_eq!(t.copper_age("minecraft:oxidized_lightning_rod"), Some(4));
        assert_eq!(t.copper_age("minecraft:waxed_oxidized_copper_chest"), None);
        assert_eq!(t.copper_age("minecraft:copper_ore"), None);
        assert_eq!(t.copper_age("minecraft:copper_torch"), None);
        assert_eq!(t.weathering["minecraft:oxidized_copper"].1, None);
        let axe = t.for_item("minecraft:iron_axe").expect("топор");
        assert_eq!(axe.len(), 7);
        assert_eq!(axe[0].sound.as_deref(), Some("minecraft:item.axe.strip"));
        assert!(axe.iter().any(|x| x.copper_chest));
        let hoe = t.for_item("minecraft:wooden_hoe").expect("мотыга");
        assert_eq!(hoe[0].disallowed_faces, vec![0]);
        assert!(hoe[1].drop_from_face);
        assert!(t.for_item("minecraft:golden_shovel").is_some());
        assert_eq!(t.copper_age("minecraft:weathered_copper"), Some(3));
        assert_eq!(t.waxed["minecraft:exposed_copper"], "minecraft:waxed_exposed_copper");
        let name = |p: [i32; 3]| {
            if p[1] == 0 { "minecraft:oak_log".to_owned() } else { "minecraft:air".to_owned() }
        };
        assert!(axe[0].rules.iter().any(|(p, _)| p.test([0, 0, 0], &name)));
    }
}
