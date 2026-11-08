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
    /// `features` пресета: у `the_void` от него зависит стартовая платформа.
    pub features: bool,
}

impl Flat {
    /// Пресет `the_void` с включёнными фичами: платформа 33×33 на `minY + 3`.
    pub fn void_platform(&self) -> bool {
        self.features && is_void_biome(&self.biome)
    }
}

/// Шум оверворлда: имя `noise_settings` и, для `single_biome_surface`, один биом.
#[derive(Clone, Debug, PartialEq)]
pub struct Noise {
    /// `minecraft:overworld`, `minecraft:large_biomes` или `minecraft:amplified`.
    pub settings: String,
    /// `Some` — `minecraft:fixed`. `None` — `multi_noise` оверворлда.
    pub biome: Option<String>,
}

impl Noise {
    pub fn overworld() -> Self {
        Self {
            settings: "minecraft:overworld".into(),
            biome: None,
        }
    }
}

/// Плоские слои или шум (`level-type`).
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Flat(Flat),
    Noise(Noise),
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

fn namespaced(name: &str) -> String {
    let name = name.trim();
    if name.contains(':') {
        name.to_owned()
    } else {
        format!("minecraft:{name}")
    }
}

fn preset_key(level_type: &str) -> &str {
    level_type
        .trim()
        .strip_prefix("minecraft:")
        .unwrap_or(level_type.trim())
}

fn is_void_biome(name: &str) -> bool {
    name == "minecraft:the_void" || name == "the_void"
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
        features: v["features"].as_bool().unwrap_or(false),
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

/// Слои по `generator-settings`: JSON, имя пресета или `classic_flat`.
pub fn flat_settings(data_dir: &Path, settings: &str) -> Result<Flat, String> {
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

/// `{"biome":"minecraft:desert"}` в `generator-settings` одиночного биома.
fn biome_override(settings: &str) -> Result<Option<String>, String> {
    let s = settings.trim();
    if !s.starts_with('{') {
        return Ok(None);
    }
    let v: Value = serde_json::from_str(s).map_err(|e| format!("generator-settings: {e}"))?;
    Ok(v["biome"].as_str().map(namespaced))
}

/// Оверворлд пресета `worldgen/world_preset/<имя>.json`.
fn noise_preset(data_dir: &Path, name: &str, generator_settings: &str) -> Result<Noise, String> {
    let path = data_dir.join(format!("data/minecraft/worldgen/world_preset/{name}.json"));
    let text = std::fs::read_to_string(&path).map_err(|e| format!("world preset {name}: {e}"))?;
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("world preset {name}: {e}"))?;
    let generator = &v["dimensions"]["minecraft:overworld"]["generator"];
    let typ = generator["type"].as_str().unwrap_or("");
    if typ != "minecraft:noise" {
        return Err(format!(
            "world preset {name}: overworld generator is {typ}, not minecraft:noise"
        ));
    }
    let settings = match &generator["settings"] {
        Value::String(s) => namespaced(s),
        _ => {
            return Err(format!(
                "world preset {name}: inline noise settings are not supported"
            ));
        }
    };
    let source = &generator["biome_source"];
    let mut biome = match source["type"].as_str().unwrap_or("") {
        "minecraft:multi_noise" => None,
        "minecraft:fixed" => Some(namespaced(
            source["biome"].as_str().unwrap_or("minecraft:plains"),
        )),
        other => {
            return Err(format!(
                "world preset {name}: biome source {other} is not supported"
            ));
        }
    };
    // Ванильный выделенный сервер биом не выбирает и оставляет plains из пресета.
    // JSON с полем `biome` — то, что клиент пишет в `level.dat` при настройке.
    if biome.is_some()
        && let Some(over) = biome_override(generator_settings)?
    {
        biome = Some(over);
    }
    Ok(Noise { settings, biome })
}

/// Тип из настроек: плоские слои или шум пресета.
fn resolve_kind(data_dir: &Path, level_type: &str, settings: &str) -> Result<Kind, String> {
    let kind = preset_key(level_type);
    match kind {
        "" | "flat" => Ok(Kind::Flat(flat_settings(data_dir, settings)?)),
        "normal" | "large_biomes" | "amplified" | "single_biome_surface" => {
            Ok(Kind::Noise(noise_preset(data_dir, kind, settings)?))
        }
        _ => Err(format!(
            "level-type={level_type}: use minecraft:flat, minecraft:normal, minecraft:large_biomes, minecraft:amplified or minecraft:single_biome_surface"
        )),
    }
}

fn flag(tag: Option<&Tag>) -> bool {
    match tag {
        Some(Tag::Byte(b)) => *b != 0,
        Some(Tag::Int(b)) => *b != 0,
        _ => false,
    }
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
            Some(Tag::String(t)) if t == "minecraft:noise" => {
                // Старый файл без `biome_source` — мультишум оверворлда.
                let mut noise = Noise::overworld();
                if let Some(Tag::String(s)) = g.get("settings") {
                    noise.settings = s.clone();
                }
                noise.biome = match g.get("biome_source") {
                    Some(Tag::Compound(src)) => match src.get("type") {
                        Some(Tag::String(t)) if t == "minecraft:fixed" => match src.get("biome") {
                            Some(Tag::String(b)) => Some(b.clone()),
                            _ => Some("minecraft:plains".into()),
                        },
                        _ => None,
                    },
                    _ => None,
                };
                return Some(Kind::Noise(noise));
            }
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
        Some(Kind::Flat(Flat {
            layers,
            biome,
            features: flag(st.get("features")),
        }))
    })();
    Some((*seed, generator))
}

