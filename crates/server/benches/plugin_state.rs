//! Снимок игрока для плагинов: `player_state` и отпечаток состояния.
//!
//! Это путь `Player.getHealth()`/`getLocation()`/`getInventory()`: Java
//! зовёт его на каждый геттер. Замер показывает, что отдаёт кэш текста
//! и сколько стоит сам отпечаток.

use chunkwake_net::Uuid;
use chunkwake_server::{Properties, build_play_plan};
use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::path::PathBuf;

fn data_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("CHUNKWAKE_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("../../target/release/generated/26.3"));
    dir.join("reports/light.csv").is_file().then_some(dir)
}

fn state(c: &mut Criterion) {
    let Some(data) = data_dir() else {
        eprintln!("state bench: нет данных, пропуск");
        return;
    };
    let props = Properties::parse("").expect("настройки");
    let world = std::env::temp_dir().join(format!("chunkwake-bench-state-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&world);
    let boot = chunkwake_data::boot::load_or_build(&data).expect("данные");
    let (plan, _storage) = build_play_plan(&data, boot, &props, &world).expect("план");
    let uuid = Uuid::from_u128(0xfeed_face);
    plan.present_player(uuid, "Bench");

    let mut group = c.benchmark_group("plugin_state");
    // Отпечаток: дешёвый вызов, который Java делает на каждый геттер.
    group.bench_function("fingerprint", |b| {
        b.iter(|| black_box(plan.state_version(uuid)));
    });
    // Полный снимок: текст 41 строки инвентаря, кэшируется по отпечатку.
    group.bench_function("player_state", |b| {
        b.iter(|| black_box(plan.player_state(uuid)));
    });
    group.finish();
    let _ = std::fs::remove_dir_all(&world);
}

criterion_group!(benches, state);
criterion_main!(benches);
