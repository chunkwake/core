//! Реестры и теги для фазы configuration.
//!
//! Записи синхронизируемых (data-driven) реестров берутся из `data/minecraft/<реестр>/`,
//! числовые ID встроенных реестров — из `reports/registries.json`, теги — из
//! `data/minecraft/tags/<реестр>/`. Всё это создаёт ванильный генератор (`--server --reports`).

use crate::DataError;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Синхронизируемые реестры в порядке отправки
/// (`RegistryDataLoader.SYNCHRONIZED_REGISTRIES` из 26.3).
pub const SYNCHRONIZED: &[&str] = &[
    "worldgen/biome",
    "chat_type",
    "trim_pattern",
    "trim_material",
    "wolf_variant",
    "wolf_sound_variant",
    "pig_variant",
    "pig_sound_variant",
    "frog_variant",
    "cat_variant",
    "cat_sound_variant",
    "cow_sound_variant",
    "cow_variant",
    "chicken_sound_variant",
    "chicken_variant",
    "zombie_nautilus_variant",
    "painting_variant",
    "sulfur_cube_archetype",
    "dimension_type",
    "damage_type",
    "banner_pattern",
    "enchantment",
    "jukebox_song",
    "instrument",
    "test_environment",
    "test_instance",
    "dialog",
    "world_clock",
    "timeline",
    "decorated_pot_pattern",
    "block_transformer",
    "worldgen/block_state_provider",
];

/// Реестр с записями в порядке их сетевых ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registry {
    /// Путь без namespace, например `worldgen/biome`.
    pub name: String,
    /// Полные ID записей (`minecraft:plains`); индекс = сетевой ID.
    pub entries: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryTags {
    pub registry: String,
    /// Имя тега (`minecraft:mineable/axe`) → сетевые ID записей.
    pub tags: Vec<(String, Vec<i32>)>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct RegistryData {
    pub synchronized: Vec<Registry>,
    pub tags: Vec<RegistryTags>,
}

/// `reports/registries.json`: реестр → запись → числовой ID.
pub type Builtin = HashMap<String, HashMap<String, i32>>;

impl RegistryData {
    /// Сетевой ID записи синхронизируемого реестра (`worldgen/biome`, `minecraft:plains`).
    pub fn network_id(&self, registry: &str, entry: &str) -> Result<i32, DataError> {
        self.synchronized
            .iter()
            .find(|r| r.name == registry)
            .and_then(|r| r.entries.iter().position(|e| e == entry))
            .map(|i| i as i32)
            .ok_or_else(|| DataError::Io(format!("unknown {registry} entry {entry}")))
    }

    /// `root` — каталог вывода генератора (содержит `data/` и `reports/`).
    pub fn load(root: &Path) -> Result<Self, DataError> {
        Self::load_with(root, &load_builtin(root)?)
    }

    /// То же, но `registries.json` уже прочитан вызывающим: в образе запуска
    /// он разбирается один раз на весь сервер.
    pub fn load_with(root: &Path, builtin: &Builtin) -> Result<Self, DataError> {
        let data = root.join("data/minecraft");
        let mut ids = builtin.clone();

        let mut synchronized = Vec::new();
        for name in SYNCHRONIZED {
            let dir = data.join(name);
            let mut entries: Vec<String> = json_files(&dir)?
                .into_iter()
                .map(|rel| format!("minecraft:{rel}"))
                .collect();
            entries.sort();
            ids.insert(
                name.to_string(),
                entries
                    .iter()
                    .enumerate()
                    .map(|(i, e)| (e.clone(), i as i32))
                    .collect(),
            );
            synchronized.push(Registry {
                name: name.to_string(),
                entries,
            });
        }

        // Отправляем теги только сетевых реестров: встроенных и синхронизируемых.
        let mut tags = Vec::new();
        let mut names: Vec<&String> = ids.keys().collect();
        names.sort();
        for name in names {
            let dir = data.join("tags").join(name);
            if !dir.is_dir() {
                continue;
            }
            let resolved = resolve_tags(&dir, &ids[name])?;
            if !resolved.is_empty() {
                tags.push(RegistryTags {
                    registry: name.clone(),
                    tags: resolved,
                });
            }
        }
        Ok(Self { synchronized, tags })
    }
}

fn format_err(path: &Path, msg: impl ToString) -> DataError {
    DataError::Format {
        file: path.display().to_string(),
        msg: msg.to_string(),
    }
}

fn read_json(path: &Path) -> Result<serde_json::Value, DataError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| DataError::Io(format!("read {}: {e}", path.display())))?;
    serde_json::from_str(&text).map_err(|e| format_err(path, e))
}

