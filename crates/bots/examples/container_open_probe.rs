//! Живая проба `InventoryOpenEvent` с настоящим содержимым контейнера:
//! открывает сундук три раза, плагин пишет/читает предметы через
//! `getTopInventory()`, проверяя запись в block entity.
//!
//! Запуск: `container_open_probe <addr> <packets.json>`.

use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use chunkwake_protocol::play::pack_block_pos;
use std::time::Duration;

async fn joined(c: &mut Client) {
    loop {
        if let Event::Position(pos) = c.next_event().await.unwrap() {
            println!("spawn={pos:?}");
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
        eprintln!("usage: container_open_probe <addr> <packets.json>");
        return;
    }
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let id = |name| table.id(State::Play, Direction::Clientbound, name).unwrap();
    let open = id("open_screen");
    let content = id("container_set_content");
    let use_on = table
        .id(State::Play, Direction::Serverbound, "use_item_on")
        .unwrap();
    let sb_close = table
        .id(State::Play, Direction::Serverbound, "container_close")
        .unwrap();
    let sb_click = table
        .id(State::Play, Direction::Serverbound, "container_click")
        .unwrap();

    let mut c = Client::connect(&a[1], "chestcontents", ids, 777, "26.3")
        .await
        .unwrap();
    c.capture_also = vec![open, content];
    joined(&mut c).await;
    drain(&mut c, 1200).await;

    for attempt in 1..=3 {
        let before = c.captured_by.iter().filter(|(id, _)| *id == open).count();
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

        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while tokio::time::Instant::now() < deadline {
            if tokio::time::timeout_at(deadline, c.next_event())
                .await
                .is_err()
            {
                break;
            }
            if c.captured_by.iter().filter(|(id, _)| *id == open).count() > before {
                break;
            }
        }
        let window = c
            .captured_by
            .iter()
            .rev()
            .find(|(id, _)| *id == open)
            .and_then(|(_, payload)| payload.first().copied());
        println!("open {attempt} window={window:?}");
        drain(&mut c, 600).await;
        if attempt == 1
            && let Some(window) = window
        {
            let mut w = Writer::new();
            w.var_int(sb_click)
                .var_int(window as i32)
                .var_int(0)
                .u16(0)
                .u8(0)
                .var_int(0)
                .var_int(0)
                .bool(false);
            c.send_raw(&w.buf).await.unwrap();
            drain(&mut c, 600).await;
        }
        if let Some(window) = window {
            let mut w = Writer::new();
            w.var_int(sb_close).var_int(window as i32);
            c.send_raw(&w.buf).await.unwrap();
        }
        drain(&mut c, 700).await;
    }
    let opens = c.captured_by.iter().filter(|(id, _)| *id == open).count();
    // CustomModelData 42 -> floats[1] + f32 42.0 + три пустых списка.
    let cmd = [0x42, 0x28, 0x00, 0x00, 0x00, 0x00, 0x00];
    let cmd_hits: usize = c
        .captured_by
        .iter()
        .filter(|(id, _)| *id == content)
        .map(|(_, payload)| payload.windows(cmd.len()).filter(|w| *w == cmd).count())
        .sum();
    println!("open_screen={opens} custom_model_data_42_patterns={cmd_hits}");
}
