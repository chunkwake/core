//! `client_information` в play глазами второго игрока и самого игрока.
//!
//! `client_info_probe <addr> <packets.json>`: бот `changer` после входа шлёт
//! настройки по очереди (рука, части скина без шляпы, обзор), бот `watcher`
//! записывает `set_entity_data` и `player_info_update`. У `changer` считаются
//! загруженные чанки после каждой смены обзора.
//! Ваниль (`handleClientInformation`): шляпа сменилась → `UPDATE_HAT` всем;
//! рука и скин → `set_entity_data` только с изменёнными индексами 15/16,
//! следящим и себе; обзор → кольцо чанков выгружается или догружается.
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::configuration::{ClientInfo, client_information};
use std::time::Duration;

async fn drain(c: &mut Client, ms: u64) {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
    while let Ok(Ok(_)) = tokio::time::timeout_at(deadline, c.next_event()).await {}
}

async fn joined(c: &mut Client) {
    loop {
        if let Event::Position(_) = c.next_event().await.unwrap() {
            break;
        }
    }
}

fn hex(p: &[u8]) -> String {
    p.iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn dump(who: &str, entity_data: i32, info: i32, got: &[(i32, Vec<u8>)]) {
    for (id, p) in got {
        if *id == entity_data {
            let mut r = chunkwake_protocol::Reader::new(p);
            let e = r.var_int().unwrap();
            // Только записи модели игрока: индексы 15/16.
            let rest = r.remaining();
            if rest.first().is_some_and(|&i| i == 15 || i == 16) {
                println!("  {who} set_entity_data entity={e}: {}", hex(rest));
            }
        } else if *id == info {
            let actions = p[0];
            if actions & 0x80 != 0 && actions != 0xff && actions & 0x01 == 0 {
                println!(
                    "  {who} player_info_update actions={actions:#04x}: {}",
                    hex(p)
                );
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let sb = table
        .id(State::Play, Direction::Serverbound, "client_information")
        .unwrap();
    let entity_data = table
        .id(State::Play, Direction::Clientbound, "set_entity_data")
        .unwrap();
    let info = table
        .id(State::Play, Direction::Clientbound, "player_info_update")
        .unwrap();

    let mut watcher = Client::connect(&a[1], "watcher", Ids::load(&table).unwrap(), 777, "26.3")
        .await
        .unwrap();
    watcher.capture_also = vec![entity_data, info];
    joined(&mut watcher).await;
    let watch = tokio::spawn(async move {
        drain(&mut watcher, 9000).await;
        watcher.captured_by
    });

    let mut changer = Client::connect(&a[1], "changer", Ids::load(&table).unwrap(), 777, "26.3")
        .await
        .unwrap();
    changer.capture_also = vec![entity_data, info];
    joined(&mut changer).await;
    drain(&mut changer, 3000).await;
    println!("changer chunks at join: {}", changer.loaded.len());
    changer.captured_by.clear();

    let base = ClientInfo {
        view_distance: 32,
        skin_parts: 0x7f,
        main_hand: 1,
        allows_listing: true,
    };
    let steps = [
        (
            "left hand",
            ClientInfo {
                main_hand: 0,
                ..base
            },
        ),
        (
            "hat off",
            ClientInfo {
                main_hand: 0,
                skin_parts: 0x3f,
                ..base
            },
        ),
        (
            "same again",
            ClientInfo {
                main_hand: 0,
                skin_parts: 0x3f,
                ..base
            },
        ),
        ("both back", base),
        (
            "view 4",
            ClientInfo {
                view_distance: 4,
                ..base
            },
        ),
        (
            "view 1",
            ClientInfo {
                view_distance: 1,
                ..base
            },
        ),
        (
            "view 7",
            ClientInfo {
                view_distance: 7,
                ..base
            },
        ),
    ];
    for (name, step) in steps {
        changer
            .send_raw(&client_information(sb, &step))
            .await
            .unwrap();
        drain(&mut changer, 1000).await;
        println!("{name}: changer chunks {}", changer.loaded.len());
        dump("changer", entity_data, info, &changer.captured_by);
        changer.captured_by.clear();
    }

    let seen = watch.await.unwrap();
    println!("watcher:");
    dump("watcher", entity_data, info, &seen);
}
