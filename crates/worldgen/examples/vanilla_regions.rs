//! Стенд сверки с ванилью: генерация настоящим `server.jar` и побайтовое
//! сравнение блоков с нашим генератором.
//!
//! `vanilla_regions <server.jar> <java> <данные> <каталог> <seed> <радиус> [level-type] [generator-settings]`
//!
//! `level-type` по умолчанию `minecraft:normal`. Для плоского мира вторым
//! аргументом идёт пресет (`minecraft:the_void`) или JSON слоёв.
//! Каталог должен быть пустым: там появятся `world/` и `server.properties`.
//! Печатает число расхождений и первое из них, а при нуле — «совпали».
//!
//! `CHUNKWAKE_ORIGIN=cx,cz` сдвигает квадрат шума: чанки
//! `(ox-radius)..(ox+radius)` по обеим осям. Без переменной центр — `(0, 0)`,
//! диапазон тот же `-radius..radius`. Плоский мир не сдвигается.
//! `CHUNKWAKE_PROPS=1` сравнивает свойства блока, не только имя.

use chunkwake_world::anvil::{self, Format, Names, RegionStore};
use chunkwake_world::nbt::Compound;
use chunkwake_world::{ChunkColumn, bits_for};
use chunkwake_worldgen::Generator;
use std::collections::{HashMap, HashSet};

/// Разобранная блок-сущность: id, таблица лута и семя.
type EntityMap<'a> = HashMap<[i32; 3], (&'a str, Option<&'a str>, Option<i64>)>;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 7 {
        eprintln!(
            "usage: vanilla_regions <jar> <java> <data> <work> <seed> <radius> [level-type] [generator-settings]"
        );
        std::process::exit(2);
    }
    let level_type = a.get(7).map(String::as_str).unwrap_or("minecraft:normal");
    let settings = a.get(8).map(String::as_str).unwrap_or("");
    let result = run(
        Path::new(&a[1]),
        Path::new(&a[2]),
        Path::new(&a[3]),
        Path::new(&a[4]),
        a[5].parse().unwrap(),
        a[6].parse().unwrap(),
        level_type,
        settings,
    );
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

#[allow(clippy::too_many_arguments)]
fn run(
    jar: &Path,
    java: &Path,
    data: &Path,
    dir: &Path,
    seed: i64,
    radius: i32,
    level_type: &str,
    generator_settings: &str,
) -> Result<(), String> {
    let choice = Choice {
        level_type: level_type.to_owned(),
        generator_settings: generator_settings.to_owned(),
    };
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    // `CHUNKWAKE_REUSE=1` — сверить с уже сгенерированным миром без запуска jar.
    let saved = dir.join("world/dimensions/minecraft/overworld/region");
    if std::env::var_os("CHUNKWAKE_REUSE").is_some() && saved.is_dir() {
        return compare(&saved, data, seed, radius, &choice);
    }
    // Прошлый запуск мог оставить `session.lock`: сносим мир, но не каталог.
    let world = dir.join("world");
    if world.exists() {
        std::fs::remove_dir_all(&world).map_err(|e| format!("{}: {e}", world.display()))?;
    }
    let port = free_port();
    // 26.3 читает `generator-settings` как JSON. Имя пресета плоского мира
    // разворачивается в объект `settings` из файла пресета.
    let written = flat_properties_settings(data, level_type, generator_settings)?;
    std::fs::write(
        dir.join("server.properties"),
        format!(
            "level-seed={seed}\nlevel-type={level_type}\n\
             generator-settings={written}\nonline-mode=false\n\
             server-port={port}\n"
        ),
    )
    .map_err(|e| format!("server.properties: {e}"))?;
    std::fs::write(dir.join("eula.txt"), "eula=true\n").ok();

    let mut child = Command::new(java)
        .arg("-Xmx2G")
        .arg("-jar")
        .arg(jar)
        .arg("nogui")
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("cannot start {}: {e}", java.display()))?;

    wait_for(&mut child, "Done (", Duration::from_secs(240))?;
    // Форс-лоад квадрата: без него сервер не сгенерирует дальние чанки.
    let square = chunk_square(radius);
    for (x, z) in &square {
        send(&mut child, &format!("forceload add {} {}", x * 16, z * 16))?;
    }
    // Генерация растёт с числом чанков; фиксированные 6 с обрезают большое окно.
    std::thread::sleep(Duration::from_secs(8 + square.len() as u64 * 2));
    send(&mut child, "save-all flush")?;
    std::thread::sleep(Duration::from_secs(15));
    send(&mut child, "stop")?;
    let _ = child.wait();

    // С 26.3 чанки оверворлда лежат в `dimensions/minecraft/overworld/region`.
    let regions = world.join("dimensions/minecraft/overworld/region");
    let regions = if regions.is_dir() {
        regions
    } else {
        world.join("region")
    };
    compare(&regions, data, seed, radius, &choice)
}

/// Тип мира стенда: то, что пишется в `server.properties`.
struct Choice {
    level_type: String,
    generator_settings: String,
}

/// Разобранный пресет. `normal` без файла — оверворлд, как раньше.
enum Resolved {
    Noise {
        settings: String,
        biome: Option<String>,
    },
    Flat {
        layers: Vec<(String, usize)>,
        biome: String,
        features: bool,
    },
}

