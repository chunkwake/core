//! Замер того, что клиент видит от сервера: скорость мобов и частота тика.
//!
//! `cargo run -p chunkwake-bots --bin chunkwake-watch -- <addr> <packets.json> [seconds]`
//! Подключается одним ботом, собирает по сети появление и движение сущностей,
//! печатает путь в секунду для каждой и тип сущности.

use chunkwake_bots::{Client, Entities, Event, Ids};
use chunkwake_data::packets::PacketTable;
use std::time::{Duration, Instant};

const PROTOCOL: i32 = 777;
const VERSION: &str = "26.3";

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(addr) = args.get(1).cloned() else {
        eprintln!("usage: chunkwake-watch <addr> <packets.json> [seconds]");
        std::process::exit(2);
    };
    let table = args
        .get(2)
        .cloned()
        .unwrap_or("target/debug/generated/26.3/reports/packets.json".into());
    let seconds = args
        .get(3)
        .and_then(|n| n.parse::<u64>().ok())
        .unwrap_or(10)
        .max(1);
    let ids = match PacketTable::load(table.as_ref())
        .map_err(|e| e.to_string())
        .and_then(|t| Ids::load(&t))
    {
        Ok(ids) => ids,
        Err(e) => {
            eprintln!("error: {table}: {e}");
            std::process::exit(1);
        }
    };
    let mut client = match Client::connect(&addr, "watcher", ids, PROTOCOL, VERSION).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };
    println!("joined in {:?}; watching {} s", client.join_time, seconds);
    let mut entities = Entities::default();
    // Окно замера начинается после первого полученного тика, чтобы не
    // смешивать путь с догоняющими пакетами входа.
    let mut started: Option<Instant> = None;
    let waiting = Instant::now();
    // Игровое время: по нему видно, с какой частотой идёт тик сервера.
    let mut game: Option<(i64, Instant)> = None;
    let mut game_ticks: i64 = 0;
    let mut tick = tokio::time::interval(Duration::from_millis(50));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = tick.tick() => {
                entities.tick();
                if started.is_none() && !entities.is_empty() {
                    started = Some(Instant::now());
                    entities.reset_paths();
                }
                if started.is_some_and(|s| s.elapsed() >= Duration::from_secs(seconds)) {
                    break;
                }
                // Ни одной сущности за отведённое время: не виснем молча.
                if started.is_none() && waiting.elapsed() >= Duration::from_secs(seconds.max(10)) {
                    println!("no entities seen; nothing to measure");
                    break;
                }
            }
            event = client.next_event() => {
                match event {
                    Ok(e @ (Event::EntitySpawned { .. } | Event::EntityMoved { .. } | Event::EntityRemoved(_))) => {
                        entities.apply(&e);
                    }
                    Ok(Event::GameTime(t)) => {
                        if game.is_none() {
                            game = Some((t, Instant::now()));
                        } else {
                            game_ticks = t;
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("error: {e}");
                        break;
                    }
                }
            }
        }
    }
    if let Some((first, at)) = game {
        let secs = at.elapsed().as_secs_f64();
        if game_ticks > first {
            let ticks = game_ticks - first;
            println!(
                "server time: {ticks} ticks in {secs:.2} s = {:.2} ticks/s",
                ticks as f64 / secs
            );
        } else {
            println!("server time: no samples yet");
        }
    }
    let elapsed = started.map_or(0.0, |s| s.elapsed().as_secs_f64());
    println!("{elapsed:.2} s window; {} entities", entities.len());
    println!("id     type   path   blocks/s   moves   moves/s");
    let mut by_kind: std::collections::HashMap<i32, (usize, f64)> = Default::default();
    for e in entities.all() {
        let speed = e.path / elapsed.max(1e-9);
        println!(
            "{:<6} {:<6} {:>6.2} {:>10.3} {:>7} {:>9.1}",
            e.entity_id,
            e.kind,
            e.path,
            speed,
            e.moves,
            e.moves as f64 / elapsed.max(1e-9),
        );
        let slot = by_kind.entry(e.kind).or_insert((0, 0.0));
        slot.0 += 1;
        slot.1 += speed;
    }
    // Средняя по типу: с ней и надо сравнивать ванильную скорость вида.
    // Средняя по типу и распределение: в ванили большинство мобов стоят,
    // и «слишком быстро» обычно про самых активных, а не про среднее.
    let mut kinds: Vec<_> = by_kind.into_iter().collect();
    kinds.sort_by_key(|(kind, _)| *kind);
    for (kind, (count, total)) in kinds {
        let mut speeds: Vec<f64> = entities
            .all()
            .iter()
            .filter(|e| e.kind == kind)
            .map(|e| e.path / elapsed.max(1e-9))
            .collect();
        speeds.sort_by(|a, b| a.total_cmp(b));
        let median = speeds.get(speeds.len() / 2).copied().unwrap_or(0.0);
        let max = speeds.last().copied().unwrap_or(0.0);
        // Самый длинный шаг за тик переводим в блоки в секунду: это скорость,
        // с которой моб идёт, — её и видит игрок, когда моб рядом.
        let longest = entities
            .all()
            .iter()
            .filter(|e| e.kind == kind)
            .map(|e| e.longest_step)
            .fold(0.0f64, f64::max);
        println!(
            "type {kind}: {count} entities, mean {:.3}, median {median:.3}, max {max:.3} blocks/s, \
             walk step {:.4} blocks/tick ({:.2} blocks/s)",
            total / count as f64,
            longest,
            longest * 20.0
        );
    }
}
