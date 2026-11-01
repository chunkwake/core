//! Замер полной загрузки территории так, как её делает сервер:
//! `stream_bench <данные> <seed> <радиус>`.
//!
//! Повторяет путь `send_chunks`: кольца от центра, для каждого — генерация,
//! свет волной, кодирование. Печатает время до первого кольца, время по
//! фазам и итог, чтобы видеть, где именно уходит время на r=8 и r=32.

use chunkwake_world::ChunkColumn;
use chunkwake_world::light::LightTable;
use chunkwake_world::{bits_for, light};
use chunkwake_worldgen::pipeline::Overworld;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = Path::new(&a[1]);
    let seed: i64 = a[2].parse().unwrap();
    let radius: i32 = a.get(3).map_or(8, |v| v.parse().unwrap());

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
    let rows = chunkwake_data::world::load_light_rows(data).unwrap();
    let table = LightTable::new(rows.light.iter().copied());
    let mut noise = chunkwake_worldgen::Generator::load_with_version(data, seed, None).unwrap();
    noise.bind_biomes(biome_ids.clone());
    let state = |n: &str| blocks.default_state(n).ok().map(|s| s as u32);
    let ow = Overworld::new(Arc::new(noise), &state, &biome_ids)
        .unwrap()
        .with_features(data, &blocks.states(), &state, &biome_ids)
        .unwrap();

    let block_bits = bits_for(blocks.state_count());
    let biome_bits = bits_for(biome_ids.len().max(1));

    // Кольца от центра.
    let mut rings: Vec<Vec<(i32, i32)>> = vec![Vec::new(); (radius + 1) as usize];
    for dx in -radius..=radius {
        for dz in -radius..=radius {
            let k = dx.abs().max(dz.abs()) as usize;
            rings[k].push((dx, dz));
        }
    }
    let total: usize = rings.iter().map(Vec::len).sum();
    println!("радиус {radius}: {total} чанков, {} колец", rings.len());

    let mut columns: HashMap<(i32, i32), ChunkColumn> = HashMap::new();
    let (mut gen_ms, mut light_ms, mut enc_ms) = (0.0f64, 0.0f64, 0.0f64);
    let started = Instant::now();
    let mut first_none = true;
    for (i, ring) in rings.iter().enumerate() {
        let t = Instant::now();
        for &(x, z) in ring {
            // `None` — чанк уже отдан: берём готовый столбец из `columns`.
            let Some(b) = ow.chunk(x, z) else {
                continue;
            };
            let col = match b.biomes {
                Some(ref biomes) => ChunkColumn::from_blocks_with_biomes(
                    &b.states, biomes, block_bits, biome_bits, true,
                ),
                None => ChunkColumn::from_blocks(&b.states, 0, block_bits, biome_bits, true),
            };
            columns.insert((x, z), col);
        }
        gen_ms += t.elapsed().as_secs_f64() * 1000.0;

        // Свет волной по кольцу и соседям (как `light_ring`).
        let t = Instant::now();
        let mut area: HashMap<(i32, i32), ChunkColumn> = HashMap::new();
        let origins: HashSet<(i32, i32)> = ring.iter().copied().collect();
        for &pos in ring {
            if let Some(c) = columns.get(&pos) {
                area.insert(pos, c.clone());
            }
            for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let n = (pos.0 + dx, pos.1 + dz);
                if !area.contains_key(&n)
                    && let Some(c) = columns.get(&n)
                {
                    area.insert(n, c.clone());
                }
            }
        }
        let touched = light::spread_wave(&mut area, &origins, &table);
        for pos in touched {
            if let Some(updated) = area.get(&pos)
                && let Some(dst) = columns.get_mut(&pos)
            {
                dst.sky_light = updated.sky_light.clone();
                dst.block_light = updated.block_light.clone();
            }
        }
        light_ms += t.elapsed().as_secs_f64() * 1000.0;

        // Кодирование кольца.
        let t = Instant::now();
        let mut bytes = 0usize;
        if i > 0 {
            for &(x, z) in &rings[i - 1] {
                if let Some(c) = columns.get(&(x, z)) {
                    bytes += c.encode_body().len();
                }
            }
        }
        enc_ms += t.elapsed().as_secs_f64() * 1000.0;
        let _ = bytes;
        if i == 1 && first_none {
            first_none = false;
            println!(
                "первое кольцо готово через {} ms",
                started.elapsed().as_millis()
            );
        }
        if i % 8 == 0 || i == rings.len() - 1 {
            println!(
                "кольцо {i:>2}: всего {:>6} ms | генерация {:>7.0} | свет {:>7.0} | кодирование {:>6.0}",
                started.elapsed().as_millis(),
                gen_ms,
                light_ms,
                enc_ms
            );
        }
    }
    println!(
        "итого {} ms: генерация {:.0}, свет {:.0}, кодирование {:.0}",
        started.elapsed().as_millis(),
        gen_ms,
        light_ms,
        enc_ms
    );
}