/// Все записи встроенного реестра (`item`) с их ID.
pub fn builtin_entries(root: &Path, registry: &str) -> Result<HashMap<String, i32>, DataError> {
    builtin_entries_of(&load_builtin(root)?, registry)
}

/// То же для уже прочитанного `registries.json`.
pub fn builtin_entries_of(
    builtin: &Builtin,
    registry: &str,
) -> Result<HashMap<String, i32>, DataError> {
    builtin
        .get(registry)
        .cloned()
        .ok_or_else(|| DataError::Io(format!("unknown registry {registry}")))
}

/// ID записи встроенного реестра (`entity_type`, `minecraft:player`) из `registries.json`.
pub fn builtin_id(root: &Path, registry: &str, entry: &str) -> Result<i32, DataError> {
    builtin_id_of(&load_builtin(root)?, registry, entry)
}

/// То же для уже прочитанного `registries.json`.
pub fn builtin_id_of(builtin: &Builtin, registry: &str, entry: &str) -> Result<i32, DataError> {
    builtin
        .get(registry)
        .and_then(|e| e.get(entry))
        .copied()
        .ok_or_else(|| DataError::Io(format!("unknown {registry} entry {entry}")))
}

/// Все записи встроенного реестра (`block_entity_type`) с их номерами.
pub fn builtin_ids_of(
    builtin: &Builtin,
    registry: &str,
) -> Result<HashMap<String, i32>, DataError> {
    builtin_entries_of(builtin, registry)
}

/// Ключи всех реестров для плагинов (`Registry` Bukkit): встроенные — по
/// числовому ID, синхронизируемые — по сетевому порядку, `worldgen/structure`
/// — из файлов данных (он не синхронизируется, но плагинам нужен).
pub fn all_keys(
    root: &Path,
    builtin: &Builtin,
    data: &RegistryData,
) -> Result<HashMap<String, Vec<String>>, DataError> {
    let mut out = HashMap::new();
    for (name, entries) in builtin {
        let mut v: Vec<(&i32, &String)> = entries.iter().map(|(k, id)| (id, k)).collect();
        v.sort();
        out.insert(name.clone(), v.into_iter().map(|(_, k)| k.clone()).collect());
    }
    for r in &data.synchronized {
        out.insert(r.name.clone(), r.entries.clone());
    }
    let dir = root.join("data/minecraft/worldgen/structure");
    let structures = json_files(&dir)?;
    // Служебные «реестры» со свойствами записей: `ключ значение…` по строке.
    // `Structure.getStructureType` — поле `type` файла структуры.
    out.insert(
        "#structure_type".into(),
        structures
            .iter()
            .map(|s| {
                let json = read_json(&dir.join(format!("{s}.json")))?;
                let kind = json["type"].as_str().unwrap_or("minecraft:jigsaw");
                Ok(format!("minecraft:{s} {kind}"))
            })
            .collect::<Result<_, DataError>>()?,
    );
    out.insert(
        "worldgen/structure".into(),
        structures.into_iter().map(|s| format!("minecraft:{s}")).collect(),
    );
    // `MusicInstrument`: `ключ звук длительность дальность`.
    let dir = root.join("data/minecraft/instrument");
    out.insert(
        "#instrument".into(),
        json_files(&dir)?
            .iter()
            .map(|s| {
                let json = read_json(&dir.join(format!("{s}.json")))?;
                let sound = match &json["sound_event"] {
                    serde_json::Value::String(id) => id.clone(),
                    other => other["sound_id"].as_str().unwrap_or("").to_owned(),
                };
                Ok(format!(
                    "minecraft:{s} {sound} {} {}",
                    json["use_duration"].as_f64().unwrap_or(0.0),
                    json["range"].as_f64().unwrap_or(0.0)
                ))
            })
            .collect::<Result<_, DataError>>()?,
    );
    Ok(out)
}

