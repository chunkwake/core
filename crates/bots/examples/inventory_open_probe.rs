//! `InventoryOpenEvent` для ванильного сундука: бот открывает сундук
//! `use_item_on`, серверный плагин (`BEHAVIOR_INV_OPEN=1`) пишет событие.
//!
//! Запуск: `inventory_open_probe <addr> <packets.json>`.

use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use chunkwake_protocol::play::pack_block_pos;
use std::time::Duration;

async fn joined(c: &mut Client) {
    loop {
        if let Event::Position(_) = c.next_event().await.unwrap() {
            break;
        }
    }
}

async fn drain(c: &mut Client, ms: u64) {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
    while let Ok(Ok(_)) = tokio::time::timeout_at(deadline, c.next_event()).await {}
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 3 {
        eprintln!("usage: inventory_open_probe <addr> <packets.json>");
        return;
    }
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let id = |name| table.id(State::Play, Direction::Clientbound, name).unwrap();
    let open = id("open_screen");
    let close = id("container_close");
    let use_on = table
        .id(State::Play, Direction::Serverbound, "use_item_on")
        .unwrap();

    let mut c = Client::connect(&a[1], "chestopen", ids, 777, "26.3")
        .await
        .unwrap();
    c.capture_also = vec![open, close];
    joined(&mut c).await;
    drain(&mut c, 800).await;

    let mut w = Writer::new();
    w.var_int(use_on)
        .var_int(0)
        .i64(pack_block_pos([8, -60, 10]))
        .var_int(1)
        .f32(0.5)
        .f32(1.0)
        .f32(0.5)
        .u8(0)
        .u8(0)
        .var_int(1);
    c.send_raw(&w.buf).await.unwrap();
    drain(&mut c, 3000).await;

    let opens = c.captured_by.iter().filter(|(id, _)| *id == open).count();
    let closes = c.captured_by.iter().filter(|(id, _)| *id == close).count();
    println!("open_screen={opens} container_close={closes}");
    for (id, payload) in &c.captured_by {
        if *id == open {
            println!("open bytes={}", payload.len());
        }
    }
}
