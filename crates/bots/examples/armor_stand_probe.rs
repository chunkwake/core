//! Стойка для брони: постановка, надевание шлема, два удара и дроп.
//!
//! `armor_stand_probe <addr> <packets.json> <registries.json>`
//!
//! `ARMOR_STAND_KEEP=1` оставляет стойку в мире и сохраняет её; после
//! перезапуска сервера `ARMOR_STAND_MODE=check` проверяет загрузку из NBT.
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer};
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

/// Выбрать слот хотбара (слот игрока мог сохраниться с прошлого запуска).
async fn hold(c: &mut Client, id: i32, slot: u16) {
    let mut w = Writer::new();
    w.var_int(id).u16(slot);
    c.send_raw(&w.buf).await.unwrap();
    pump(c, 100).await;
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let reg: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&a[3]).unwrap()).unwrap();
    let stand = reg["minecraft:entity_type"]["entries"]["minecraft:armor_stand"]["protocol_id"]
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
    // Проверка после перезапуска: стойка и её шлем должны прийти из NBT.
    if std::env::var("ARMOR_STAND_MODE").as_deref() == Ok("check") {
        let equip = table
            .id(State::Play, Direction::Clientbound, "set_equipment")
            .unwrap();
        c.capture_also = vec![equip];
        let helmet = reg["minecraft:item"]["entries"]["minecraft:iron_helmet"]["protocol_id"]
            .as_i64()
            .unwrap() as i32;
        let mut found = None;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout_at(deadline, c.next_event()).await {
                Ok(Ok(Event::EntitySpawned {
                    entity_id, kind, ..
                })) if kind == stand => {
                    found = Some(entity_id);
                    break;
                }
                Ok(Ok(_)) => {}
                _ => break,
            }
        }
        let entity = found.expect("стойка не загрузилась из NBT");
        let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
        while tokio::time::Instant::now() < deadline {
            let _ = tokio::time::timeout_at(deadline, c.next_event()).await;
        }
        let mut head = false;
        for (id, payload) in &c.captured_by {
            if *id != equip {
                continue;
            }
            let mut r = Reader::new(payload);
            if r.var_int().ok() != Some(entity) {
                continue;
            }
            while let Ok(raw) = r.u8() {
                let more = raw & 0x80 != 0;
                let slot = raw & 0x7f;
                let Ok(stack) = chunkwake_protocol::inventory::read_stack(&mut r) else {
                    break;
                };
                if slot == 4 && stack.is_some_and(|s| s.item == helmet) {
                    head = true;
                }
                if !more {
                    break;
                }
            }
        }
        assert!(head, "у загруженной стойки нет шлема");
        let chat = sb("chat_command");
        cmd(&mut c, chat, "gamemode creative").await;
        let mut w = Writer::new();
        w.var_int(sb("attack")).var_int(entity);
        c.send_raw(&w.buf).await.unwrap();
        pump(&mut c, 300).await;
        println!("armor_stand: ok (nbt)");
        return;
    }
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
    // Повторный запуск: ломаем стойки, оставшиеся от прошлого прогона
    // (`/kill @e` сущности этого типа пока не выбирает). Собираем их до
    // команд: `pump` выбрасывает события, и спавн старой стойки потерялся бы.
    let mut old = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout_at(deadline, c.next_event()).await {
            Ok(Ok(Event::EntitySpawned {
                entity_id, kind, ..
            })) if kind == stand => old.push(entity_id),
            Ok(Ok(_)) => {}
            _ => break,
        }
    }
    if !old.is_empty() {
        println!("old armor stands: {old:?}");
    }
    // В творческом хватает одного удара, в выживании нужен второй подряд.
    for _ in 0..2 {
        for id in &old {
            let mut w = Writer::new();
            w.var_int(sb("attack")).var_int(*id);
            c.send_raw(&w.buf).await.unwrap();
        }
    }
    pump(&mut c, 300).await;

    let chat = sb("chat_command");
    cmd(&mut c, chat, "gamemode creative").await;
    cmd(&mut c, chat, "difficulty peaceful").await;
    cmd(&mut c, chat, "time set day").await;
    cmd(&mut c, chat, "clear @s").await;
    cmd(
        &mut c,
        chat,
        "item replace entity @s hotbar.0 with minecraft:armor_stand 1",
    )
    .await;
    hold(&mut c, sb("set_carried_item"), 0).await;

    // Стойка на верхнюю грань блока в двух блоках от игрока.
    let mut w = Writer::new();
    w.var_int(sb("use_item_on"))
        .var_int(0)
        .i64(pack_block([x + 2, y - 1, z]))
        .var_int(1)
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
            })) if kind == stand => {
                entity = Some(entity_id);
                break;
            }
            Ok(Ok(_)) => {}
            _ => break,
        }
    }
    let entity = entity.expect("стойка не появилась");

    // Надеваем шлем ударом по голове (в творческом предмет не тратится).
    cmd(
        &mut c,
        chat,
        "item replace entity @s hotbar.0 with minecraft:iron_helmet 1",
    )
    .await;
    hold(&mut c, sb("set_carried_item"), 0).await;
    let mut w = Writer::new();
    w.var_int(sb("interact")).var_int(entity).var_int(0);
    chunkwake_protocol::play::lp_vec3(&mut w, [0.0, 1.7, 0.0]);
    w.bool(false);
    c.send_raw(&w.buf).await.unwrap();
    pump(&mut c, 300).await;

    // Подготовка проверки NBT: оставить стойку в мире и сохранить её.
    if std::env::var("ARMOR_STAND_KEEP").is_ok() {
        cmd(&mut c, chat, "save-all").await;
        println!("armor_stand: placed");
        return;
    }

    // В выживании два быстрых удара ломают стойку и роняют её и снаряжение.
    cmd(
        &mut c,
        chat,
        "effect give @s minecraft:resistance 30 4 true",
    )
    .await;
    cmd(&mut c, chat, "gamemode survival").await;
    for _ in 0..2 {
        let mut w = Writer::new();
        w.var_int(sb("attack")).var_int(entity);
        c.send_raw(&w.buf).await.unwrap();
    }

    let mut removed = false;
    let mut drops = 0;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout_at(deadline, c.next_event()).await {
            Ok(Ok(Event::EntityRemoved(list))) if list.contains(&entity) => removed = true,
            Ok(Ok(Event::EntitySpawned { kind, .. })) if kind == item => drops += 1,
            Ok(Ok(_)) => {}
            _ => break,
        }
        if removed && drops >= 2 {
            break;
        }
    }
    assert!(removed, "стойка не исчезла после ударов");
    assert!(drops >= 2, "нет дропа стойки и шлема (дропов: {drops})");
    println!("armor_stand: ok");
}