fn resolve(data: &Path, choice: &Choice) -> Result<Resolved, String> {
    let key = choice
        .level_type
        .strip_prefix("minecraft:")
        .unwrap_or(&choice.level_type);
    match key {
        "normal" => Ok(Resolved::Noise {
            settings: "minecraft:overworld".into(),
            biome: None,
        }),
        "large_biomes" | "amplified" | "single_biome_surface" => {
            noise_preset(data, key, &choice.generator_settings)
        }
        "flat" => flat_preset(data, &choice.generator_settings),
        other => Err(format!(
            "level-type={other}: use normal, large_biomes, amplified, single_biome_surface or flat"
        )),
    }
}

/// В `server.properties` плоский пресет — JSON `settings`, не имя файла.
fn flat_properties_settings(
    data: &Path,
    level_type: &str,
    settings: &str,
) -> Result<String, String> {
    let key = level_type.strip_prefix("minecraft:").unwrap_or(level_type);
    if key != "flat" {
        return Ok(settings.to_owned());
    }
    let s = settings.trim();
    if s.starts_with('{') {
        return Ok(s.to_owned());
    }
    let name = if s.is_empty() {
        "classic_flat"
    } else {
        s.strip_prefix("minecraft:").unwrap_or(s)
    };
    let path = data.join(format!(
        "data/minecraft/worldgen/flat_level_generator_preset/{name}.json"
    ));
    let text = std::fs::read_to_string(&path).map_err(|e| format!("flat preset {name}: {e}"))?;
    let v: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("flat preset {name}: {e}"))?;
    serde_json::to_string(&v["settings"]).map_err(|e| e.to_string())
}

fn namespaced(name: &str) -> String {
    if name.contains(':') {
        name.to_owned()
    } else {
        format!("minecraft:{name}")
    }
}

fn noise_preset(data: &Path, name: &str, generator_settings: &str) -> Result<Resolved, String> {
    let path = data.join(format!("data/minecraft/worldgen/world_preset/{name}.json"));
    let text = std::fs::read_to_string(&path).map_err(|e| format!("world preset {name}: {e}"))?;
    let v: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("world preset {name}: {e}"))?;
    let generator = &v["dimensions"]["minecraft:overworld"]["generator"];
    let settings = generator["settings"]
        .as_str()
        .map(namespaced)
        .ok_or_else(|| format!("world preset {name}: settings is not a string"))?;
    let source = &generator["biome_source"];
    let mut biome = match source["type"].as_str().unwrap_or("") {
        "minecraft:multi_noise" => None,
        "minecraft:fixed" => Some(namespaced(
            source["biome"].as_str().unwrap_or("minecraft:plains"),
        )),
        other => return Err(format!("world preset {name}: biome source {other}")),
    };
    let trimmed = generator_settings.trim();
    if biome.is_some() && trimmed.starts_with('{') {
        let over: serde_json::Value =
            serde_json::from_str(trimmed).map_err(|e| format!("generator-settings: {e}"))?;
        if let Some(name) = over["biome"].as_str() {
            biome = Some(namespaced(name));
        }
    }
    Ok(Resolved::Noise { settings, biome })
}

fn flat_preset(data: &Path, generator_settings: &str) -> Result<Resolved, String> {
    let s = generator_settings.trim();
    let settings = if s.starts_with('{') {
        serde_json::from_str(s).map_err(|e| format!("generator-settings: {e}"))?
    } else {
        let name = if s.is_empty() {
            "classic_flat"
        } else {
            s.strip_prefix("minecraft:").unwrap_or(s)
        };
        let path = data.join(format!(
            "data/minecraft/worldgen/flat_level_generator_preset/{name}.json"
        ));
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("flat preset {name}: {e}"))?;
        let v: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("flat preset {name}: {e}"))?;
        v["settings"].clone()
    };
    let layers = settings["layers"]
        .as_array()
        .ok_or("flat preset: no layers")?
        .iter()
        .map(|l| {
            let block = namespaced(l["block"].as_str().ok_or("flat layer: no block")?);
            let height = l["height"].as_u64().unwrap_or(1) as usize;
            Ok((block, height))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(Resolved::Flat {
        layers,
        biome: settings["biome"]
            .as_str()
            .unwrap_or("minecraft:plains")
            .to_owned(),
        features: settings["features"].as_bool().unwrap_or(false),
    })
}

/// Свободный TCP-порт: ванильный сервер не должен драться с нашим за 25565.
fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    listener.local_addr().expect("addr").port()
}

fn send(child: &mut Child, line: &str) -> Result<(), String> {
    let stdin = child.stdin.as_mut().ok_or("no stdin")?;
    writeln!(stdin, "{line}").map_err(|e| format!("stdin: {e}"))?;
    stdin.flush().map_err(|e| format!("stdin: {e}"))
}

fn wait_for(child: &mut Child, needle: &str, limit: Duration) -> Result<(), String> {
    let start = Instant::now();
    let mut log = String::new();
    let mut buf = [0u8; 8192];
    let mut out = child.stdout.take().ok_or("no stdout")?;
    while start.elapsed() < limit {
        match out.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                log.push_str(&String::from_utf8_lossy(&buf[..n]));
                if log.contains(needle) {
                    // Дочитываем в фоне: иначе полный канал остановит сервер.
                    std::thread::spawn(move || {
                        let mut sink = std::io::sink();
                        let _ = std::io::copy(&mut out, &mut sink);
                    });
                    return Ok(());
                }
            }
            Err(e) => return Err(format!("stdout: {e}")),
        }
    }
    let tail: Vec<&str> = log.lines().rev().take(8).collect();
    Err(format!(
        "server did not start: {}",
        tail.into_iter().rev().collect::<Vec<_>>().join(" | ")
    ))
}

