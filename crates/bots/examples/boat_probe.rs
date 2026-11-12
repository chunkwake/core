//! Проверка лодок живым ботом: поставить лодку взглядом под ноги, сесть,
//! проехать на ней, сойти шифтом, разбить кулаком (выпадает предмет) и
//! поставить вторую — после перезапуска она должна прийти снова.
//!
//! `boat_probe <addr> <packets.json> place|check`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use std::time::Duration;

async fn wait<T>(c: &mut Client, secs: u64, mut f: impl FnMut(&Event) -> Option<T>) -> Option<T> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    loop {
        let ev = tokio::time::timeout_at(deadline, c.next_event())
            .await
            .ok()?
            .ok()?;
        if let Some(v) = f(&ev) {
            return Some(v);
        }
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let sb = |n: &str| table.id(State::Play, Direction::Serverbound, n).unwrap();
    // Типы сущностей из отчёта реестров: ловим именно лодку и предмет.
    let reg: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::path::Path::new(&a[2]).with_file_name("registries.json"))
            .unwrap(),
    )
    .unwrap();
    let ty = |n: &str| {
        reg["minecraft:entity_type"]["entries"][n]["protocol_id"]
            .as_i64()
            .unwrap() as i32
    };
    let (boat_ty, item_ty) = (ty("minecraft:oak_boat"), ty("minecraft:item"));
    println!("oak_boat type {boat_ty}, item type {item_ty}");
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    let pos = wait(&mut c, 30, |e| match e {
        Event::Position(p) => Some(*p),
        _ => None,
    })
    .await
    .unwrap();
    println!("spawn {pos:?}");
    if a[3] == "check" {
        // Лодки с диска приходят с остальными сущностями при входе.
        let found = wait(&mut c, 10, |e| match e {
            Event::EntitySpawned {
                entity_id,
                kind,
                pos,
            } if *kind == boat_ty => Some((*entity_id, *pos)),
            _ => None,
        })
        .await;
        println!("boat after restart: {found:?}");
        return;
    }
    let cmd = |c: &str| {
        let mut w = Writer::new();
        w.var_int(sb("chat_command")).string(c);
        w.buf
    };
    c.send_raw(&cmd("gamemode survival")).await.unwrap();
    c.send_raw(&cmd("clear boater")).await.unwrap();
    c.send_raw(&cmd("give boater minecraft:oak_boat 2"))
        .await
        .unwrap();
    // Взгляд вниз и вперёд (на юг): лодка встаёт на землю перед игроком.
    let mut w = Writer::new();
    w.var_int(sb("move_player_rot")).f32(0.0).f32(60.0).u8(1);
    c.send_raw(&w.buf).await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    let use_item = |seq: i32| {
        let mut w = Writer::new();
        w.var_int(sb("use_item"))
            .var_int(0)
            .var_int(seq)
            .f32(0.0)
            .f32(60.0);
        w.buf
    };
    c.send_raw(&use_item(1)).await.unwrap();
    let (boat, kind, bpos) = wait(&mut c, 5, |e| match e {
        Event::EntitySpawned {
            entity_id,
            kind,
            pos,
        } if *kind == boat_ty => Some((*entity_id, *kind, *pos)),
        _ => None,
    })
    .await
    .expect("лодка не появилась");
    println!("boat {boat} kind {kind} at {bpos:?}");

    // Сесть: `interact` без шифта.
    let mut w = Writer::new();
    w.var_int(sb("interact"))
        .var_int(boat)
        .var_int(0)
        .u8(0)
        .bool(false);
    c.send_raw(&w.buf).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    // Проехать 6 блоков на юг.
    let mut p = bpos;
    for _ in 0..30 {
        p[2] += 0.2;
        let mut w = Writer::new();
        w.var_int(sb("move_vehicle"))
            .f64(p[0])
            .f64(p[1])
            .f64(p[2])
            .f32(0.0)
            .f32(0.0)
            .bool(true);
        c.send_raw(&w.buf).await.unwrap();
        let mut w = Writer::new();
        w.var_int(sb("paddle_boat")).bool(true).bool(true);
        c.send_raw(&w.buf).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    // Сойти: шифт.
    let mut w = Writer::new();
    w.var_int(sb("player_input")).u8(0x20);
    c.send_raw(&w.buf).await.unwrap();
    let off = wait(&mut c, 5, |e| match e {
        Event::Position(p) => Some(*p),
        _ => None,
    })
    .await
    .expect("не высадили");
    println!("boat drove to {p:?}, dismounted at {off:?}");
    let mut w = Writer::new();
    w.var_int(sb("player_input")).u8(0);
    c.send_raw(&w.buf).await.unwrap();
    c.move_to(off).await.unwrap();

    // Разбить: пять ударов кулаком (10 урона каждый, ломается после 40).
    for _ in 0..5 {
        let mut w = Writer::new();
        w.var_int(sb("attack")).var_int(boat);
        c.send_raw(&w.buf).await.unwrap();
    }
    let mut removed = false;
    let mut dropped = None;
    while let Some(ev) = wait(&mut c, 3, |e| match e {
        Event::EntityRemoved(ids) => Some((ids.contains(&boat), None)),
        Event::EntitySpawned {
            entity_id, kind, ..
        } if *kind == item_ty => Some((false, Some((*entity_id, *kind)))),
        _ => None,
    })
    .await
    {
        removed |= ev.0;
        if ev.1.is_some() {
            dropped = ev.1;
        }
        if removed && dropped.is_some() {
            break;
        }
    }
    println!("broken: removed={removed} drop={dropped:?}");

    // Вторая лодка (лодки не стакаются: она в слоте 2 хотбара) остаётся до перезапуска.
    let mut w = Writer::new();
    w.var_int(sb("set_carried_item")).u16(1);
    c.send_raw(&w.buf).await.unwrap();
    let mut w = Writer::new();
    w.var_int(sb("move_player_rot")).f32(180.0).f32(60.0).u8(1);
    c.send_raw(&w.buf).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    c.send_raw(&use_item(2)).await.unwrap();
    let second = wait(&mut c, 5, |e| match e {
        Event::EntitySpawned {
            entity_id,
            kind,
            pos,
        } if *kind == boat_ty => Some((*entity_id, *kind, *pos)),
        _ => None,
    })
    .await;
    println!("second boat {second:?}");
    c.send_raw(&cmd("save-all")).await.unwrap();
    tokio::time::sleep(Duration::from_secs(3)).await;
}
