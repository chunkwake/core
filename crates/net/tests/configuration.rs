//! Фаза configuration: known packs → registry_data → update_tags → finish.

use chunkwake_data::packets::PacketTable;
use chunkwake_data::registries::{Registry, RegistryData, RegistryTags};
use chunkwake_net::configuration::ConfigurationPlan;
use chunkwake_net::{Connection, GameVersion, LoggedIn, NetConfig, Server};
use chunkwake_protocol::configuration::{KnownPack, read_known_packs, select_known_packs};
use chunkwake_protocol::packets::{Handshake, NextState};
use chunkwake_protocol::{Reader, Writer};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

const PROTOCOL: i32 = 9999;
const PACKETS: &str = include_str!("../../data/tests/fixtures/packets.json");

// ID из фикстуры (совпадают с 26.3).
const CB_DISCONNECT: i32 = 2;
const CB_FINISH: i32 = 3;
const CB_REGISTRY_DATA: i32 = 7;
const CB_UPDATE_ENABLED_FEATURES: i32 = 13;
const CB_UPDATE_TAGS: i32 = 14;
const CB_SELECT_KNOWN_PACKS: i32 = 15;
const SB_CLIENT_INFORMATION: i32 = 0;
const SB_FINISH: i32 = 3;
const SB_SELECT_KNOWN_PACKS: i32 = 7;

fn plan() -> ConfigurationPlan {
    let packets = PacketTable::parse(PACKETS).unwrap();
    let registries = RegistryData {
        synchronized: vec![Registry {
            name: "dimension_type".into(),
            entries: vec!["minecraft:overworld".into(), "minecraft:the_end".into()],
        }],
        tags: vec![RegistryTags {
            registry: "block".into(),
            tags: vec![("minecraft:logs".into(), vec![5, 9])],
        }],
    };
    ConfigurationPlan::build(&packets, &registries, "26.3").unwrap()
}

async fn start() -> (std::net::SocketAddr, mpsc::UnboundedReceiver<LoggedIn>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = Server::new(NetConfig {
        version: GameVersion {
            name: "26.3".into(),
            protocol: PROTOCOL,
        },
        motd: "hello".into(),
        max_players: 20,
        compression_threshold: -1,
        online_mode: false,
        enable_status: true,
        session_server: String::new(),
        configuration: Some(Arc::new(plan())),
        admit: None,
    });
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(server.serve(listener, move |p, _conn| {
        let _ = tx.send(p);
    }));
    (addr, rx)
}

/// Логинится офлайн и возвращает соединение в состоянии configuration.
async fn login(addr: std::net::SocketAddr) -> Connection {
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
    assert_eq!(p[0], 0x02, "login success");
    c.send(&[0x03]).await.unwrap();
    c
}

async fn expect_greeting(c: &mut Connection) {
    let p = c.read_packet().await.unwrap();
    let mut r = Reader::new(&p);
    assert_eq!(r.var_int().unwrap(), CB_UPDATE_ENABLED_FEATURES);
    assert_eq!(r.var_int().unwrap(), 1);
    assert_eq!(r.string(256).unwrap(), "minecraft:vanilla");

    let p = c.read_packet().await.unwrap();
    let mut r = Reader::new(&p);
    assert_eq!(r.var_int().unwrap(), CB_SELECT_KNOWN_PACKS);
    assert_eq!(
        read_known_packs(&mut r).unwrap(),
        vec![KnownPack::vanilla("26.3")]
    );
}

#[tokio::test]
async fn full_configuration() {
    let (addr, mut players) = start().await;
    let mut c = login(addr).await;
    expect_greeting(&mut c).await;

    // Клиент шлёт client_information (игнорируется) и свои known packs.
    c.send(&[SB_CLIENT_INFORMATION as u8, 1, 2, 3])
        .await
        .unwrap();
    c.send(&select_known_packs(
        SB_SELECT_KNOWN_PACKS,
        &[KnownPack::vanilla("26.3")],
    ))
    .await
    .unwrap();

    let p = c.read_packet().await.unwrap();
    let mut r = Reader::new(&p);
    assert_eq!(r.var_int().unwrap(), CB_REGISTRY_DATA);
    assert_eq!(r.string(256).unwrap(), "minecraft:dimension_type");
    assert_eq!(r.var_int().unwrap(), 2);
    assert_eq!(r.string(256).unwrap(), "minecraft:overworld");
    assert!(!r.bool().unwrap(), "data comes from the known pack");

    let p = c.read_packet().await.unwrap();
    let mut r = Reader::new(&p);
    assert_eq!(r.var_int().unwrap(), CB_UPDATE_TAGS);
    assert_eq!(r.var_int().unwrap(), 1);
    assert_eq!(r.string(256).unwrap(), "minecraft:block");
    assert_eq!(r.var_int().unwrap(), 1);
    assert_eq!(r.string(256).unwrap(), "minecraft:logs");
    assert_eq!(r.var_int().unwrap(), 2);
    assert_eq!((r.var_int().unwrap(), r.var_int().unwrap()), (5, 9));

    let p = c.read_packet().await.unwrap();
    assert_eq!(Reader::new(&p).var_int().unwrap(), CB_FINISH);

    // Сервер не отдаёт игрока, пока клиент не подтвердил finish.
    assert!(players.try_recv().is_err());
    c.send(&[SB_FINISH as u8]).await.unwrap();
    c.flush().await.unwrap();
    assert_eq!(players.recv().await.unwrap().name, "Steve");
}

#[tokio::test]
async fn client_without_vanilla_pack_is_disconnected() {
    let (addr, mut players) = start().await;
    let mut c = login(addr).await;
    expect_greeting(&mut c).await;
    c.send(&select_known_packs(SB_SELECT_KNOWN_PACKS, &[]))
        .await
        .unwrap();

    let p = c.read_packet().await.unwrap();
    let mut r = Reader::new(&p);
    assert_eq!(r.var_int().unwrap(), CB_DISCONNECT);
    assert_eq!(r.u8().unwrap(), 8, "NBT string root");
    let len = r.u16().unwrap() as usize;
    let text = std::str::from_utf8(r.bytes(len).unwrap()).unwrap();
    assert!(text.contains("minecraft:core:26.3"), "{text}");
    drop(c);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(players.try_recv().is_err());
}
