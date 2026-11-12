//! Проверка загрузки статистики с диска: при входе сервер присылает
//! `award_stats` со всеми сохранёнными счётчиками.
//!
//! `stats_load_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::PacketTable;
use std::collections::HashMap;
use std::time::Duration;

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let reg: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::path::Path::new(&a[2]).with_file_name("registries.json"))
            .unwrap(),
    )
    .unwrap();
    let custom = |n: &str| {
        reg["minecraft:custom_stat"]["entries"][n]["protocol_id"]
            .as_i64()
            .unwrap() as i32
    };
    let (traded, fished) = (
        custom("minecraft:traded_with_villager"),
        custom("minecraft:fish_caught"),
    );
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    let mut stats: HashMap<(i32, i32), i32> = HashMap::new();
    let deadline = tokio::time::Instant::now() + Duration::from_millis(3500);
    while let Ok(Ok(ev)) = tokio::time::timeout_at(deadline, c.next_event()).await {
        if let Event::Stats { stats: got } = ev {
            for (key, n) in got {
                let e = stats.entry(key).or_insert(0);
                *e = (*e).max(n);
            }
        }
    }
    let get = |kind: i32, value: i32| stats.get(&(kind, value)).copied().unwrap_or(0);
    assert!(
        get(8, traded) >= 1,
        "traded_with_villager не загрузился: {stats:?}"
    );
    assert!(get(8, fished) >= 1, "fish_caught не загрузился: {stats:?}");
    println!(
        "stats from disk: traded={} fish={} (всего {})",
        get(8, traded),
        get(8, fished),
        stats.len()
    );
    println!("stats_load: ok");
}
