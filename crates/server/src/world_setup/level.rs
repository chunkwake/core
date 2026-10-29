//! Параметры генерации мира: сид (`level-seed`), тип (`level-type`), слои плоского
//! мира (`generator-settings` или пресет) и их запись в `level.dat`, чтобы мир
//! при перезапуске оставался тем же.

use chunkwake_world::nbt::{self, Compound, Tag};
use serde_json::Value;
use std::path::Path;

/// Плоский мир: слои снизу вверх и биом (`FlatLevelGeneratorSettings`).
#[derive(Clone, Debug, PartialEq)]
pub struct Flat {
    pub layers: Vec<(String, usize)>,
    pub biome: String,
}

/// Плоские слои или шум оверворлда (`level-type=minecraft:normal`).
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Flat(Flat),
    Noise,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Level {
    pub seed: i64,
    pub kind: Kind,
}

/// `WorldOptions.parseSeed`: число как есть, иначе `String.hashCode`; пусто — случайный.
pub fn parse_seed(text: &str) -> i64 {
    let t = text.trim();
    if t.is_empty() {
        return rand::random();
    }
    t.parse::<i64>().unwrap_or_else(|_| java_hash(t) as i64)
}

/// `String.hashCode` по UTF-16.
fn java_hash(s: &str) -> i32 {
    s.encode_utf16()
        .fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(c as i32))
}

/// `BiomeManager.obfuscateSeed`: SHA-256 сида. Guava `hashLong` кладёт long
/// little-endian, `HashCode.asLong` читает первые 8 байт тоже little-endian.
pub fn hashed_seed(seed: i64) -> i64 {
    chunkwake_worldgen::biome::zoom_seed(seed)
}

