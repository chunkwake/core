//! Держит соединение N секунд (нагрузочные пробы: чанки не выгружаются).
//!
//! `hold_bot <addr> <packets.json> <seconds>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use std::time::Duration;

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let _ = table.id(State::Play, Direction::Serverbound, "chat_command");
    let seconds: u64 = a.get(3).and_then(|v| v.parse().ok()).unwrap_or(60);
    let mut c = Client::connect(&a[1], "holder", ids, 777, "26.3")
        .await
        .unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            break;
        }
        match tokio::time::timeout(left, c.next_event()).await {
            Ok(Ok(Event::Position(_))) => continue,
            Ok(Ok(_)) => continue,
            Ok(Err(_)) => break,
            Err(_) => break,
        }
    }
    println!("hold_bot: done {seconds}s");
}
