//! Свет: пролив пачки новых чанков и пересчёт одного столбца.
//!
//! `spread_wave` — это путь `light_ring` при входе игрока (пачка до 96
//! чанков), `relight_column` — шаблон мира при старте и чанк без света в
//! файле. Мир строится в памяти: данные игры для замера не нужны.

use chunkwake_world::ChunkColumn;
use chunkwake_world::light::{self, LightTable};
use criterion::{Criterion, criterion_group, criterion_main};
use std::collections::{HashMap, HashSet};
use std::hint::black_box;

const AIR: u32 = 0;
const STONE: u32 = 1;
const GLOWSTONE: u32 = 2;

/// Состояния: воздух, камень, светящийся блок.
fn table() -> LightTable {
    LightTable::new([(0, 0, true), (0, 15, false), (15, 15, false)])
}

/// Столбец с рельефом: камень до `top`, воздух выше, редкие светильники в
/// пещерах. Разные `top` по столбцам дают наклонный рельеф, а он и есть
/// нагрузка для неба.
fn column(top: usize) -> ChunkColumn {
    let mut states = vec![AIR; 24 * 4096];
    for y in 0..top.min(24 * 16) {
        for z in 0..16 {
            for x in 0..16 {
                states[(y * 16 + z) * 16 + x] = STONE;
            }
        }
    }
    // Светильники в «пещерах»: без них блочный свет не проливается вовсе.
    for i in (0..top.saturating_sub(4)).step_by(7) {
        states[((i) * 16 + 8) * 16 + 8] = GLOWSTONE;
    }
    ChunkColumn::from_blocks(&states, 0, 2, 0, true)
}

fn light_bench(c: &mut Criterion) {
    let t = table();

    // Пачка как в `light_ring`: 96 чанков, все новые, одни origin.
    let mut group = c.benchmark_group("light");
    group.sample_size(20);
    group.bench_function("spread_wave_96", |b| {
        // Столбцы копируются на каждую итерацию: `spread_wave` пишет в них.
        let base: Vec<((i32, i32), ChunkColumn)> = (0..96)
            .map(|i| {
                let (dx, dz) = (i % 12, i / 12);
                ((dx, dz), column(60 + (dx as usize % 5) * 3))
            })
            .collect();
        b.iter(|| {
            let mut columns: HashMap<(i32, i32), ChunkColumn> =
                base.iter().map(|(p, c)| (*p, c.clone())).collect();
            let origins: HashSet<(i32, i32)> = base.iter().map(|(p, _)| *p).collect();
            black_box(light::spread_wave(&mut columns, &origins, &t));
        });
    });

    // Одиночный столбец без соседей: шаблон мира и чанк без сохранённого света.
    group.bench_function("relight_column", |b| {
        b.iter(|| {
            let mut c = column(60);
            light::relight_column(&mut c, &t);
            black_box(c);
        });
    });
    group.finish();
}

criterion_group!(benches, light_bench);
criterion_main!(benches);
