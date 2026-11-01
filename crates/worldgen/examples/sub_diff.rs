//! Массовая сверка акуифера: наши `Fill` против ванильных `computeSubstance`.
//!
//! `sub_diff <данные> <seed> <cx> <cz> <points.txt> <vanilla.txt>`
//!
//! Ванильный файл: строка на точку, `y|S|W|L|A`.

use chunkwake_worldgen::Fill;
use chunkwake_worldgen::Generator;
use std::collections::HashMap;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::path::PathBuf::from(&a[1]);
    let seed: i64 = a[2].parse::<i64>().unwrap();
    let cx: i32 = a[3].parse().unwrap();
    let cz: i32 = a[4].parse().unwrap();
    let points = std::fs::read_to_string(&a[5]).unwrap();
    let vanilla = std::fs::read_to_string(&a[6]).unwrap();

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
    let t = g.terrain(cx, cz);

    let mut total = 0usize;
    let mut bad = 0usize;
    let mut shown = 0usize;
    for (p, v) in points.lines().zip(vanilla.lines()) {
        let f: Vec<&str> = p.split_whitespace().collect();
        let v: Vec<&str> = v.trim().split('|').collect();
        if f.len() < 3 || v.len() < 2 {
            continue;
        }
        let (x, y, z) = (
            f[0].parse::<i32>().unwrap(),
            f[1].parse::<i32>().unwrap(),
            f[2].parse::<i32>().unwrap(),
        );
        let ours =
            t.fills[((y - g.min_y) as usize * 16 + (z & 15) as usize) * 16 + (x & 15) as usize];
        let ours_mark = match ours {
            Fill::Solid => "S",
            Fill::Liquid | Fill::Water => "W",
            Fill::Lava => "L",
            Fill::Air => "A",
        };
        total += 1;
        // Ванила печатает полное имя состояния (`Block{minecraft:water}[level=0]`),
        // когда блок не опознан как источник; приводим к одной букве.
        let their = v[1];
        let their = if their.starts_with("Block{minecraft:water") {
            "W"
        } else if their.starts_with("Block{minecraft:lava") {
            "L"
        } else if their.starts_with("Block{minecraft:air") {
            "A"
        } else {
            their
        };
        if ours_mark != their {
            bad += 1;
            if shown < 12 {
                shown += 1;
                println!("  ({x},{y},{z}) наше {ours_mark} ваниль {their}");
            }
        }
    }
    println!("акуифер: расходится {bad} из {total}");
}