/// Имена состояний и биомов для чтения `.mca`.
struct FileNames<'a> {
    blocks: &'a chunkwake_data::world::Blocks,
    biomes: &'a HashMap<String, u32>,
}

impl Names for FileNames<'_> {
    fn block_name(&self, id: u32) -> Option<(&str, &[(String, String)])> {
        self.blocks.state(id)
    }
    fn block_id(&self, name: &str, props: &[(String, String)]) -> Option<u32> {
        self.blocks.state_id(name, props)
    }
    fn block_default(&self, name: &str) -> Option<u32> {
        self.blocks.default_state(name).ok().map(|s| s as u32)
    }
    fn biome_name(&self, id: u32) -> Option<&str> {
        self.biomes
            .iter()
            .find(|(_, v)| **v == id)
            .map(|(k, _)| k.as_str())
    }
    fn biome_id(&self, name: &str) -> Option<u32> {
        self.biomes.get(name).copied()
    }
}

/// Центр квадрата. `CHUNKWAKE_ORIGIN=cx,cz`, иначе `(0, 0)`.
fn origin() -> (i32, i32) {
    let Ok(raw) = std::env::var("CHUNKWAKE_ORIGIN") else {
        return (0, 0);
    };
    let Some((x, z)) = raw.split_once(',') else {
        panic!("CHUNKWAKE_ORIGIN must be cx,cz");
    };
    (
        x.trim().parse().expect("CHUNKWAKE_ORIGIN cx"),
        z.trim().parse().expect("CHUNKWAKE_ORIGIN cz"),
    )
}

/// Чанки `(ox - radius)..(ox + radius)`. При нулевом центре это `-radius..radius`.
fn chunk_square(radius: i32) -> Vec<(i32, i32)> {
    let (ox, oz) = origin();
    (ox - radius..ox + radius)
        .flat_map(|cx| (oz - radius..oz + radius).map(move |cz| (cx, cz)))
        .collect()
}

