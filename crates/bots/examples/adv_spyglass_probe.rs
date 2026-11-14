//! Триггер `using_item`: подзорная труба и попугай
//! (`adventure/spyglass_at_parrot`).
//!
//! `adv_spyglass_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use std::collections::HashMap;
use std::time::Duration;

const ADV: &str = "minecraft:adventure/spyglass_at_parrot";
const CRITERION: &str = "spyglass_at_parrot";

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

    fn done(&self) -> bool {
        self.adv
            .get(ADV)
            .is_some_and(|v| v.iter().any(|c| c == CRITERION))
    }

    async fn cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(180).await;
    }

    async fn select(&mut self, slot: u16) {
        let mut w = self.packet(self.sb_select);
        w.u16(slot);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(120).await;
    }

    async fn use_item(&mut self, yaw: f32, pitch: f32) {
        self.seq += 1;
        let mut w = self.packet(self.sb_use);
        w.var_int(0).var_int(self.seq).f32(yaw).f32(pitch);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(600).await;
    }

    async fn release(&mut self) {
        self.seq += 1;
        let mut w = self.packet(self.sb_action);
        w.var_int(6).i64(0).u8(0).var_int(self.seq);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(250).await;
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
        "difficulty peaceful".into(),
        "gamemode creative".into(),
        "effect clear @s".into(),
        "kill @e[type=!minecraft:player,distance=..48]".into(),
        "fill -4 99 -4 4 99 8 minecraft:stone".into(),
        "fill -4 100 -4 4 104 8 minecraft:air".into(),
        "tp @s 0.5 100 0.5 0 16".into(),
        // Попугай на площадке в четырёх блоках по +Z.
        "summon minecraft:parrot 0.5 100 4.5 {NoAI:1b}".into(),
        "item replace entity @s hotbar.0 with minecraft:spyglass".into(),
        format!("advancement revoke @s only {ADV}"),
        "advancement grant @s only minecraft:adventure/root".into(),
    ];
    for line in setup {
        p.cmd(&line).await;
    }
    p.select(0).await;
    p.pump(600).await;

    // Смотрим на попугая через подзорную трубу.
    p.use_item(0.0, 16.0).await;
    p.pump(400).await;
    assert!(
        p.done(),
        "spyglass_at_parrot не закрылся: {:?}",
        p.adv.get(ADV)
    );
    println!("spyglass_at_parrot ok");

    // Отпускаем трубу, смотрим вверх: критерий не должен срабатывать.
    p.release().await;
    p.cmd(&format!("advancement revoke @s only {ADV}")).await;
    p.cmd("tp @s 0.5 100 0.5 0 -35").await;
    p.use_item(0.0, -35.0).await;
    p.pump(400).await;
    assert!(
        !p.done(),
        "spyglass_at_parrot сработал без попугая в прицеле: {:?}",
        p.adv.get(ADV)
    );
    println!("missed parrot ok");
    p.release().await;
    println!("adv_spyglass: ok");
}