/// Слои из JSON `{"layers":[{"block","height"}],"biome"}` (как `generator-settings`).
fn flat_from_json(v: &Value) -> Result<Flat, String> {
    let layers = v["layers"]
        .as_array()
        .ok_or("generator-settings: no \"layers\" array")?
        .iter()
        .map(|l| {
            let block = l["block"]
                .as_str()
                .ok_or("generator-settings: layer without \"block\"")?;
            let block = if block.contains(':') {
                block.to_owned()
            } else {
                format!("minecraft:{block}")
            };
            let height = l["height"].as_u64().unwrap_or(1) as usize;
            Ok((block, height))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(Flat {
        layers,
        biome: v["biome"].as_str().unwrap_or("minecraft:plains").to_owned(),
    })
}

/// Пресет `worldgen/flat_level_generator_preset/<имя>.json`.
fn preset(data_dir: &Path, name: &str) -> Result<Flat, String> {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    let path = data_dir.join(format!(
        "data/minecraft/worldgen/flat_level_generator_preset/{name}.json"
    ));
    let text = std::fs::read_to_string(&path).map_err(|e| format!("flat preset {name}: {e}"))?;
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("flat preset {name}: {e}"))?;
    flat_from_json(&v["settings"])
}

/// Слои по `level-type` и `generator-settings`: JSON, имя пресета или `classic_flat`.
pub fn flat_settings(data_dir: &Path, level_type: &str, settings: &str) -> Result<Flat, String> {
    let kind = level_type
        .trim()
        .strip_prefix("minecraft:")
        .unwrap_or(level_type.trim());
    match kind {
        "" | "flat" => {}
        "large_biomes" | "amplified" | "single_biome_surface" => {
            tracing::warn!(
                "level-type={level_type}: terrain generation is not implemented yet, using a flat world"
            );
        }
        _ => {
            return Err(format!(
                "level-type={level_type}: use minecraft:flat or minecraft:normal"
            ));
        }
    }
    let s = settings.trim();
    if s.starts_with('{') {
        let v: Value = serde_json::from_str(s).map_err(|e| format!("generator-settings: {e}"))?;
        flat_from_json(&v)
    } else if s.is_empty() {
        preset(data_dir, "classic_flat")
    } else {
        preset(data_dir, s)
    }
}

/// Тип из настроек: `normal` — шум, остальное поддерживаемое — плоские слои.
fn resolve_kind(data_dir: &Path, level_type: &str, settings: &str) -> Result<Kind, String> {
    let kind = level_type
        .trim()
        .strip_prefix("minecraft:")
        .unwrap_or(level_type.trim());
    if kind == "normal" {
        return Ok(Kind::Noise);
    }
    Ok(Kind::Flat(flat_settings(data_dir, level_type, settings)?))
}

/// `level.dat` → сид и тип (`Data.WorldGenSettings`); чужой генератор — `None`.
fn read_level(root: &Compound) -> Option<(i64, Option<Kind>)> {
    let Some(Tag::Compound(data)) = root.get("Data") else {
        return None;
    };
    let Some(Tag::Compound(gen_settings)) = data.get("WorldGenSettings") else {
        return None;
    };
    let Some(Tag::Long(seed)) = gen_settings.get("seed") else {
        return None;
    };
    let generator = (|| {
        let Some(Tag::Compound(dims)) = gen_settings.get("dimensions") else {
            return None;
        };
        let Some(Tag::Compound(ow)) = dims.get("minecraft:overworld") else {
            return None;
        };
        let Some(Tag::Compound(g)) = ow.get("generator") else {
            return None;
        };
        match g.get("type") {
            Some(Tag::String(t)) if t == "minecraft:noise" => return Some(Kind::Noise),
            Some(Tag::String(t)) if t == "minecraft:flat" => {}
            _ => return None,
        }
        let Some(Tag::Compound(st)) = g.get("settings") else {
            return None;
        };
        let Some(Tag::List(list)) = st.get("layers") else {
            return None;
        };
        let layers = list
            .iter()
            .filter_map(|l| {
                let Tag::Compound(l) = l else { return None };
                let Some(Tag::String(b)) = l.get("block") else {
                    return None;
                };
                let h = match l.get("height") {
                    Some(Tag::Int(h)) => *h as usize,
                    _ => 1,
                };
                Some((b.clone(), h))
            })
            .collect();
        let biome = match st.get("biome") {
            Some(Tag::String(b)) => b.clone(),
            _ => "minecraft:plains".into(),
        };
        Some(Kind::Flat(Flat { layers, biome }))
    })();
    Some((*seed, generator))
}

fn level_nbt(level: &Level, data_version: i32, name: &str) -> Compound {
    let s = |v: &str| Tag::String(v.to_owned());
    let generator = match &level.kind {
        Kind::Noise => Compound::from([
            ("type".into(), s("minecraft:noise")),
            ("settings".into(), s("minecraft:overworld")),
        ]),
        Kind::Flat(flat) => {
            let layers = flat
                .layers
                .iter()
                .map(|(b, h)| {
                    Tag::Compound(Compound::from([
                        ("block".into(), s(b)),
                        ("height".into(), Tag::Int(*h as i32)),
                    ]))
                })
                .collect();
            Compound::from([
                ("type".into(), s("minecraft:flat")),
                (
                    "settings".into(),
                    Tag::Compound(Compound::from([
                        ("layers".into(), Tag::List(layers)),
                        ("biome".into(), s(&flat.biome)),
                        ("features".into(), Tag::Byte(0)),
                        ("lakes".into(), Tag::Byte(0)),
                    ])),
                ),
            ])
        }
    };
    let overworld = Compound::from([
        ("type".into(), s("minecraft:overworld")),
        ("generator".into(), Tag::Compound(generator)),
    ]);
    let gen_settings = Compound::from([
        ("seed".into(), Tag::Long(level.seed)),
        ("generate_features".into(), Tag::Byte(1)),
        ("bonus_chest".into(), Tag::Byte(0)),
        (
            "dimensions".into(),
            Tag::Compound(Compound::from([(
                "minecraft:overworld".into(),
                Tag::Compound(overworld),
            )])),
        ),
    ]);
    let data = Compound::from([
        ("DataVersion".into(), Tag::Int(data_version)),
        ("LevelName".into(), s(name)),
        ("version".into(), Tag::Int(19133)),
        ("WorldGenSettings".into(), Tag::Compound(gen_settings)),
    ]);
    Compound::from([("Data".into(), Tag::Compound(data))])
}

/// Сид и слои: из `level.dat`, если мир уже есть; иначе из настроек — и записать.
pub fn load_or_create(
    world_dir: &Path,
    data_dir: &Path,
    seed: &str,
    level_type: &str,
    settings: &str,
    data_version: i32,
) -> Result<Level, String> {
    let path = world_dir.join("level.dat");
    let fresh = || resolve_kind(data_dir, level_type, settings);
    if let Some(root) = nbt::read_gzip_file(&path)? {
        if let Some((seed, kind)) = read_level(&root) {
            let kind = match kind {
                Some(k) => k,
                None => fresh()?,
            };
            return Ok(Level { seed, kind });
        }
        tracing::warn!(
            "{}: no WorldGenSettings, keeping the file and using server.properties",
            path.display()
        );
        return Ok(Level {
            seed: parse_seed(seed),
            kind: fresh()?,
        });
    }
    let level = Level {
        seed: parse_seed(seed),
        kind: fresh()?,
    };
    std::fs::create_dir_all(world_dir)
        .map_err(|e| format!("create {}: {e}", world_dir.display()))?;
    let name = world_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("world");
    nbt::write_gzip_file(&path, &level_nbt(&level, data_version, name))?;
    Ok(level)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_match_java() {
        assert_eq!(parse_seed("12345"), 12345);
        assert_eq!(parse_seed("-7"), -7);
        // "hello".hashCode() == 99162322
        assert_eq!(parse_seed("hello"), 99162322);
        assert_eq!(
            java_hash("Chunkwake"),
            "Chunkwake"
                .chars()
                .fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(c as i32))
        );
    }

    #[test]
    fn json_layers() {
        let v: Value = serde_json::from_str(r#"{"layers":[{"block":"bedrock","height":1},{"block":"minecraft:stone","height":3}],"biome":"minecraft:desert"}"#).unwrap();
        let f = flat_from_json(&v).unwrap();
        assert_eq!(
            f.layers,
            vec![
                ("minecraft:bedrock".into(), 1),
                ("minecraft:stone".into(), 3)
            ]
        );
        assert_eq!(f.biome, "minecraft:desert");
    }

    #[test]
    fn level_dat_roundtrip() {
        let level = Level {
            seed: -42,
            kind: Kind::Flat(Flat {
                layers: vec![
                    ("minecraft:bedrock".into(), 1),
                    ("minecraft:sand".into(), 5),
                ],
                biome: "minecraft:desert".into(),
            }),
        };
        let (seed, kind) = read_level(&level_nbt(&level, 4000, "w")).unwrap();
        assert_eq!(seed, -42);
        assert_eq!(kind, Some(level.kind));
    }

    #[test]
    fn noise_level_dat_roundtrip() {
        let level = Level {
            seed: 7,
            kind: Kind::Noise,
        };
        let (seed, kind) = read_level(&level_nbt(&level, 4000, "w")).unwrap();
        assert_eq!(seed, 7);
        assert_eq!(kind, Some(Kind::Noise));
    }
}