fn compare(
    regions: &Path,
    data: &Path,
    seed: i64,
    radius: i32,
    choice: &Choice,
) -> Result<(), String> {
    let resolved = resolve(data, choice)?;
    let (settings, biome) = match resolved {
        Resolved::Flat {
            layers,
            biome,
            features,
        } => return compare_flat(regions, data, radius, &layers, &biome, features),
        Resolved::Noise { settings, biome } => (settings, biome),
    };
    if std::env::var_os("CHUNKWAKE_BLOCKS").is_some() {
        return compare_blocks(regions, data, seed, radius, &settings, biome.as_deref());
    }
    // `CHUNKWAKE_NO_CARVERS=1` — сверка без карверов, для базы расхождений.
    let no_carvers = std::env::var_os("CHUNKWAKE_NO_CARVERS").is_some();
    let mut noise = Generator::load_settings(data, seed, None, &settings)?;
    let registries =
        chunkwake_data::registries::RegistryData::load(data).map_err(|e| e.to_string())?;
    let biomes: HashMap<String, u32> = registries
        .synchronized
        .iter()
        .find(|r| r.name == "worldgen/biome")
        .map(|r| {
            r.entries
                .iter()
                .enumerate()
                .map(|(i, n)| (n.clone(), i as u32))
                .collect()
        })
        .unwrap_or_default();
    if let Some(name) = &biome {
        noise.set_fixed_biome(name, &biomes)?;
    }
    let blocks = chunkwake_data::world::Blocks::load(data).map_err(|e| e.to_string())?;
    let names = FileNames {
        blocks: &blocks,
        biomes: &biomes,
    };

    let store = RegionStore::new(regions);
    let mut chunks: HashMap<(i32, i32), Compound> = HashMap::new();
    let mut seen = HashSet::new();
    for (cx, cz) in chunk_square(radius) {
        let key = (cx >> 5, cz >> 5);
        if !seen.insert(key) {
            continue;
        }
        for (pos, root) in store.load_region(key.0, key.1)? {
            chunks.insert(pos, root);
        }
    }

    let format = Format {
        data_version: version(data)?,
        min_section: noise.min_y / 16,
    };
    let empty = vec![0u32; noise.height as usize * 256];
    let template = ChunkColumn::from_blocks(&empty, 0, 15, 7, true);

    let (mut checked, mut bad_chunks, mut bad_blocks) = (0, 0, 0);
    // Разбивка: (наше, ваниль, полоса y по 32) → число блоков.
    let mut pairs: HashMap<(String, String, i32), usize> = HashMap::new();
    let mut vanilla_names: HashMap<String, usize> = HashMap::new();
    let mut first: Option<(i32, i32, i32, String, String)> = None;
    for (cx, cz) in chunk_square(radius) {
        let Some(nbt) = chunks.get(&(cx, cz)) else {
            continue;
        };
        let (want, _) = anvil::chunk_from_nbt(nbt, &template, format, &names)?;
        // Карверы на классах заполнения: 0 воздух, 1 камень, 2 вода, 3 лава.
        let mut terrain = noise.terrain(cx, cz);
        let mut classes: Vec<u32> = terrain
            .fills
            .iter()
            .map(|f| match f {
                chunkwake_worldgen::Fill::Air => 0,
                chunkwake_worldgen::Fill::Solid => 1,
                chunkwake_worldgen::Fill::Liquid | chunkwake_worldgen::Fill::Water => 2,
                chunkwake_worldgen::Fill::Lava => 3,
            })
            .collect();
        if !no_carvers && let Some(mask) = noise.carving_mask(cx, cz) {
            let b = chunkwake_worldgen::CarveBlocks {
                air: 0,
                water: 2,
                lava: 3,
                // `default_fluid` оверворлда — вода.
                liquid: 2,
                bedrock: u32::MAX,
                grass: u32::MAX - 1,
                mycelium: u32::MAX - 2,
                dirt: u32::MAX - 3,
            };
            noise.carve(&mut terrain, &mask, &b, &mut classes, |_, _, _, _, _| None);
        }
        let fill: Vec<chunkwake_worldgen::Fill> = classes
            .iter()
            .map(|c| match c {
                0 => chunkwake_worldgen::Fill::Air,
                2 => chunkwake_worldgen::Fill::Water,
                3 => chunkwake_worldgen::Fill::Lava,
                _ => chunkwake_worldgen::Fill::Solid,
            })
            .collect();
        checked += 1;
        if checked == 1 {
            // Диагностика: сколько непустых блоков видит каждая сторона.
            let non_air = |c: &ChunkColumn| {
                (0..noise.height as usize)
                    .flat_map(|y| (0..16).flat_map(move |z| (0..16).map(move |x| (x, y, z))))
                    .filter(|&(x, y, z)| {
                        names.block_name(c.block(x, y, z)).is_some_and(|(n, _)| {
                            !matches!(
                                n,
                                "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
                            )
                        })
                    })
                    .count()
            };
            let ours = fill
                .iter()
                .filter(|f| !matches!(f, chunkwake_worldgen::Fill::Air))
                .count();
            println!(
                "чанк ({cx}, {cz}): ваниль непустых {}, наше {}",
                non_air(&want),
                ours
            );
        }
        let mut bad = 0;
        for y in 0..noise.height as usize {
            for z in 0..16 {
                for x in 0..16 {
                    let ours = fill[(y * 16 + z) * 16 + x];
                    let theirs = want.block(x, y, z);
                    // Сравниваем «заполнение»: воздух, камень, вода, лава.
                    // Стадия поверхности живёт в сервере, поэтому здесь
                    // проверяется только плотность и акифер.
                    let ours_kind = fill_name(ours);
                    let theirs_kind = kind_name(theirs, &blocks, &names);
                    if ours_kind != theirs_kind {
                        bad += 1;
                        let band = (noise.min_y + y as i32).div_euclid(32) * 32;
                        if let Some((n, _)) = names.block_name(theirs) {
                            *vanilla_names
                                .entry(format!("{ours_kind}>{n}"))
                                .or_insert(0usize) += 1;
                        }
                        *pairs
                            .entry((ours_kind.to_owned(), theirs_kind.to_owned(), band))
                            .or_insert(0) += 1;
                        first.get_or_insert((
                            cx * 16 + x as i32,
                            noise.min_y + y as i32,
                            cz * 16 + z as i32,
                            ours_kind.to_owned(),
                            theirs_kind.to_owned(),
                        ));
                    }
                }
            }
        }
        if bad > 0 {
            bad_chunks += 1;
            bad_blocks += bad;
        }
    }
    if std::env::var_os("CHUNKWAKE_NAMES").is_some() {
        let mut v: Vec<_> = vanilla_names.into_iter().collect();
        v.sort_by_key(|e| std::cmp::Reverse(e.1));
        for (n, c) in v.iter().take(25) {
            println!("  {c:>6}  {n}");
        }
    }
    let mut top: Vec<_> = pairs.into_iter().collect();
    top.sort_by_key(|e| std::cmp::Reverse(e.1));
    for ((ours, theirs, band), n) in top.iter().take(14) {
        println!(
            "  {n:>6}  наше {ours:<6} ваниль {theirs:<6} y {band}..{}",
            band + 31
        );
    }
    match first {
        Some((x, y, z, ours, theirs)) => println!(
            "расхождений {bad_blocks} в {bad_chunks} чанках из {checked}; первое ({x}, {y}, {z}): наше {ours}, ваниль {theirs}"
        ),
        None => println!("{checked} чанков совпали с ванилью"),
    }
    Ok(())
}

/// Наша сторона: `Fill` как класс блока.
fn fill_name(fill: chunkwake_worldgen::Fill) -> &'static str {
    match fill {
        chunkwake_worldgen::Fill::Air => "air",
        chunkwake_worldgen::Fill::Solid => "solid",
        chunkwake_worldgen::Fill::Liquid | chunkwake_worldgen::Fill::Water => "water",
        chunkwake_worldgen::Fill::Lava => "lava",
    }
}

