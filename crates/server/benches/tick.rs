//! Тик мира и цена горячих путей: `cargo bench -p chunkwake-server`.
//!
//! Строит настоящий `PlayPlan` из подготовленных данных и меряет `tick_once`
//! в нескольких состояниях: пустой мир, мир с мобами и предметами, тесное
//! стадо. Отдельно — один блок: чтение и запись. Без данных бенч пропускается
//! (в CI он не нужен).

use chunkwake_server::{Properties, build_play_plan};
use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::path::PathBuf;

/// Каталог подготовленных данных: `CHUNKWAKE_DATA` или общий кэш релиза.
fn data_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("CHUNKWAKE_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("../../target/release/generated/26.3"));
    dir.join("reports/light.csv").is_file().then_some(dir)
}

fn tick(c: &mut Criterion) {
    let Some(data) = data_dir() else {
        eprintln!("tick bench: нет данных в CHUNKWAKE_DATA, пропуск");
        return;
    };
    let props = match Properties::parse("") {
        Ok(p) => p,
        Err(e) => {
            eprintln!("tick bench: настройки не разобрались ({e}), пропуск");
            return;
        }
    };
    // Мир собирается в памяти: `level.dat` и регионы не нужны, чанки
    // генерируются по требованию. Каталог мира — временный.
    let world = std::env::temp_dir().join(format!("chunkwake-bench-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&world);
    let boot = match chunkwake_data::boot::load_or_build(&data) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("tick bench: данные не загрузились ({e}), пропуск");
            return;
        }
    };
    let built = match build_play_plan(&data, boot, &props, &world) {
        Ok((plan, _storage)) => plan,
        Err(e) => {
            eprintln!("tick bench: план не построился ({e}), пропуск");
            return;
        }
    };
    let plan = std::sync::Arc::new(built);
    plan.set_self_weak();
    plan.set_gen_pool(chunkwake_net::chunkgen::GenPool::new(
        chunkwake_net::chunkgen::workers(),
    ));

    let mut group = c.benchmark_group("tick");
    // Пустой мир: спавн загружен, игроков нет. Это и есть цель «idle».
    group.bench_function("idle", |b| b.iter(|| plan.tick_once()));
    let r = 6;
    plan.set_simulated((-r..=r).flat_map(|x| (-r..=r).map(move |z| (x, z))));

    // Блок: чтение из готового столбца и полная запись (свет, индексы, кадры).
    let origin = [8, 70, 8];
    group.bench_function("block_read", |b| {
        b.iter(|| black_box(chunkwake_net::play::bench::read(&plan, origin, 256)));
    });
    if let Some(stone) = chunkwake_net::play::bench::state(&plan, "minecraft:stone") {
        let mut up = false;
        group.bench_function("block_write", |b| {
            b.iter(|| {
                // Блок ставится и снимается по очереди: одинаковое состояние
                // `set_block` отсекает сразу (`old == state`), и мерился бы
                // пустой ход. Свет при этом пересчитывается, как в игре.
                up = !up;
                let state = if up { stone } else { 0 };
                black_box(chunkwake_net::play::bench::write(&plan, origin, state));
            });
        });
    }

    // Мир с мобами и предметами: тик проходит AI, физику и подбор целиком.
    // Это тот случай, где видна цена `block()` и пространственных запросов.
    let center = [8.0, 70.0, 8.0];
    let made = chunkwake_net::play::bench::populate(&plan, center, 200, 200);
    eprintln!("tick bench: население {made} мобов, 200 предметов");
    group.bench_function("mobs", |b| b.iter(|| plan.tick_once()));
    group.finish();

    // Тесное стадо: больше мобов на том же радиусе, без предметов.
    let heavy = chunkwake_net::play::bench::populate(&plan, center, 1000, 0);
    eprintln!("tick bench: население {heavy} мобов (heavy)");
    let mut heavy_group = c.benchmark_group("tick_heavy");
    heavy_group.sample_size(20);
    heavy_group.bench_function("mobs_1k", |b| b.iter(|| plan.tick_once()));
    heavy_group.finish();
    let _ = std::fs::remove_dir_all(&world);
}

criterion_group!(benches, tick);
criterion_main!(benches);
