//! Сверка финальной плотности: наши числа против выгрузки ванили.
//!
//! `density_diff <данные> <seed> <points.txt> <vanilla.txt>`

use chunkwake_worldgen::Generator;
use std::collections::HashMap;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::path::PathBuf::from(&a[1]);
    let seed: i64 = a[2].parse::<i64>().unwrap();
    let points = std::fs::read_to_string(&a[3]).unwrap();
    let vanilla = std::fs::read_to_string(&a[4]).unwrap();

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

    let mut worst: Vec<(f32, String)> = Vec::new();
    let mut same = 0usize;
    let mut total = 0usize;
    for (p, v) in points.lines().zip(vanilla.lines()) {
        let f: Vec<&str> = p.split_whitespace().collect();
        if f.len() < 3 {
            continue;
        }
        let (x, y, z) = (
            f[0].parse::<i32>().unwrap(),
            f[1].parse::<i32>().unwrap(),
            f[2].parse::<i32>().unwrap(),
        );
        let want = v.trim().parse::<f32>().unwrap();
        let ours = g.debug_density(x, y, z);
        total += 1;
        if ours.to_bits() == want.to_bits() {
            same += 1;
        } else {
            worst.push((
                (ours - want).abs(),
                format!("{p}: наше {ours} ваниль {want}"),
            ));
        }
    }
    worst.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    println!("совпало побитово {same} из {total}");
    println!("худшие расхождения:");
    for (d, text) in worst.iter().take(8) {
        println!("  {d:.9}  {text}");
    }
}