fn level_nbt(level: &Level, data_version: i32, name: &str) -> Compound {
    let s = |v: &str| Tag::String(v.to_owned());
    let generator = match &level.kind {
        Kind::Noise(noise) => {
            let biome_source = match &noise.biome {
                None => Compound::from([
                    ("type".into(), s("minecraft:multi_noise")),
                    ("preset".into(), s("minecraft:overworld")),
                ]),
                Some(biome) => Compound::from([
                    ("type".into(), s("minecraft:fixed")),
                    ("biome".into(), s(biome)),
                ]),
            };
            Compound::from([
                ("type".into(), s("minecraft:noise")),
                ("settings".into(), s(&noise.settings)),
                ("biome_source".into(), Tag::Compound(biome_source)),
            ])
        }
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
                        ("features".into(), Tag::Byte(i8::from(flat.features))),
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
    // Ад и Край: в каждом `world_preset` ванили, плоский мир тоже.
    let mut dimensions = Compound::from([("minecraft:overworld".into(), Tag::Compound(overworld))]);
    dimensions.insert(
        "minecraft:the_nether".into(),
        Tag::Compound(nether_dimension()),
    );
    dimensions.insert("minecraft:the_end".into(), Tag::Compound(end_dimension()));
    let gen_settings = Compound::from([
        ("seed".into(), Tag::Long(level.seed)),
        ("generate_features".into(), Tag::Byte(1)),
        ("bonus_chest".into(), Tag::Byte(0)),
        ("dimensions".into(), Tag::Compound(dimensions)),
    ]);
    let data = Compound::from([
        ("DataVersion".into(), Tag::Int(data_version)),
        ("LevelName".into(), s(name)),
        ("version".into(), Tag::Int(19133)),
        ("WorldGenSettings".into(), Tag::Compound(gen_settings)),
    ]);
    Compound::from([("Data".into(), Tag::Compound(data))])
}

/// Дописывает в `level.dat` измерения, которых там ещё нет: Ад и Край есть
/// в каждом пресете ванили, плоский мир не исключение.
///
/// Остальные поля файла не меняются: мир мог быть создан другой сборкой,
/// и перезапись `WorldGenSettings` затёрла бы его настройки.
fn add_missing_dimensions(
    path: &Path,
    root: &Compound,
    _kind: &Kind,
    data_version: i32,
) -> Result<(), String> {
    let Some(Tag::Compound(data)) = root.get("Data").cloned() else {
        return Ok(());
    };
    let Some(Tag::Compound(mut world_gen)) = data.get("WorldGenSettings").cloned() else {
        return Ok(());
    };
    let Some(Tag::Compound(mut dimensions)) = world_gen.get("dimensions").cloned() else {
        return Ok(());
    };
    let mut added = Vec::new();
    for (key, dim) in [
        ("minecraft:the_nether", nether_dimension as fn() -> Compound),
        ("minecraft:the_end", end_dimension),
    ] {
        if !dimensions.contains_key(key) {
            dimensions.insert(key.into(), Tag::Compound(dim()));
            added.push(key);
        }
    }
    if added.is_empty() {
        return Ok(());
    }
    world_gen.insert("dimensions".into(), Tag::Compound(dimensions));
    let mut data = data;
    data.insert("WorldGenSettings".into(), Tag::Compound(world_gen));
    let mut root = root.clone();
    root.insert("Data".into(), Tag::Compound(data));
    // `DataVersion` оставляем как есть: файл принадлежит другой версии.
    let _ = data_version;
    nbt::write_gzip_file(path, &root)?;
    tracing::info!("level.dat: дописаны измерения {}", added.join(", "));
    Ok(())
}

/// Генератор Края для `level.dat`: `biome_source` `minecraft:the_end`.
fn end_dimension() -> Compound {
    let s = |v: &str| Tag::String(v.to_owned());
    Compound::from([
        ("type".into(), s("minecraft:the_end")),
        (
            "generator".into(),
            Tag::Compound(Compound::from([
                ("type".into(), s("minecraft:noise")),
                ("settings".into(), s("minecraft:end")),
                (
                    "biome_source".into(),
                    Tag::Compound(Compound::from([("type".into(), s("minecraft:the_end"))])),
                ),
            ])),
        ),
    ])
}

/// Генератор Нижнего мира для `level.dat`.
fn nether_dimension() -> Compound {
    let s = |v: &str| Tag::String(v.to_owned());
    Compound::from([
        ("type".into(), s("minecraft:the_nether")),
        (
            "generator".into(),
            Tag::Compound(Compound::from([
                ("type".into(), s("minecraft:noise")),
                ("settings".into(), s("minecraft:nether")),
                (
                    "biome_source".into(),
                    Tag::Compound(Compound::from([
                        ("type".into(), s("minecraft:multi_noise")),
                        ("preset".into(), s("minecraft:nether")),
                    ])),
                ),
            ])),
        ),
    ])
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
            // Мир, созданный до появления Нижнего мира, своего измерения не
            // имеет: дописываем его, не трогая остальной `level.dat`.
            let _ = add_missing_dimensions(&path, &root, &kind, data_version);
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
        let v: Value = serde_json::from_str(r#"{"layers":[{"block":"bedrock","height":1},{"block":"minecraft:stone","height":3}],"biome":"minecraft:desert","features":true}"#).unwrap();
        let f = flat_from_json(&v).unwrap();
        assert_eq!(
            f.layers,
            vec![
                ("minecraft:bedrock".into(), 1),
                ("minecraft:stone".into(), 3)
            ]
        );
        assert_eq!(f.biome, "minecraft:desert");
        assert!(f.features);
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
                features: false,
            }),
        };
        let (seed, kind) = read_level(&level_nbt(&level, 4000, "w")).unwrap();
        assert_eq!(seed, -42);
        assert_eq!(kind, Some(level.kind));
    }

    #[test]
    fn void_flat_keeps_features() {
        let level = Level {
            seed: 1,
            kind: Kind::Flat(Flat {
                layers: vec![("minecraft:air".into(), 1)],
                biome: "minecraft:the_void".into(),
                features: true,
            }),
        };
        let (_, kind) = read_level(&level_nbt(&level, 4000, "w")).unwrap();
        let Some(Kind::Flat(flat)) = kind else {
            panic!("expected flat");
        };
        assert!(flat.void_platform());
        assert!(flat.features);
    }

    #[test]
    fn noise_level_dat_roundtrip() {
        let level = Level {
            seed: 7,
            kind: Kind::Noise(Noise::overworld()),
        };
        let (seed, kind) = read_level(&level_nbt(&level, 4000, "w")).unwrap();
        assert_eq!(seed, 7);
        assert_eq!(kind, Some(level.kind));
    }

    #[test]
    fn noise_presets_roundtrip() {
        for (settings, biome) in [
            ("minecraft:large_biomes", None),
            ("minecraft:amplified", None),
            ("minecraft:overworld", Some("minecraft:desert".into())),
        ] {
            let level = Level {
                seed: 9,
                kind: Kind::Noise(Noise {
                    settings: settings.into(),
                    biome: biome.clone(),
                }),
            };
            let (_, kind) = read_level(&level_nbt(&level, 4000, "w")).unwrap();
            assert_eq!(kind, Some(level.kind), "{settings}");
        }
    }

    #[test]
    fn old_noise_without_biome_source_is_overworld() {
        let s = |v: &str| Tag::String(v.to_owned());
        let generator = Compound::from([
            ("type".into(), s("minecraft:noise")),
            ("settings".into(), s("minecraft:overworld")),
        ]);
        let overworld = Compound::from([
            ("type".into(), s("minecraft:overworld")),
            ("generator".into(), Tag::Compound(generator)),
        ]);
        let root = Compound::from([(
            "Data".into(),
            Tag::Compound(Compound::from([(
                "WorldGenSettings".into(),
                Tag::Compound(Compound::from([
                    ("seed".into(), Tag::Long(3)),
                    (
                        "dimensions".into(),
                        Tag::Compound(Compound::from([(
                            "minecraft:overworld".into(),
                            Tag::Compound(overworld),
                        )])),
                    ),
                ])),
            )])),
        )]);
        let (seed, kind) = read_level(&root).unwrap();
        assert_eq!(seed, 3);
        assert_eq!(kind, Some(Kind::Noise(Noise::overworld())));
    }

    /// Пресеты из выгрузки 26.3. Без `CHUNKWAKE_DATA` тест пропускается.
    #[test]
    fn presets_match_vanilla_files() {
        let Some(data) = std::env::var_os("CHUNKWAKE_DATA") else {
            return;
        };
        let data = Path::new(&data);
        let large = resolve_kind(data, "minecraft:large_biomes", "").unwrap();
        assert_eq!(
            large,
            Kind::Noise(Noise {
                settings: "minecraft:large_biomes".into(),
                biome: None,
            })
        );
        let amplified = resolve_kind(data, "amplified", "").unwrap();
        assert_eq!(
            amplified,
            Kind::Noise(Noise {
                settings: "minecraft:amplified".into(),
                biome: None,
            })
        );
        let single = resolve_kind(data, "minecraft:single_biome_surface", "").unwrap();
        assert_eq!(
            single,
            Kind::Noise(Noise {
                settings: "minecraft:overworld".into(),
                biome: Some("minecraft:plains".into()),
            })
        );
        let desert = resolve_kind(
            data,
            "single_biome_surface",
            r#"{"biome":"minecraft:desert"}"#,
        )
        .unwrap();
        assert_eq!(
            desert,
            Kind::Noise(Noise {
                settings: "minecraft:overworld".into(),
                biome: Some("minecraft:desert".into()),
            })
        );
        let normal = resolve_kind(data, "minecraft:normal", "").unwrap();
        assert_eq!(normal, Kind::Noise(Noise::overworld()));
        let void = resolve_kind(data, "minecraft:flat", "the_void").unwrap();
        let Kind::Flat(flat) = void else {
            panic!("the_void is flat");
        };
        assert_eq!(flat.biome, "minecraft:the_void");
        assert!(flat.void_platform());
    }
}
