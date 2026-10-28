//! Unbounded legacy load generator using the shared protocol client.
use chunkwake_bots::{Client, Ids};
use chunkwake_data::packets::PacketTable;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

const PROTOCOL: i32 = 777;
const VERSION: &str = "26.3";

#[derive(Default)]
struct Stats {
    joined: AtomicU64,
    failed: AtomicU64,
    online: AtomicU64,
    join_us: AtomicU64,
    packets_in: AtomicU64,
    bytes_in: AtomicU64,
    moves_out: AtomicU64,
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let addr = args.get(1).cloned().unwrap_or("127.0.0.1:25565".into());
    let count = args
        .get(2)
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(100);
    let table = args
        .get(3)
        .cloned()
        .unwrap_or("target/debug/generated/26.3/reports/packets.json".into());
    let ids = PacketTable::load(table.as_ref())
        .map_err(|e| e.to_string())
        .and_then(|t| Ids::load(&t))
        .unwrap_or_else(|e| {
            eprintln!("error: {table}: {e}");
            std::process::exit(1);
        });
    let spread = args
        .get(4)
        .and_then(|n| n.parse::<f64>().ok())
        .unwrap_or(0.0);
    let stats = Arc::new(Stats::default());
    println!("{count} bots → {addr}");
    for i in 0..count {
        let (addr, stats) = (addr.clone(), stats.clone());
        tokio::spawn(async move {
            let result = async {
                let mut client = Client::connect(&addr, &format!("bot{i}"), ids, PROTOCOL, VERSION).await?;
                stats.joined.fetch_add(1, Ordering::Relaxed);
                stats.join_us.fetch_add(client.join_time.as_micros() as u64, Ordering::Relaxed);
                stats.online.fetch_add(1, Ordering::Relaxed);
                let mut tick = tokio::time::interval(Duration::from_millis(50));
                let mut angle = i as f64;
                let h = (i as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
                let unit = |bits: u64| (bits % 10_000) as f64 / 10_000.0 * 2.0 - 1.0;
                let (ox, oz) = (8.5 + spread * unit(h), 8.5 + spread * unit(h >> 32));
                let result: Result<(), String> = loop {
                    tokio::select! {
                        _ = tick.tick() => {
                            angle += 0.05;
                            if let Err(e) = client.move_to([ox + 8.0 * angle.cos(), -60.0, oz + 8.0 * angle.sin()]).await { break Err(e); }
                            stats.moves_out.fetch_add(1, Ordering::Relaxed);
                        }
                        event = client.next_event() => {
                            if let Err(e) = event { break Err(e); }
                            stats.packets_in.fetch_add(1, Ordering::Relaxed);
                            stats.bytes_in.fetch_add(client.bytes_in, Ordering::Relaxed);
                            client.bytes_in = 0;
                        }
                    }
                };
                stats.online.fetch_sub(1, Ordering::Relaxed);
                result
            }.await;
            if let Err(e) = result {
                let n = stats.failed.fetch_add(1, Ordering::Relaxed) + 1;
                if n <= 5 {
                    eprintln!("bot {i}: {e}");
                }
            }
        });
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let mut last = (0, 0, 0);
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let joined = stats.joined.load(Ordering::Relaxed);
        let now = (
            stats.packets_in.load(Ordering::Relaxed),
            stats.bytes_in.load(Ordering::Relaxed),
            stats.moves_out.load(Ordering::Relaxed),
        );
        println!(
            "online {:>5} | joined {:>5} failed {:>3} | avg join {:>6.1} ms | in {:>7} pkt/s {:>7.2} MiB/s | moves out {:>6}/s",
            stats.online.load(Ordering::Relaxed),
            joined,
            stats.failed.load(Ordering::Relaxed),
            stats.join_us.load(Ordering::Relaxed) as f64 / joined.max(1) as f64 / 1000.0,
            now.0 - last.0,
            (now.1 - last.1) as f64 / (1024.0 * 1024.0),
            now.2 - last.2
        );
        last = now;
    }
}
