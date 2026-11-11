//! `hasClientLoaded`: до `player_loaded` сервер игнорирует действия игрока,
//! после — выполняет. Проверяем на `player_action` DROP_ITEM: до загрузки
//! предмет не падает, после — появляется сущность предмета.
//!
//! Нужен быстрый мир (flat, небольшой view-distance): окно до
//! `player_loaded` — всего 60 тиков.
//!
//! `load_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use std::time::Duration;

/// `ServerboundPlayerActionPacket.Action.DROP_ITEM` (26.3).
const DROP_ITEM: i32 = 5;

fn pack_block(p: [i32; 3]) -> i64 {
    ((p[0] as i64 & 0x3FF_FFFF) << 38) | ((p[2] as i64 & 0x3FF_FFFF) << 12) | (p[1] as i64 & 0xFFF)
}

fn player_action(id: i32, action: i32, pos: [i32; 3], sequence: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id)
        .var_int(action)
        .i64(pack_block(pos))
        .u8(1)
        .var_int(sequence);
    w.buf
}

struct Probe {
    c: Client,
    sb_action: i32,
    sb_command: i32,
    item_kind: i32,
    pos: [f64; 3],
    sequence: i32,
    /// Сколько сущностей предмета появилось за время наблюдения.
    items: u32,
    /// Все виды появившихся сущностей (для сообщения об ошибке).
    spawned: Vec<i32>,
}

impl Probe {
    async fn pump(&mut self, ms: u64) {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
        loop {
            match tokio::time::timeout_at(deadline, self.c.next_event()).await {
                Ok(Ok(Event::EntitySpawned { kind, .. })) => {
                    self.spawned.push(kind);
                    if kind == self.item_kind {
                        self.items += 1;
                    }
                }
                Ok(Ok(Event::Position(pos))) => self.pos = pos,
                Ok(Ok(_)) => {}
                _ => break,
            }
        }
    }

    async fn cmd(&mut self, command: &str) {
        let mut w = Writer::new();
        w.var_int(self.sb_command).string(command);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(400).await;
    }

    async fn drop_one(&mut self) {
        self.sequence += 1;
        let pos = [
            self.pos[0].floor() as i32,
            self.pos[1].floor() as i32,
            self.pos[2].floor() as i32,
        ];
        self.c
            .send_raw(&player_action(
                self.sb_action,
                DROP_ITEM,
                pos,
                self.sequence,
            ))
            .await
            .unwrap();
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let reg: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&a[3]).unwrap()).unwrap();
    let item_kind = reg["minecraft:entity_type"]["entries"]["minecraft:item"]["protocol_id"]
        .as_i64()
        .unwrap() as i32;
    let sb = |name| table.id(State::Play, Direction::Serverbound, name).unwrap();
    let ids = Ids::load(&table).unwrap();
    let mut c = Client::connect(&a[1], "framer", ids, 777, "26.3")
        .await
        .unwrap();
    // Загрузку клиента отправляем сами, чтобы увидеть окно до `player_loaded`.
    c.auto_player_loaded = false;
    let pos = loop {
        if let Event::Position(at) = c.next_event().await.unwrap() {
            break at;
        }
    };
    let mut probe = Probe {
        c,
        sb_action: sb("player_action"),
        sb_command: sb("chat_command"),
        item_kind,
        pos,
        sequence: 0,
        items: 0,
        spawned: Vec::new(),
    };
    probe
        .cmd("item replace entity @s hotbar.0 with minecraft:dirt 64")
        .await;

    // До `player_loaded` сервер обязан игнорировать DROP_ITEM.
    probe.items = 0;
    probe.drop_one().await;
    probe.pump(600).await;
    assert_eq!(
        probe.items, 0,
        "DROP_ITEM до player_loaded не должен создавать предмет"
    );

    let mut loaded = Writer::new();
    loaded.var_int(sb("player_loaded"));
    probe.c.send_raw(&loaded.buf).await.unwrap();
    probe.pump(300).await;

    // После `player_loaded` то же действие выполняется.
    probe.items = 0;
    probe.drop_one().await;
    probe.pump(600).await;
    assert!(
        probe.items > 0,
        "DROP_ITEM после player_loaded должен создать предмет; spawns={:?} pos={:?}",
        probe.spawned,
        probe.pos
    );

    println!("load gate: ok (предмет до загрузки не выпал, после — выпал)");
}
