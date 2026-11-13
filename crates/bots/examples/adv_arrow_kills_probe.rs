//! Триггер `killed_by_arrow`: `adventure/two_birds_one_arrow` (два фантома
//! одной стрелой), `adventure/arbalistic` (5 уникальных типов одной стрелой) и
//! `adventure/whos_the_pillager_now` (разбойник из арбалета).
//!
//! `adv_arrow_kills_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use std::collections::HashMap;
use std::time::Duration;

const TWO_BIRDS: &str = "minecraft:adventure/two_birds_one_arrow";
const ARBALISTIC: &str = "minecraft:adventure/arbalistic";
const PILLAGER: &str = "minecraft:adventure/whos_the_pillager_now";
const BX: f64 = 1300.5;
const BZ: f64 = 1200.5;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_use: i32,
    sb_action: i32,
    sb_select: i32,
    seq: i32,
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
        self.pump(200).await;
    }

    async fn select(&mut self, slot: u16) {
        let mut w = self.packet(self.sb_select);
        w.u16(slot);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(120).await;
    }

    /// Зарядить арбалет и выстрелить: use (заряд) -> release (зарядка) -> use.
    async fn shoot(&mut self) {
        self.seq += 1;
        let mut w = self.packet(self.sb_use);
        w.var_int(0).var_int(self.seq).f32(0.0).f32(0.0);
        self.c.send_raw(&w.buf).await.unwrap();
        // 25 тиков заряда плюс запас на сетевую задержку.
        self.pump(2400).await;

        self.seq += 1;
        let mut w = self.packet(self.sb_action);
        w.var_int(6).i64(0).u8(0).var_int(self.seq);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(300).await;

        self.seq += 1;
        let mut w = self.packet(self.sb_use);
        w.var_int(0).var_int(self.seq).f32(0.0).f32(0.0);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(1800).await;
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
        sb_use: sb("use_item"),
        sb_action: sb("player_action"),
        sb_select: sb("set_carried_item"),
        seq: 0,
        adv: HashMap::new(),
    };
    p.pump(2000).await;

    let setup: Vec<String> = vec![
        "difficulty easy".into(),
        "gamemode survival".into(),
        "effect clear @s".into(),
        "clear @s".into(),
        "time set midnight".into(),
        // Дорожка: игрок на полу y=99, площадки под фантомов - y=100.
        "fill 1296 99 1194 1304 99 1226 minecraft:stone".into(),
        "fill 1296 100 1194 1304 104 1226 minecraft:air".into(),
        format!("tp @s {BX} 100 {BZ} 0 0"),
        "item replace entity @s hotbar.0 with minecraft:crossbow".into(),
        "item replace entity @s hotbar.1 with minecraft:arrow 64".into(),
        format!("advancement revoke @s only {TWO_BIRDS}"),
        format!("advancement revoke @s only {ARBALISTIC}"),
        format!("advancement revoke @s only {PILLAGER}"),
        "advancement grant @s only minecraft:adventure/ol_betsy".into(),
    ];
    for line in setup {
        p.cmd(&line).await;
    }
    // Чанк загружен - убираем мобов и стрелы прошлых прогонов.
    p.pump(1200).await;
    p.cmd("kill @e[type=!minecraft:player,distance=..48]").await;
    p.cmd("kill @e[type=minecraft:arrow,distance=..64]").await;
    p.pump(400).await;
    p.select(0).await;
    p.cmd("enchant @s minecraft:piercing 4").await;
    p.pump(600).await;

    // Два фантома одной стрелой.
    p.cmd("kill @e[type=!minecraft:player,distance=..48]").await;
    p.cmd("setblock 1300 100 1206 minecraft:stone").await;
    p.cmd("setblock 1300 100 1212 minecraft:stone").await;
    p.cmd("summon minecraft:phantom 1300.5 101.2 1206.5 {NoAI:1b,Health:1.0f}")
        .await;
    p.cmd("summon minecraft:phantom 1300.5 101.2 1212.5 {NoAI:1b,Health:1.0f}")
        .await;
    p.pump(600).await;
    p.select(0).await;
    p.shoot().await;
    assert!(
        p.done(TWO_BIRDS, "two_birds"),
        "two_birds_one_arrow не закрылся: {:?}",
        p.adv.get(TWO_BIRDS)
    );
    println!("two_birds_one_arrow ok");

    // Пять уникальных типов одной стрелой.
    p.cmd("setblock 1300 100 1206 minecraft:air").await;
    p.cmd("setblock 1300 100 1212 minecraft:air").await;
    p.cmd("kill @e[type=!minecraft:player,distance=..48]").await;
    p.cmd("kill @e[type=minecraft:arrow,distance=..64]").await;
    for (mob, z) in [
        ("zombie", 1206),
        ("skeleton", 1210),
        ("drowned", 1214),
        ("piglin", 1218),
        ("creeper", 1222),
    ] {
        p.cmd(&format!(
            "summon minecraft:{mob} 1300.5 100 {z}.5 {{NoAI:1b,Health:1.0f}}"
        ))
        .await;
    }
    p.pump(600).await;
    p.select(0).await;
    p.shoot().await;
    assert!(
        p.done(ARBALISTIC, "arbalistic"),
        "arbalistic не закрылся: {:?}",
        p.adv.get(ARBALISTIC)
    );
    println!("arbalistic ok");

    // Разбойник из арбалета (пронзание не нужно).
    p.cmd("kill @e[type=!minecraft:player,distance=..48]").await;
    p.cmd("kill @e[type=minecraft:arrow,distance=..64]").await;
    p.cmd("summon minecraft:pillager 1300.5 100 1206.5 {NoAI:1b,Health:1.0f}")
        .await;
    p.pump(600).await;
    p.select(0).await;
    p.shoot().await;
    assert!(
        p.done(PILLAGER, "kill_pillager"),
        "whos_the_pillager_now не закрылся: {:?}",
        p.adv.get(PILLAGER)
    );
    println!("whos_the_pillager_now ok");
    println!("adv_arrow_kills: ok");
}
