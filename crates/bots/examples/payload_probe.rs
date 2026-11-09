//! Каналы плагинов: клиент объявляет канал и шлёт в него сообщение.
//!
//! `payload_probe <addr> <packets.json> [канал]`
//!
//! Бот отправляет `minecraft:register` с каналом (по умолчанию
//! `chunkwake:echo`), затем три байта в канал и печатает все пришедшие
//! `custom_payload`: `minecraft:register` сервера после входа и ответ
//! плагина, если он слушает канал.
use chunkwake_bots::{Client, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer};
use std::time::Duration;

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let channel = a.get(3).cloned().unwrap_or_else(|| "chunkwake:echo".into());
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let sb = table
        .id(State::Play, Direction::Serverbound, "custom_payload")
        .unwrap();
    let cb = table
        .id(State::Play, Direction::Clientbound, "custom_payload")
        .unwrap();
    let mut c = Client::connect(&a[1], "payload", ids, 777, "26.3")
        .await
        .unwrap();
    c.capture_also = vec![cb];
    let send = |ch: &str, data: &[u8]| {
        let mut w = Writer::new();
        w.var_int(sb).string(ch);
        w.buf.extend_from_slice(data);
        w.buf
    };
    tokio::time::sleep(Duration::from_secs(1)).await;
    c.send_raw(&send("minecraft:register", channel.as_bytes()))
        .await
        .unwrap();
    c.send_raw(&send(&channel, &[1, 2, 3])).await.unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(4);
    while tokio::time::Instant::now() < deadline {
        let _ = tokio::time::timeout(Duration::from_millis(200), c.next_event()).await;
        for (pid, body) in std::mem::take(&mut c.captured_by) {
            if pid != cb {
                continue;
            }
            let mut r = Reader::new(&body);
            let ch = r.string(32767).unwrap().to_owned();
            println!("custom_payload {ch} {:?}", r.remaining());
        }
    }
}
