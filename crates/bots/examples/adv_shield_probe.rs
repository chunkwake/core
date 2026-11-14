//! `using_item` для поднятого щита (`LivingEntity.useItem` + `BLOCKS_ATTACKS`):
//! триггер идёт каждый тик, пока щит поднят. Временные `custom/shield_probe`
//! и `custom/shield_probe_neg`, файлы `shield_probe.advancement.json`,
//! `shield_probe_neg.advancement.json`.
//!
//! `adv_shield_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use std::collections::HashMap;
use std::time::Duration;

const POS: &str = "minecraft:custom/shield_probe";
const POS_CRITERION: &str = "raise_shield";
const NEG: &str = "minecraft:custom/shield_probe_neg";
const NEG_CRITERION: &str = "draw_bow";

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
        self.pump(180).await;
    }

    async fn select(&mut self, slot: u16) {
        let mut w = self.packet(self.sb_select);
        w.u16(slot);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(120).await;
    }

    async fn start_use(&mut self) {
        self.seq += 1;
        let mut w = self.packet(self.sb_use);
        w.var_int(0).var_int(self.seq).f32(0.0).f32(0.0);
        self.c.send_raw(&w.buf).await.unwrap();
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
        "gamemode survival".into(),
        "effect clear @s".into(),
        "clear @s".into(),
        "execute in minecraft:overworld run tp @s 1984.5 100 8.5 0 0".into(),
        "item replace entity @s hotbar.0 with minecraft:shield".into(),
        format!("advancement revoke @s only {POS}"),
        format!("advancement revoke @s only {NEG}"),
    ];
    for line in setup {
        p.cmd(&line).await;
    }
    p.select(0).await;
    // `use_item` в load-gated списке: даём клиенту догрузиться.
    p.pump(1500).await;

    // Поднимаем щит: триггер `using_item` должен сработать в ближайший тик.
    p.start_use().await;
    p.pump(500).await;

    assert!(
        p.done(POS, POS_CRITERION),
        "using_item для щита не сработал: {:?}",
        p.adv.get(POS)
    );
    println!("shield using_item ok");

    assert!(
        !p.done(NEG, NEG_CRITERION),
        "using_item сработал на лук, хотя поднят щит: {:?}",
        p.adv.get(NEG)
    );
    println!("bow filter skipped ok");

    p.release().await;
    println!("adv_shield: ok");
}