/// `registries.json`: `{"minecraft:block": {"entries": {"minecraft:stone": {"protocol_id": 1}}}}`.
pub fn load_builtin(root: &Path) -> Result<Builtin, DataError> {
    let path = root.join("reports/registries.json");
    let json = read_json(&path)?;
    let obj = json
        .as_object()
        .ok_or_else(|| format_err(&path, "expected object"))?;
    let mut out = HashMap::new();
    for (registry, body) in obj {
        let name = registry.strip_prefix("minecraft:").unwrap_or(registry);
        let mut entries = HashMap::new();
        for (entry, v) in body["entries"].as_object().into_iter().flatten() {
            let id = v["protocol_id"]
                .as_i64()
                .ok_or_else(|| format_err(&path, format!("{entry}: no protocol_id")))?;
            entries.insert(entry.clone(), id as i32);
        }
        out.insert(name.to_owned(), entries);
    }
    Ok(out)
}

/// Относительные пути `*.json` без расширения, через `/`. Нет каталога — пусто.
fn json_files(dir: &Path) -> Result<Vec<String>, DataError> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), DataError> {
        let rd = match std::fs::read_dir(dir) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(DataError::Io(format!("read {}: {e}", dir.display()))),
        };
        for entry in rd {
            let path: PathBuf = entry
                .map_err(|e| DataError::Io(format!("read {}: {e}", dir.display())))?
                .path();
            if path.is_dir() {
                walk(base, &path, out)?;
            } else if path.extension().is_some_and(|e| e == "json") {
                let rel = path.strip_prefix(base).unwrap().with_extension("");
                let rel: Vec<_> = rel.iter().map(|c| c.to_string_lossy()).collect();
                out.push(rel.join("/"));
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out)?;
    out.sort();
    Ok(out)
}

/// Разворачивает теги: ссылки `#minecraft:x`, записи `{"id", "required"}`.
/// Неизвестные записи пропускаются (как необязательные).
fn resolve_tags(
    dir: &Path,
    ids: &HashMap<String, i32>,
) -> Result<Vec<(String, Vec<i32>)>, DataError> {
    let mut raw: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for rel in json_files(dir)? {
        let path = dir.join(format!("{rel}.json"));
        let json = read_json(&path)?;
        let values = json["values"]
            .as_array()
            .ok_or_else(|| format_err(&path, "no values"))?
            .iter()
            .filter_map(|v| match v {
                serde_json::Value::String(s) => Some(s.clone()),
                serde_json::Value::Object(o) => o.get("id")?.as_str().map(str::to_owned),
                _ => None,
            })
            .collect();
        raw.insert(format!("minecraft:{rel}"), values);
    }

    fn expand(
        tag: &str,
        raw: &BTreeMap<String, Vec<String>>,
        ids: &HashMap<String, i32>,
        done: &mut HashMap<String, Vec<i32>>,
        stack: &mut HashSet<String>,
    ) -> Vec<i32> {
        if let Some(v) = done.get(tag) {
            return v.clone();
        }
        if !stack.insert(tag.to_owned()) {
            return Vec::new(); // цикл ссылок
        }
        let mut out = Vec::new();
        for value in raw.get(tag).into_iter().flatten() {
            if let Some(child) = value.strip_prefix('#') {
                out.extend(expand(child, raw, ids, done, stack));
            } else if let Some(&id) = ids.get(value) {
                out.push(id);
            }
        }
        let mut seen = HashSet::new();
        out.retain(|id| seen.insert(*id));
        stack.remove(tag);
        done.insert(tag.to_owned(), out.clone());
        out
    }

    let mut done = HashMap::new();
    let mut stack = HashSet::new();
    Ok(raw
        .keys()
        .map(|tag| (tag.clone(), expand(tag, &raw, ids, &mut done, &mut stack)))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_references_and_dedup() {
        let dir = std::env::temp_dir().join(format!("chunkwake-tags-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("mineable")).unwrap();
        std::fs::write(
            dir.join("logs.json"),
            r##"{"values":["minecraft:oak_log","#minecraft:mineable/axe"]}"##,
        )
        .unwrap();
        std::fs::write(
            dir.join("mineable/axe.json"),
            r#"{"values":["minecraft:oak_log",{"id":"minecraft:barrel","required":false},"minecraft:unknown"]}"#,
        )
        .unwrap();
        let ids = HashMap::from([
            ("minecraft:oak_log".to_owned(), 5),
            ("minecraft:barrel".to_owned(), 9),
        ]);
        let tags = resolve_tags(&dir, &ids).unwrap();
        assert_eq!(
            tags,
            vec![
                ("minecraft:logs".to_owned(), vec![5, 9]),
                ("minecraft:mineable/axe".to_owned(), vec![5, 9]),
            ]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
