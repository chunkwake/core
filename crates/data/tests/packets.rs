use chunkwake_data::packets::{Direction, PacketTable, State};

const FIXTURE: &str = include_str!("fixtures/packets.json");

#[test]
fn parses_and_strips_namespace() {
    let t = PacketTable::parse(FIXTURE).unwrap();
    assert_eq!(
        t.id(
            State::Configuration,
            Direction::Clientbound,
            "finish_configuration"
        )
        .unwrap(),
        3
    );
    // Одинаковое имя в разных направлениях — разные записи.
    assert_eq!(
        t.id(State::Login, Direction::Clientbound, "hello").unwrap(),
        1
    );
    assert_eq!(
        t.id(State::Login, Direction::Serverbound, "hello").unwrap(),
        0
    );
    assert!(t.id(State::Play, Direction::Clientbound, "login").is_err());
}

#[test]
fn hardcoded_ids_match_fixture() {
    PacketTable::parse(FIXTURE)
        .unwrap()
        .verify_hardcoded()
        .unwrap();
}

#[test]
fn mismatch_is_reported() {
    let broken = FIXTURE.replace(
        r#""minecraft:login_finished": { "protocol_id": 2 }"#,
        r#""minecraft:login_finished": { "protocol_id": 9 }"#,
    );
    let err = PacketTable::parse(&broken)
        .unwrap()
        .verify_hardcoded()
        .unwrap_err()
        .to_string();
    assert!(err.contains("login_finished"), "{err}");
}

/// Проверка на реальных данных: `CHUNKWAKE_PACKETS_JSON=.../reports/packets.json`.
#[test]
fn real_packets_json_if_available() {
    let Ok(path) = std::env::var("CHUNKWAKE_PACKETS_JSON") else {
        return;
    };
    let t = PacketTable::load(path.as_ref()).unwrap();
    t.verify_hardcoded().unwrap();
    assert!(t.len() > 100);
}
