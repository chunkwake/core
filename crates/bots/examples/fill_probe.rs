//! Время `/fill` от команды до ответа сервера: одна и та же мера у нас и у
//! ванили. Заливает куб попеременно камнем и воздухом `N` раз и печатает
//! минимум, медиану и максимум.
//!
//! `fill_probe <addr> <packets.json> <name> <x1 y1 z1 x2 y2 z2> [N]` — игрок
//! должен быть оператором, область — загружена.
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let id = |d, n| table.id(State::Play, d, n).unwrap();
    let cmd = id(Direction::Serverbound, "chat_command");
    let chat = id(Direction::Clientbound, "system_chat");
    let area = &a[4];
    let runs: usize = a.get(5).and_then(|n| n.parse().ok()).unwrap_or(20);
    let mut c = Client::connect(&a[1], &a[3], ids, 777, "26.3")
        .await
        .unwrap();
    c.capture_also.push(chat);
    loop {
        if let Event::Position(_) = c.next_event().await.unwrap() {
            break;
        }
    }
    // Дать серверу догрузить чанки вокруг и разослать стартовые сообщения.
    let settle = tokio::time::Instant::now() + Duration::from_secs(5);
    while let Ok(Ok(_)) = tokio::time::timeout_at(settle, c.next_event()).await {}
    let mut times = Vec::with_capacity(runs);
    for i in 0..runs {
        c.captured_by.clear();
        let block = if i % 2 == 0 { "stone" } else { "air" };
        let mut w = Writer::new();
        w.var_int(cmd).string(&format!("fill {area} {block}"));
        let start = Instant::now();
        c.send_raw(&w.buf).await.unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while !c.captured_by.iter().any(|(p, _)| *p == chat) {
            if tokio::time::timeout_at(deadline, c.next_event())
                .await
                .is_err()
            {
                panic!("нет ответа на /fill");
            }
        }
        times.push(start.elapsed().as_secs_f64() * 1000.0);
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    times.sort_by(f64::total_cmp);
    println!(
        "fill {area}: min {:.1} мс, медиана {:.1} мс, max {:.1} мс ({runs} раз)",
        times[0],
        times[runs / 2],
        times[runs - 1]
    );
}
