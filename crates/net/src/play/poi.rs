//! Точки интереса ванили: реестр 26.3 без элементов, соответствие блок → тип
//! зашито в коде, как `PoiTypes`. Теги читаются из датапака.

use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Тип точки интереса по имени блока. `None` — блок точкой не является.
/// Обе половины кровати — `home`: у состояния в мире нет свойств.
pub(super) fn of_block(name: &str) -> Option<&'static str> {
    Some(match name {
        "minecraft:blast_furnace" => "minecraft:armorer",
        "minecraft:smoker" => "minecraft:butcher",
        "minecraft:cartography_table" => "minecraft:cartographer",
        "minecraft:brewing_stand" => "minecraft:cleric",
        "minecraft:composter" => "minecraft:farmer",
        "minecraft:barrel" => "minecraft:fisherman",
        "minecraft:fletching_table" => "minecraft:fletcher",
        "minecraft:cauldron"
        | "minecraft:water_cauldron"
        | "minecraft:lava_cauldron"
        | "minecraft:powder_snow_cauldron" => "minecraft:leatherworker",
        "minecraft:lectern" => "minecraft:librarian",
        "minecraft:stonecutter" => "minecraft:mason",
        "minecraft:loom" => "minecraft:shepherd",
        "minecraft:smithing_table" => "minecraft:toolsmith",
        "minecraft:grindstone" => "minecraft:weaponsmith",
        "minecraft:bell" => "minecraft:meeting",
        "minecraft:bee_nest" => "minecraft:bee_nest",
        "minecraft:beehive" => "minecraft:beehive",
        "minecraft:nether_portal" => "minecraft:nether_portal",
        "minecraft:lodestone" => "minecraft:lodestone",
        "minecraft:lightning_rod" => "minecraft:lightning_rod",
        n if n.ends_with("_bed") => "minecraft:home",
        _ => return None,
    })
}

/// Бит группы [`chunkwake_world::poi_bits`] по имени блока.
///
/// Скан столбца спрашивает это у каждого из 98 тыс. блоков, поэтому группа
/// считается один раз на имя при сборке мира, а не на блок.
pub fn group_of(name: &str) -> u16 {
    use chunkwake_world::poi_bits as b;
    match of_block(name) {
        None => 0,
        Some(_) if name.ends_with("_bed") => b::HOME,
        Some("minecraft:bell") => b::MEETING,
        Some("minecraft:bee_nest" | "minecraft:beehive") => b::BEEHIVE,
        Some("minecraft:nether_portal" | "minecraft:lodestone" | "minecraft:lightning_rod") => {
            b::OTHER
        }
        Some(_) => b::JOB,
    }
}

/// Теги `point_of_interest_type`. Каталога нет — пустая карта, мир всё равно встаёт.
pub fn load_tags(data_dir: &Path) -> HashMap<String, HashSet<String>> {
    let dir = data_dir.join("data/minecraft/tags/point_of_interest_type");
    let mut raw: HashMap<String, Vec<String>> = HashMap::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return HashMap::new();
    };
    for ent in entries.flatten() {
        let path = ent.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let values = v["values"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|x| x.as_str().map(str::to_owned))
            .collect();
        raw.insert(format!("minecraft:{stem}"), values);
    }
    let mut out = HashMap::new();
    for name in raw.keys() {
        let mut set = HashSet::new();
        let mut seen = HashSet::new();
        expand(&raw, name, &mut set, &mut seen);
        out.insert(name.clone(), set);
    }
    out
}

fn expand(
    raw: &HashMap<String, Vec<String>>,
    name: &str,
    set: &mut HashSet<String>,
    seen: &mut HashSet<String>,
) {
    if !seen.insert(name.to_owned()) {
        return;
    }
    let Some(values) = raw.get(name) else {
        return;
    };
    for value in values {
        if let Some(tag) = value.strip_prefix('#') {
            let tag = if tag.contains(':') {
                tag.to_owned()
            } else {
                format!("minecraft:{tag}")
            };
            expand(raw, &tag, set, seen);
        } else if value.contains(':') {
            set.insert(value.clone());
        } else {
            set.insert(format!("minecraft:{value}"));
        }
    }
}

/// Имя из команды (`farmer`, `#village`, `minecraft:#village`) → типы для поиска.
pub(super) fn query(
    tags: &HashMap<String, HashSet<String>>,
    name: &str,
) -> Option<HashSet<String>> {
    let name = normalize(name);
    if let Some(tag) = name.strip_prefix('#') {
        let set = tags.get(tag)?.clone();
        return (!set.is_empty()).then_some(set);
    }
    known(&name).then(|| HashSet::from([name]))
}

fn known(name: &str) -> bool {
    matches!(
        name,
        "minecraft:armorer"
            | "minecraft:butcher"
            | "minecraft:cartographer"
            | "minecraft:cleric"
            | "minecraft:farmer"
            | "minecraft:fisherman"
            | "minecraft:fletcher"
            | "minecraft:leatherworker"
            | "minecraft:librarian"
            | "minecraft:mason"
            | "minecraft:shepherd"
            | "minecraft:toolsmith"
            | "minecraft:weaponsmith"
            | "minecraft:meeting"
            | "minecraft:home"
            | "minecraft:bee_nest"
            | "minecraft:beehive"
            | "minecraft:nether_portal"
            | "minecraft:lodestone"
            | "minecraft:lightning_rod"
    )
}

fn normalize(raw: &str) -> String {
    let raw = raw.trim();
    let raw = raw
        .strip_prefix("minecraft:#")
        .map(|rest| format!("#{rest}"))
        .unwrap_or_else(|| raw.to_owned());
    if let Some(tag) = raw.strip_prefix('#') {
        let tag = tag.strip_prefix("minecraft:").unwrap_or(tag);
        format!("#minecraft:{tag}")
    } else if raw.contains(':') {
        raw
    } else {
        format!("minecraft:{raw}")
    }
}
