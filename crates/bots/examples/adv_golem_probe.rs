//! Триггер `summoned_entity`: железный голем из тыквы и блоков железа
//! (`adventure/summon_iron_golem`).
//!
//! `adv_golem_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer};
use std::collections::HashMap;
use std::time::Duration;

const ADV: &str = "minecraft:adventure/summon_iron_golem";
const CRITERION: &str = "summoned_golem";
const BX: f64 = 1300.5;
const BY: i32 = 100;
const BZ: i32 = 8;

struct Probe {
    c: Client,
    sb_command: i32,
    cb_add_entity: i32,
    golem_type: i32,
    golem: Option<i32>,
    adv: HashMap<String, Vec<String>>,
}

impl Probe {
    fn packet(&self, id: i32) -> Writer {
        let mut w = Writer::new();
        w.var_int(id);
        w
    }

    fn drain(&mut self) {
        for (id, body) in std::mem::take(&mut self.c.captured_by) {
            if id != self.cb_add_entity {
                continue;
            }
            let mut r = Reader::new(&body);
            let Ok(entity) = r.var_int() else { continue };
            if r.bytes(16).is_err() {
                continue;
            }
            let Ok(ty) = r.var_int() else { continue };
            if ty == self.golem_type {
                self.golem = Some(entity);
            }
        }
    }

    async fn pump(&mut self, ms: u64) {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
        loop {
            let ev = match tokio::time::timeout_at(deadline, self.c.next_event()).await {
                Ok(Ok(ev)) => ev,
                Ok(Err(e)) => {
                    eprintln!("event error: {e}");
                    break;
                }
                Err(_) => break,
            };
            if let Event::Advancements {
                reset, progress, ..
            } = ev
            {
                if reset {
                    self.adv.clear();
                }
                for (id, criteria) in progress {
                    if criteria.is_empty() {
                        self.adv.remove(&id);
                    } else {
                        self.adv.entry(id).or_default().extend(criteria);
                    }
                }
            }
        }
        self.drain();
    }

    fn done(&self) -> bool {
        self.adv
            .get(ADV)
            .is_some_and(|v| v.iter().any(|c| c == CRITERION))
    }

    async fn cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(200).await;
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let reg: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::path::Path::new(&a[2]).with_file_name("registries.json"))
            .unwrap(),
    )
    .unwrap();
    let golem_type = reg["minecraft:entity_type"]["entries"]["minecraft:iron_golem"]["protocol_id"]
        .as_i64()
        .unwrap() as i32;
    let ids = Ids::load(&table).unwrap();
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n| table.id(State::Play, Direction::Clientbound, n).unwrap();
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    let cb_add_entity = cb("add_entity");
    c.capture_also = vec![cb_add_entity];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        cb_add_entity,
        golem_type,
        golem: None,
        adv: HashMap::new(),
    };
    p.pump(2000).await;

    let setup: Vec<String> = vec![
        "difficulty peaceful".into(),
        "gamemode survival".into(),
        "effect clear @s".into(),
        "clear @s".into(),
        "time set noon".into(),
        // Площадка; структура ставится в (1300, 100..102, 8).
        "fill 1296 99 4 1304 99 12 minecraft:stone".into(),
        "fill 1296 100 4 1304 103 12 minecraft:air".into(),
        "tp @s 1303.5 100 8.5".into(),
        format!("advancement revoke @s only {ADV}"),
        "advancement grant @s only minecraft:adventure/trade".into(),
        // Основание и средний ряд (без левого блока - сначала отрицание).
        format!("setblock {BX} {BY} {BZ} minecraft:iron_block"),
        format!("setblock 1299 101 {BZ} minecraft:iron_block"),
        format!("setblock {BX} 101 {BZ} minecraft:iron_block"),
        format!("setblock 1301 101 {BZ} minecraft:iron_block"),
        format!("setblock 1299 101 {BZ} minecraft:air"),
    ];
    for line in setup {
        p.cmd(&line).await;
    }
    p.pump(400).await;

    // Неполный шаблон: тыква не даёт голема.
    p.cmd(&format!("setblock {BX} 102 {BZ} minecraft:carved_pumpkin"))
        .await;
    p.pump(600).await;
    assert!(
        p.golem.is_none() && !p.done(),
        "голем появился без блока железа: {:?}",
        p.adv.get(ADV)
    );
    println!("incomplete pattern skipped ok");

    // Достраиваем и ставим тыкву заново.
    p.cmd(&format!("setblock {BX} 102 {BZ} minecraft:air"))
        .await;
    p.cmd(&format!("setblock 1299 101 {BZ} minecraft:iron_block"))
        .await;
    p.cmd(&format!("setblock {BX} 102 {BZ} minecraft:carved_pumpkin"))
        .await;
    p.pump(800).await;

    let golem = p.golem.expect("add_entity железного голема не пришёл");
    assert!(
        p.done(),
        "summon_iron_golem не закрылся: {:?}",
        p.adv.get(ADV)
    );
    println!("iron golem entity {golem}");

    // `/summon` в ванили триггер не вызывает: голем есть, достижения нет.
    p.cmd(&format!("advancement revoke @s only {ADV}")).await;
    p.golem = None;
    p.cmd(&format!(
        "summon minecraft:iron_golem {} {} {}",
        BX, BY as f64, BZ as f64
    ))
    .await;
    p.pump(600).await;
    assert!(p.golem.is_some(), "/summon не создал голема");
    assert!(
        !p.done(),
        "summon_iron_golem сработал на /summon: {:?}",
        p.adv.get(ADV)
    );
    println!("/summon without trigger ok");
    println!("adv_golem: ok");
}
