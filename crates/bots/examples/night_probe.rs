//! Ночь в творческом режиме: сколько и каких мобов приходит естественным спавном.
//!
//! `night_probe <addr> <packets.json> [секунды]`
//!
//! Бот стоит на точке спавна, время заморожено на полуночи, погода ясная.
//! Через 5 с после входа, когда чанки уже загружены, убиваются все сущности
//! кроме игроков. Считаются новые `add_entity` по видам (каждый id — один
//! раз; `kind = -1` у бота — синхронизация позиции, а не появление) и монстры,
//! видимые раз в минуту: заполнен ли лимит категории. В конце печатается доля
//! каждого вида среди монстров. Творческий игрок участвует в спавне, но мобы
//! его не трогают.
//!
//! Имя бота — `CHUNKWAKE_BOT_NAME` (по умолчанию `pather`), ему нужен оператор.
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Duration;
use tokio::time::Instant;

async fn wait<T>(c: &mut Client, secs: f64, mut f: impl FnMut(&Event) -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + Duration::from_secs_f64(secs);
    loop {
        let ev = match tokio::time::timeout_at(deadline, c.next_event()).await {
            Ok(Ok(ev)) => ev,
            Ok(Err(_)) => continue,
            Err(_) => return None,
        };
        if let Some(v) = f(&ev) {
            return Some(v);
        }
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let secs: f64 = a.get(3).map_or(300.0, |s| s.parse().unwrap());
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let sb = |n: &str| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let reg: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::path::Path::new(&a[2]).with_file_name("registries.json"))
            .unwrap(),
    )
    .unwrap();
    let names: HashMap<i32, String> = reg["minecraft:entity_type"]["entries"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (v["protocol_id"].as_i64().unwrap() as i32, k.clone()))
        .collect();
    let name = std::env::var("CHUNKWAKE_BOT_NAME").unwrap_or_else(|_| "pather".into());
    let mut c = Client::connect(&a[1], &name, ids, 777, "26.3")
        .await
        .unwrap();
    let cb = |n: &str| table.id(State::Play, Direction::Clientbound, n).unwrap();
    let watch = [
        "level_chunk_with_light",
        "set_health",
        "respawn",
        "forget_level_chunk",
        "set_chunk_cache_center",
        "player_position",
        "game_event",
    ];
    let debug = std::env::var("PROBE_TRACE").is_ok();
    if debug {
        c.capture_also = watch.iter().map(|n| cb(n)).collect();
    }
    let mut seen_ids: BTreeMap<&str, u32> = BTreeMap::new();
    let spawn = wait(&mut c, 30.0, |e| match e {
        Event::Position(p) => Some(*p),
        _ => None,
    })
    .await
    .unwrap();
    let mut respawn = Writer::new();
    respawn.var_int(sb("client_command")).var_int(0);
    c.send_raw(&respawn.buf).await.unwrap();
    println!("spawn {spawn:?}");
    let cmd = |s: &str| {
        let mut w = Writer::new();
        w.var_int(sb("chat_command")).string(s);
        w.buf
    };
    for s in [
        "difficulty normal",
        "gamemode creative",
        "time set midnight",
        "gamerule advance_time false",
        "weather clear",
        "gamerule spawn_mobs true",
        "gamerule spawn_monsters true",
    ] {
        c.send_raw(&cmd(s)).await.unwrap();
    }
    // Убить после загрузки чанков: иначе сущности мира ещё не в памяти,
    // переживают команду и занимают лимит категории.
    let _ = wait(&mut c, 5.0, |_| None::<()>).await;
    c.send_raw(&cmd("kill @e[type=!minecraft:player]"))
        .await
        .unwrap();
    let _ = wait(&mut c, 3.0, |_| None::<()>).await;
    let start = Instant::now();
    let mut seen = HashSet::new();
    let mut by_kind: BTreeMap<String, u32> = BTreeMap::new();
    let mut gone = 0u32;
    let mut marks = Vec::new();
    let monsters = [
        "minecraft:zombie",
        "minecraft:zombie_villager",
        "minecraft:skeleton",
        "minecraft:creeper",
        "minecraft:spider",
        "minecraft:enderman",
        "minecraft:witch",
        "minecraft:slime",
    ];
    // Видимые сейчас монстры: заполнен ли лимит категории.
    let mut alive: HashMap<i32, String> = HashMap::new();
    let mut alive_marks = Vec::new();
    let mut next_minute = 60;
    while start.elapsed().as_secs_f64() < secs {
        if start.elapsed().as_secs() >= next_minute {
            alive_marks.push(alive.len());
            next_minute += 60;
        }
        let ev = match tokio::time::timeout(Duration::from_millis(250), c.next_event()).await {
            Ok(Ok(ev)) => ev,
            _ => continue,
        };
        for (pid, body) in std::mem::take(&mut c.captured_by) {
            if let Some(n) = watch.iter().find(|n| cb(n) == pid) {
                *seen_ids.entry(n).or_default() += 1;
                if *n == "set_health" || *n == "respawn" || *n == "game_event" {
                    println!(
                        "t{} {n} {:?}",
                        start.elapsed().as_secs(),
                        &body[..body.len().min(12)]
                    );
                }
            }
        }
        if debug {
            let tag = match &ev {
                Event::EntitySpawned { kind, .. } if *kind >= 0 => "add",
                Event::EntitySpawned { .. } => "sync",
                Event::EntityRemoved(_) => "remove",
                Event::EntityMoved { .. } => "move",
                _ => "other",
            };
            *seen_ids.entry(tag).or_default() += 1;
        }
        match ev {
            Event::EntitySpawned {
                entity_id, kind, ..
            } if kind >= 0 && seen.insert(entity_id) => {
                let n = names
                    .get(&kind)
                    .cloned()
                    .unwrap_or_else(|| kind.to_string());
                if monsters.contains(&n.as_str()) {
                    alive.insert(entity_id, n.clone());
                }
                *by_kind.entry(n).or_default() += 1;
                if seen.len().is_multiple_of(25) {
                    marks.push((seen.len(), start.elapsed().as_secs()));
                }
            }
            Event::EntityRemoved(list) => {
                for id in &list {
                    alive.remove(id);
                }
                gone += list.iter().filter(|id| seen.contains(id)).count() as u32;
            }
            _ => {}
        }
    }
    let total: u32 = by_kind
        .iter()
        .filter(|(k, _)| monsters.contains(&k.as_str()))
        .map(|(_, n)| n)
        .sum();
    println!("за {secs} с: {} сущностей, {gone} удалено", seen.len());
    if debug {
        println!("пакеты {seen_ids:?}");
    }
    println!("каждые 25: {marks:?}");
    println!(
        "монстров видно по минутам: {alive_marks:?}, в конце {}",
        alive.len()
    );
    for (k, n) in &by_kind {
        let share = if monsters.contains(&k.as_str()) && total > 0 {
            format!(" ({:.0} %)", f64::from(*n) * 100.0 / f64::from(total))
        } else {
            String::new()
        };
        println!("  {k}: {n}{share}");
    }
}
