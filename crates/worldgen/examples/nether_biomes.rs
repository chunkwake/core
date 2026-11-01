//! Биомы Нижнего мира: имена по чанку и сетевые ID.
//!
//! `nether_biomes <данные> <seed> [cx] [cz]`

use chunkwake_worldgen::Generator;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::path::PathBuf::from(&a[1]);
    let seed: i64 = a[2].parse().expect("seed");
    let cx: i32 = a.get(3).map_or(0, |v| v.parse().expect("cx"));
    let cz: i32 = a.get(4).map_or(0, |v| v.parse().expect("cz"));
    let regs = chunkwake_data::registries::RegistryData::load(&data).unwrap();
    let names: Vec<String> = regs
        .synchronized
        .iter()
        .find(|r| r.name == "worldgen/biome")
        .map(|r| r.entries.clone())
        .unwrap_or_default();
    let ids: std::collections::HashMap<String, u32> = names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.clone(), i as u32))
        .collect();
    let mut g = Generator::load_settings(&data, seed, None, "minecraft:nether").unwrap();
    g.bind_biomes(ids);
    let biomes = g.biomes(cx, cz).expect("biomes");
    let mut counts = std::collections::HashMap::new();
    for b in &biomes {
        *counts.entry(*b).or_insert(0usize) += 1;
    }
    let mut list: Vec<_> = counts.into_iter().collect();
    list.sort_by_key(|e| std::cmp::Reverse(e.1));
    println!("чанк ({cx},{cz}): клеток {}", biomes.len());
    for (id, n) in list {
        println!(
            "  {n:>4}  id {id}  {}",
            names.get(id as usize).map_or("?", String::as_str)
        );
    }
}
