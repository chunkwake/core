//! Пустые чанки после портала: бот строит портал, уходит в Ад, выходит из
//! портала, возвращается и разбирает каждый `level_chunk_with_light` так же,
//! как клиент. Пустой чанк (ни одного блока) или чанк с числом секций другого
//! измерения — то, что игрок видит дырой в мире.
//!
//! `portal_chunks_probe <addr> <packets.json> <name> [secs]` — бот должен
//! быть оператором.
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// Пропускает paletted container так, как его читает клиент: прямая палитра
/// до `max_indirect` бит, дальше — глобальные ID с указанной шириной.
fn skip_container(r: &mut Reader, entries: usize, max_indirect: u8) -> Result<(), String> {
    let bits = r.u8().map_err(|e| e.to_string())?;
    if bits == 0 {
        r.var_int().map_err(|e| e.to_string())?;
        return Ok(());
    }
    if bits <= max_indirect {
        let n = r.var_int().map_err(|e| e.to_string())?;
        for _ in 0..n {
            r.var_int().map_err(|e| e.to_string())?;
        }
    }
    let per = 64 / bits as usize;
    for _ in 0..entries.div_ceil(per) {
        r.i64().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// (x, z, число секций, блоков всего, блоков в нижней секции).
fn decode(body: &[u8]) -> Result<(i32, i32, usize, u32, u16), String> {
    let mut r = Reader::new(body);
    let e = |e: chunkwake_protocol::ProtocolError| e.to_string();
    let xz = r.i64().map_err(e)?;
    let (x, z) = ((xz >> 32) as i32, xz as i32);
    for _ in 0..r.var_int().map_err(e)? {
        r.var_int().map_err(e)?;
        for _ in 0..r.var_int().map_err(e)? {
            r.i64().map_err(e)?;
        }
    }
    let len = r.var_int().map_err(e)? as usize;
    let data = &r.remaining()[..len];
    let mut s = Reader::new(data);
    let (mut sections, mut total, mut first) = (0, 0u32, None);
    while !s.remaining().is_empty() {
        let non_air = s.u16().map_err(e)?;
        s.u16().map_err(e)?;
        skip_container(&mut s, 4096, 8)?;
        skip_container(&mut s, 64, 3)?;
        sections += 1;
        total += non_air as u32;
        first.get_or_insert(non_air);
    }
    Ok((x, z, sections, total, first.unwrap_or(0)))
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let id = |s, n| table.id(State::Play, s, n).unwrap();
    let cmd_id = id(Direction::Serverbound, "chat_command");
    let loaded_id = id(Direction::Serverbound, "player_loaded");
    let respawn_id = id(Direction::Clientbound, "respawn");
    let chunk_id = id(Direction::Clientbound, "level_chunk_with_light");
    let secs: u64 = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(45);
    let mut c = Client::connect(&a[1], &a[3], ids, 777, "26.3").await.unwrap();
    let t0 = Instant::now();
    let mut p = loop {
        if let Ok(Event::Position(p)) = c.next_event().await {
            break p;
        }
    };
    // `[x z]` — сначала в нетронутое место: гонка видна на свежих чанках.
    if let (Some(tx), Some(tz)) = (a.get(5), a.get(6)) {
        let mut w = Writer::new();
        w.var_int(cmd_id).string(&format!("tp @s {tx} 120 {tz}"));
        c.send_raw(&w.buf).await.unwrap();
        p = loop {
            if let Ok(Event::Position(p)) = c.next_event().await {
                break p;
            }
        };
        // Чанки вокруг новой точки.
        let until = Instant::now() + Duration::from_secs(6);
        while Instant::now() < until {
            let _ = tokio::time::timeout(Duration::from_millis(100), c.next_event()).await;
        }
    }
    let (x, y, z) = (p[0].floor() as i32, p[1].floor() as i32, p[2].floor() as i32);
    println!("start {p:?}");
    for line in [
        "gamemode creative".to_string(),
        format!("fill {} {} {} {} {} {} air", x - 3, y, z - 2, x + 4, y + 4, z + 2),
        format!("fill {} {} {} {} {} {} obsidian", x - 1, y - 1, z, x + 2, y + 3, z),
        format!("fill {} {} {} {} {} {} nether_portal[axis=x]", x, y, z, x + 1, y + 2, z),
    ] {
        let mut w = Writer::new();
        w.var_int(cmd_id).string(&line);
        c.send_raw(&w.buf).await.unwrap();
    }
    c.capture = Some(respawn_id);
    c.capture_also.push(chunk_id);
    // Фаза: 0 — дома до портала, 1 — в Аду, 2 — снова дома.
    let mut phase = 0usize;
    let mut stats: Vec<BTreeMap<String, usize>> = vec![BTreeMap::new(); 3];
    let mut empty: Vec<Vec<(i32, i32)>> = vec![Vec::new(); 3];
    let mut respawned_at: Option<Instant> = None;
    let mut arrival: Option<[f64; 3]> = None;
    let mut stepped = 0;
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end {
        match tokio::time::timeout(Duration::from_millis(100), c.next_event()).await {
            Ok(Ok(Event::Position(p))) => {
                if respawned_at.is_some() && arrival.is_none() {
                    arrival = Some(p);
                    let mut w = Writer::new();
                    w.var_int(loaded_id);
                    c.send_raw(&w.buf).await.unwrap();
                }
            }
            Ok(Err(e)) => {
                println!("err {e}");
                break;
            }
            _ => {}
        }
        for (_, body) in std::mem::take(&mut c.captured_by) {
            let key = match decode(&body) {
                Ok((cx, cz, sections, total, _)) => {
                    if total == 0 {
                        empty[phase].push((cx, cz));
                    }
                    format!("{sections} sections{}", if total == 0 { ", EMPTY" } else { "" })
                }
                Err(err) => format!("undecodable: {err}"),
            };
            *stats[phase].entry(key).or_default() += 1;
        }
        if !c.captured.is_empty() {
            c.captured.clear();
            phase = (phase + 1).min(2);
            println!("{:5.1}s RESPAWN -> phase {phase}", t0.elapsed().as_secs_f32());
            respawned_at = Some(Instant::now());
            arrival = None;
            stepped = 0;
        }
        // Выйти из портала на той стороне и войти снова — домой.
        if phase == 1
            && let (Some(t), Some(p)) = (respawned_at, arrival)
        {
            if stepped == 0 && t.elapsed() > Duration::from_secs(4) {
                c.move_to([p[0], p[1], p[2] + 2.0]).await.unwrap();
                stepped = 1;
            } else if stepped == 1 && t.elapsed() > Duration::from_secs(6) {
                c.move_to(p).await.unwrap();
                stepped = 2;
            }
        }
    }
    for (i, name) in ["home before", "nether", "home after"].iter().enumerate() {
        println!("{name}: {:?}", stats[i]);
        if !empty[i].is_empty() {
            println!("  empty chunks: {:?}", &empty[i][..empty[i].len().min(20)]);
        }
    }
}
