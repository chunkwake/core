//! Modern Velocity forwarding: сервер просит `velocity:player_info`, проверяет
//! HMAC-SHA256 и входит с профилем прокси, не спрашивая Mojang.

use chunkwake_net::velocity;
use chunkwake_net::{Connection, GameVersion, LoggedIn, NetConfig, Server};
use chunkwake_protocol::packets::{Handshake, NextState};
use chunkwake_protocol::{Reader, Writer};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use uuid::Uuid;

const PROTOCOL: i32 = 9998;
const SECRET: &[u8] = b"velocity-test-secret";
const UUID: Uuid = Uuid::from_u128(0x0123_4567_89ab_cdef_0011_2233_4455_6677);

async fn start() -> (std::net::SocketAddr, mpsc::UnboundedReceiver<LoggedIn>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = Server::new(NetConfig {
        version: GameVersion {
            name: "26.3".into(),
            protocol: PROTOCOL,
        },
        motd: "velocity".into(),
        max_players: 20,
        compression_threshold: -1,
        online_mode: false,
        velocity_secret: Some(SECRET.to_vec()),
        enable_status: false,
        session_server: String::new(),
        configuration: None,
        admit: None,
    });
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(server.serve(listener, move |p, _conn| {
        let _ = tx.send(p);
    }));
    (addr, rx)
}

async fn connect(addr: std::net::SocketAddr) -> Connection {
    let mut c = Connection::new(TcpStream::connect(addr).await.unwrap());
    let hs = Handshake {
        protocol_version: PROTOCOL,
        server_address: "localhost",
        server_port: addr.port(),
        next_state: NextState::Login,
    };
    c.send(&hs.encode()).await.unwrap();
    let mut w = Writer::new();
    w.var_int(0x00).string("Steve").uuid(Uuid::nil());
    c.send(&w.buf).await.unwrap();
    c
}

/// Ответ прокси: подпись (32 байта) + данные профиля.
fn forwarding(secret: &[u8], version: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(version)
        .string("203.0.113.7")
        .uuid(UUID)
        .string("Steve")
        .var_int(1)
        .string("textures")
        .string("dGV4")
        .bool(true)
        .string("c2ln");
    let mut out = velocity::sign(secret, &w.buf).to_vec();
    out.extend_from_slice(&w.buf);
    out
}

async fn request(c: &mut Connection) -> i32 {
    let p = c.read_packet().await.unwrap();
    let mut r = Reader::new(&p);
    assert_eq!(r.var_int().unwrap(), 0x04, "login plugin request");
    let id = r.var_int().unwrap();
    assert_eq!(r.string(64).unwrap(), "velocity:player_info");
    assert!(r.remaining().is_empty(), "request has no payload");
    id
}

#[tokio::test]
async fn velocity_forwarding_logs_in() {
    let (addr, mut players) = start().await;
    let mut c = connect(addr).await;
    let id = request(&mut c).await;

    let mut w = Writer::new();
    w.var_int(0x02).var_int(id).bool(true);
    w.buf.extend_from_slice(&forwarding(SECRET, 1));
    c.send(&w.buf).await.unwrap();

    let p = c.read_packet().await.unwrap();
    let mut r = Reader::new(&p);
    assert_eq!(r.var_int().unwrap(), 0x02, "login success");
    assert_eq!(r.uuid().unwrap(), UUID);
    assert_eq!(r.string(16).unwrap(), "Steve");
    assert_eq!(r.var_int().unwrap(), 1);
    assert_eq!(r.string(64).unwrap(), "textures");
    assert_eq!(r.string(32767).unwrap(), "dGV4");
    assert!(r.bool().unwrap());
    assert_eq!(r.string(1024).unwrap(), "c2ln");
    let _session_id = r.uuid().unwrap();
    assert!(r.remaining().is_empty());

    c.send(&[0x03]).await.unwrap();
    c.flush().await.unwrap();
    let player = players.recv().await.unwrap();
    assert_eq!(player.uuid, UUID);
    assert_eq!(player.name, "Steve");
    assert_eq!(player.properties.len(), 1);
}

#[tokio::test]
async fn velocity_forwards_later_versions_with_key_tail() {
    let (addr, mut players) = start().await;
    let mut c = connect(addr).await;
    let id = request(&mut c).await;

    let mut payload = Writer::new();
    payload
        .var_int(2)
        .string("203.0.113.7")
        .uuid(UUID)
        .string("Steve")
        .var_int(0)
        // Версия 2 добавляет ключ чата после свойств; он покрыт подписью,
        // но для входа не нужен.
        .bool(true)
        .uuid(UUID);
    let mut data = velocity::sign(SECRET, &payload.buf).to_vec();
    data.extend_from_slice(&payload.buf);
    let mut w = Writer::new();
    w.var_int(0x02).var_int(id).bool(true);
    w.buf.extend_from_slice(&data);
    c.send(&w.buf).await.unwrap();

    let p = c.read_packet().await.unwrap();
    let mut r = Reader::new(&p);
    assert_eq!(r.var_int().unwrap(), 0x02);
    assert_eq!(r.uuid().unwrap(), UUID);
    c.send(&[0x03]).await.unwrap();
    c.flush().await.unwrap();
    assert_eq!(players.recv().await.unwrap().uuid, UUID);
}

#[tokio::test]
async fn bad_velocity_signature_is_disconnected() {
    let (addr, _players) = start().await;
    let mut c = connect(addr).await;
    let id = request(&mut c).await;

    let mut w = Writer::new();
    w.var_int(0x02).var_int(id).bool(true);
    w.buf.extend_from_slice(&forwarding(b"wrong-secret", 1));
    c.send(&w.buf).await.unwrap();

    let p = c.read_packet().await.unwrap();
    let mut r = Reader::new(&p);
    assert_eq!(r.var_int().unwrap(), 0x00, "login disconnect");
    assert!(
        r.string(262144)
            .unwrap()
            .contains("Invalid proxy forwarding data")
    );
}

#[tokio::test]
async fn direct_client_without_velocity_is_disconnected() {
    let (addr, _players) = start().await;
    let mut c = connect(addr).await;
    let _ = request(&mut c).await;
    // Обычный клиент не знает канал: ответ «данных нет».
    let mut w = Writer::new();
    w.var_int(0x02).var_int(0).bool(false);
    c.send(&w.buf).await.unwrap();
    let p = c.read_packet().await.unwrap();
    let mut r = Reader::new(&p);
    assert_eq!(r.var_int().unwrap(), 0x00);
}
