//! Триггер `location.position` на временном датапак-достижении.
//!
//! `adv_position_probe <addr> <packets.json> <registries.json>`
//!
//! Готовый файл достижения — `crates/bots/examples/pos_probe.advancement.json`:
//! скопируйте его в
//! `<generated>/data/minecraft/advancement/custom/pos_probe.json` и
//! перезапустите сервер (достижения читаются при старте). Критерий `here`:
//! `position` x/z 5995..6005, y 95..105.
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use std::collections::HashMap;
use std::time::Duration;

const ADV: &str = "minecraft:custom/pos_probe";
const CRITERION: &str = "here";

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

    async fn goto(&mut self, pos: [f64; 3]) {
        self.cmd(&format!("tp @s {} {} {}", pos[0], pos[1], pos[2]))
            .await;
        self.pump(1000).await;
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let c = Client::connect(&a[1], "framer", ids, 777, "26.3")
        .await
        .unwrap();
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        adv: HashMap::new(),
    };
    p.pump(2000).await;

    p.cmd("difficulty peaceful").await;
    p.cmd("gamemode survival").await;
    p.cmd("effect clear @s").await;
    p.cmd(&format!("advancement revoke @s only {ADV}")).await;

    // Вне области: по каждой оси отдельно.
    for (label, pos) in [
        ("x", [6100.5, 100.0, 6000.5]),
        ("y", [6000.5, 200.0, 6000.5]),
        ("z", [6000.5, 100.0, 6100.5]),
    ] {
        p.goto(pos).await;
        assert!(
            !p.done(),
            "position сработал вне области по {label}: {:?}",
            p.adv.get(ADV)
        );
        println!("outside {label} ok");
    }

    // Внутри: x/z 6000.5, y 100.
    p.goto([6000.5, 100.0, 6000.5]).await;
    assert!(
        p.done(),
        "position не сработал внутри области: {:?}",
        p.adv.get(ADV)
    );
    println!("inside ok");

    p.cmd(&format!("advancement revoke @s only {ADV}")).await;
    println!("adv_position: ok");
}
