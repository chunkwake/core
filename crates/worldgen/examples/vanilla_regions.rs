//! Стенд сверки с ванилью: генерация настоящим `server.jar` и побайтовое
//! сравнение блоков с нашим генератором.
//!
//! `vanilla_regions <server.jar> <java> <данные> <каталог> <seed> <радиус>`
//!
//! Каталог должен быть пустым: там появятся `world/` и `server.properties`.
//! Печатает число расхождений и первое из них, а при нуле — «совпали».

use chunkwake_world::anvil::{self, Format, Names, RegionStore};
use chunkwake_world::nbt::Compound;
use chunkwake_world::{ChunkColumn, bits_for};
use chunkwake_worldgen::Generator;
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 7 {
        eprintln!("usage: vanilla_regions <jar> <java> <data> <work> <seed> <radius>");
        std::process::exit(2);
    }
    let result = run(
        Path::new(&a[1]),
        Path::new(&a[2]),
        Path::new(&a[3]),
        Path::new(&a[4]),
        a[5].parse().unwrap(),
        a[6].parse().unwrap(),
    );
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

fn run(
    jar: &Path,
    java: &Path,
    data: &Path,
    dir: &Path,
    seed: i64,
    radius: i32,
) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    // `CHUNKWAKE_REUSE=1` — сверить с уже сгенерированным миром без запуска jar.
    let saved = dir.join("world/dimensions/minecraft/overworld/region");
    if std::env::var_os("CHUNKWAKE_REUSE").is_some() && saved.is_dir() {
        return compare(&saved, data, seed, radius);
    }
    // Прошлый запуск мог оставить `session.lock`: сносим мир, но не каталог.
    let world = dir.join("world");
    if world.exists() {
        std::fs::remove_dir_all(&world).map_err(|e| format!("{}: {e}", world.display()))?;
    }
    let port = free_port();
    std::fs::write(
        dir.join("server.properties"),
        format!(
            "level-seed={seed}\nlevel-type=minecraft:normal\nonline-mode=false\n\
             server-port={port}\n"
        ),
    )
    .ok();
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
    for x in -radius..radius {
        for z in -radius..radius {
            send(&mut child, &format!("forceload add {} {}", x * 16, z * 16))?;
        }
    }
    std::thread::sleep(Duration::from_secs(6));
    send(&mut child, "save-all flush")?;
    std::thread::sleep(Duration::from_secs(10));
    send(&mut child, "stop")?;
    let _ = child.wait();

    // С 26.3 чанки оверворлда лежат в `dimensions/minecraft/overworld/region`.
    let regions = world.join("dimensions/minecraft/overworld/region");
    let regions = if regions.is_dir() {
        regions
    } else {
        world.join("region")
    };
    compare(&regions, data, seed, radius)
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

