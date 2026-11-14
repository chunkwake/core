//! Остальные поля `location` (`light`, `block`, `fluid`, `smokey`,
//! `can_see_sky`, `dimension`) на временном датапак-достижении.
//!
//! `adv_location_fields_probe <addr> <packets.json> <registries.json>`
//!
//! Готовый файл достижения — `crates/bots/examples/loc_probe.advancement.json`:
//! скопируйте его в
//! `<generated>/data/minecraft/advancement/custom/loc_probe.json` и
//! перезапустите сервер (достижения читаются при старте).
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use std::collections::HashMap;
use std::time::Duration;

const ADV: &str = "minecraft:custom/loc_probe";

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

    fn done(&self, criterion: &str) -> bool {
        self.adv
            .get(ADV)
            .is_some_and(|v| v.iter().any(|c| c == criterion))
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
    let c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        adv: HashMap::new(),
    };
    p.pump(2000).await;

    for line in [
        "difficulty peaceful",
        "gamemode creative",
        "effect clear @s",
        "time set midnight",
        // Игрок мог остаться в другом измерении (например, после дракона):
        // `light`/`dimension` в предикате требуют Верхний мир.
        "execute in minecraft:overworld run tp @s 20.5 100 8.5 0 0",
        "setblock 20 100 8 minecraft:water",
        // Чистим площадку для дыма от прошлых проб (костёр в мире сохраняется).
        "setblock 30 98 8 minecraft:air",
        "fill 25 99 3 35 100 13 minecraft:air",
        // Ревок последним: за время расстановки критерий мог закрыться.
        "advancement revoke @s only minecraft:custom/loc_probe",
    ] {
        p.cmd(line).await;
    }

    // Вода под ногами: блок и жидкость совпадают, но ночью сырой свет 4 < 10.
    p.goto([20.5, 100.0, 8.5]).await;
    assert!(
        !p.done("wet"),
        "light не проверился: wet закрылся ночью: {:?}",
        p.adv.get(ADV)
    );
    assert!(
        !p.done("not_lava"),
        "fluid не проверился: not_lava закрылся в воде"
    );
    println!("wet (night) skipped ok");

    p.cmd("time set day").await;
    p.pump(1200).await;
    assert!(
        p.done("wet"),
        "block/fluid/light не сработали днём: {:?}",
        p.adv.get(ADV)
    );
    assert!(
        !p.done("not_lava"),
        "fluid не проверился: not_lava закрылся в воде"
    );
    println!("wet (day) ok");

    // Сухая точка: дыма нет, потом ставим зажжённый костёр двумя блоками ниже.
    p.goto([30.5, 100.0, 8.5]).await;
    assert!(
        !p.done("smoky"),
        "smokey сработал без костра: {:?}",
        p.adv.get(ADV)
    );
    p.cmd("setblock 30 98 8 minecraft:campfire[lit=true]").await;
    p.pump(1200).await;
    assert!(
        p.done("smoky"),
        "smokey не сработал с костром: {:?}",
        p.adv.get(ADV)
    );
    println!("smoky ok");

    assert!(p.done("wet"), "wet потерялся");
    println!("adv_location_fields: ok");
}
