//! Отладка команд: шлёт команды по очереди и печатает обновления блоков в
//! точке (координаты относительно точки появления).
//!
//! `cmd_probe <addr> <packets.json> <dx,dy,dz> <cmd>...`; в командах `~x ~y ~z`
//! заменяются на точку появления.
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use std::time::Duration;

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let sb = table.id(State::Play, Direction::Serverbound, "chat_command").unwrap();
    let d: Vec<i32> = a[3].split(',').map(|v| v.parse().unwrap()).collect();
    let mut c = Client::connect(&a[1], "carter", ids, 777, "26.3").await.unwrap();
    let spawn = loop {
        if let Event::Position(p) = c.next_event().await.unwrap() {
            break p;
        }
    };
    let [x, y, z] = spawn.map(|v| v.floor() as i32);
    let at = [x + d[0], y + d[1], z + d[2]];
    for s in &a[4..] {
        let s = s.replace("~x", &x.to_string()).replace("~y", &y.to_string()).replace("~z", &z.to_string());
        println!("> {s}");
        let mut w = Writer::new();
        w.var_int(sb).string(&s);
        c.send_raw(&w.buf).await.unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_millis(400);
        while let Ok(Ok(ev)) = tokio::time::timeout_at(deadline, c.next_event()).await {
            match ev {
                Event::BlockUpdate(p, st) if p == at => println!("  {st}"),
                Event::BlockUpdates(v) => v.iter().filter(|(p, _)| *p == at).for_each(|(_, st)| println!("  * {st}")),
                _ => {}
            }
        }
    }
}
