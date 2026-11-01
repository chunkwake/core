//! Один чанк: наши `Fill` из `terrain` против ванильного акуифера.
//!
//! `one_chunk <данные> <seed> <cx> <cz> <vanilla.txt>`
//!
//! Печатает гистограмму расхождений и первые координаты.
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

    let mut hist: HashMap<(String, String, i32), usize> = HashMap::new();
    let mut shown = 0;
    for (i, line) in vanilla.lines().enumerate() {
        let (ys, mark) = line.split_once('|').unwrap_or((line, "?"));
        let y: i32 = ys.trim().parse().unwrap_or(0);
        // Порядок файла: x, затем z, затем y — как генерировали.
        let idx = i % 16;
        let _ = idx;
        let ry = (y - g.min_y) as usize;
        let _ = ry;
        let _ = mark;
    }
    // Проще: файл сгенерирован в порядке x → z → y.
    let mut k = 0usize;
    for x in 0..16usize {
        for z in 0..16usize {
            for y in -64..128i32 {
                let line = vanilla.lines().nth(k).unwrap_or("");
                k += 1;
                let (ys, mark) = line.split_once('|').unwrap_or((line, "?"));
                let vy: i32 = ys.trim().parse().unwrap_or(i32::MIN);
                let their = if mark.starts_with("Block{minecraft:water") {
                    "W"
                } else if mark.starts_with("Block{minecraft:lava") {
                    "L"
                } else if mark.starts_with("Block{minecraft:air") {
                    "A"
                } else {
                    mark
                };
                let ours = t.fills[((y - g.min_y) as usize * 16 + z) * 16 + x];
                let ours_mark = match ours {
                    Fill::Solid => "S",
                    Fill::Liquid | Fill::Water => "W",
                    Fill::Lava => "L",
                    Fill::Air => "A",
                };
                if vy != y {
                    println!("порядок файла не совпал: ждали {y}, в файле {vy}");
                    return;
                }
                if ours_mark != their {
                    *hist
                        .entry((ours_mark.into(), their.into(), y >> 5))
                        .or_insert(0) += 1;
                    if shown < 6 {
                        shown += 1;
                        println!(
                            "  ({}, {y}, {}) наше {ours_mark} ваниль {their}",
                            cx * 16 + x as i32,
                            cz * 16 + z as i32
                        );
                    }
                }
            }
        }
    }
    let total: usize = hist.values().sum();
    println!("расходится {total}");
    let mut list: Vec<_> = hist.into_iter().collect();
    list.sort_by_key(|e| std::cmp::Reverse(e.1));
    for ((o, v, band), n) in list.iter().take(6) {
        println!(
            "  {n:>6} наше {o} ваниль {v} y {}..{}",
            band * 32,
            band * 32 + 31
        );
    }
}
