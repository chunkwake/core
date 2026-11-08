//! Repro: build a nether portal around the bot by commands, go through it,
//! step out and back in on the other side, and log respawns, positions and
//! chunk counts.
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let cmd_id = table
        .id(State::Play, Direction::Serverbound, "chat_command")
        .unwrap();
    let loaded_id = table
        .id(State::Play, Direction::Serverbound, "player_loaded")
        .unwrap();
    let respawn_id = table
        .id(State::Play, Direction::Clientbound, "respawn")
        .unwrap();
    let secs: u64 = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(60);
    let mut c = Client::connect(&a[1], &a[3], ids, 777, "26.3")
        .await
        .unwrap();
    let t0 = Instant::now();
    let p = loop {
        if let Ok(Event::Position(p)) = c.next_event().await {
            break p;
        }
    };
    println!("start {p:?}");
    let (x, y, z) = (
        p[0].floor() as i32,
        p[1].floor() as i32,
        p[2].floor() as i32,
    );
    for line in [
        format!("gamemode {}", a.get(5).map_or("survival", |s| s)),
        format!(
            "fill {} {} {} {} {} {} air",
            x - 3,
            y,
            z - 2,
            x + 4,
            y + 4,
            z + 2
        ),
        format!(
            "fill {} {} {} {} {} {} obsidian",
            x - 1,
            y - 1,
            z,
            x + 2,
            y + 3,
            z
        ),
        format!(
            "fill {} {} {} {} {} {} nether_portal[axis=x]",
            x,
            y,
            z,
            x + 1,
            y + 2,
            z
        ),
    ] {
        let mut w = Writer::new();
        w.var_int(cmd_id).string(&line);
        c.send_raw(&w.buf).await.unwrap();
    }
    c.capture = Some(respawn_id);
    let mut since = c.chunks_in;
    let mut respawned_at: Option<Instant> = None;
    let mut arrival: Option<[f64; 3]> = None;
    let mut stepped = 0;
    let end = Instant::now() + Duration::from_secs(secs);
    let mut last_report = Instant::now();
    while Instant::now() < end {
        match tokio::time::timeout(Duration::from_millis(100), c.next_event()).await {
            Ok(Ok(Event::Position(p))) => {
                println!("{:5.1}s pos {p:?}", t0.elapsed().as_secs_f32());
                if respawned_at.is_some() && arrival.is_none() {
                    arrival = Some(p);
                    let mut w = Writer::new();
                    w.var_int(loaded_id);
                    c.send_raw(&w.buf).await.unwrap();
                }
            }
            Ok(Err(e)) => {
                println!("err {e}");
                break;
            }
            _ => {}
        }
        if !c.captured.is_empty() {
            c.captured.clear();
            println!(
                "{:5.1}s RESPAWN (chunks before {})",
                t0.elapsed().as_secs_f32(),
                c.chunks_in - since
            );
            since = c.chunks_in;
            respawned_at = Some(Instant::now());
            arrival = None;
            stepped = 0;
        }
        // Step out of the exit portal and back in, to come home.
        if let (Some(t), Some(p)) = (respawned_at, arrival) {
            if stepped == 0 && t.elapsed() > Duration::from_secs(3) {
                c.move_to([p[0], p[1], p[2] + 2.0]).await.unwrap();
                stepped = 1;
                println!("{:5.1}s stepped out", t0.elapsed().as_secs_f32());
            } else if stepped == 1 && t.elapsed() > Duration::from_secs(5) {
                c.move_to(p).await.unwrap();
                stepped = 2;
                println!("{:5.1}s stepped back in", t0.elapsed().as_secs_f32());
            }
        }
        if last_report.elapsed() > Duration::from_secs(3) {
            last_report = Instant::now();
            println!(
                "{:5.1}s chunks since respawn {}",
                t0.elapsed().as_secs_f32(),
                c.chunks_in - since
            );
        }
    }
}
