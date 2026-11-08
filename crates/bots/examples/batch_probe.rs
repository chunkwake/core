//! Темп отправки чанков (`PlayerChunkSender`) при входе.
//!
//! `batch_probe <addr> <packets.json> [скорость]`: бот отвечает на каждый
//! `chunk_batch_finished` значением `скорость` (по умолчанию 64, как бот
//! библиотеки) и печатает размеры первых пачек с временем от входа.
//! Ваниль: первая пачка — 9 чанков (`START_CHUNKS_PER_TICK`), до ответа
//! клиента больше пачек нет; затем до `min(скорость, 64)` за тик.
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let finished = table
        .id(State::Play, Direction::Clientbound, "chunk_batch_finished")
        .unwrap();
    let mut c = Client::connect(&a[1], "batcher", Ids::load(&table).unwrap(), 777, "26.3")
        .await
        .unwrap();
    c.capture = Some(finished);
    let start = Instant::now();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(12);
    let mut seen = 0;
    let mut sizes = Vec::new();
    while let Ok(next) = tokio::time::timeout_at(deadline, c.next_event()).await {
        let e = match next {
            Ok(e) => e,
            Err(err) => {
                println!("error at {} ms: {err}", start.elapsed().as_millis());
                break;
            }
        };
        if let Event::Position(_) = e {
            println!("position at {} ms", start.elapsed().as_millis());
        }
        while seen < c.captured.len() {
            let mut r = chunkwake_protocol::Reader::new(&c.captured[seen]);
            let n = r.var_int().unwrap();
            sizes.push((start.elapsed().as_millis(), n));
            seen += 1;
        }
    }
    for (i, (ms, n)) in sizes.iter().take(8).enumerate() {
        println!("batch {i}: {n} chunks at {ms} ms");
    }
    let total: i32 = sizes.iter().map(|s| s.1).sum();
    println!(
        "batches {}, chunks {total}, loaded {}",
        sizes.len(),
        c.loaded.len()
    );
}
