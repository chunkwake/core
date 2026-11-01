//! Отладка акуифера: `skip_above_y` и границы сетки.
//!
//! `aqua_debug <данные> <seed> <cx> <cz>`

use chunkwake_worldgen::Generator;
use std::collections::HashMap;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::path::PathBuf::from(&a[1]);
    let seed: i64 = a[2].parse::<i64>().unwrap();
    let cx: i32 = a[3].parse().unwrap();
    let cz: i32 = a[4].parse().unwrap();

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
    g.bind_biomes(biomes);
    println!("{}", g.debug_aquifer_skip(cx, cz));
}
