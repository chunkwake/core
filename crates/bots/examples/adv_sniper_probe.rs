//! Триггер `player_killed_entity` с дистанцией и `killing_blow`:
//! `adventure/sniper_duel` (скелет со стрелы с ≥ 50 блоков).
//!
//! `adv_sniper_probe <addr> <packets.json> <registries.json>`
//!
//! Нужен `simulation-distance` не меньше 4: стрела тикает только в
//! симулируемых чанках, а цель в 52 блоках.
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use std::collections::HashMap;
use std::time::Duration;

const ADV: &str = "minecraft:adventure/sniper_duel";
const CRITERION: &str = "killed_skeleton";
const BX: f64 = 200.5;
const BZ: f64 = 200.5;
const SZ: f64 = 252.5;
/// Прицел: полный заряд, наклон вверх под траекторию (52 блока).
const PITCH: f32 = -8.5;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_use: i32,
    sb_action: i32,
    sb_move: i32,
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
        self.pump(200).await;
    }

    async fn select(&mut self, slot: u16) {
        let mut w = self.packet(self.sb_select);
        w.u16(slot);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(120).await;
    }

    async fn pose(&mut self, pitch: f32) {
        let mut w = self.packet(self.sb_move);
        for v in [BX, 100.0, BZ] {
            w.f64(v);
        }
        w.f32(0.0).f32(pitch).u8(1);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(150).await;
    }

    async fn use_item(&mut self, pitch: f32) {
        self.seq += 1;
        let mut w = self.packet(self.sb_use);
        w.var_int(0).var_int(self.seq).f32(0.0).f32(pitch);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(1500).await;
    }

    async fn release(&mut self) {
        self.seq += 1;
        let mut w = self.packet(self.sb_action);
        w.var_int(6).i64(0).u8(0).var_int(self.seq);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(2500).await;
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
        sb_move: sb("move_player_pos_rot"),
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
        // Коридор 60 блоков: пол и воздух над ним.
        "fill 196 99 196 204 99 256 minecraft:stone".into(),
        "fill 196 100 196 204 106 256 minecraft:air".into(),
        format!("tp @s {BX} 100 {BZ} 0 {PITCH}"),
        format!("summon minecraft:skeleton {BX} 100 {SZ} {{NoAI:1b,Health:1.0f}}"),
        "give @s minecraft:bow 1".into(),
        "give @s minecraft:arrow 64".into(),
        format!("advancement revoke @s only {ADV}"),
        "advancement grant @s only minecraft:adventure/shoot_arrow".into(),
    ];
    for line in setup {
        p.cmd(&line).await;
    }
    p.select(0).await;
    p.pump(800).await;

    // Разброс лука ±1°: на 52 блоках это ±0.9 блока, поэтому пробуем
    // несколько стрел, пока скелет не умрёт.
    let mut shots = 0;
    while shots < 32 && !p.done() {
        p.pose(PITCH).await;
        p.use_item(PITCH).await;
        p.release().await;
        shots += 1;
    }
    assert!(
        p.done(),
        "sniper_duel не закрылся за {shots} выстрелов: {:?}",
        p.adv.get(ADV)
    );
    println!("sniper_duel after {shots} shots");
    println!("adv_sniper: ok");
}
