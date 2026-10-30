//! Один тик мира: `cargo bench -p chunkwake-server`.
//!
//! Строит настоящий `PlayPlan` из подготовленных данных и меряет `tick_once`
//! в трёх состояниях: пустой мир, мир с редстоуном и мир с мобами. Без
//! данных бенч пропускается — в CI он не нужен.

use chunkwake_server::{Properties, build_play_plan};
use criterion::{Criterion, criterion_group, criterion_main};
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
    // Мир с мобами и предметами: тик проходит AI, физику и подбор целиком.
    // Это тот случай, где видна цена `block()` и пространственных запросов.
    let r = 6;
    plan.set_simulated((-r..=r).flat_map(|x| (-r..=r).map(move |z| (x, z))));
    let center = [8.0, 70.0, 8.0];
    let made = chunkwake_net::play::bench::populate(&plan, center, 200, 200);
    eprintln!("tick bench: население {made} мобов, 200 предметов");
    group.bench_function("mobs", |b| b.iter(|| plan.tick_once()));
    group.finish();
    let _ = std::fs::remove_dir_all(&world);
}

criterion_group!(benches, tick);
criterion_main!(benches);
