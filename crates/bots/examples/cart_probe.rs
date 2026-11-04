//! Проверка вагонеток живым ботом: линия рельсов с питаемым рельсом у упора и
//! детектором; вагонетка сама разгоняется, проезжает детектор (он включается),
//! бот садится, сходит шифтом, ломает её и ставит вторую для перезапуска.
//!
//! `cart_probe <addr> <packets.json> place|check`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use std::time::Duration;

async fn wait<T>(c: &mut Client, secs: f64, mut f: impl FnMut(&Event) -> Option<T>) -> Option<T> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs_f64(secs);
    loop {
        let ev = tokio::time::timeout_at(deadline, c.next_event()).await.ok()?.ok()?;
        if let Some(v) = f(&ev) {
            return Some(v);
        }
    }
}

fn pack(p: [i32; 3]) -> i64 {
    ((p[0] as i64 & 0x3FF_FFFF) << 38) | ((p[2] as i64 & 0x3FF_FFFF) << 12) | (p[1] as i64 & 0xFFF)
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let sb = |n: &str| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let dir = std::path::Path::new(&a[2]);
    let reg: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.with_file_name("registries.json")).unwrap()).unwrap();
    let ty = |n: &str| reg["minecraft:entity_type"]["entries"][n]["protocol_id"].as_i64().unwrap() as i32;
    let (cart_ty, item_ty) = (ty("minecraft:minecart"), ty("minecraft:item"));
    let blocks: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.with_file_name("blocks.json")).unwrap()).unwrap();
    let detector_on: Vec<i32> = blocks["minecraft:detector_rail"]["states"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["properties"]["powered"] == "true")
        .map(|s| s["id"].as_i64().unwrap() as i32)
        .collect();
    let mut c = Client::connect(&a[1], "carter", ids, 777, "26.3").await.unwrap();
    let spawn = wait(&mut c, 30.0, |e| match e {
        Event::Position(p) => Some(*p),
        _ => None,
    })
    .await
    .unwrap();
    println!("spawn {spawn:?}");
    if a[3] == "check" {
        let found = wait(&mut c, 10.0, |e| match e {
            Event::EntitySpawned { entity_id, kind, pos } if *kind == cart_ty => Some((*entity_id, *pos)),
            _ => None,
        })
        .await;
        println!("cart after restart: {found:?}");
        return;
    }
    let cmd = |s: String| {
        let mut w = Writer::new();
        w.var_int(sb("chat_command")).string(&s);
        w.buf
    };
    let [x, y, z] = spawn.map(|v| v.floor() as i32);
    for s in [
        format!("fill {} {y} {} {} {} {} minecraft:air", x - 1, z + 1, x + 1, y + 3, z + 30),
        format!("fill {x} {} {} {x} {} {} minecraft:stone", y - 1, z + 1, y - 1, z + 30),
        format!("setblock {x} {y} {} minecraft:stone", z + 2),
        format!("fill {x} {y} {} {x} {y} {} minecraft:rail", z + 3, z + 30),
        format!("setblock {x} {y} {} minecraft:powered_rail", z + 3),
        format!("setblock {} {y} {} minecraft:redstone_block", x + 1, z + 3),
        format!("setblock {x} {y} {} minecraft:detector_rail", z + 5),
        "gamemode survival".into(),
        "give carter minecraft:minecart".into(),
        "give carter minecraft:minecart".into(),
    ] {
        c.send_raw(&cmd(s)).await.unwrap();
    }
    let det0 = [x, y, z + 5];
    let _ = wait(&mut c, 0.8, |e| -> Option<()> {
        match e {
            Event::BlockUpdate(p, s) if *p == det0 => println!("setup det {s}"),
            Event::BlockUpdates(v) => v.iter().filter(|(p, _)| *p == det0).for_each(|(_, s)| println!("setup det* {s}")),
            _ => {}
        }
        None
    }).await;
    let rail = [x, y, z + 3];
    let mut w = Writer::new();
    w.var_int(sb("use_item_on")).var_int(0).i64(pack(rail)).var_int(1).f32(0.5).f32(1.0).f32(0.5).u8(0).u8(0).var_int(5);
    c.send_raw(&w.buf).await.unwrap();
    let (cart, cpos) = wait(&mut c, 5.0, |e| match e {
        Event::EntitySpawned { entity_id, kind, pos } if *kind == cart_ty => Some((*entity_id, *pos)),
        _ => None,
    })
    .await
    .expect("вагонетка не появилась");
    println!("cart {cart} at {cpos:?}");
    // Сама едет на юг от упора; детектор на z+20 включается.
    let mut last = cpos;
    let mut detector = false;
    let det = [x, y, z + 5];
    let start = tokio::time::Instant::now();
    // До остановки: после старта сервер тикает медленнее, за фиксированное
    // время вагонетка могла ещё катиться.
    let mut still = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_secs(30) && still.elapsed() < Duration::from_secs(3) {
        match wait(&mut c, 0.5, |e| match e {
            Event::EntitySpawned { entity_id, pos, .. } if *entity_id == cart => Some((Some(*pos), false)),
            Event::BlockUpdate(p, s) if *p == det && detector_on.contains(s) => Some((None, true)),
            Event::BlockUpdates(v) if v.iter().any(|(p, s)| *p == det && detector_on.contains(s)) => Some((None, true)),
            _ => None,
        })
        .await
        {
            Some((Some(p), _)) => {
                if p != last {
                    still = tokio::time::Instant::now();
                }
                last = p;
            }
            Some((None, true)) => detector = true,
            _ => {}
        }
    }
    println!("cart rolled to {last:?} (from z {}) in {:.1}s, detector powered: {detector}", cpos[2], start.elapsed().as_secs_f64());

    // Сесть, сойти шифтом.
    let mut w = Writer::new();
    w.var_int(sb("interact")).var_int(cart).var_int(0).u8(0).bool(false);
    c.send_raw(&w.buf).await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    let mut w = Writer::new();
    w.var_int(sb("player_input")).u8(0x20);
    c.send_raw(&w.buf).await.unwrap();
    let off = wait(&mut c, 5.0, |e| match e {
        Event::Position(p) => Some(*p),
        _ => None,
    })
    .await;
    println!("dismounted at {off:?}");
    let mut w = Writer::new();
    w.var_int(sb("player_input")).u8(0);
    c.send_raw(&w.buf).await.unwrap();
    if let Some(off) = off {
        c.move_to(off).await.unwrap();
    }
    for _ in 0..5 {
        let mut w = Writer::new();
        w.var_int(sb("attack")).var_int(cart);
        c.send_raw(&w.buf).await.unwrap();
    }
    let (mut removed, mut dropped) = (false, false);
    while let Some((r, d)) = wait(&mut c, 3.0, |e| match e {
        Event::EntityRemoved(v) => Some((v.contains(&cart), false)),
        Event::EntitySpawned { kind, .. } if *kind == item_ty => Some((false, true)),
        _ => None,
    })
    .await
    {
        removed |= r;
        dropped |= d;
        if removed && dropped {
            break;
        }
    }
    println!("broken: removed={removed} drop={dropped}");
    // Вторая — на плоский рельс у детектора, остаётся до перезапуска.
    c.send_raw(&cmd("give carter minecraft:minecart".into())).await.unwrap();
    let mut w = Writer::new();
    w.var_int(sb("set_carried_item")).u16(1);
    c.send_raw(&w.buf).await.unwrap();
    let mut w = Writer::new();
    w.var_int(sb("use_item_on")).var_int(0).i64(pack([x, y, z + 10])).var_int(1).f32(0.5).f32(1.0).f32(0.5).u8(0).u8(0).var_int(6);
    c.send_raw(&w.buf).await.unwrap();
    let second = wait(&mut c, 5.0, |e| match e {
        Event::EntitySpawned { entity_id, kind, pos } if *kind == cart_ty => Some((*entity_id, *pos)),
        _ => None,
    })
    .await;
    println!("second cart {second:?}");
    c.send_raw(&cmd("save-all".into())).await.unwrap();
    tokio::time::sleep(Duration::from_secs(3)).await;
}
