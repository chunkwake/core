//! Repro: go to the Nether through a portal built by commands, then build an
//! empty frame there and light it with flint and steel. Prints block updates
//! inside the frame.
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let id = |dir, name| table.id(State::Play, dir, name).unwrap();
    let cmd_id = id(Direction::Serverbound, "chat_command");
    let loaded_id = id(Direction::Serverbound, "player_loaded");
    let use_on_id = id(Direction::Serverbound, "use_item_on");
    let respawn_id = id(Direction::Clientbound, "respawn");
    let mut c = Client::connect(&a[1], &a[3], ids, 777, "26.3").await.unwrap();
    let t0 = Instant::now();
    let cmd = |line: String| {
        let mut w = Writer::new();
        w.var_int(cmd_id).string(&line);
        w.buf
    };
    let p = loop {
        if let Ok(Event::Position(p)) = c.next_event().await {
            break p;
        }
    };
    let (x, y, z) = (p[0].floor() as i32, p[1].floor() as i32, p[2].floor() as i32);
    for line in [
        "gamemode creative".to_string(),
        format!("fill {} {} {} {} {} {} air", x - 3, y, z - 2, x + 4, y + 4, z + 2),
        format!("fill {} {} {} {} {} {} obsidian", x - 1, y - 1, z, x + 2, y + 3, z),
        format!("fill {} {} {} {} {} {} nether_portal[axis=x]", x, y, z, x + 1, y + 2, z),
    ] {
        c.send_raw(&cmd(line)).await.unwrap();
    }
    c.capture = Some(respawn_id);
    let mut frame: Option<[i32; 3]> = None;
    let mut arrived = false;
    let end = Instant::now() + Duration::from_secs(25);
    let mut lit_at: Option<Instant> = None;
    while Instant::now() < end {
        match tokio::time::timeout(Duration::from_millis(100), c.next_event()).await {
            Ok(Ok(Event::Position(p))) if !c.captured.is_empty() && !arrived => {
                arrived = true;
                println!("{:5.1}s arrived {p:?}", t0.elapsed().as_secs_f32());
                let mut w = Writer::new();
                w.var_int(loaded_id);
                c.send_raw(&w.buf).await.unwrap();
                tokio::time::sleep(Duration::from_secs(1)).await;
                let (x, y, z) = (p[0].floor() as i32 + 5, p[1].floor() as i32, p[2].floor() as i32 + 3);
                frame = Some([x, y, z]);
                // Stand outside the arrival portal first.
                c.move_to([x as f64 + 0.5, y as f64, z as f64 - 2.5]).await.unwrap();
                for line in [
                    format!("fill {} {} {} {} {} {} air", x - 2, y, z - 3, x + 3, y + 4, z + 1),
                    format!("fill {} {} {} {} {} {} obsidian", x - 2, y - 2, z - 3, x + 3, y - 2, z + 1),
                    format!("fill {} {} {} {} {} {} obsidian", x - 1, y - 1, z, x + 2, y + 3, z),
                    format!("fill {} {} {} {} {} {} air", x, y, z, x + 1, y + 2, z),
                    "item replace entity @s weapon.mainhand with flint_and_steel".to_string(),
                ] {
                    c.send_raw(&cmd(line)).await.unwrap();
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
                let mut w = Writer::new();
                let pos = [x, y - 1, z];
                let packed = ((pos[0] as i64 & 0x3ff_ffff) << 38) | ((pos[2] as i64 & 0x3ff_ffff) << 12) | (pos[1] as i64 & 0xfff);
                w.var_int(use_on_id).var_int(0).i64(packed).var_int(1).f32(0.5).f32(1.0).f32(0.5).u8(0).u8(0).var_int(1);
                c.send_raw(&w.buf).await.unwrap();
                lit_at = Some(Instant::now());
                println!("{:5.1}s used flint and steel on {pos:?}", t0.elapsed().as_secs_f32());
            }
            Ok(Ok(Event::BlockUpdate(at, state))) => report(frame, at, state, &t0),
            Ok(Ok(Event::BlockUpdates(list))) => {
                for (at, state) in list {
                    report(frame, at, state, &t0);
                }
            }
            Ok(Err(e)) => {
                println!("err {e}");
                break;
            }
            _ => {}
        }
        if lit_at.is_some_and(|t| t.elapsed() > Duration::from_secs(3)) {
            break;
        }
    }
}

fn report(frame: Option<[i32; 3]>, at: [i32; 3], state: i32, t0: &Instant) {
    let Some([x, y, z]) = frame else { return };
    if at[2] == z && (x..=x + 1).contains(&at[0]) && (y..=y + 2).contains(&at[1]) {
        println!("{:5.1}s update {at:?} -> state {state}", t0.elapsed().as_secs_f32());
    }
}
