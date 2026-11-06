//! Отпечаток генерации: `gen_hash <данные> <seed> <длина> <ширина>`.
//!
//! Чанки строятся строго по порядку в одном потоке: сначала полоса
//! `x = 0..длина`, `z = 0..ширина`, затем обратно соседняя полоса
//! `z = ширина..ширина + ширина / 2`. Вторая полоса возвращается к краю
//! первой, где прото-чанки успели остыть и сжаться, — правка хранения
//! прото-чанков не должна менять ни одного блока. Печатает хеш FNV всех
//! состояний, блок-сущностей и пометок жидкости.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = Path::new(&a[1]);
    let seed: i64 = a[2].parse().unwrap();
    let len: i32 = a.get(3).map_or(120, |v| v.parse().unwrap());
    let width: i32 = a.get(4).map_or(20, |v| v.parse().unwrap());

    let registries = chunkwake_data::registries::RegistryData::load(data).unwrap();
    let biome_ids: HashMap<String, u32> = registries
        .synchronized
        .iter()
        .find(|r| r.name == "worldgen/biome")
        .map(|r| r.entries.iter().enumerate().map(|(i, n)| (n.clone(), i as u32)).collect())
        .unwrap_or_default();
    let blocks = chunkwake_data::world::Blocks::load(data).unwrap();
    let mut noise = chunkwake_worldgen::Generator::load_with_version(data, seed, None).unwrap();
    noise.bind_biomes(biome_ids.clone());
    let state = |n: &str| blocks.default_state(n).ok().map(|s| s as u32);
    let world = chunkwake_worldgen::pipeline::Overworld::new(Arc::new(noise), &state, &biome_ids)
        .unwrap()
        .with_features(data, &blocks.states(), &state, &biome_ids)
        .unwrap();

    let mut order: Vec<(i32, i32)> = Vec::new();
    for x in 0..len {
        for z in 0..width {
            order.push((x, z));
        }
    }
    for x in (0..len).rev() {
        for z in width..width + width / 2 {
            order.push((x, z));
        }
    }
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |v: u64| {
        h ^= v;
        h = h.wrapping_mul(0x0100_0000_01b3);
    };
    let started = std::time::Instant::now();
    for (i, &(x, z)) in order.iter().enumerate() {
        let Some(c) = world.chunk(x, z) else {
            println!("chunk {x} {z}: already given");
            continue;
        };
        mix(x as u64);
        mix(z as u64);
        for s in &c.states {
            mix(u64::from(*s));
        }
        for (p, id, _) in &c.block_entities {
            mix(p[0] as u64 ^ (p[1] as u64) << 20 ^ (p[2] as u64) << 40);
            for b in id.bytes() {
                mix(u64::from(b));
            }
        }
        for p in &c.fluid_posts {
            mix(p[0] as u64 ^ (p[1] as u64) << 20 ^ (p[2] as u64) << 40);
        }
        if i as i32 == len * width {
            eprintln!("first strip done: {}", world.mem_stats());
        }
        if i % 500 == 0 {
            eprintln!("{i}/{} {:.0} s", order.len(), started.elapsed().as_secs_f64());
        }
    }
    println!("chunks {} hash {h:016x}", order.len());
}
