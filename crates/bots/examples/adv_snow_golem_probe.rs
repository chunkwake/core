//! `summoned_entity` для снежного голема: тыква на двух блоках снега
//! (`CarvedPumpkinBlock`). В ванили достижения на снежного голема нет,
//! поэтому проба использует временное `custom/snow_probe`
//! (`snow_probe.advancement.json`).
//!
//! `adv_snow_golem_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer};
use std::collections::HashMap;
use std::time::Duration;

const ADV: &str = "minecraft:custom/snow_probe";
const CRITERION: &str = "summoned";
const X: i32 = 1500;
const Z: i32 = 8;

struct Probe {
    c: Client,
    sb_command: i32,
    cb_add_entity: i32,
    snow_type: i32,
    snow: Option<i32>,
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
            if ty == self.snow_type {
                self.snow = Some(entity);
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
    let snow_type = reg["minecraft:entity_type"]["entries"]["minecraft:snow_golem"]["protocol_id"]
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
        snow_type,
        snow: None,
        adv: HashMap::new(),
    };
    p.pump(2000).await;

    let setup: Vec<String> = vec![
        "difficulty peaceful".into(),
        "gamemode creative".into(),
        "effect clear @s".into(),
        format!("fill {} 98 4 {} 98 12 minecraft:stone", X - 4, X + 4),
        format!("fill {} 99 4 {} 103 12 minecraft:air", X - 4, X + 4),
        format!("tp @s {} 99 12.5", X as f64 + 0.5),
        // Чанк загружен - убираем старых големов.
        "kill @e[type=!minecraft:player,distance=..48]".into(),
        format!("advancement revoke @s only {ADV}"),
        // Верхний блок снега; нижнего пока нет - шаблон неполный.
        format!("setblock {X} 100 {Z} minecraft:snow_block"),
        format!("setblock {X} 101 {Z} minecraft:carved_pumpkin"),
    ];
    for line in setup {
        p.cmd(&line).await;
    }
    p.pump(500).await;

    assert!(
        p.snow.is_none() && !p.done(),
        "снежный голем появился без второго блока снега: {:?}",
        p.adv.get(ADV)
    );
    println!("incomplete pattern skipped ok");

    // Достраиваем нижний блок и ставим тыкву заново.
    p.cmd(&format!("setblock {X} 99 {Z} minecraft:snow_block"))
        .await;
    p.cmd(&format!("setblock {X} 101 {Z} minecraft:air")).await;
    p.cmd(&format!("setblock {X} 101 {Z} minecraft:carved_pumpkin"))
        .await;
    p.pump(700).await;

    let snow = p.snow.expect("add_entity снежного голема не пришёл");
    assert!(
        p.done(),
        "summoned_entity для снежного голема не сработал: {:?}",
        p.adv.get(ADV)
    );
    println!("snow golem entity {snow}");
    println!("adv_snow_golem: ok");
}
