//! Картина: установка на стену, спавн сущности и дроп после удара.
//!
//! `painting_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use std::time::Duration;

fn pack_block([x, y, z]: [i32; 3]) -> i64 {
    ((x as i64 & 0x3FF_FFFF) << 38) | ((z as i64 & 0x3FF_FFFF) << 12) | (y as i64 & 0xFFF)
}

async fn pump(c: &mut Client, ms: u64) {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
    while let Ok(Ok(_)) = tokio::time::timeout_at(deadline, c.next_event()).await {}
}

async fn cmd(c: &mut Client, id: i32, line: &str) {
    let mut w = Writer::new();
    w.var_int(id).string(line);
    c.send_raw(&w.buf).await.unwrap();
    pump(c, 250).await;
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let reg: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&a[3]).unwrap()).unwrap();
    let painting = reg["minecraft:entity_type"]["entries"]["minecraft:painting"]["protocol_id"]
        .as_i64()
        .unwrap() as i32;
    let item = reg["minecraft:entity_type"]["entries"]["minecraft:item"]["protocol_id"]
        .as_i64()
        .unwrap() as i32;
    let sb = |name| table.id(State::Play, Direction::Serverbound, name).unwrap();
    let ids = Ids::load(&table).unwrap();
    let mut c = Client::connect(&a[1], "framer", ids, 777, "26.3")
        .await
        .unwrap();
    let spawn = loop {
        if let Event::Position(at) = c.next_event().await.unwrap() {
            break at;
        }
    };
    let (x, y, z) = (
        spawn[0].floor() as i32,
        spawn[1].floor() as i32,
        spawn[2].floor() as i32,
    );
    let chat = sb("chat_command");
    cmd(&mut c, chat, "gamemode creative").await;
    cmd(&mut c, chat, "clear @s").await;
    // Стена 2x2: влезает картина 1x1.
    cmd(
        &mut c,
        chat,
        &format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            x + 3,
            y,
            z - 1,
            x + 3,
            y + 1,
            z
        ),
    )
    .await;
    cmd(
        &mut c,
        chat,
        "item replace entity @s hotbar.0 with minecraft:painting 1",
    )
    .await;

    let mut w = Writer::new();
    w.var_int(sb("use_item_on"))
        .var_int(0)
        .i64(pack_block([x + 3, y, z]))
        .var_int(4)
        .f32(0.5)
        .f32(0.5)
        .f32(0.5)
        .u8(0)
        .u8(0)
        .var_int(1);
    c.send_raw(&w.buf).await.unwrap();

    let mut entity = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout_at(deadline, c.next_event()).await {
            Ok(Ok(Event::EntitySpawned {
                entity_id, kind, ..
            })) if kind == painting => {
                entity = Some(entity_id);
                break;
            }
            Ok(Ok(_)) => {}
            _ => break,
        }
    }
    let entity = entity.expect("картина не появилась");

    // В творческом картина исчезает без дропа, как в ванили.
    cmd(&mut c, chat, "gamemode survival").await;
    let mut w = Writer::new();
    w.var_int(sb("attack")).var_int(entity);
    c.send_raw(&w.buf).await.unwrap();
    let mut dropped = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout_at(deadline, c.next_event()).await {
            Ok(Ok(Event::EntitySpawned { kind, .. })) if kind == item => {
                dropped = true;
                break;
            }
            Ok(Ok(_)) => {}
            _ => break,
        }
    }
    assert!(dropped, "картина не выпала предметом");
    println!("painting: ok");
}
