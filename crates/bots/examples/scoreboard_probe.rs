//! Sidebar/scoreboard packets from the Bukkit scoreboard API.
//!
//! Server plugin mode: `BEHAVIOR_SCOREBOARD=1` (BehaviorProbe creates an
//! objective, sidebar display and two scores after the player joins).
//! Run: `scoreboard_probe <addr> <packets.json>`.

use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
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
        eprintln!("usage: scoreboard_probe <addr> <packets.json>");
        return;
    }
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let names = [
        "set_objective",
        "set_score",
        "reset_score",
        "set_display_objective",
    ];
    let mut capture = Vec::new();
    let mut by_id = BTreeMap::new();
    for name in names {
        if let Ok(id) = table.id(State::Play, Direction::Clientbound, name) {
            capture.push(id);
            by_id.insert(id, name);
        }
    }
    let mut c = Client::connect(&a[1], "scoreboard", ids, 777, "26.3")
        .await
        .unwrap();
    c.capture_also = capture;
    joined(&mut c).await;
    drain(&mut c, 6000).await;
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for (id, _) in &c.captured_by {
        if let Some(name) = by_id.get(id) {
            *counts.entry(name).or_default() += 1;
        }
    }
    for name in names {
        println!("{name}={}", counts.get(name).copied().unwrap_or(0));
    }
    println!("packets={}", c.captured_by.len());
}
