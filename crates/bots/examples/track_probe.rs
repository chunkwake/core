//! Сущность видна только в отправленном чанке (`ChunkMap.isChunkTracked`).
//!
//! `track_probe <addr> <packets.json>`: бот `resident` уже в мире, бот
//! `arrival` входит рядом и пишет по порядку `level_chunk_with_light`,
//! `chunk_batch_finished` и `add_entity`. Ваниль: спавн `resident` приходит
//! после пачки с его чанком, не раньше, и не в той же пачке, а тиком позже.
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use std::time::Duration;

async fn drain(c: &mut Client, ms: u64) {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
    while let Ok(Ok(_)) = tokio::time::timeout_at(deadline, c.next_event()).await {}
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let id = |name| table.id(State::Play, Direction::Clientbound, name).unwrap();
    let (chunk, finished, add) = (
        id("level_chunk_with_light"),
        id("chunk_batch_finished"),
        id("add_entity"),
    );

    let mut resident = Client::connect(&a[1], "resident", Ids::load(&table).unwrap(), 777, "26.3")
        .await
        .unwrap();
    let mut home = [0.0; 3];
    loop {
        if let Event::Position(p) = resident.next_event().await.unwrap() {
            home = p;
            break;
        }
    }
    let keep = tokio::spawn(async move { drain(&mut resident, 6000).await });
    let target = ((home[0].floor() as i32) >> 4, (home[2].floor() as i32) >> 4);
    println!("resident chunk {target:?}");

    let mut arrival = Client::connect(&a[1], "arrival", Ids::load(&table).unwrap(), 777, "26.3")
        .await
        .unwrap();
    arrival.capture_also = vec![chunk, finished, add];
    drain(&mut arrival, 4000).await;
    keep.await.unwrap();

    let mut batch = 0;
    let mut chunk_batch = None;
    for (pid, p) in &arrival.captured_by {
        let mut r = chunkwake_protocol::Reader::new(p);
        if *pid == chunk {
            let b = r.remaining();
            let pos = (
                i32::from_be_bytes([b[0], b[1], b[2], b[3]]),
                i32::from_be_bytes([b[4], b[5], b[6], b[7]]),
            );
            if pos == target && chunk_batch.is_none() {
                chunk_batch = Some(batch);
                println!("resident's chunk in batch {batch}");
            }
        } else if *pid == finished {
            batch += 1;
        } else if *pid == add {
            let _e = r.var_int().unwrap();
            let _uuid = r.uuid().unwrap();
            let kind = r.var_int().unwrap();
            println!("add_entity kind={kind} after {batch} finished batches");
        }
    }
}
