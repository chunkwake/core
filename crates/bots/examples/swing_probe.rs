//! Взмах руки (`punch` → `swing_animation`) глазами второго игрока.
//!
//! `swing_probe <addr> <packets.json>`: бот A шлёт `punch` по расписанию,
//! бот B считает пришедшие `swing_animation` (сущность, рука, вид, длительность).
//! Ваниль (`SwingState.startIfAble`, взмах 6 тиков): два `punch` в одном тике —
//! оба рассылаются (`ticks == 0`), через 2 тика — нет (`ticks <= 3`), через
//! 6 тиков — да. Ожидание: 3 пакета у B, ни одного у A.
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use std::time::Duration;

async fn drain(c: &mut Client, ms: u64) {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
    while let Ok(Ok(_)) = tokio::time::timeout_at(deadline, c.next_event()).await {}
}

async fn joined(c: &mut Client) {
    loop {
        if let Event::Position(_) = c.next_event().await.unwrap() {
            break;
        }
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let punch = table
        .id(State::Play, Direction::Serverbound, "punch")
        .unwrap();
    let swing = table
        .id(State::Play, Direction::Clientbound, "swing_animation")
        .unwrap();

    let mut watcher = Client::connect(&a[1], "watcher", Ids::load(&table).unwrap(), 777, "26.3")
        .await
        .unwrap();
    watcher.capture = Some(swing);
    joined(&mut watcher).await;
    let watch = tokio::spawn(async move {
        drain(&mut watcher, 6000).await;
        watcher.captured
    });

    let mut puncher = Client::connect(&a[1], "puncher", Ids::load(&table).unwrap(), 777, "26.3")
        .await
        .unwrap();
    puncher.capture = Some(swing);
    joined(&mut puncher).await;
    drain(&mut puncher, 1500).await;

    let mut w = Writer::new();
    w.var_int(punch);
    // Два в одном тике, затем через ~2 тика, затем через ~6.
    for wait in [0, 100, 300] {
        drain(&mut puncher, wait).await;
        if wait == 0 {
            puncher.send_raw(&w.buf).await.unwrap();
        }
        puncher.send_raw(&w.buf).await.unwrap();
        println!("punch after {wait} ms");
    }
    drain(&mut puncher, 1500).await;

    let seen = watch.await.unwrap();
    println!("watcher swing_animation: {}", seen.len());
    for p in &seen {
        let mut r = chunkwake_protocol::Reader::new(p);
        let (e, hand, kind, dur) = (
            r.var_int().unwrap(),
            r.var_int().unwrap(),
            r.var_int().unwrap(),
            r.var_int().unwrap(),
        );
        println!("  entity={e} hand={hand} kind={kind} duration={dur}");
    }
    println!("puncher swing_animation: {}", puncher.captured.len());
}
