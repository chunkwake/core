//! Крипер, скелет и паук живым ботом; сравнивается с `server.jar`.
//!
//! `monster_probe <addr> <packets.json> creeper|skeleton|spider|spider_day [сложность]`
//!
//! - `creeper`: крипер в трёх блоках. Печатает тики от `DATA_SWELL_DIR = 1`
//!   до взрыва (удаления) и сколько блоков пола стало воздухом.
//! - `skeleton`: скелет в десяти блоках. Печатает тики между стрелами и
//!   разосланы ли лук (`set_equipment`) и натяжение (флаг 8).
//! - `spider`: бот в кольце стен в три блока, паук снаружи; печатает высшую точку
//!   и время подхода.
//! - `spider_day`: полдень, паук в шести блоках: подходит ли за 15 секунд.
//!
//! Моб призывается без NBT: иначе ваниль пропускает `finalizeSpawn` и
//! скелет остаётся без лука.
//!
//! Имя бота — `CHUNKWAKE_BOT_NAME` (по умолчанию `pather`), ему нужен оператор.
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer};
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

/// Поле `set_entity_data`: (индекс, тип, первое число значения).
type Field = (u8, i32, i64);

/// Поля `set_entity_data` сущности.
fn meta(body: &[u8]) -> Option<(i32, Vec<Field>)> {
    let mut r = Reader::new(body);
    let id = r.var_int().ok()?;
    let mut out = Vec::new();
    loop {
        let index = r.u8().ok()?;
        if index == 0xff {
            break;
        }
        let ty = r.var_int().ok()?;
        let v = match ty {
            0 => i64::from(r.u8().ok()?),
            1 => i64::from(r.var_int().ok()?),
            3 => r.f32().ok()? as i64,
            8 => i64::from(r.u8().ok()?),
            // Остальное не разбираем: дальше не прочитать.
            _ => {
                out.push((index, ty, 0));
                break;
            }
        };
        out.push((index, ty, v));
    }
    Some((id, out))
}

