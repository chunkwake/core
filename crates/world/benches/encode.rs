//! Кодирование тела чанка: карты высот, палитры секций и упаковка.
//!
//! Это путь `level_chunk_with_light`: тело собирается один раз на чанк и
//! сжимается отдельно, поэтому мс на чанк здесь умножаются на 96 чанков
//! пачки при входе игрока.

use chunkwake_world::ChunkColumn;
use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

const AIR: u32 = 0;
const STONE: u32 = 1;
const DIRT: u32 = 2;
const GRASS: u32 = 3;
const WATER: u32 = 4;
const COAL: u32 = 5;

/// Столбец с рельефом, рудами и водой: палитра каждой секции — 4–6 записей,
/// как у настоящего чанка (не однородный камень и не шум).
fn column() -> ChunkColumn {
    let mut states = vec![AIR; 24 * 4096];
    let at = |x: usize, y: usize, z: usize| (y * 16 + z) * 16 + x;
    for y in 0..60 {
        for z in 0..16 {
            for x in 0..16 {
                let state = match y {
                    0..=45 => STONE,
                    46..=52 => DIRT,
                    53..=56 => GRASS,
                    _ => AIR,
                };
                states[at(x, y, z)] = state;
            }
        }
    }
    // Руда в камне: без неё палитра секции — один камень.
    for y in (0..40).step_by(3) {
        for z in 0..16 {
            for x in (0..16).step_by(4) {
                states[at(x, y, z)] = COAL;
            }
        }
    }
    // Вода в углублениях поверхности: ещё одна запись палитры.
    for z in 0..8 {
        for x in 0..8 {
            states[at(x, 57, z)] = WATER;
        }
    }
    ChunkColumn::from_blocks(&states, 0, 8, 0, true)
}

fn encode(c: &mut Criterion) {
    let column = column();
    let mut group = c.benchmark_group("chunk_encode");
    group.bench_function("encode_body", |b| {
        b.iter(|| black_box(column.encode_body()));
    });
    group.bench_function("encode_biomes", |b| {
        b.iter(|| black_box(column.encode_biomes()));
    });
    group.finish();
}

criterion_group!(benches, encode);
criterion_main!(benches);
