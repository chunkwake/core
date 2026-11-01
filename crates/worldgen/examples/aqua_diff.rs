//! Сверка входов акуифера с ванилью.
//!
//! `aqua_diff <данные> <seed> <points.txt> <vanilla.txt>`
//!
//! Ванильная строка: `плотность|chunk_surface_level|barrier|floodedness|spread|lava|exclusion`.

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

    let names = [
        "плотность",
        "surface_level",
        "barrier",
        "floodedness",
        "spread",
        "lava",
        "exclusion",
    ];
    let mut same = [0usize; 7];
    let mut total = 0usize;
    let mut worst: Vec<(f32, String)> = Vec::new();

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
        let want: Vec<f32> = v
            .trim()
            .split('|')
            .filter_map(|s| s.parse::<f32>().ok())
            .collect();
        if want.len() < 7 {
            continue;
        }
        total += 1;
        let ours = g.debug_aquifer(x, y, z);
        for (i, (o, w)) in ours.iter().zip(want.iter()).enumerate() {
            if o.to_bits() == w.to_bits() {
                same[i] += 1;
            } else if i > 0 {
                worst.push((
                    (o - w).abs(),
                    format!("{} {p}: наше {o} ваниль {w}", names[i]),
                ));
            }
        }
    }
    println!("точек {total}");
    for (i, n) in names.iter().enumerate() {
        println!("  {n:<14} совпало {} из {total}", same[i]);
    }
    worst.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    println!("худшие:");
    for (d, t) in worst.iter().take(6) {
        println!("  {d:.6}  {t}");
    }
}
