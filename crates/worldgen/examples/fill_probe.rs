//! Рельеф без фич и без стадии поверхности: какие классы клеток даёт шум.

use chunkwake_worldgen::Fill;
use chunkwake_worldgen::Generator;
use std::collections::HashMap;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::path::PathBuf::from(&a[1]);
    let seed: i64 = a[2].parse::<i64>().unwrap();
    let settings = a.get(3).map_or("minecraft:overworld", String::as_str);
    let cx: i32 = a.get(4).map_or(0, |v| v.parse().unwrap());
    let cz: i32 = a.get(5).map_or(0, |v| v.parse().unwrap());

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
    let mut noise = Generator::load_settings(&data, seed, None, settings).unwrap();
    noise.bind_biomes(biomes);
    let t = noise.terrain(cx, cz);
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for f in &t.fills {
        let k = match f {
            Fill::Air => "air",
            Fill::Solid => "solid",
            Fill::Liquid => "liquid(solid->default_fluid)",
            Fill::Water => "water",
            Fill::Lava => "lava",
        };
        *counts.entry(k).or_insert(0) += 1;
    }
    println!("{settings} чанк ({cx},{cz}): {counts:?}");
    // Вода по высоте: сколько её выше моря.
    let sea = noise.sea_level;
    let mut above = 0;
    let mut below = 0;
    let h = noise.height as usize;
    for y in 0..h {
        for z in 0..16usize {
            for x in 0..16usize {
                if matches!(t.fills[(y * 16 + z) * 16 + x], Fill::Water) {
                    if noise.min_y + y as i32 >= sea {
                        above += 1;
                    } else {
                        below += 1;
                    }
                }
            }
        }
    }
    println!("  вода: выше моря {above}, ниже {below}");
}
