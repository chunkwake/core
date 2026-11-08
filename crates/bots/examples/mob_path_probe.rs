//! Поиск пути зомби живым ботом: зомби в загоне-«П» из забора (выше прыжка,
//! но ниже глаз — бот ему виден) идёт к боту в обход. Печатает клетки, по
//! которым он прошёл, и время до подхода; сравнивается с `server.jar`.
//!
//! `mob_path_probe <addr> <packets.json> [секунд]`
//! `mob_path_probe uuid <имя>` — офлайн-UUID для `ops.json`.
//!
//! Имя бота — `CHUNKWAKE_BOT_NAME` (по умолчанию `pather`), ему нужен оператор.
//! `PROBE_TRACE=1` печатает точки по шаговым дельтам ванили (тик движения, x, y, z).
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use std::time::Duration;

async fn wait<T>(c: &mut Client, secs: f64, mut f: impl FnMut(&Event) -> Option<T>) -> Option<T> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs_f64(secs);
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

/// Забор с соединениями к соседям из `set`.
fn fence(p: [i32; 2], set: &[[i32; 2]]) -> String {
    let has = |dx: i32, dz: i32| set.contains(&[p[0] + dx, p[1] + dz]);
    format!(
        "oak_fence[east={},west={},north={},south={},waterlogged=false]",
        has(1, 0),
        has(-1, 0),
        has(0, -1),
        has(0, 1)
    )
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a[1] == "uuid" {
        println!("{}", chunkwake_protocol::packets::offline_uuid(&a[2]));
        return;
    }
    let secs: f64 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(40.0);
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let sb = |n: &str| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let dir = std::path::Path::new(&a[2]);
    let reg: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.with_file_name("registries.json")).unwrap(),
    )
    .unwrap();
    let zombie_ty = reg["minecraft:entity_type"]["entries"]["minecraft:zombie"]["protocol_id"]
        .as_i64()
        .unwrap() as i32;
    let name = std::env::var("CHUNKWAKE_BOT_NAME").unwrap_or_else(|_| "pather".into());
    let mut c = Client::connect(&a[1], &name, ids, 777, "26.3")
        .await
        .unwrap();
    let spawn = wait(&mut c, 30.0, |e| match e {
        Event::Position(p) => Some(*p),
        _ => None,
    })
    .await
    .unwrap();
    // Если бот погиб в прошлом прогоне, ваниль держит его на экране смерти.
    let mut respawn = Writer::new();
    respawn.var_int(sb("client_command")).var_int(0);
    c.send_raw(&respawn.buf).await.unwrap();
    let spawn = wait(&mut c, 2.0, |e| match e {
        Event::Position(p) => Some(*p),
        _ => None,
    })
    .await
    .unwrap_or(spawn);
    println!("spawn {spawn:?}");
    let cmd = |s: String| {
        let mut w = Writer::new();
        w.var_int(sb("chat_command")).string(&s);
        w.buf
    };
    let [x, y, z] = spawn.map(|v| v.floor() as i32);
    // Загон: передняя стенка z−4 от x−3 до x+3, боковые x±3 до z−8.
    let mut pen: Vec<[i32; 2]> = (-3..=3).map(|dx| [x + dx, z - 4]).collect();
    for dz in 5..=8 {
        pen.push([x - 3, z - dz]);
        pen.push([x + 3, z - dz]);
    }
    let mut setup = vec![
        "difficulty normal".to_string(),
        "time set midnight".into(),
        "gamerule spawn_monsters false".into(),
        "gamerule spawn_mobs false".into(),
        "gamerule doMobSpawning false".into(),
        "kill @e[type=minecraft:zombie]".into(),
        "effect give @s minecraft:resistance 120 4 true".into(),
        format!(
            "fill {} {y} {} {} {} {} minecraft:air",
            x - 10,
            z - 16,
            x + 10,
            y + 4,
            z + 3
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            x - 10,
            y - 1,
            z - 16,
            x + 10,
            y - 1,
            z + 3
        ),
    ];
    for p in &pen {
        setup.push(format!(
            "setblock {} {y} {} {}",
            p[0],
            p[1],
            fence(*p, &pen)
        ));
    }
    // Второй проход: если сервер пересчитал соединения по соседям в процессе.
    for p in &pen {
        setup.push(format!(
            "setblock {} {y} {} {}",
            p[0],
            p[1],
            fence(*p, &pen)
        ));
    }
    setup.push("gamemode survival".into());
    for s in setup {
        c.send_raw(&cmd(s)).await.unwrap();
    }
    let _ = wait(&mut c, 2.0, |_| None::<()>).await;
    let at = [x as f64 + 0.5, y as f64, (z - 6) as f64 + 0.5];
    c.send_raw(&cmd(format!(
        "summon minecraft:zombie {} {} {} {{PersistenceRequired:1b}}",
        at[0], at[1], at[2]
    )))
    .await
    .unwrap();
    let (id, mut pos) = wait(&mut c, 5.0, |e| match e {
        Event::EntitySpawned {
            entity_id,
            kind,
            pos,
        } if *kind == zombie_ty && (pos[0] - at[0]).abs() < 1.0 && (pos[2] - at[2]).abs() < 1.0 => {
            Some((*entity_id, *pos))
        }
        _ => None,
    })
    .await
    .expect("зомби не появился");
    println!("zombie {id} at {pos:?}");
    let start = tokio::time::Instant::now();
    let mut cells: Vec<[i32; 2]> = vec![[pos[0].floor() as i32 - x, pos[2].floor() as i32 - z]];
    let mut arrived = None;
    let mut min_y = pos[1];
    let mut max_y = pos[1];
    // Тики движения по шаговым дельтам ванили (с первого ненулевого шага).
    let mut ticks: Option<i32> = None;
    let trace = std::env::var("PROBE_TRACE").is_ok();
    'outer: while start.elapsed().as_secs_f64() < secs {
        let Some(points) = wait(&mut c, 0.5, |e| match e {
            Event::EntitySpawned { entity_id, pos, .. } if *entity_id == id => {
                Some(vec![(0, *pos)])
            }
            Event::EntityMoved {
                entity_id,
                delta,
                steps,
            } if *entity_id == id => {
                let at = |d: &[f64; 3]| [pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]];
                Some(if steps.is_empty() {
                    vec![(0, at(delta))]
                } else {
                    steps.iter().map(|(t, d)| (*t, at(d))).collect()
                })
            }
            _ => None,
        })
        .await
        else {
            continue;
        };
        for (t, p) in points {
            if p != pos || ticks.is_some() {
                *ticks.get_or_insert(0) += t;
            }
            pos = p;
            if trace {
                println!(
                    "t{:>4} {:.4} {:.4} {:.4}",
                    ticks.unwrap_or(0),
                    p[0] - spawn[0],
                    p[1] - spawn[1],
                    p[2] - spawn[2]
                );
            }
            min_y = min_y.min(p[1]);
            max_y = max_y.max(p[1]);
            let cell = [p[0].floor() as i32 - x, p[2].floor() as i32 - z];
            if cells.last() != Some(&cell) {
                cells.push(cell);
            }
            let d = ((p[0] - spawn[0]).powi(2) + (p[2] - spawn[2]).powi(2)).sqrt();
            if d < 1.5 {
                arrived = Some(start.elapsed().as_secs_f64());
                break 'outer;
            }
        }
    }
    println!("cells (dx,dz from bot): {cells:?}");
    println!("y range {min_y:.3}..{max_y:.3}");
    match arrived {
        Some(t) => println!("arrived in {t:.2}s ({:.0} ticks)", t * 20.0),
        None => println!("not arrived, last {pos:?}"),
    }
    if let Some(t) = ticks {
        println!("ticks of motion (stepped deltas): {t}");
    }
}
