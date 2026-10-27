//! Интеграционные тесты: настоящий TCP-клиент проходит status и login.

use chunkwake_net::{Connection, GameVersion, LoggedIn, NetConfig, Server};
use chunkwake_protocol::packets::{Handshake, NextState};
use chunkwake_protocol::{Reader, Writer};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

const PROTOCOL: i32 = 9999;

async fn start(compression: i32) -> (std::net::SocketAddr, mpsc::UnboundedReceiver<LoggedIn>) {
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
        session_server: String::new(),
        configuration: None,
    });
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(server.serve(listener, move |p, _conn| {
        let _ = tx.send(p);
    }));
    (addr, rx)
}

async fn connect(addr: std::net::SocketAddr, next_state: NextState) -> Connection {
    let mut c = Connection::new(TcpStream::connect(addr).await.unwrap());
    let hs = Handshake {
        protocol_version: PROTOCOL,
        server_address: "localhost",
        server_port: addr.port(),
        next_state,
    };
    c.send(&hs.encode()).await.unwrap();
    c
}

#[tokio::test]
async fn status_and_ping() {
    let (addr, _rx) = start(256).await;
    let mut c = connect(addr, NextState::Status).await;
    c.send(&[0x00]).await.unwrap();
    let resp = c.read_packet().await.unwrap();
    let mut r = Reader::new(&resp);
    assert_eq!(r.var_int().unwrap(), 0x00);
    let json: serde_json::Value = serde_json::from_str(r.string(32767).unwrap()).unwrap();
    assert_eq!(json["version"]["protocol"], PROTOCOL);
    assert_eq!(json["version"]["name"], "26.3");
    assert_eq!(json["description"]["text"], "hello");

    let mut w = Writer::new();
    w.var_int(0x01).i64(0x1234_5678);
    c.send(&w.buf).await.unwrap();
    let pong = c.read_packet().await.unwrap();
    let mut r = Reader::new(&pong);
    assert_eq!(r.var_int().unwrap(), 0x01);
    assert_eq!(r.i64().unwrap(), 0x1234_5678);
}

#[tokio::test]
async fn offline_login_with_compression() {
    let (addr, mut rx) = start(64).await;
    let mut c = connect(addr, NextState::Login).await;
    let mut w = Writer::new();
    w.var_int(0x00).string("Steve").uuid(uuid::Uuid::nil());
    c.send(&w.buf).await.unwrap();

    let p = c.read_packet().await.unwrap();
    let mut r = Reader::new(&p);
    assert_eq!(r.var_int().unwrap(), 0x03, "set compression");
    assert_eq!(r.var_int().unwrap(), 64);
    c.codec.threshold = Some(64);

    let p = c.read_packet().await.unwrap();
    let mut r = Reader::new(&p);
    assert_eq!(r.var_int().unwrap(), 0x02, "login success");
    let uuid = r.uuid().unwrap();
    assert_eq!(r.string(16).unwrap(), "Steve");
    assert_eq!(r.var_int().unwrap(), 0, "no properties");
    let _session_id = r.uuid().unwrap();
    assert!(r.remaining().is_empty());
    assert_eq!(uuid, chunkwake_protocol::packets::offline_uuid("Steve"));

    c.send(&[0x03]).await.unwrap(); // login acknowledged
    let player = rx.recv().await.unwrap();
    assert_eq!(player.name, "Steve");
    assert_eq!(player.uuid, uuid);
}

#[tokio::test]
async fn wrong_version_is_rejected() {
    let (addr, _rx) = start(-1).await;
    let mut c = Connection::new(TcpStream::connect(addr).await.unwrap());
    let hs = Handshake {
        protocol_version: 1,
        server_address: "x",
        server_port: 1,
        next_state: NextState::Login,
    };
    c.send(&hs.encode()).await.unwrap();
    let p = c.read_packet().await.unwrap();
    let mut r = Reader::new(&p);
    assert_eq!(r.var_int().unwrap(), 0x00, "login disconnect");
    assert!(r.string(262144).unwrap().contains("26.3"));
}
