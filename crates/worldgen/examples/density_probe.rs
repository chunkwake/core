//! Плотность в одной точке: наша и какая должна быть по ванили.
//!
//! `density_probe <данные> <seed> <x> <y> <z> <плотность-по-ванили>`

use chunkwake_worldgen::Generator;
use std::collections::HashMap;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::path::PathBuf::from(&a[1]);
    let seed: i64 = a[2].parse::<i64>().unwrap();
    let x: i32 = a[3].parse().unwrap();
    let y: i32 = a[4].parse().unwrap();
    let z: i32 = a[5].parse().unwrap();

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

    // Считаем финальную плотность и воду ровно там, куда смотрит ванила.
    let d = g.debug_density(x, y, z);
    println!("плотность в ({x},{y},{z}) = {d}");
    let t = g.terrain(x >> 4, z >> 4);
    let ry = (y - g.min_y) as usize;
    let rx = (x & 15) as usize;
    let rz = (z & 15) as usize;
    let idx = (ry * 16 + rz) * 16 + rx;
    println!("fill = {:?}", t.fills.get(idx));
}
