//! `Player.sendTitle`/`sendActionBar`/`playSound`/`sendBlockChange`:
//! бот ловит пакеты, которые адресованы только ему.
//!
//! Запуск: `player_effects_probe <addr> <packets.json>`.
//! На сервере должен быть плагин, который после входа игрока шлёт эффекты
//! (`BEHAVIOR_EFFECTS=1` у BehaviorProbe).

use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Reader;
use std::collections::BTreeMap;
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
        eprintln!("usage: player_effects_probe <addr> <packets.json>");
        return;
    }
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let names = [
        "sound",
        "set_title_text",
        "set_subtitle_text",
        "set_action_bar_text",
        "block_update",
        "container_set_content",
        "level_particles",
        "set_entity_motion",
        "remove_entities",
        "add_entity",
    ];
    let mut capture = Vec::new();
    let mut by_id = BTreeMap::new();
    for name in names {
        if let Ok(id) = table.id(State::Play, Direction::Clientbound, name) {
            capture.push(id);
            by_id.insert(id, name);
        }
    }

    let name = a.get(3).map(String::as_str).unwrap_or("effects");
    let mut c = Client::connect(&a[1], name, ids, 777, "26.3")
        .await
        .unwrap();
    c.capture_also = capture;
    joined(&mut c).await;
    drain(&mut c, 8000).await;

    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for (id, _) in &c.captured_by {
        if let Some(name) = by_id.get(id) {
            *counts.entry(name).or_default() += 1;
        }
    }
    for name in names {
        println!("{name}={}", counts.get(name).copied().unwrap_or(0));
    }
    if std::env::var("SOUND_DETAILS").is_ok() {
        for (id, payload) in &c.captured_by {
            if by_id.get(id) != Some(&"sound") {
                continue;
            }
            let mut r = Reader::new(payload);
            let sound = r.var_int();
            let source = r.var_int();
            let x = r.i32();
            let y = r.i32();
            let z = r.i32();
            let volume = r.f32();
            let pitch = r.f32();
            println!(
                "sound_raw sound={sound:?} source={source:?} x={x:?} y={y:?} z={z:?} \
                 vol={volume:?} pitch={pitch:?}"
            );
        }
    }
    for (id, payload) in &c.captured_by {
        if by_id.get(id) != Some(&"block_update") {
            continue;
        }
        let mut r = Reader::new(payload);
        if let (Ok(pos), Ok(state)) = (r.i64(), r.var_int()) {
            let [x, y, z] = chunkwake_protocol::play::unpack_block_pos(pos);
            println!("block_update pos={x},{y},{z} state={state}");
        }
    }
    println!("packets={}", c.captured_by.len());
}
