//! Стенд Нижнего мира: генерация шумом `minecraft:nether` и побайтовая сверка
//! с регионами ванильного `server.jar`.
//!
//! `vanilla_nether <данные> <каталог> <seed> <радиус>`
//!
//! Каталог — мир, созданный ванилью (`dimensions/minecraft/the_nether/region`).
//! `CHUNKWAKE_ORIGIN=cx,cz` и `CHUNKWAKE_NO_FEATURES=1` работают как в
//! `vanilla_regions`.

use chunkwake_world::ChunkColumn;
use chunkwake_world::anvil::{self, Format, Names, RegionStore};
use chunkwake_world::nbt::Compound;
use chunkwake_worldgen::Generator;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 5 {
        eprintln!("usage: vanilla_nether <data> <work> <seed> <radius>");
        std::process::exit(2);
    }
    let data = PathBuf::from(&a[1]);
    let dir = PathBuf::from(&a[2]);
    let seed: i64 = a[3].parse().expect("seed");
    let radius: i32 = a[4].parse().expect("radius");
    let regions = dir.join("world/dimensions/minecraft/the_nether/region");
    let regions = if regions.is_dir() {
        regions
    } else {
        // Старый формат: `DIM-1/region`.
        let legacy = dir.join("world/DIM-1/region");
        if legacy.is_dir() { legacy } else { regions }
    };
    if let Err(e) = compare(&regions, &data, seed, radius) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

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

fn chunk_square(radius: i32) -> Vec<(i32, i32)> {
    let (ox, oz) = origin();
    (ox - radius..ox + radius)
        .flat_map(|cx| (oz - radius..oz + radius).map(move |cz| (cx, cz)))
        .collect()
}

fn version(data: &Path) -> Result<i32, String> {
    let text = std::fs::read_to_string(data.join("version.json"))
        .map_err(|e| format!("version.json: {e}"))?;
    let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    json["world_version"]
        .as_i64()
        .map(|v| v as i32)
        .ok_or_else(|| "no world_version".to_owned())
}

fn compare(regions: &Path, data: &Path, seed: i64, radius: i32) -> Result<(), String> {
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
    let mut noise = Generator::load_settings(data, seed, None, "minecraft:nether")?;
    noise.bind_biomes(biomes.clone());
    let (min_y, height) = (noise.min_y, noise.height);
    println!(
        "нижний мир: min_y {min_y}, height {height}, море {}",
        noise.sea_level
    );
    let blocks = chunkwake_data::world::Blocks::load(data).map_err(|e| e.to_string())?;
    let state = |n: &str| blocks.default_state(n).ok().map(|s| s as u32);
    let world = chunkwake_worldgen::pipeline::Overworld::new_for(
        std::sync::Arc::new(noise),
        &state,
        &biomes,
        "minecraft:netherrack",
        "minecraft:lava",
    )?;
    let world = if std::env::var_os("CHUNKWAKE_NO_FEATURES").is_some() {
        world
    } else {
        world.with_features(data, &blocks.states(), &state, &biomes)?
    };
    if let Some((done, all)) = world.feature_coverage() {
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
    let format = Format {
        data_version: version(data)?,
        min_section: min_y / 16,
    };
    let empty = vec![0u32; height as usize * 256];
    let template = ChunkColumn::from_blocks(&empty, 0, 15, 7, false);
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
    for (cx, cz) in chunk_square(radius) {
        let Some(nbt) = chunks.get(&(cx, cz)) else {
            continue;
        };
        let (want, _) = anvil::chunk_from_nbt(nbt, &template, format, &names)?;
        let Some(ours) = world.chunk(cx, cz) else {
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
    }
    let mut top: Vec<_> = pairs.into_iter().collect();
    top.sort_by_key(|e| std::cmp::Reverse(e.1));
    let grep = std::env::var("CHUNKWAKE_GREP").ok();
    let shown = top.iter().filter(|((a, b), _)| {
        grep.as_deref()
            .is_none_or(|g| a.contains(g) || b.contains(g))
    });
    for ((a, b), n) in shown.take(if grep.is_some() { usize::MAX } else { 30 }) {
        println!("  {n:>6}  наше {a:<22} ваниль {b}");
    }
    println!("блоков расходится {bad} в {checked} чанках");
    Ok(())
}

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
