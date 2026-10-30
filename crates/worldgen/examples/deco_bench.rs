//! Замер декорации по частям и на разном числе потоков:
//! `deco_bench <данные> <seed> [радиус]`.
//!
//! Печатает стоимость прото-чанка (шум, поверхность, карверы), стоимость
//! декорации фичами и ускорение одного общего `Overworld` на 1/2/4/все
//! потоках — так его использует пул генерации сервера. Запускать в release.

use chunkwake_worldgen::pipeline::Overworld;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = Path::new(&a[1]);
    let seed: i64 = a[2].parse().unwrap();
    let radius: i32 = a.get(3).map_or(6, |v| v.parse().unwrap());
    let registries = chunkwake_data::registries::RegistryData::load(data).unwrap();
    let biome_ids: HashMap<String, u32> = registries
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
    let blocks = chunkwake_data::world::Blocks::load(data).unwrap();
    let mut noise = chunkwake_worldgen::Generator::load_with_version(data, seed, None).unwrap();
    noise.bind_biomes(biome_ids.clone());
    let noise = Arc::new(noise);
    let state = |n: &str| blocks.default_state(n).ok().map(|s| s as u32);

    let plain = Overworld::new(noise.clone(), &state, &biome_ids).unwrap();
    let t = Instant::now();
    let mut n = 0;
    for cx in -radius..=radius {
        for cz in -radius..=radius {
            std::hint::black_box(plain.chunk(cx, cz));
            n += 1;
        }
    }
    let proto_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!("прото-чанк:            {:>8.2} ms/чанк ({n} чанков)", proto_ms / n as f64);

    let full = Overworld::new(noise.clone(), &state, &biome_ids)
        .unwrap()
        .with_features(data, &blocks.states(), &state, &biome_ids)
        .unwrap();
    let t = Instant::now();
    for cx in -radius..=radius {
        for cz in -radius..=radius {
            std::hint::black_box(full.chunk(cx, cz));
        }
    }
    let all_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!("с декорацией:          {:>8.2} ms/чанк", all_ms / n as f64);
    println!("декорация:             {:>8.2} ms/чанк", (all_ms - proto_ms) / n as f64);

    // Параллельность: та же пачка на N потоках. Один `Overworld` на всех —
    // именно так работает пул генерации сервера.
    let full = Arc::new(full);
    let coords: Vec<(i32, i32)> = (-radius..=radius)
        .flat_map(|x| (-radius..=radius).map(move |z| (x, z)))
        .collect();
    let threads = std::thread::available_parallelism().map_or(4, |p| p.get());
    for workers in [1usize, 2, 4, threads] {
        // Свежий экземпляр: кэш блоков не должен прощать повторный счёт.
        let ow = Arc::new(
            Overworld::new(noise.clone(), &state, &biome_ids)
                .unwrap()
                .with_features(data, &blocks.states(), &state, &biome_ids)
                .unwrap(),
        );
        let _ = &full;
        let t = Instant::now();
        let per = (coords.len() + workers - 1) / workers;
        std::thread::scope(|s| {
            for chunk in coords.chunks(per) {
                let ow = ow.clone();
                s.spawn(move || {
                    for &(x, z) in chunk {
                        std::hint::black_box(ow.chunk(x, z));
                    }
                });
            }
        });
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        println!(
            "{workers} потоков:            {:>8.2} ms/чанк (ускорение x{:.1})",
            ms / n as f64,
            all_ms / ms
        );
    }
}
