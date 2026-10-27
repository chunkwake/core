//! Online-mode: клиент шифрует секрет RSA-ключом сервера, дальше трафик идёт
//! через AES/CFB8, а сервер проверяет игрока у (поддельного) session server.

use chunkwake_net::{Connection, GameVersion, LoggedIn, NetConfig, Server};
use chunkwake_protocol::packets::{Handshake, NextState, server_hash};
use chunkwake_protocol::{Reader, Writer};
use rsa::pkcs8::DecodePublicKey;
use rsa::{Pkcs1v15Encrypt, RsaPublicKey};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

const PROTOCOL: i32 = 9999;
const PROFILE_ID: &str = "069a79f444e94726a5befca90e38aaf5";

/// Отвечает 200 с профилем, если `serverId` совпал с ожидаемым, иначе 204.
async fn mock_session_server() -> (
    String,
    mpsc::UnboundedSender<String>,
    mpsc::UnboundedReceiver<String>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (expect_tx, mut expect_rx) = mpsc::unbounded_channel::<String>();
    let (seen_tx, seen_rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        loop {
            let (mut s, _) = listener.accept().await.unwrap();
            let mut req = Vec::new();
            let mut buf = [0u8; 1024];
            while !req.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = s.read(&mut buf).await.unwrap();
                if n == 0 {
                    break;
                }
                req.extend_from_slice(&buf[..n]);
            }
            let line = String::from_utf8_lossy(&req)
                .lines()
                .next()
                .unwrap_or("")
                .to_owned();
            let expected = expect_rx.recv().await.unwrap();
            // Порядок параметров: username, затем serverId (последний перед " HTTP/1.1").
            let ok =
                line.contains("username=Steve&") && line.contains(&format!("serverId={expected} "));
            let _ = seen_tx.send(line);
            let resp = if ok {
                let body = format!(
                    r#"{{"id":"{PROFILE_ID}","name":"Steve","properties":[{{"name":"textures","value":"dGV4","signature":"c2ln"}}]}}"#
                );
                format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                )
            } else {
                "HTTP/1.1 204 No Content\r\nconnection: close\r\n\r\n".to_owned()
            };
            s.write_all(resp.as_bytes()).await.unwrap();
        }
    });
    (base, expect_tx, seen_rx)
}

async fn start(
    session_server: String,
) -> (std::net::SocketAddr, mpsc::UnboundedReceiver<LoggedIn>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = Server::new(NetConfig {
        version: GameVersion {
            name: "26.3".into(),
            protocol: PROTOCOL,
        },
        motd: "hello".into(),
        max_players: 20,
        compression_threshold: 64,
        online_mode: true,
        session_server,
        configuration: None,
    });
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(server.serve(listener, move |p, _conn| {
        let _ = tx.send(p);
    }));
    (addr, rx)
}

/// Проходит обмен ключами; `tamper` портит хэш, чтобы session server отказал.
async fn login(
    addr: std::net::SocketAddr,
    expect: &mpsc::UnboundedSender<String>,
    tamper: bool,
) -> Connection {
    let mut c = Connection::new(TcpStream::connect(addr).await.unwrap());
    let hs = Handshake {
        protocol_version: PROTOCOL,
        server_address: "localhost",
        server_port: addr.port(),
        next_state: NextState::Login,
    };
    c.send(&hs.encode()).await.unwrap();
    let mut w = Writer::new();
    w.var_int(0x00).string("Steve").uuid(uuid::Uuid::nil());
    c.send(&w.buf).await.unwrap();

    let p = c.read_packet().await.unwrap();
    let mut r = Reader::new(&p);
    assert_eq!(r.var_int().unwrap(), 0x01, "encryption request");
    assert_eq!(r.string(20).unwrap(), "");
    let n = r.var_int().unwrap() as usize;
    let der = r.bytes(n).unwrap().to_vec();
    let n = r.var_int().unwrap() as usize;
    let token = r.bytes(n).unwrap().to_vec();
    assert!(r.bool().unwrap(), "should authenticate");

    let public = RsaPublicKey::from_public_key_der(&der).unwrap();
    let secret = [0x5au8; 16];
    let mut rng = rand::thread_rng();
    let enc_secret = public.encrypt(&mut rng, Pkcs1v15Encrypt, &secret).unwrap();
    let enc_token = public.encrypt(&mut rng, Pkcs1v15Encrypt, &token).unwrap();
    let hash = server_hash("", &secret, &der);
    expect
        .send(if tamper { "bogus".into() } else { hash })
        .unwrap();

    let mut w = Writer::new();
    w.var_int(0x01)
        .byte_array(&enc_secret)
        .byte_array(&enc_token);
    c.send(&w.buf).await.unwrap();
    c.enable_encryption(&secret);
    c
}

#[tokio::test]
async fn online_login_success() {
    let (base, expect, mut seen) = mock_session_server().await;
    let (addr, mut players) = start(base).await;
    let mut c = login(addr, &expect, false).await;

    let p = c.read_packet().await.unwrap();
    let mut r = Reader::new(&p);
    assert_eq!(r.var_int().unwrap(), 0x03, "set compression (encrypted)");
    assert_eq!(r.var_int().unwrap(), 64);
    c.codec.threshold = Some(64);

    let p = c.read_packet().await.unwrap();
    let mut r = Reader::new(&p);
    assert_eq!(r.var_int().unwrap(), 0x02, "login success");
    let uuid = r.uuid().unwrap();
    assert_eq!(uuid, uuid::Uuid::parse_str(PROFILE_ID).unwrap());
    assert_eq!(r.string(16).unwrap(), "Steve");
    assert_eq!(r.var_int().unwrap(), 1, "one property");
    assert_eq!(r.string(64).unwrap(), "textures");
    assert_eq!(r.string(32767).unwrap(), "dGV4");
    assert!(r.bool().unwrap());
    assert_eq!(r.string(1024).unwrap(), "c2ln");
    let _session_id = r.uuid().unwrap();
    assert!(r.remaining().is_empty(), "nothing after sessionId");

    c.send(&[0x03]).await.unwrap();
    let player = players.recv().await.unwrap();
    assert_eq!(player.uuid, uuid);
    assert_eq!(player.properties.len(), 1);
    assert!(
        seen.recv()
            .await
            .unwrap()
            .starts_with("GET /session/minecraft/hasJoined?")
    );
}

#[tokio::test]
async fn online_login_rejected_by_session_server() {
    let (base, expect, _seen) = mock_session_server().await;
    let (addr, _players) = start(base).await;
    let mut c = login(addr, &expect, true).await;

    let p = c.read_packet().await.unwrap();
    let mut r = Reader::new(&p);
    assert_eq!(r.var_int().unwrap(), 0x00, "login disconnect (encrypted)");
    assert!(
        r.string(262144)
            .unwrap()
            .contains("Failed to verify username")
    );
}
