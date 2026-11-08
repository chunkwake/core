//! Дальность слежения за сущностями (`ChunkMap.TrackedEntity.updatePlayer`).
//!
//! `entity_range_probe <addr> <packets.json> [секунд] [entities.csv]`: бот входит и стоит,
//! пишет по порядку `level_chunk_with_light` и `add_entity`. Для каждого
//! типа печатает, сколько пришло, самое дальнее по горизонтали и сколько
//! пришло раньше своего чанка. Ваниль: не дальше
//! `min(обзор × 16, clientTrackingRange × 16)` и всегда после чанка.
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use std::collections::{BTreeMap, HashSet};
use std::time::Duration;

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let secs: u64 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(8);
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let id = |name| table.id(State::Play, Direction::Clientbound, name).unwrap();
    let (chunk, add) = (id("level_chunk_with_light"), id("add_entity"));

    let mut c = Client::connect(
        &a[1],
        &std::env::var("CHUNKWAKE_BOT_NAME").unwrap_or("ranger".into()),
        Ids::load(&table).unwrap(),
        777,
        "26.3",
    )
    .await
    .unwrap();
    c.capture_also = vec![chunk, add];
    let mut home = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    // Ошибка разбора одного пакета (бот знает не все) — не конец потока.
    let mut errors = 0;
    while let Ok(ev) = tokio::time::timeout_at(deadline, c.next_event()).await {
        match ev {
            Ok(Event::Position(p)) if home.is_none() => home = Some(p),
            Ok(_) => {}
            Err(_) if errors < 1000 => errors += 1,
            Err(e) => panic!("{e}"),
        }
    }
    println!("parse errors {errors}, chunks_in {}", c.chunks_in);
    let home = home.expect("no position");
    println!("home {:.1} {:.1} {:.1}", home[0], home[1], home[2]);

    let mut chunks = HashSet::new();
    // тип → (сколько, самое дальнее, раньше чанка)
    let mut kinds: BTreeMap<i32, (u32, f64, u32)> = BTreeMap::new();
    for (pid, p) in &c.captured_by {
        let mut r = chunkwake_protocol::Reader::new(p);
        if *pid == chunk {
            let b = r.remaining();
            chunks.insert((
                i32::from_be_bytes([b[0], b[1], b[2], b[3]]),
                i32::from_be_bytes([b[4], b[5], b[6], b[7]]),
            ));
        } else if *pid == add {
            let _e = r.var_int().unwrap();
            let _uuid = r.uuid().unwrap();
            let kind = r.var_int().unwrap();
            let x = r.f64().unwrap();
            let _y = r.f64().unwrap();
            let z = r.f64().unwrap();
            let d = ((x - home[0]).powi(2) + (z - home[2]).powi(2)).sqrt();
            let at = ((x.floor() as i32) >> 4, (z.floor() as i32) >> 4);
            let k = kinds.entry(kind).or_default();
            k.0 += 1;
            k.1 = k.1.max(d);
            if !chunks.contains(&at) {
                k.2 += 1;
            }
        }
    }
    let names: Vec<String> = a
        .get(4)
        .and_then(|f| std::fs::read_to_string(f).ok())
        .map(|t| {
            t.lines()
                .map(|l| l.split(';').next().unwrap_or("").to_owned())
                .collect()
        })
        .unwrap_or_default();
    println!("chunks {}", chunks.len());
    for (kind, (n, far, early)) in kinds {
        let name = names.get(kind as usize).map_or("?", |s| s.as_str());
        println!("{name} ({kind}): {n} spawned, farthest {far:.1}, before chunk {early}");
    }
}
