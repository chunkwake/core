//! Нагрузочный тест сервера: N офлайн-ботов проходят login и configuration,
//! затем ходят по кругу (20 пакетов движения в секунду) и отвечают на keep-alive.
//!
//! `chunkwake-bots [адрес] [число ботов] [packets.json]`
//! По умолчанию: `127.0.0.1:25565 100 target/debug/generated/26.3/reports/packets.json`.

use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_net::Connection;
use chunkwake_protocol::configuration::{KnownPack, select_known_packs};
use chunkwake_protocol::packets::{Handshake, NextState};
use chunkwake_protocol::{Reader, Writer};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::net::TcpStream;

const PROTOCOL: i32 = 777;
const VERSION: &str = "26.3";
/// Пауза между подключениями, чтобы не упереться в backlog сокета.
const CONNECT_SPACING: Duration = Duration::from_millis(5);

#[derive(Clone, Copy)]
struct Ids {
    cfg_known_packs: i32,
    cfg_finish: i32,
    cfg_keep_alive: i32,
    sb_cfg_known_packs: i32,
    sb_cfg_finish: i32,
    sb_cfg_keep_alive: i32,
    keep_alive: i32,
    sb_keep_alive: i32,
    sb_move_pos: i32,
}

impl Ids {
    fn load(t: &PacketTable) -> Result<Self, String> {
        use Direction::{Clientbound as C, Serverbound as S};
        use State::{Configuration as Cfg, Play};
        let id = |s, d, n| t.id(s, d, n).map_err(|e| e.to_string());
        Ok(Self {
            cfg_known_packs: id(Cfg, C, "select_known_packs")?,
            cfg_finish: id(Cfg, C, "finish_configuration")?,
            cfg_keep_alive: id(Cfg, C, "keep_alive")?,
            sb_cfg_known_packs: id(Cfg, S, "select_known_packs")?,
            sb_cfg_finish: id(Cfg, S, "finish_configuration")?,
            sb_cfg_keep_alive: id(Cfg, S, "keep_alive")?,
            keep_alive: id(Play, C, "keep_alive")?,
            sb_keep_alive: id(Play, S, "keep_alive")?,
            sb_move_pos: id(Play, S, "move_player_pos")?,
        })
    }
}

#[derive(Default)]
struct Stats {
    joined: AtomicU64,
    failed: AtomicU64,
    online: AtomicU64,
    /// Суммарное время входа (handshake → play), мкс.
    join_us: AtomicU64,
    packets_in: AtomicU64,
    bytes_in: AtomicU64,
    moves_out: AtomicU64,
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let addr = args.get(1).cloned().unwrap_or("127.0.0.1:25565".into());
    let count: usize = args.get(2).and_then(|n| n.parse().ok()).unwrap_or(100);
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

    let stats = Arc::new(Stats::default());
    println!("{count} bots → {addr}");
    for i in 0..count {
        let (addr, stats) = (addr.clone(), stats.clone());
        tokio::spawn(async move {
            if let Err(e) = bot(&addr, i, ids, &stats).await {
                stats.failed.fetch_add(1, Ordering::Relaxed);
                if stats.failed.load(Ordering::Relaxed) <= 5 {
                    eprintln!("bot {i}: {e}");
                }
            }
        });
        tokio::time::sleep(CONNECT_SPACING).await;
    }

    // Отчёт раз в секунду.
    let mut last = (0, 0, 0);
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let joined = stats.joined.load(Ordering::Relaxed);
        let now = (
            stats.packets_in.load(Ordering::Relaxed),
            stats.bytes_in.load(Ordering::Relaxed),
            stats.moves_out.load(Ordering::Relaxed),
        );
        let avg_join = stats.join_us.load(Ordering::Relaxed) / joined.max(1);
        println!(
            "online {:>5} | joined {:>5} failed {:>3} | avg join {:>6.1} ms | in {:>7} pkt/s {:>7.2} MiB/s | moves out {:>6}/s",
            stats.online.load(Ordering::Relaxed),
            joined,
            stats.failed.load(Ordering::Relaxed),
            avg_join as f64 / 1000.0,
            now.0 - last.0,
            (now.1 - last.1) as f64 / (1024.0 * 1024.0),
            now.2 - last.2,
        );
        last = now;
    }
}

