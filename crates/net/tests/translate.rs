//! Перевод протокола: клиент «чужой» версии ходит через `Translator`,
//! клиент нашей версии — мимо него.

use chunkwake_net::translate::{Dir, Translated, Translator};
use chunkwake_net::{Connection, GameVersion, LoggedIn, NetConfig, Server};
use chunkwake_protocol::packets::{Handshake, NextState};
use chunkwake_protocol::{Reader, Writer};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

const PROTOCOL: i32 = 9999;
/// Версия, которую фиктивный переводчик «знает».
const OLD: i32 = 760;
/// Клиент до 1.8: сжатия не знает.
const LEGACY: i32 = 5;
/// Пакет, на который переводчик отвечает клиенту сам, не отдавая серверу.
const ECHO_IN: u8 = 0x7F;
const ECHO_OUT: u8 = 0x7E;
/// Пакет, который переводчик отправляет по таймеру (забирается `poll`).
const SCHEDULED: u8 = 0x7D;

#[derive(Default)]
struct Fake {
    opened: AtomicUsize,
    clientbound: AtomicUsize,
    serverbound: AtomicUsize,
    polled: AtomicBool,
    closed: AtomicBool,
    /// Клиентские id пакетов в порядке перевода.
    seen: Mutex<Vec<i32>>,
}

impl Translator for Fake {
    fn open(&self, protocol: i32, _addr: SocketAddr) -> Option<u64> {
        (protocol == OLD || protocol == LEGACY).then(|| self.opened.fetch_add(1, Ordering::SeqCst) as u64 + 1)
    }

    fn translate(&self, _id: u64, dir: Dir, packet: &[u8], out: &mut Translated) -> Result<(), String> {
        match dir {
            Dir::Serverbound => {
                let first = self.serverbound.fetch_add(1, Ordering::SeqCst) == 0;
                if first {
                    // Handshake: версия клиента переписывается на серверную.
                    let hs = Handshake::decode(packet).map_err(|e| format!("{e:?}"))?;
                    assert!(hs.protocol_version == OLD || hs.protocol_version == LEGACY);
                    let hs = Handshake {
                        protocol_version: PROTOCOL,
                        ..hs
                    };
                    out.push((Dir::Serverbound, hs.encode()));
                } else if packet == [ECHO_IN] {
                    out.push((Dir::Clientbound, vec![ECHO_OUT]));
                } else {
                    out.push((Dir::Serverbound, packet.to_vec()));
                }
            }
            Dir::Clientbound => {
                self.clientbound.fetch_add(1, Ordering::SeqCst);
                let id = Reader::new(packet).var_int().unwrap();
                self.seen.lock().unwrap().push(id);
                out.push((Dir::Clientbound, packet.to_vec()));
            }
        }
        Ok(())
    }

    fn poll(&self, _id: u64, out: &mut Translated) -> Result<(), String> {
        if !self.polled.swap(true, Ordering::SeqCst) {
            out.push((Dir::Clientbound, vec![SCHEDULED]));
        }
        Ok(())
    }

    fn close(&self, _id: u64) {
        self.closed.store(true, Ordering::SeqCst);
    }
}

async fn start(
    compression: i32,
    translator: Arc<Fake>,
) -> (SocketAddr, mpsc::UnboundedReceiver<(LoggedIn, Connection)>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = Server::new(NetConfig {
        version: GameVersion {
            name: "26.3".into(),
            protocol: PROTOCOL,
        },
        motd: "hello".into(),
        max_players: 20,
        compression_threshold: compression,
        online_mode: false,
        enable_status: true,
        session_server: String::new(),
        configuration: None,
        admit: None,
    });
    assert!(server.set_translator(translator));
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(server.serve(listener, move |p, conn| {
        let _ = tx.send((p, conn));
    }));
    (addr, rx)
}

async fn connect(addr: SocketAddr, protocol: i32, next_state: NextState) -> Connection {
    let mut c = Connection::new(TcpStream::connect(addr).await.unwrap());
    let hs = Handshake {
        protocol_version: protocol,
        server_address: "localhost",
        server_port: addr.port(),
        next_state,
    };
    c.send(&hs.encode()).await.unwrap();
    c
}