fn ticks(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 20.0
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mode = a[3].as_str();
    let difficulty = a.get(4).map_or("normal", |s| s.as_str());
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let sb = |n: &str| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n: &str| table.id(State::Play, Direction::Clientbound, n).unwrap();
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
    let species = match mode {
        "creeper" => "minecraft:creeper",
        "skeleton" => "minecraft:skeleton",
        _ => "minecraft:spider",
    };
    let mob_ty = ty(species);
    let arrow_ty = ty("minecraft:arrow");
    let name = std::env::var("CHUNKWAKE_BOT_NAME").unwrap_or_else(|_| "pather".into());
    let mut c = Client::connect(&a[1], &name, ids, 777, "26.3")
        .await
        .unwrap();
    let (data_id, equip_id) = (cb("set_entity_data"), cb("set_equipment"));
    c.capture_also = vec![data_id, equip_id];
    let spawn = wait(&mut c, 30.0, |e| match e {
        Event::Position(p) => Some(*p),
        _ => None,
    })
    .await
    .unwrap();
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
    let mut setup = vec![
        format!("difficulty {difficulty}"),
        if mode == "spider_day" {
            "time set noon".to_string()
        } else {
            "time set midnight".to_string()
        },
        "weather clear".into(),
        "gamerule spawn_monsters false".into(),
        "gamerule spawn_mobs false".into(),
        "gamerule doMobSpawning false".into(),
        "kill @e[type=!minecraft:player]".into(),
        "effect give @s minecraft:resistance 120 4 true".into(),
        format!(
            "fill {} {y} {} {} {} {} minecraft:air",
            x - 12,
            z - 16,
            x + 12,
            y + 6,
            z + 4
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            x - 12,
            y - 2,
            z - 16,
            x + 12,
            y - 1,
            z + 4
        ),
    ];
    if mode == "spider" {
        // Кольцо выше прыжка вокруг бота: обойти нельзя, только перелезть.
        setup.push(format!(
            "fill {} {y} {} {} {} {} minecraft:stone hollow",
            x - 3,
            z - 3,
            x + 3,
            y + 2,
            z + 3
        ));
    }
    setup.push("gamemode survival".into());
    for s in setup {
        c.send_raw(&cmd(s)).await.unwrap();
    }
    let _ = wait(&mut c, 2.0, |_| None::<()>).await;
    c.captured_by.clear();
    let dz = match mode {
        "creeper" => 3,
        "skeleton" => 10,
        _ => 6,
    };
    let at = [x as f64 + 0.5, y as f64, (z - dz) as f64 + 0.5];
    c.send_raw(&cmd(format!(
        "summon {species} {} {} {}",
        at[0], at[1], at[2]
    )))
    .await
    .unwrap();
    let (id, mut pos) = wait(&mut c, 5.0, |e| match e {
        Event::EntitySpawned {
            entity_id,
            kind,
            pos,
        } if *kind == mob_ty => Some((*entity_id, *pos)),
        _ => None,
    })
    .await
    .expect("моб не появился");
    let start = Instant::now();
    println!("{species} {id} at {pos:?}");
    let secs = match mode {
        "creeper" => 10.0,
        "skeleton" => 20.0,
        _ => 25.0,
    };
    let mut swell_at = None;
    let mut boom_at = None;
    let mut arrows = Vec::new();
    // Без NBT `/summon` вызывает `finalizeSpawn`: скелет получает лук.
    let mut seen = std::collections::HashSet::new();
    let mut bow = false;
    let mut drawing = 0;
    let mut air = 0;
    let mut top = pos[1];
    let mut arrived = None;
    let mut fields = std::collections::BTreeSet::new();
    let floor_y = y - 1;
    let home = [spawn[0], spawn[2]];
    while start.elapsed().as_secs_f64() < secs {
        let ev = match tokio::time::timeout(Duration::from_millis(250), c.next_event()).await {
            Ok(Ok(ev)) => ev,
            _ => continue,
        };
        for (pid, body) in std::mem::take(&mut c.captured_by) {
            if pid == equip_id {
                if Reader::new(&body).var_int().ok() == Some(id) {
                    bow = true;
                }
            } else if let Some((eid, list)) = meta(&body)
                && eid == id
            {
                for (i, t, v) in list {
                    fields.insert((i, t));
                    if mode == "creeper" && i == 16 && v == 1 && swell_at.is_none() {
                        swell_at = Some(ticks(start));
                    }
                    if mode == "skeleton" && i == 8 && v & 1 == 1 {
                        drawing += 1;
                    }
                }
            }
        }
        match ev {
            Event::EntitySpawned {
                entity_id, kind, ..
            } if kind == arrow_ty && entity_id != id && seen.insert(entity_id) => {
                if std::env::var("PROBE_TRACE").is_ok() {
                    println!("arrow {entity_id} t{:.0}", ticks(start));
                }
                arrows.push(ticks(start))
            }
            Event::EntitySpawned {
                entity_id, pos: p, ..
            } if entity_id == id => pos = p,
            Event::EntityMoved {
                entity_id, delta, ..
            } if entity_id == id => {
                pos = [pos[0] + delta[0], pos[1] + delta[1], pos[2] + delta[2]];
            }
            Event::EntityRemoved(gone) if gone.contains(&id) => {
                boom_at = Some(ticks(start));
                if mode == "creeper" {
                    // Блоки воронки приходят следом.
                    let _ = wait(&mut c, 1.0, |e| {
                        match e {
                            Event::BlockUpdate(p, 0) if p[1] <= floor_y => air += 1,
                            Event::BlockUpdates(list) => {
                                air += list
                                    .iter()
                                    .filter(|(p, s)| *s == 0 && p[1] <= floor_y)
                                    .count()
                            }
                            _ => {}
                        }
                        None::<()>
                    })
                    .await;
                    break;
                }
            }
            _ => {}
        }
        top = top.max(pos[1]);
        let d = ((pos[0] - home[0]).powi(2) + (pos[2] - home[1]).powi(2)).sqrt();
        if arrived.is_none() && d < 2.0 {
            arrived = Some(ticks(start));
            if mode.starts_with("spider") {
                break;
            }
        }
    }
    println!("fields {fields:?}");
    match mode {
        "creeper" => {
            println!("swell_at {swell_at:?} boom_at {boom_at:?}");
            if let (Some(a), Some(b)) = (swell_at, boom_at) {
                println!("fuse ticks {:.1}", b - a);
            }
            println!("floor blocks to air {air}");
        }
        "skeleton" => {
            let gaps: Vec<i64> = arrows
                .windows(2)
                .map(|w| (w[1] - w[0]).round() as i64)
                .collect();
            println!(
                "bow {bow} drawing {drawing} arrows {} gaps {gaps:?}",
                arrows.len()
            );
            println!("first arrow at {:?}", arrows.first().map(|t| t.round()));
        }
        _ => {
            println!(
                "top {:.3} (floor {}), arrived {:?}, last {:?}",
                top,
                y,
                arrived.map(|t| t.round()),
                [pos[0] - spawn[0], pos[1] - spawn[1], pos[2] - spawn[2]]
            );
        }
    }
}