fn compare(regions: &Path, data: &Path, seed: i64, radius: i32) -> Result<(), String> {
    if std::env::var_os("CHUNKWAKE_BLOCKS").is_some() {
        return compare_blocks(regions, data, seed, radius);
    }
    // `CHUNKWAKE_NO_CARVERS=1` — сверка без карверов, для базы расхождений.
    let no_carvers = std::env::var_os("CHUNKWAKE_NO_CARVERS").is_some();
    let noise = Generator::load(data, seed)?;
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
            if !seen.insert(key) {
                continue;
            }
            for (pos, root) in store.load_region(key.0, key.1)? {
                chunks.insert(pos, root);
            }
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
    for cx in -radius..radius {
        for cz in -radius..radius {
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
                    chunkwake_worldgen::Fill::Water => 2,
                    chunkwake_worldgen::Fill::Lava => 3,
                })
                .collect();
            if !no_carvers && let Some(mask) = noise.carving_mask(cx, cz) {
                let b = chunkwake_worldgen::CarveBlocks {
                    air: 0,
                    water: 2,
                    lava: 3,
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
                                *vanilla_names.entry(format!("{ours_kind}>{n}")).or_insert(0usize) += 1;
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
    }
    if std::env::var_os("CHUNKWAKE_NAMES").is_some() {
        let mut v: Vec<_> = vanilla_names.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        for (n, c) in v.iter().take(25) {
            println!("  {c:>6}  {n}");
        }
    }
    let mut top: Vec<_> = pairs.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1));
    for ((ours, theirs, band), n) in top.iter().take(14) {
        println!("  {n:>6}  наше {ours:<6} ваниль {theirs:<6} y {band}..{}", band + 31);
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
        chunkwake_worldgen::Fill::Water => "water",
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
        "water" | "kelp" | "kelp_plant" | "seagrass" | "tall_seagrass" | "bubble_column" => {
            "water"
        }
        "lava" => "lava",
        "glow_lichen" | "pointed_dripstone" | "vine" | "cave_vines" | "cave_vines_plant"
        | "hanging_roots" | "spore_blossom" | "moss_carpet" | "small_dripleaf"
        | "big_dripleaf" | "big_dripleaf_stem" | "azalea" | "flowering_azalea"
        | "short_grass" | "tall_grass" | "fern" | "large_fern" | "sculk_vein" | "snow"
        | "dead_bush" | "sweet_berry_bush" | "sugar_cane" | "cactus" | "pumpkin"
        | "brown_mushroom" | "red_mushroom" | "lily_pad" | "firefly_bush" | "bush"
        | "leaf_litter" | "wildflowers" | "pink_petals" | "short_dry_grass"
        | "tall_dry_grass" | "cactus_flower" => "air",
        _ if short.ends_with("_leaves")
            || short.ends_with("_log")
            || short.ends_with("_tulip")
            || short.ends_with("_sapling")
            || matches!(
                short,
                "dandelion" | "poppy" | "blue_orchid" | "allium" | "azure_bluet"
                    | "oxeye_daisy" | "cornflower" | "lily_of_the_valley" | "sunflower"
                    | "lilac" | "rose_bush" | "peony" | "bee_nest" | "torchflower"
                    | "open_eyeblossom" | "closed_eyeblossom" | "mangrove_roots"
                    | "pale_moss_carpet" | "pale_hanging_moss"
                    | "resin_clump" | "creaking_heart"
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
fn compare_blocks(regions: &Path, data: &Path, seed: i64, radius: i32) -> Result<(), String> {
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
    let mut noise = Generator::load(data, seed)?;
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
    let template = ChunkColumn::from_blocks(&empty, 0, 15, 7, true);
    let short = |id: u32| {
        names
            .block_name(id)
            .map_or("?".to_owned(), |n| n.0.trim_start_matches("minecraft:").to_owned())
    };
    let (mut checked, mut bad) = (0, 0usize);
    let mut pairs: HashMap<(String, String), usize> = HashMap::new();
    let mut biome_bad = 0usize;
    // `CHUNKWAKE_REVERSE=1` — запрашивать чанки в обратном порядке: фичи
    // через границу чанка зависят от порядка декорации.
    let mut order: Vec<(i32, i32)> =
        (-radius..radius).flat_map(|cx| (-radius..radius).map(move |cz| (cx, cz))).collect();
    if std::env::var_os("CHUNKWAKE_REVERSE").is_some() {
        order.reverse();
    }
    for (cx, cz) in order {
        {
            let Some(nbt) = chunks.get(&(cx, cz)) else {
                continue;
            };
            let (want, _) = anvil::chunk_from_nbt(nbt, &template, format, &names)?;
            // `None` — чанк уже отдан раньше в этом проходе; сверять нечего.
            let Some(ours) = overworld.chunk(cx, cz) else {
                continue;
            };
            checked += 1;
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
                    }
                }
            }
        }
    }
    let mut top: Vec<_> = pairs.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1));
    // `CHUNKWAKE_GREP=подстрока` — все пары с ней вместо первых 30.
    let grep = std::env::var("CHUNKWAKE_GREP").ok();
    let shown = top.iter().filter(|((a, b), _)| {
        grep.as_deref().is_none_or(|g| a.contains(g) || b.contains(g))
    });
    for ((a, b), n) in shown.take(if grep.is_some() { usize::MAX } else { 30 }) {
        println!("  {n:>6}  наше {a:<22} ваниль {b}");
    }
    println!("блоков расходится {bad} в {checked} чанках, клеток биомов {biome_bad}");
    Ok(())
}
