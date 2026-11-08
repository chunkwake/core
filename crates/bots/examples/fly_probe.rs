//! Замер подгрузки чанков в полёте: бот в творческом летит по прямой и раз в
//! секунду печатает, сколько чанков пришло и какая доля квадрата обзора
//! вокруг него уже есть у клиента — «дыры» в мире числом.
//!
//! `fly_probe <addr> <packets.json> <скорость блок/с> <секунды> [обзор]`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let speed: f64 = a[3].parse().unwrap();
    let secs: u64 = a[4].parse().unwrap();
    let view: i32 = a.get(5).and_then(|s| s.parse().ok()).unwrap_or(12);
    let cmd = table
        .id(State::Play, Direction::Serverbound, "chat_command")
        .unwrap();
    let mut c = Client::connect(&a[1], "flyer", ids, 777, "26.3")
        .await
        .unwrap();
    let mut pos = loop {
        if let Ok(Event::Position(p)) = c.next_event().await {
            break p;
        }
    };
    let mut w = Writer::new();
    w.var_int(cmd).string("gamemode creative");
    c.send_raw(&w.buf).await.unwrap();
    pos[1] = 150.0;
    let mut tick = tokio::time::interval(Duration::from_millis(50));
    let start = Instant::now();
    let mut last = Instant::now();
    let mut chunks_last = 0u64;
    let mut worst = 1.0f64;
    let mut sum = 0.0;
    let mut samples = 0;
    loop {
        tokio::select! {
            _ = tick.tick() => {
                pos[0] += speed / 20.0;
                c.move_to(pos).await.unwrap();
                if last.elapsed() >= Duration::from_secs(1) {
                    last = Instant::now();
                    let (cx, cz) = ((pos[0] as i32) >> 4, (pos[2] as i32) >> 4);
                    let r = view - 2;
                    let mut total = 0;
                    let mut got = 0;
                    for dx in -r..=r {
                        for dz in -r..=r {
                            total += 1;
                            if c.loaded.contains(&(cx + dx, cz + dz)) {
                                got += 1;
                            }
                        }
                    }
                    let cover = got as f64 / total as f64;
                    let t = start.elapsed().as_secs();
                    if t >= 3 {
                        worst = worst.min(cover);
                        sum += cover;
                        samples += 1;
                    }
                    println!(
                        "{t:>3} с  x={:>6.0}  чанков/с {:>4}  обзор {:>5.1}%",
                        pos[0],
                        c.chunks_in - chunks_last,
                        cover * 100.0
                    );
                    chunks_last = c.chunks_in;
                    if t >= secs {
                        break;
                    }
                }
            }
            event = c.next_event() => {
                event.unwrap();
            }
        }
    }
    println!(
        "итог: средний обзор {:.1}%, худший {:.1}%, чанков {}",
        sum / samples.max(1) as f64 * 100.0,
        worst * 100.0,
        c.chunks_in
    );
}
