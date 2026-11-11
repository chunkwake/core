//! Слизни у неподвижного бота: сколько разных слизней прислал сервер
//! (`add_entity`), сколько ударов (падение здоровья в `set_health`) и
//! смертей за время прогона. Сверка частоты появления и укусов с ванилью.
//!
//! `slime_probe <addr> <packets.json> <секунды>`; имя — `CHUNKWAKE_BOT_NAME`.
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use std::collections::HashSet;
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let secs: u64 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(90);
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let sb = |n: &str| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n: &str| table.id(State::Play, Direction::Clientbound, n).unwrap();
    let reg: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::path::Path::new(&a[2]).with_file_name("registries.json"))
            .unwrap(),
    )
    .unwrap();
    let slime = reg["minecraft:entity_type"]["entries"]["minecraft:slime"]["protocol_id"]
        .as_i64()
        .unwrap() as i32;
    let name = std::env::var("CHUNKWAKE_BOT_NAME").unwrap_or_else(|_| "slimer".into());
    let mut c = Client::connect(&a[1], &name, ids, 777, "26.3")
        .await
        .unwrap();
    let health_id = cb("set_health");
    c.capture_also = vec![health_id];
    let mut respawn = Writer::new();
    respawn.var_int(sb("client_command")).var_int(0);
    let mut seen = HashSet::new();
    let mut me = [0.0f64; 3];
    let mut where_: std::collections::BTreeMap<(i32, i32), u32> = Default::default();
    let mut live = HashSet::new();
    let (mut hits, mut deaths, mut health) = (0, 0, 20.0f32);
    let start = Instant::now();
    let mut next_report = 10;
    while start.elapsed() < Duration::from_secs(secs) {
        let ev = tokio::time::timeout(Duration::from_millis(200), c.next_event()).await;
        match ev {
            Ok(Ok(Event::Position(p))) => me = p,
            Ok(Ok(Event::EntitySpawned {
                entity_id,
                kind,
                pos,
            })) if kind == slime => {
                if !seen.contains(&entity_id) {
                    let c = |v: f64| (v.floor() as i32) >> 4;
                    *where_
                        .entry((c(pos[0]) - c(me[0]), c(pos[2]) - c(me[2])))
                        .or_default() += 1;
                }
                seen.insert(entity_id);
                live.insert(entity_id);
            }
            Ok(Ok(Event::EntityRemoved(gone))) => {
                for g in gone {
                    live.remove(&g);
                }
            }
            Ok(Err(e)) => {
                println!("disconnected: {e}");
                break;
            }
            _ => {}
        }
        for (pid, body) in std::mem::take(&mut c.captured_by) {
            if pid != health_id || body.len() < 4 {
                continue;
            }
            let h = f32::from_be_bytes(body[..4].try_into().unwrap());
            if std::env::var("TRACE").is_ok() {
                println!("{:>7.2}s health {h}", start.elapsed().as_secs_f64());
            }
            if h < health {
                hits += 1;
            }
            if h <= 0.0 && health > 0.0 {
                deaths += 1;
                c.send_raw(&respawn.buf).await.unwrap();
            }
            health = h;
        }
        let t = start.elapsed().as_secs();
        if t >= next_report {
            println!(
                "t={t:>3}s slimes seen={} live={} hits={hits} deaths={deaths} health={health}",
                seen.len(),
                live.len()
            );
            next_report += 10;
        }
    }
    println!("me {me:?}; first seen by chunk offset: {where_:?}");
}
