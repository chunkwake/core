//! Проба входа клиентом 1.21.1 (767) через ViaBackwards: login до
//! `login_success` и первые пакеты configuration в формате 1.21.1.
//!
//! `cargo run -p chunkwake-bots --example via_login_probe -- 127.0.0.1:25565`

use chunkwake_net::Connection;
use chunkwake_protocol::packets::{Handshake, NextState};
use chunkwake_protocol::{Reader, Writer};
use tokio::net::TcpStream;

const PROTOCOL: i32 = 767;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:25565".into());
    let stream = TcpStream::connect(&addr).await.unwrap();
    let port = stream.peer_addr().unwrap().port();
    let mut c = Connection::new(stream);
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
    .unwrap();
    let mut w = Writer::new();
    w.var_int(0x00).string("OldBot").uuid(uuid::Uuid::nil());
    c.send(&w.buf).await.unwrap();
    loop {
        let p = c.read_packet().await.expect("login");
        let mut r = Reader::new(&p);
        match r.var_int().unwrap() {
            0x03 => {
                let t = r.var_int().unwrap();
                println!("login: set_compression {t}");
                c.codec.threshold = (t >= 0).then_some(t as usize);
            }
            0x02 => {
                let uuid = r.uuid().unwrap();
                let name = r.string(16).unwrap();
                println!("login: success {name} {uuid}");
                break;
            }
            0x00 => panic!("login: disconnect {}", r.string(262144).unwrap()),
            id => println!("login: packet 0x{id:02x}"),
        }
    }
    // login_acknowledged → configuration (1.21.1: 0x0E select_known_packs,
    // 0x07 registry_data, 0x03 finish_configuration).
    c.send(&[0x03]).await.unwrap();
    c.flush().await.unwrap();
    let mut registries = 0;
    for _ in 0..200 {
        let p = match c.read_packet().await {
            Ok(p) => p,
            Err(e) => panic!("configuration: {e:?}"),
        };
        let mut r = Reader::new(&p);
        match r.var_int().unwrap() {
            0x07 => registries += 1,
            0x0E => {
                println!("configuration: select_known_packs");
                // Клиент без общих паков: сервер шлёт реестры целиком.
                let mut w = Writer::new();
                w.var_int(0x07).var_int(0);
                c.send(&w.buf).await.unwrap();
                c.flush().await.unwrap();
            }
            0x03 => {
                println!("configuration: finish after {registries} registries");
                c.send(&[0x03]).await.unwrap();
                c.flush().await.unwrap();
                play(&mut c).await;
                return;
            }
            0x02 => panic!("configuration: disconnect"),
            id => println!("configuration: packet 0x{id:02x} ({} bytes)", p.len()),
        }
    }
    panic!("configuration did not finish");
}

/// Play в формате 1.21.1: несколько секунд пакетов мира, ответы на
/// keep_alive и chunk_batch_finished; отключение — провал.
async fn play(c: &mut Connection) {
    use std::collections::BTreeMap;
    let mut seen: BTreeMap<i32, usize> = BTreeMap::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    while let Ok(packet) = tokio::time::timeout_at(deadline, c.read_packet()).await {
        let p = packet.expect("play");
        let mut r = Reader::new(&p);
        let id = r.var_int().unwrap();
        *seen.entry(id).or_default() += 1;
        match id {
            // disconnect
            0x1D => panic!("play: disconnect {:?}", &p[1..p.len().min(200)]),
            // keep_alive → serverbound keep_alive
            0x26 => {
                let mut w = Writer::new();
                w.var_int(0x18).i64(r.i64().unwrap());
                c.send(&w.buf).await.unwrap();
                c.flush().await.unwrap();
            }
            // chunk_batch_finished → chunk_batch_received
            0x0C => {
                let mut w = Writer::new();
                w.var_int(0x08);
                w.buf.extend_from_slice(&25.0f32.to_be_bytes());
                c.send(&w.buf).await.unwrap();
                c.flush().await.unwrap();
            }
            _ => {}
        }
    }
    let login = seen.get(&0x2B).copied().unwrap_or(0);
    let chunks = seen.get(&0x27).copied().unwrap_or(0);
    println!("play: login {login}, chunks {chunks}, packets {seen:x?}");
    assert_eq!(login, 1, "play login");
    assert!(chunks > 0, "chunks");
}
