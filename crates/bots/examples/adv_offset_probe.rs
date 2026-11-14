//! `offsetX/Y/Z` в `location_check` для `item_used_on_block`: плита ставится
//! на камень, и предикат смотрит на соседний блок (`offsetY=1`).
//! Временные `custom/offset_probe` и `custom/offset_probe_neg`
//! (`offset_probe.advancement.json`, `offset_probe_neg.advancement.json`).
//!
//! `adv_offset_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use std::collections::HashMap;
use std::time::Duration;

const POS: &str = "minecraft:custom/offset_probe";
const POS_CRITERION: &str = "slab_above";
const NEG: &str = "minecraft:custom/offset_probe_neg";
const NEG_CRITERION: &str = "slab_two_above";
const X: i32 = 1840;
const Z: i32 = 8;

fn pack_pos([x, y, z]: [i32; 3]) -> i64 {
    ((x as i64 & 0x3ffffff) << 38) | ((z as i64 & 0x3ffffff) << 12) | (y as i64 & 0xfff)
}

struct Probe {
    c: Client,
    sb_command: i32,
    sb_use_on: i32,
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

    /// ПКМ по верхней грани блока (`face = 1`).
    async fn place_on_top(&mut self, pos: [i32; 3]) {
        self.seq += 1;
        let mut w = self.packet(self.sb_use_on);
        w.var_int(0)
            .i64(pack_pos(pos))
            .var_int(1)
            .f32(0.5)
            .f32(1.0)
            .f32(0.5)
            .u8(0)
            .u8(0)
            .var_int(self.seq);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(450).await;
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
        sb_use_on: sb("use_item_on"),
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
        "execute in minecraft:overworld run tp @s 1840.5 100 8.5 0 30".into(),
        format!("fill {} 99 4 {} 104 12 minecraft:stone", X - 4, X + 4),
        format!("fill {} 100 4 {} 104 12 minecraft:air", X - 4, X + 4),
        format!("setblock {X} 100 {Z} minecraft:stone"),
        "item replace entity @s hotbar.0 with minecraft:oak_slab 4".into(),
        format!("advancement revoke @s only {POS}"),
        format!("advancement revoke @s only {NEG}"),
    ];
    for line in setup {
        p.cmd(&line).await;
    }
    p.select(0).await;
    p.pump(500).await;

    // Ставим плиту на камень: событие идёт с pos камня, плита уже на y+1.
    p.place_on_top([X, 100, Z]).await;
    p.pump(600).await;

    assert!(
        p.done(POS, POS_CRITERION),
        "offsetY=1 (плита сверху) не сработал: {:?}",
        p.adv.get(POS)
    );
    println!("offsetY=1 slab ok");

    assert!(
        !p.done(NEG, NEG_CRITERION),
        "offsetY=2 сработал (там воздух): {:?}",
        p.adv.get(NEG)
    );
    println!("offsetY=2 skipped ok");
    println!("adv_offset: ok");
}
