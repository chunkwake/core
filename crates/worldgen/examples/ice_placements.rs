//! Сколько вызовов даёт размещение: `rarity_filter` × `in_square` × `biome`.
//!
//! `ice_placements <данные> <seed> <cx> <cz> <radius>`

use chunkwake_worldgen::Generator;
use std::collections::HashMap;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::path::PathBuf::from(&a[1]);
    let seed: i64 = a[2].parse::<i64>().unwrap();
    let cx0: i32 = a[3].parse().unwrap();
    let cz0: i32 = a[4].parse().unwrap();
    let rad: i32 = a[5].parse().unwrap();

    let regs = chunkwake_data::registries::RegistryData::load(&data).unwrap();
    let biomes: HashMap<String, u32> = regs
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
    let mut g = Generator::load_with_version(&data, seed, None).unwrap();
    g.bind_biomes(biomes.clone());
    println!("биомы загружены: {}", biomes.len());

    // Климат в клетках 4×4 чанка: печатаем биом в центре каждого второго чанка.
    let mut counts: HashMap<u32, usize> = HashMap::new();
    for cx in cx0 - rad..cx0 + rad {
        for cz in cz0 - rad..cz0 + rad {
            if let Some(list) = g.biomes(cx, cz) {
                for b in list {
                    *counts.entry(b).or_insert(0) += 1;
                }
            }
        }
    }
    let names: Vec<String> = regs
        .synchronized
        .iter()
        .find(|r| r.name == "worldgen/biome")
        .map(|r| r.entries.clone())
        .unwrap_or_default();
    let mut list: Vec<_> = counts.into_iter().collect();
    list.sort_by_key(|e| std::cmp::Reverse(e.1));
    for (id, n) in list.iter().take(8) {
        println!(
            "  {n:>5}  {}",
            names.get(*id as usize).map_or("?", String::as_str)
        );
    }
}