#[tokio::test]
async fn old_client_status_goes_through_translator() {
    let fake = Arc::new(Fake::default());
    let (addr, _rx) = start(256, fake.clone()).await;
    let mut c = connect(addr, OLD, NextState::Status).await;

    // Переводчик ответил клиенту сам — сервер пакета не видел.
    c.send(&[ECHO_IN]).await.unwrap();
    let mut got = vec![c.read_packet().await.unwrap(), c.read_packet().await.unwrap()];
    got.sort();
    assert_eq!(got, vec![vec![SCHEDULED], vec![ECHO_OUT]]);

    c.send(&[0x00]).await.unwrap();
    let resp = c.read_packet().await.unwrap();
    let mut r = Reader::new(&resp);
    assert_eq!(r.var_int().unwrap(), 0x00);
    let json: serde_json::Value = serde_json::from_str(r.string(32767).unwrap()).unwrap();
    assert_eq!(json["version"]["protocol"], PROTOCOL);

    let mut w = Writer::new();
    w.var_int(0x01).i64(42);
    c.send(&w.buf).await.unwrap();
    let pong = c.read_packet().await.unwrap();
    assert_eq!(Reader::new(&pong).var_int().unwrap(), 0x01);

    assert_eq!(fake.opened.load(Ordering::SeqCst), 1);
    assert_eq!(*fake.seen.lock().unwrap(), vec![0x00, 0x01]);
    // Сервер закрыл соединение после pong — сессия перевода закрыта.
    for _ in 0..100 {
        if fake.closed.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(fake.closed.load(Ordering::SeqCst));
}

#[tokio::test]
async fn old_client_login_with_compression_and_shared_frames() {
    let fake = Arc::new(Fake::default());
    let (addr, mut rx) = start(64, fake.clone()).await;
    let mut c = connect(addr, OLD, NextState::Login).await;
    let mut w = Writer::new();
    w.var_int(0x00).string("Steve").uuid(uuid::Uuid::nil());
    c.send(&w.buf).await.unwrap();

    assert_eq!(c.read_packet().await.unwrap(), vec![SCHEDULED]);
    let p = c.read_packet().await.unwrap();
    let mut r = Reader::new(&p);
    assert_eq!(r.var_int().unwrap(), 0x03, "set compression");
    assert_eq!(r.var_int().unwrap(), 64);
    c.codec.threshold = Some(64);
    let p = c.read_packet().await.unwrap();
    assert_eq!(Reader::new(&p).var_int().unwrap(), 0x02, "login success");
    c.send(&[0x03]).await.unwrap();
    c.flush().await.unwrap();

    let (player, mut conn) = rx.recv().await.unwrap();
    assert_eq!(player.name, "Steve");
    assert_eq!(conn.client_protocol(), Some(OLD));

    // Общие кадры (сжатые для всех) переводятся по одному пакету.
    let codec = conn.codec;
    let mut frames = Vec::new();
    let big = {
        let mut w = Writer::new();
        w.var_int(0x40);
        w.buf.extend_from_slice(&[7u8; 300]);
        w.buf
    };
    codec.encode(&big, &mut frames);
    codec.encode(&[0x41, 1], &mut frames);
    conn.send_frames(&frames).await.unwrap();
    conn.send_owned_frames(frames.clone()).await.unwrap();

    for expected in [&big, &vec![0x41, 1], &big, &vec![0x41, 1]] {
        assert_eq!(&c.read_packet().await.unwrap(), expected);
    }
    assert_eq!(
        *fake.seen.lock().unwrap(),
        vec![0x03, 0x02, 0x40, 0x41, 0x40, 0x41]
    );
}

#[tokio::test]
async fn current_client_bypasses_translator() {
    let fake = Arc::new(Fake::default());
    let (addr, mut rx) = start(-1, fake.clone()).await;
    let mut c = connect(addr, PROTOCOL, NextState::Login).await;
    let mut w = Writer::new();
    w.var_int(0x00).string("Alex").uuid(uuid::Uuid::nil());
    c.send(&w.buf).await.unwrap();
    let p = c.read_packet().await.unwrap();
    assert_eq!(Reader::new(&p).var_int().unwrap(), 0x02, "login success");
    c.send(&[0x03]).await.unwrap();
    c.flush().await.unwrap();
    let (_, conn) = rx.recv().await.unwrap();
    assert_eq!(conn.client_protocol(), None);
    assert_eq!(fake.opened.load(Ordering::SeqCst), 0);
    assert_eq!(fake.clientbound.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn unknown_version_is_refused() {
    let fake = Arc::new(Fake::default());
    let (addr, _rx) = start(-1, fake.clone()).await;
    let mut c = connect(addr, OLD + 1, NextState::Login).await;
    let mut w = Writer::new();
    w.var_int(0x00).string("Alex").uuid(uuid::Uuid::nil());
    c.send(&w.buf).await.unwrap();
    let p = c.read_packet().await.unwrap();
    let mut r = Reader::new(&p);
    assert_eq!(r.var_int().unwrap(), 0x00, "login disconnect");
    assert!(r.string(32767).unwrap().contains("Unsupported client version"));
    assert_eq!(fake.opened.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn legacy_client_logs_in_without_compression() {
    let fake = Arc::new(Fake::default());
    let (addr, mut rx) = start(64, fake.clone()).await;
    let mut c = connect(addr, LEGACY, NextState::Login).await;
    let mut w = Writer::new();
    w.var_int(0x00).string("Steve").uuid(uuid::Uuid::nil());
    c.send(&w.buf).await.unwrap();
    assert_eq!(c.read_packet().await.unwrap(), vec![SCHEDULED]);
    let p = c.read_packet().await.unwrap();
    assert_eq!(Reader::new(&p).var_int().unwrap(), 0x02, "login success, no set_compression");
    c.send(&[0x03]).await.unwrap();
    c.flush().await.unwrap();
    let (_, conn) = rx.recv().await.unwrap();
    assert_eq!(conn.client_protocol(), Some(LEGACY));
    assert!(conn.codec.threshold.is_none());
}
