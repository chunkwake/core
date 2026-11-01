//! Наш акуифер по столбцу: сравнение с выгрузкой ванили.
//!
//! `col_diff <данные> <seed> <cx> <cz> <vanilla.txt>`
//!
//! Ванильный файл: строки `y|S` (камень), `y|W` (вода), `y|L` (лава), `y|A` (воздух).

use chunkwake_worldgen::Fill;
use chunkwake_worldgen::Generator;
use std::collections::HashMap;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::path::PathBuf::from(&a[1]);
    let seed: i64 = a[2].parse::<i64>().unwrap();
    let cx: i32 = a[3].parse().unwrap();
    let cz: i32 = a[4].parse().unwrap();
    let vanilla = std::fs::read_to_string(&a[5]).unwrap();

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

    // Координаты берём из файла: в нём точки `x y z` не хранятся, поэтому
    // столбец задаётся аргументами.
    let x = a[6].parse::<i32>().unwrap() - cx * 16;
    let z = a[7].parse::<i32>().unwrap() - cz * 16;
    let mut bad = 0;
    println!(
        "столбец x={} z={} чанка ({cx},{cz})",
        cx * 16 + x,
        cz * 16 + z
    );
    for line in vanilla.lines() {
        let (ys, mark) = line.split_once('|').unwrap_or((line, "?"));
        let y: i32 = ys.trim().parse().unwrap_or(0);
        let ry = (y - g.min_y) as usize;
        let ours = t.fills[(ry * 16 + z as usize) * 16 + x as usize];
        let ours_mark = match ours {
            Fill::Solid => "S",
            Fill::Liquid | Fill::Water => "W",
            Fill::Lava => "L",
            Fill::Air => "A",
        };
        let flag = if ours_mark == mark {
            ""
        } else {
            "  <-- расхождение"
        };
        if !flag.is_empty() {
            bad += 1;
        }
        if y % 4 == 0 || !flag.is_empty() {
            println!("  y={y:<4} наше {ours_mark} ваниль {mark}{flag}");
        }
    }
    println!("расхождений в столбце: {bad}");
}
