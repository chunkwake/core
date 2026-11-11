//! Modern Velocity forwarding через настоящий прокси: бот входит на порт
//! Velocity, прокси передаёт профиль backend'у, сервер отвечает телепортом.
//!
//! `velocity_probe <proxy addr> <packets.json>`; имя — `CHUNKWAKE_BOT_NAME`.
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::PacketTable;

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let name = std::env::var("CHUNKWAKE_BOT_NAME").unwrap_or_else(|_| "velobot".into());
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let mut c = Client::connect(&a[1], &name, ids, 777, "26.3")
        .await
        .unwrap();
    loop {
        match c.next_event().await.unwrap() {
            Event::Position(pos) => {
                println!("velocity: ok, {name} at {pos:?}");
                return;
            }
            Event::Other => {}
            event => println!("velocity: {event:?}"),
        }
    }
}