/// Сторона ванили: состояние → класс блока. Всё, что не жидкость и не воздух,
/// считается твёрдым: поверхность (трава, песок) — тоже твёрдое.
fn kind_name(
    state: u32,
    blocks: &chunkwake_data::world::Blocks,
    names: &FileNames<'_>,
) -> &'static str {
    let Some((name, props)) = names.block_name(state) else {
        return "unknown";
    };
    let _ = blocks;
    // Фичи сажают в воду и воздух растения и наросты: для класса заполнения
    // они — то, что было под ними. Залитые водой блоки — вода.
    if props.iter().any(|(k, v)| k == "waterlogged" && v == "true") {
        return "water";
    }
    let short = name.trim_start_matches("minecraft:");
    match short {
        "air" | "cave_air" | "void_air" => "air",
        "water" | "kelp" | "kelp_plant" | "seagrass" | "tall_seagrass" | "bubble_column" => "water",
        "lava" => "lava",
        "glow_lichen" | "pointed_dripstone" | "vine" | "cave_vines" | "cave_vines_plant"
        | "hanging_roots" | "spore_blossom" | "moss_carpet" | "small_dripleaf" | "big_dripleaf"
        | "big_dripleaf_stem" | "azalea" | "flowering_azalea" | "short_grass" | "tall_grass"
        | "fern" | "large_fern" | "sculk_vein" | "snow" | "dead_bush" | "sweet_berry_bush"
        | "sugar_cane" | "cactus" | "pumpkin" | "brown_mushroom" | "red_mushroom" | "lily_pad"
        | "firefly_bush" | "bush" | "leaf_litter" | "wildflowers" | "pink_petals"
        | "short_dry_grass" | "tall_dry_grass" | "cactus_flower" => "air",
        _ if short.ends_with("_leaves")
            || short.ends_with("_log")
            || short.ends_with("_tulip")
            || short.ends_with("_sapling")
            || matches!(
                short,
                "dandelion"
                    | "poppy"
                    | "blue_orchid"
                    | "allium"
                    | "azure_bluet"
                    | "oxeye_daisy"
                    | "cornflower"
                    | "lily_of_the_valley"
                    | "sunflower"
                    | "lilac"
                    | "rose_bush"
                    | "peony"
                    | "bee_nest"
                    | "torchflower"
                    | "open_eyeblossom"
                    | "closed_eyeblossom"
                    | "mangrove_roots"
                    | "pale_moss_carpet"
                    | "pale_hanging_moss"
                    | "resin_clump"
                    | "creaking_heart"
            ) =>
        {
            // Деревья и цветы стоят на поверхности в воздухе.
            "air"
        }
        _ => "solid",
    }
}

/// `DataVersion` из `version.json` кэша данных.
fn version(data: &Path) -> Result<i32, String> {
    let text = std::fs::read_to_string(data.join("version.json"))
        .map_err(|e| format!("version.json: {e}"))?;
    let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    json["world_version"]
        .as_i64()
        .map(|v| v as i32)
        .ok_or_else(|| "no world_version".to_owned())
}

// Заглушка, чтобы `bits_for` не считался неиспользуемым импортом.
#[allow(dead_code)]
fn _bits() -> u32 {
    bits_for(16)
}

#[allow(dead_code)]
fn _unused(_p: PathBuf) {}

