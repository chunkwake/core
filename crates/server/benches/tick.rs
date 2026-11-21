//! Тик мира и цена горячих путей: `cargo bench -p chunkwake-server`.
//!
//! Строит настоящий `PlayPlan` из подготовленных данных и меряет `tick_once`
//! в нескольких состояниях: пустой мир, мир с мобами и предметами, тесное
//! стадо. Отдельно — один блок: чтение и запись. Без данных бенч пропускается
//! (в CI он не нужен).

use chunkwake_server::{Properties, build_play_plan};
use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Каталог подготовленных данных: `CHUNKWAKE_DATA` или общий кэш релиза.
fn data_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("CHUNKWAKE_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("../../target/release/generated/26.3"));
    dir.join("reports/light.csv").is_file().then_some(dir)
}

/// Собирает `PlayPlan` с настройками `props` во временном мире `tag`.
fn build(
    data: &Path,
    props: &str,
    tag: &str,
) -> Option<(Arc<chunkwake_net::play::PlayPlan>, PathBuf)> {
    let props = match Properties::parse(props) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("tick bench: настройки не разобрались ({e}), пропуск");
            return None;
        }
    };
    // Мир собирается в памяти: `level.dat` и регионы не нужны, чанки
    // генерируются по требованию. Каталог мира — временный.
    let world = std::env::temp_dir().join(format!("chunkwake-bench-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&world);
    let boot = match chunkwake_data::boot::load_or_build(data) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("tick bench: данные не загрузились ({e}), пропуск");
            return None;
        }
    };
    let built = match build_play_plan(data, boot, &props, &world) {
        Ok((plan, _storage)) => plan,
        Err(e) => {
            eprintln!("tick bench: план не построился ({e}), пропуск");
            return None;
        }
    };
    let plan = Arc::new(built);
    plan.set_self_weak();
    plan.set_gen_pool(chunkwake_net::chunkgen::GenPool::new(
        chunkwake_net::chunkgen::workers(),
    ));
    Some((plan, world))
}

fn tick(c: &mut Criterion) {
    let Some(data) = data_dir() else {
        eprintln!("tick bench: нет данных в CHUNKWAKE_DATA, пропуск");
        return;
    };
    let Some((plan, world)) = build(&data, "", "noise") else {
        return;
    };

    let mut group = c.benchmark_group("tick");
    // Машина разработчика шумит: рядом с открытой IDE (msedgewebview2, Cursor,
    // javaw держат ядра) один и тот же код давал разброс в 2–4 раза. Поэтому
    // минимум — «сколько коду нужно, когда ему дали ядро», — и есть главная
    // цифра; её же печатает `min_time_ns` ниже.
    group.warm_up_time(std::time::Duration::from_millis(500));
    group.measurement_time(std::time::Duration::from_secs(2));
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

    // `/fill` 32×16×32 (16 384 блока) камнем и обратно воздухом.
    if let Some(stone) = chunkwake_net::play::bench::state(&plan, "minecraft:stone") {
        let mut up = false;
        group.sample_size(10);
        group.bench_function("fill_16k", |b| {
            b.iter(|| {
                up = !up;
                let state = if up { stone } else { 0 };
                black_box(chunkwake_net::play::bench::fill(
                    &plan,
                    [-16, 100, -16],
                    [15, 115, 15],
                    state,
                ));
            });
        });
        group.sample_size(100);
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

    // Случайные тики при `randomTickSpeed` 4096 по 13 × 13 чанкам спавна
    // (строка «400 тиков при 4096» в README мерит то же на живом сервере).
    if chunkwake_net::play::bench::set_rule(&plan, "random_tick_speed", 4096) {
        let mut rt = c.benchmark_group("random_ticks");
        rt.sample_size(10);
        rt.measurement_time(std::time::Duration::from_secs(5));
        rt.bench_function("speed_4096", |b| {
            b.iter(|| chunkwake_net::play::bench::random_ticks(&plan));
        });
        rt.finish();
        chunkwake_net::play::bench::set_rule(&plan, "random_tick_speed", 3);
    }
    let _ = std::fs::remove_dir_all(&world);
    drop(plan);

    // Сетки коров с шагом в блок на плоском мире — как профиль `cows` в
    // `chunkwake-lab` (50 × 50 у ванили даёт 120–221 мс на тик). Каждая сетка —
    // на своём свежем мире, чтобы стада не складывались.
    let mut grid_group = c.benchmark_group("tick_grid");
    grid_group.sample_size(10);
    grid_group.warm_up_time(std::time::Duration::from_secs(1));
    grid_group.measurement_time(std::time::Duration::from_secs(5));
    for side in [50usize, 100] {
        let Some((plan, world)) = build(&data, "level-type=minecraft:flat", &format!("grid{side}"))
        else {
            continue;
        };
        let r = 8;
        plan.set_simulated((-r..=r).flat_map(|x| (-r..=r).map(move |z| (x, z))));
        let made = chunkwake_net::play::bench::populate_grid(&plan, [0.0, -60.0, 0.0], side);
        eprintln!("tick bench: сетка {made} коров");
        // Коровы падают на землю и расходятся: разминка до установившегося режима.
        for _ in 0..100 {
            plan.tick_once();
        }
        grid_group.bench_function(format!("cows_{}", side * side), |b| {
            b.iter(|| plan.tick_once())
        });
        drop(plan);
        let _ = std::fs::remove_dir_all(&world);
    }
    grid_group.finish();
}

criterion_group!(benches, tick);
criterion_main!(benches);
