//! Плотность из объёма против точечной: `vol_probe <данные> <seed> <cx> <cz> <x> <y> <z>`

use chunkwake_worldgen::Generator;
use std::collections::HashMap;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::path::PathBuf::from(&a[1]);
    let seed: i64 = a[2].parse::<i64>().unwrap();
    let cx: i32 = a[3].parse().unwrap();
    let cz: i32 = a[4].parse().unwrap();
    let x: i32 = a[5].parse().unwrap();
    let y: i32 = a[6].parse().unwrap();
    let z: i32 = a[7].parse().unwrap();

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
    println!("точечная  {}", g.debug_density(x, y, z));
    match g.debug_volume_density(cx, cz, x, y, z) {
        Some(d) => println!("из объёма {d}"),
        None => println!("вне объёма"),
    }
}
