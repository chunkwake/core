//! Сколько чанков в секунду отдаёт сервер: `chunk_rate <данные> <seed> <радиус> <секунды>`.
//!
//! Мерит именно то, что видит игрок: генерation + свет + кодирование в пуле,
//! как в `play/stream.rs`. Нужен, чтобы проверять правки генерации на живом
//! конвейере, а не на одном прото-чанке.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = Path::new(&a[1]);
    let seed: i64 = a[2].parse().unwrap();
    let radius: i32 = a.get(3).map_or(24, |v| v.parse().unwrap());
    let seconds: u64 = a.get(4).map_or(40, |v| v.parse().unwrap());

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
    let state = |n: &str| blocks.default_state(n).ok().map(|s| s as u32);
    let workers = chunkwake_worldgen::pipeline::Overworld::new(Arc::new(noise), &state, &biome_ids)
        .unwrap()
        .with_features(data, &blocks.states(), &state, &biome_ids)
        .unwrap();

    // Кольца от центра, как при входе игрока: ближние первыми.
    let mut rings: Vec<Vec<(i32, i32)>> = vec![Vec::new(); (radius + 1) as usize];
    for dx in -radius..=radius {
        for dz in -radius..=radius {
            let k = dx.abs().max(dz.abs()) as usize;
            rings[k].push((dx, dz));
        }
    }
    let total: usize = rings.iter().map(Vec::len).sum();
    println!("радиус {radius}: {total} чанков, лимит {seconds} с");

    let ow = Arc::new(workers);
    let done = Arc::new(AtomicUsize::new(0));
    let started = Instant::now();
    let deadline = started + Duration::from_secs(seconds);
    let threads: usize = std::thread::available_parallelism().map_or(4, |p| p.get()) - 1;
    // Очередь чанков: один общий список, потоки берут следующий индекс сами.
    let queue: Arc<Vec<(i32, i32)>> = Arc::new(rings.into_iter().flatten().collect());
    let next = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();
    for _ in 0..threads.max(1) {
        let ow = ow.clone();
        let done = done.clone();
        let queue = queue.clone();
        let next = next.clone();
        handles.push(std::thread::spawn(move || {
            loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(&(x, z)) = queue.get(i) else { break };
                if Instant::now() >= deadline {
                    break;
                }
                // Полный путь: декорация и фичи, как у сервера.
                std::hint::black_box(ow.chunk(x, z));
                done.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }
    for h in handles {
        let _ = h.join();
    }
    let elapsed = started.elapsed().as_secs_f64();
    let n = done.load(Ordering::Relaxed);
    println!(
        "{n} чанков за {elapsed:.1} с: {:.1} чанков/с на {threads} потоках",
        n as f64 / elapsed
    );
}
