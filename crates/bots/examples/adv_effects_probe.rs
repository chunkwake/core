//! Триггер достижений effects_changed: «все эффекты» и «все зелья».
//!
//! `adv_effects_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use std::collections::HashMap;
use std::time::Duration;

/// Все эффекты `nether/all_effects` (надмножество `nether/all_potions`).
const EFFECTS: [&str; 34] = [
    "absorption",
    "bad_omen",
    "blindness",
    "breath_of_the_nautilus",
    "conduit_power",
    "darkness",
    "dolphins_grace",
    "fire_resistance",
    "glowing",
    "haste",
    "hero_of_the_village",
    "hunger",
    "infested",
    "invisibility",
    "jump_boost",
    "levitation",
    "mining_fatigue",
    "nausea",
    "night_vision",
    "oozing",
    "poison",
    "raid_omen",
    "regeneration",
    "resistance",
    "slow_falling",
    "slowness",
    "speed",
    "strength",
    "trial_omen",
    "water_breathing",
    "weakness",
    "weaving",
    "wind_charged",
    "wither",
];

struct Probe {
    c: Client,
    sb_command: i32,
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

    fn done(&self, id: &str, criterion: &str) -> bool {
        self.adv
            .get(id)
            .is_some_and(|v| v.iter().any(|c| c == criterion))
    }

    async fn cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(120).await;
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    c.capture_also = vec![];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        adv: HashMap::new(),
    };
    p.pump(2000).await;

    let bx = 6100.5;
    let bz = 6100.5;
    for line in [
        "difficulty peaceful".to_string(),
        "gamemode survival".to_string(),
        format!("tp boater {bx} 64 {bz} 0 0"),
        "effect clear @s".to_string(),
        "advancement revoke @s only minecraft:nether/all_potions".to_string(),
        "advancement revoke @s only minecraft:nether/all_effects".to_string(),
        "advancement grant @s only minecraft:nether/root".to_string(),
        "advancement grant @s only minecraft:nether/obtain_blaze_rod".to_string(),
        "advancement grant @s only minecraft:nether/brew_potion".to_string(),
    ] {
        p.cmd(&line).await;
    }

    for effect in EFFECTS {
        p.cmd(&format!("effect give @s minecraft:{effect} 120 0"))
            .await;
    }
    p.pump(800).await;

    assert!(
        p.done("minecraft:nether/all_potions", "all_effects"),
        "effects_changed (all_potions) не сработал: {:?}",
        p.adv.get("minecraft:nether/all_potions")
    );
    println!("effects_changed (all_potions) ok");
    assert!(
        p.done("minecraft:nether/all_effects", "all_effects"),
        "effects_changed (all_effects) не сработал: {:?}",
        p.adv.get("minecraft:nether/all_effects")
    );
    println!("effects_changed (all_effects) ok");

    p.cmd("effect clear @s").await;
    println!("adv_effects: ok");
}