/// Точная сверка блоков после всего конвейера (шум, поверхность, карверы).
/// Фичи и постройки ванили тоже попадут в расхождения — разбивка по парам
/// имён показывает, что относится к ним.
fn compare_blocks(
    regions: &Path,
    data: &Path,
    seed: i64,
    radius: i32,
    settings: &str,
    biome: Option<&str>,
) -> Result<(), String> {
    let registries =
        chunkwake_data::registries::RegistryData::load(data).map_err(|e| e.to_string())?;
    let biomes: HashMap<String, u32> = registries
        .synchronized
        .iter()
        .find(|r| r.name == "worldgen/biome")
        .map(|r| {
            r.entries
                .iter()
                .enumerate()
                .map(|(i, n)| (n.clone(), i as u32))
                .collect()
        })
        .unwrap_or_default();
    let mut noise = Generator::load_settings(data, seed, None, settings)?;
    if let Some(name) = biome {
        noise.set_fixed_biome(name, &biomes)?;
    }
    noise.bind_biomes(biomes.clone());
    let (min_y, height) = (noise.min_y, noise.height);
    let blocks = chunkwake_data::world::Blocks::load(data).map_err(|e| e.to_string())?;
    let state = |n: &str| blocks.default_state(n).ok().map(|s| s as u32);
    let overworld =
        chunkwake_worldgen::pipeline::Overworld::new(std::sync::Arc::new(noise), &state, &biomes)?;
    // `CHUNKWAKE_NO_FEATURES=1` — сверка без декорации.
    let overworld = if std::env::var_os("CHUNKWAKE_NO_FEATURES").is_some() {
        overworld
    } else {
        overworld.with_features(data, &blocks.states(), &state, &biomes)?
    };
    if let Some((done, all)) = overworld.feature_coverage() {
        println!("фичи: перенесено {done} из {all}");
    }
    let names = FileNames {
        blocks: &blocks,
        biomes: &biomes,
    };
    let store = RegionStore::new(regions);
    let mut chunks: HashMap<(i32, i32), Compound> = HashMap::new();
    let mut seen = HashSet::new();
    for (cx, cz) in chunk_square(radius) {
        let key = (cx >> 5, cz >> 5);
        if seen.insert(key) {
            for (pos, root) in store.load_region(key.0, key.1)? {
                chunks.insert(pos, root);
            }
        }
    }
    // `CHUNKWAKE_AGAINST=<каталог region>` — второй ванильный мир вместо
    // нашего: шумовой порог самой ванили (порядок декорации не детерминирован).
    let mut against: HashMap<(i32, i32), Compound> = HashMap::new();
    if let Some(dir) = std::env::var_os("CHUNKWAKE_AGAINST") {
        let other = RegionStore::new(Path::new(&dir));
        let mut seen = HashSet::new();
        for (cx, cz) in chunk_square(radius) {
            let key = (cx >> 5, cz >> 5);
            if seen.insert(key) {
                for (pos, root) in other.load_region(key.0, key.1)? {
                    against.insert(pos, root);
                }
            }
        }
    }
    let format = Format {
        data_version: version(data)?,
        min_section: min_y / 16,
    };
    let empty = vec![0u32; height as usize * 256];
    let template = ChunkColumn::from_blocks(&empty, 0, 15, 7, true);
    let props_on = std::env::var_os("CHUNKWAKE_PROPS").is_some();
    let short = |id: u32| {
        names.block_name(id).map_or("?".to_owned(), |(n, props)| {
            let base = n.trim_start_matches("minecraft:");
            if !props_on || props.is_empty() {
                return base.to_owned();
            }
            let text: Vec<_> = props.iter().map(|(k, v)| format!("{k}={v}")).collect();
            format!("{base}[{}]", text.join(","))
        })
    };
    let (mut checked, mut bad) = (0, 0usize);
    let mut pairs: HashMap<(String, String), usize> = HashMap::new();
    let mut biome_bad = 0usize;
    // `CHUNKWAKE_BELOOT=1` — сверка блок-сущностей контейнеров: тип,
    // `LootTable` и `LootTableSeed` в тех же клетках.
    let beloot = std::env::var_os("CHUNKWAKE_BELOOT").is_some();
    let (mut be_seen, mut be_bad) = (0, 0usize);
    let mut be_report = 0usize;
    // `CHUNKWAKE_REVERSE=1` — запрашивать чанки в обратном порядке: фичи
    // через границу чанка зависят от порядка декорации.
    let mut order = chunk_square(radius);
    if std::env::var_os("CHUNKWAKE_REVERSE").is_some() {
        order.reverse();
    }
    // `CHUNKWAKE_EAGER=1` — сначала декорировать все чанки окна, потом
    // кольцо (проба приоритета тикетов; на сиде 12345 хуже, чем по запросам).
    if std::env::var_os("CHUNKWAKE_EAGER").is_some() {
        for &(cx, cz) in &order {
            overworld.predecorate(cx, cz);
        }
    }
    for (cx, cz) in order {
        {
            let Some(nbt) = chunks.get(&(cx, cz)) else {
                continue;
            };
            let (want, _) = anvil::chunk_from_nbt(nbt, &template, format, &names)?;
            // `None` — чанк уже отдан раньше в этом проходе; сверять нечего.
            let Some(mut ours) = overworld.chunk(cx, cz) else {
                continue;
            };
            if let Some(nbt) = against.get(&(cx, cz)) {
                let (other, _) = anvil::chunk_from_nbt(nbt, &template, format, &names)?;
                for y in 0..height as usize {
                    for z in 0..16 {
                        for x in 0..16 {
                            ours.states[(y * 16 + z) * 16 + x] = other.block(x, y, z);
                        }
                    }
                }
                // Биомы тоже второго мира: ничьи `Climate.RTree` ваниль решает
                // по `lastResult` потока, то есть по истории запросов.
                if let Some(b) = &mut ours.biomes {
                    for (i, b) in b.iter_mut().enumerate() {
                        let (sy, rest) = (i / 64, i % 64);
                        let (qy, qz, qx) = (rest / 16, rest / 4 % 4, rest % 4);
                        *b = other.sections[sy].biome_at(qx, qy, qz);
                    }
                }
            }
            checked += 1;
            // `CHUNKWAKE_COLUMN=x,z` — столбец целиком: наше и ваниль по y.
            if let Some((qx, qz)) = std::env::var("CHUNKWAKE_COLUMN").ok().and_then(|v| {
                let (a, b) = v.split_once(',')?;
                Some((a.parse::<i32>().ok()?, b.parse::<i32>().ok()?))
            }) && (qx.div_euclid(16), qz.div_euclid(16)) == (cx, cz)
            {
                let (x, z) = (qx.rem_euclid(16) as usize, qz.rem_euclid(16) as usize);
                // Тикался ли чанк ванили: тогда жидкости уже текли.
                println!(
                    "  InhabitedTime {:?}, fluid_ticks {:?}, Status {:?}",
                    nbt.get("InhabitedTime").and_then(|t| t.as_i64()),
                    nbt.get("fluid_ticks")
                        .and_then(|t| t.as_list())
                        .map(<[_]>::len),
                    nbt.get("Status"),
                );
                if let Some(b) = &ours.biomes {
                    let top = (height as usize / 4).saturating_sub(1);
                    let mut seen = Vec::new();
                    for qy in 0..=top {
                        let id = b[qy * 16 + (z / 4) * 4 + x / 4];
                        if !seen.contains(&id) {
                            seen.push(id);
                        }
                    }
                    let names: Vec<_> = seen
                        .iter()
                        .filter_map(|id| {
                            biomes
                                .iter()
                                .find(|(_, v)| *v == id)
                                .map(|(n, _)| n.as_str())
                        })
                        .collect();
                    println!("  биомы столбца: {names:?}");
                }
                for y in (0..height as usize).rev() {
                    let a = short(ours.states[(y * 16 + z) * 16 + x]);
                    let b = short(want.block(x, y, z));
                    if a != "air" || b != "air" {
                        let mark = if a == b { " " } else { "*" };
                        println!("  {mark} y {:>4}  {a:<24} {b}", y as i32 + min_y);
                    }
                }
            }
            for y in 0..height as usize {
                for z in 0..16 {
                    for x in 0..16 {
                        let a = short(ours.states[(y * 16 + z) * 16 + x]);
                        let b = short(want.block(x, y, z));
                        if a != b {
                            bad += 1;
                            // `CHUNKWAKE_WHERE=наше,ваниль` — координаты такой пары.
                            if let Ok(w) = std::env::var("CHUNKWAKE_WHERE")
                                && w == format!("{a},{b}")
                            {
                                let wy = y as i32 + min_y;
                                println!("    {} {wy} {}", cx * 16 + x as i32, cz * 16 + z as i32);
                            }
                            *pairs.entry((a, b)).or_insert(0) += 1;
                        }
                    }
                }
            }
            if let Some(ours_b) = &ours.biomes {
                for (i, &b) in ours_b.iter().enumerate() {
                    let (sy, rest) = (i / 64, i % 64);
                    let (qy, qz, qx) = (rest / 16, rest / 4 % 4, rest % 4);
                    if want.sections[sy].biome_at(qx, qy, qz) != b {
                        biome_bad += 1;
                        // `CHUNKWAKE_BIOME_WHERE=1` — клетка, наш и ванильный ID.
                        if std::env::var_os("CHUNKWAKE_BIOME_WHERE").is_some() {
                            println!(
                                "    биом {cx},{cz} y{} q{qx},{qz}: {b} | {}",
                                min_y + (sy * 16 + qy * 4) as i32,
                                want.sections[sy].biome_at(qx, qy, qz)
                            );
                        }
                    }
                }
            }
            if beloot {
                // Контейнеры: `id`, `LootTable`, `LootTableSeed`. `components`
                // ванила пишет всегда, у нас он пустой — сверяем только то,
                // что задаёт содержимое.
                let mut mine: EntityMap = HashMap::new();
                for ([x, y, z], id, nbt) in &ours.block_entities {
                    // Список чанка может держать и соседей: фиксируем только свои.
                    if (x >> 4, z >> 4) != (cx, cz) {
                        continue;
                    }
                    let table = nbt.get("LootTable").and_then(|t| t.as_str());
                    let seed = nbt.get("LootTableSeed").and_then(|t| t.as_i64());
                    mine.insert([*x, *y, *z], (id.as_str(), table, seed));
                }
                for (pos, be) in &want.block_entities {
                    // Сверяем только то, что пишет наш конвейер: контейнеры
                    // структур (`RandomizableContainer`) — по таблице и семени.
                    let container = be.nbt.contains_key("LootTable")
                        || [
                            "minecraft:chest",
                            "minecraft:trapped_chest",
                            "minecraft:barrel",
                            "minecraft:dispenser",
                            "minecraft:hopper",
                            "minecraft:decorated_pot",
                        ]
                        .contains(&be.id.as_str());
                    if !container {
                        continue;
                    }
                    be_seen += 1;
                    let table = be.nbt.get("LootTable").and_then(|t| t.as_str());
                    let seed = be.nbt.get("LootTableSeed").and_then(|t| t.as_i64());
                    let got = mine.get(pos).copied();
                    let same = got.is_some_and(|(id, t, s)| id == be.id && t == table && s == seed);
                    if !same {
                        if be_report < 64 {
                            let block = ours
                                .states
                                .get(
                                    (((pos[1] - min_y) * 16 + (pos[2] & 15)) * 16 + (pos[0] & 15))
                                        as usize,
                                )
                                .map_or("?".to_owned(), |s| short(*s));
                            println!(
                                "    be {pos:?} ваниль {} {table:?} {seed:?} / наше {got:?} блок {block}",
                                be.id
                            );
                        }
                        be_report += 1;
                        be_bad += 1;
                    }
                }
                for (pos, (id, table, seed)) in &mine {
                    if want.block_entities.contains_key(pos) {
                        continue;
                    }
                    // Лишняя блок-сущность там, где у ванили её нет.
                    if be_report < 64 {
                        println!("    be {pos:?} лишняя {id} {table:?} {seed:?}");
                    }
                    be_report += 1;
                    be_bad += 1;
                }
            }
        }
    }
    let mut top: Vec<_> = pairs.into_iter().collect();
    top.sort_by_key(|e| std::cmp::Reverse(e.1));
    // `CHUNKWAKE_GREP=подстрока` — все пары с ней вместо первых 30.
    let grep = std::env::var("CHUNKWAKE_GREP").ok();
    let shown = top.iter().filter(|((a, b), _)| {
        grep.as_deref()
            .is_none_or(|g| a.contains(g) || b.contains(g))
    });
    for ((a, b), n) in shown.take(if grep.is_some() { usize::MAX } else { 30 }) {
        println!("  {n:>6}  наше {a:<22} ваниль {b}");
    }
    println!("блоков расходится {bad} в {checked} чанках, клеток биомов {biome_bad}");
    if beloot {
        println!("контейнеров {be_seen}, расходятся {be_bad}");
    }
    Ok(())
}

