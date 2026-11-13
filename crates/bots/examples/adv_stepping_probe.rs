//! Триггер `location.stepping_on`: порошковый снег под ногами и кожаные
//! ботинки (`adventure/walk_on_powder_snow_with_leather_boots`).
//!
//! `adv_stepping_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use std::collections::HashMap;
use std::time::Duration;

const ADV: &str = "minecraft:adventure/walk_on_powder_snow_with_leather_boots";
const CRITERION: &str = "walk_on_powder_snow_with_leather_boots";
const STAND: [f64; 3] = [10.5, 100.9, 8.5];

struct Probe {
    c: Client,
    sb_command: i32,
    sb_move_pos: i32,
    sb_tick_end: i32,
    adv: HashMap<String, Vec<String>>,
}

impl Probe {
    fn packet(&self, id: i32) -> Writer {
        let mut w = Writer::new();
        w.var_int(id);
        w
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
        self.pump(150).await;
    }

    /// `/tp` оставляет игрока в воздухе (`on_ground = false`): клиент обязан
    /// прислать позицию с флагом «на земле».
    async fn stand(&mut self) {
        let mut w = self.packet(self.sb_move_pos);
        for v in STAND {
            w.f64(v);
        }
        w.u8(1);
        self.c.send_raw(&w.buf).await.unwrap();
        let end = self.packet(self.sb_tick_end);
        self.c.send_raw(&end.buf).await.unwrap();
        self.pump(120).await;
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let c = Client::connect(&a[1], "pather", ids, 777, "26.3")
        .await
        .unwrap();
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_move_pos: sb("move_player_pos"),
        sb_tick_end: sb("client_tick_end"),
        adv: HashMap::new(),
    };
    p.pump(2000).await;

    let setup: Vec<String> = vec![
        "difficulty peaceful".to_owned(),
        "gamemode survival".to_owned(),
        "effect clear @s".to_owned(),
        "clear @s".to_owned(),
        "advancement grant @s only minecraft:adventure/sleep_in_bed".to_owned(),
        format!("advancement revoke @s only {ADV}"),
        // Площадка в воздухе: пол из камня, сверху порошковый снег.
        "fill 8 99 6 12 99 10 minecraft:stone".to_owned(),
        "fill 8 100 6 12 100 10 minecraft:air".to_owned(),
        "setblock 10 100 8 minecraft:powder_snow".to_owned(),
        "item replace entity @s armor.feet with minecraft:leather_boots".to_owned(),
        "tp @s 10.5 100.9 8.5".to_owned(),
    ];
    for line in setup {
        p.cmd(&line).await;
    }
    p.stand().await;
    p.pump(800).await;

    assert!(p.done(), "stepping_on не сработал: {:?}", p.adv.get(ADV));
    println!("stepping_on (powder snow + leather boots) ok");

    // Без ботинок критерий не должен закрываться.
    p.cmd("clear @s minecraft:leather_boots").await;
    p.cmd(&format!("advancement revoke @s only {ADV}")).await;
    p.cmd("tp @s 10.5 100.9 8.5").await;
    p.stand().await;
    p.pump(800).await;
    assert!(
        !p.done(),
        "stepping_on сработал без кожаных ботинок: {:?}",
        p.adv.get(ADV)
    );
    println!("stepping_on (without boots) skipped ok");

    p.cmd(&format!("advancement revoke @s only {ADV}")).await;
    println!("adv_stepping: ok");
}