async fn bot(addr: &str, i: usize, ids: Ids, stats: &Stats) -> Result<(), String> {
    let e = |e: chunkwake_net::NetError| format!("{e:?}");
    let started = Instant::now();
    let stream = TcpStream::connect(addr).await.map_err(|e| e.to_string())?;
    let _ = stream.set_nodelay(true);
    let port = stream.peer_addr().map(|a| a.port()).unwrap_or(25565);
    let mut c = Connection::new(stream);

    // Login (офлайн).
    c.send(
        &Handshake {
            protocol_version: PROTOCOL,
            server_address: "localhost",
            server_port: port,
            next_state: NextState::Login,
        }
        .encode(),
    )
    .await
    .map_err(e)?;
    let mut w = Writer::new();
    w.var_int(0x00)
        .string(&format!("bot{i}"))
        .uuid(uuid::Uuid::nil());
    c.send(&w.buf).await.map_err(e)?;
    loop {
        let p = c.read_packet().await.map_err(e)?;
        let mut r = Reader::new(&p);
        match r.var_int().map_err(|e| e.to_string())? {
            0x00 => return Err(format!("disconnected: {}", String::from_utf8_lossy(&p))),
            0x02 => break,
            0x03 => {
                let t = r.var_int().map_err(|e| e.to_string())?;
                c.codec.threshold = (t >= 0).then_some(t as usize);
            }
            id => return Err(format!("unexpected login packet {id}")),
        }
    }
    c.send(&[0x03]).await.map_err(e)?; // login_acknowledged

    // Configuration.
    loop {
        let p = c.read_packet().await.map_err(e)?;
        let mut r = Reader::new(&p);
        let id = r.var_int().map_err(|e| e.to_string())?;
        if id == ids.cfg_known_packs {
            c.send(&select_known_packs(
                ids.sb_cfg_known_packs,
                &[KnownPack::vanilla(VERSION)],
            ))
            .await
            .map_err(e)?;
        } else if id == ids.cfg_keep_alive {
            let mut w = Writer::new();
            w.var_int(ids.sb_cfg_keep_alive)
                .i64(r.i64().map_err(|e| e.to_string())?);
            c.send(&w.buf).await.map_err(e)?;
        } else if id == ids.cfg_finish {
            let mut w = Writer::new();
            w.var_int(ids.sb_cfg_finish);
            c.send(&w.buf).await.map_err(e)?;
            break;
        }
    }

    stats.joined.fetch_add(1, Ordering::Relaxed);
    stats
        .join_us
        .fetch_add(started.elapsed().as_micros() as u64, Ordering::Relaxed);
    stats.online.fetch_add(1, Ordering::Relaxed);
    let result = play(&mut c, i, ids, stats).await;
    stats.online.fetch_sub(1, Ordering::Relaxed);
    result
}

/// Ходит по кругу радиусом 8 блоков вокруг спавна; 20 движений в секунду.
async fn play(c: &mut Connection, i: usize, ids: Ids, stats: &Stats) -> Result<(), String> {
    let e = |e: chunkwake_net::NetError| format!("{e:?}");
    let mut tick = tokio::time::interval(Duration::from_millis(50));
    let mut angle = i as f64;
    loop {
        tokio::select! {
            _ = tick.tick() => {
                angle += 0.05;
                let mut w = Writer::new();
                w.var_int(ids.sb_move_pos)
                    .f64(8.5 + 8.0 * angle.cos())
                    .f64(-60.0)
                    .f64(8.5 + 8.0 * angle.sin())
                    .u8(1); // on ground
                c.send(&w.buf).await.map_err(e)?;
                stats.moves_out.fetch_add(1, Ordering::Relaxed);
            }
            p = c.read_packet() => {
                let p = p.map_err(e)?;
                stats.packets_in.fetch_add(1, Ordering::Relaxed);
                stats.bytes_in.fetch_add(p.len() as u64, Ordering::Relaxed);
                let mut r = Reader::new(&p);
                if r.var_int().map_err(|e| e.to_string())? == ids.keep_alive {
                    let mut w = Writer::new();
                    w.var_int(ids.sb_keep_alive).i64(r.i64().map_err(|e| e.to_string())?);
                    c.send(&w.buf).await.map_err(e)?;
                }
            }
        }
    }
}
