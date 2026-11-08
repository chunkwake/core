//! Repro: teleport into the Nether at a given point by command, wait for the
//! chunks around it to generate, then save the world.
//!
//! `nether_visit <addr> <generated-dir> <name> <x> <y> <z> [secs]`
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
    let content_id = table
        .id(State::Play, Direction::Clientbound, "container_set_content")
        .unwrap();
    let secs: u64 = a.get(7).and_then(|s| s.parse().ok()).unwrap_or(30);
    let mut c = Client::connect(&a[1], &a[3], ids, 777, "26.3")
        .await
        .unwrap();
    loop {
        if let Ok(Event::Position(_)) = c.next_event().await {
            break;
        }
    }
    let send = |line: String| {
        let mut w = Writer::new();
        w.var_int(cmd_id).string(&line);
        w.buf
    };
    for line in [
        "gamemode creative".to_string(),
        "give @s minecraft:diamond 5".to_string(),
        format!(
            "execute in minecraft:the_nether run tp @s {} {} {}",
            a[4], a[5], a[6]
        ),
    ] {
        c.send_raw(&send(line)).await.unwrap();
    }
    // Сколько раз пришло окно инвентаря после переход.
    c.capture = Some(content_id);
    let t0 = Instant::now();
    let mut loaded = false;
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end {
        match tokio::time::timeout(Duration::from_millis(200), c.next_event()).await {
            Ok(Ok(Event::Position(p))) => {
                println!("{:5.1}s pos {p:?}", t0.elapsed().as_secs_f32());
                if !loaded {
                    loaded = true;
                    let mut w = Writer::new();
                    w.var_int(loaded_id);
                    c.send_raw(&w.buf).await.unwrap();
                }
            }
            Ok(Err(e)) => {
                println!("err {e}");
                return;
            }
            _ => {}
        }
    }
    println!(
        "chunks {}, inventory windows {}",
        c.chunks_in,
        c.captured.len()
    );
    c.send_raw(&send("save-all".to_string())).await.unwrap();
    tokio::time::sleep(Duration::from_secs(5)).await;
}