/// Плоский мир: слои пресета и, у `the_void` с `features`, платформа 33×33.
fn compare_flat(
    regions: &Path,
    data: &Path,
    radius: i32,
    layers: &[(String, usize)],
    biome: &str,
    features: bool,
) -> Result<(), String> {
    let (min_y, height) = overworld_span(data)?;
    let registries =
        chunkwake_data::registries::RegistryData::load(data).map_err(|e| e.to_string())?;
    let biomes: HashMap<String, u32> = registries
        .synchronized
        .iter()
        .find(|r| r.name == "worldgen/biome")
        .map(|r| {
            r.entries
                .iter()
                .enumerate()
                .map(|(i, n)| (n.clone(), i as u32))
                .collect()
        })
        .unwrap_or_default();
    let blocks = chunkwake_data::world::Blocks::load(data).map_err(|e| e.to_string())?;
    let state = |name: &str| {
        blocks
            .default_state(name)
            .map(|s| s as u32)
            .map_err(|e| e.to_string())
    };
    let built: Vec<(u32, usize)> = layers
        .iter()
        .map(|(name, h)| Ok((state(name)?, *h)))
        .collect::<Result<_, String>>()?;
    let biome_id = biomes.get(biome).copied().unwrap_or(0);
    let template = ChunkColumn::flat(&built, (height / 16) as usize, biome_id, 15, 7, true);
    let platform = features && biome.strip_prefix("minecraft:").unwrap_or(biome) == "the_void";
    let (stone, cobble) = if platform {
        (state("minecraft:stone")?, state("minecraft:cobblestone")?)
    } else {
        (0, 0)
    };
    let names = FileNames {
        blocks: &blocks,
        biomes: &biomes,
    };
    let store = RegionStore::new(regions);
    let mut chunks: HashMap<(i32, i32), Compound> = HashMap::new();
    let mut seen = HashSet::new();
    for cx in -radius..radius {
        for cz in -radius..radius {
            let key = (cx >> 5, cz >> 5);
            if seen.insert(key) {
                for (pos, root) in store.load_region(key.0, key.1)? {
                    chunks.insert(pos, root);
                }
            }
        }
    }
    let format = Format {
        data_version: version(data)?,
        min_section: min_y / 16,
    };
    let empty = vec![0u32; height as usize * 256];
    let decode = ChunkColumn::from_blocks(&empty, 0, 15, 7, true);
    let short = |id: u32| {
        names.block_name(id).map_or("?".to_owned(), |n| {
            n.0.trim_start_matches("minecraft:").to_owned()
        })
    };
    let (mut checked, mut bad) = (0usize, 0usize);
    let mut pairs: HashMap<(String, String), usize> = HashMap::new();
    let y_local = (chunkwake_worldgen::void_platform_y(min_y) - min_y) as usize;
    for cx in -radius..radius {
        for cz in -radius..radius {
            let Some(nbt) = chunks.get(&(cx, cz)) else {
                continue;
            };
            let (want, _) = anvil::chunk_from_nbt(nbt, &decode, format, &names)?;
            let mut ours = template.clone();
            if platform && chunkwake_worldgen::void_platform_chunk(cx, cz) {
                let x0 = cx * 16;
                let z0 = cz * 16;
                for lz in 0..16 {
                    for lx in 0..16 {
                        let Some(kind) = chunkwake_worldgen::void_platform_block(x0 + lx, z0 + lz)
                        else {
                            continue;
                        };
                        let block = match kind {
                            chunkwake_worldgen::VoidPlatformBlock::Cobblestone => cobble,
                            chunkwake_worldgen::VoidPlatformBlock::Stone => stone,
                        };
                        ours.set_block(lx as usize, y_local, lz as usize, block);
                    }
                }
            }
            checked += 1;
            for y in 0..height as usize {
                for z in 0..16 {
                    for x in 0..16 {
                        let a = short(ours.block(x, y, z));
                        let b = short(want.block(x, y, z));
                        if a != b {
                            bad += 1;
                            *pairs.entry((a, b)).or_insert(0) += 1;
                        }
                    }
                }
            }
        }
    }
    let mut top: Vec<_> = pairs.into_iter().collect();
    top.sort_by_key(|e| std::cmp::Reverse(e.1));
    for ((a, b), n) in top.iter().take(30) {
        println!("  {n:>6}  наше {a:<22} ваниль {b}");
    }
    println!("блоков расходится {bad} в {checked} чанках");
    Ok(())
}

fn overworld_span(data: &Path) -> Result<(i32, i32), String> {
    let path = data.join("data/minecraft/dimension_type/overworld.json");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("dimension_type: {e}"))?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let min_y = v["min_y"].as_i64().ok_or("dimension: no min_y")? as i32;
    let height = v["height"].as_i64().ok_or("dimension: no height")? as i32;
    Ok((min_y, height))
}
