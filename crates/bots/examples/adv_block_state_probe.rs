//! `block.state` в `location`-предикате:
//! - `placed_block` + `match_block.state` — `husbandry/place_dried_ghast_in_water`;
//! - `item_used_on_block` + `location_check.block.state` — временное
//!   `custom/state_probe` (`state_probe.advancement.json`, мокрое грядка).
//!
//! `adv_block_state_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use std::collections::HashMap;
use std::time::Duration;

const GHAST: &str = "minecraft:husbandry/place_dried_ghast_in_water";
const GHAST_CRITERION: &str = "place_dried_ghast_in_water";
const CUSTOM: &str = "minecraft:custom/state_probe";
const CUSTOM_CRITERION: &str = "on_wet_farmland";

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
        "clear @s".into(),
        "time set noon".into(),
        "kill @e[type=!minecraft:player,distance=..48]".into(),
        "fill 1674 99 2 1690 99 14 minecraft:stone".into(),
        "fill 1674 100 2 1690 104 14 minecraft:air".into(),
        "tp @s 1680.5 100 8.5 0 30".into(),
        format!("advancement revoke @s only {GHAST}"),
        format!("advancement revoke @s only {CUSTOM}"),
        "advancement grant @s only minecraft:husbandry/root".into(),
        "item replace entity @s hotbar.0 with minecraft:dried_ghast 4".into(),
        "item replace entity @s hotbar.1 with minecraft:oak_slab 4".into(),
        // Сухая площадка под опыт с ghast.
        "setblock 1680 100 6 minecraft:stone".into(),
        "setblock 1680 101 6 minecraft:air".into(),
        // Вода над камнем: ghast должен стать waterlogged.
        "setblock 1680 100 10 minecraft:stone".into(),
        "setblock 1680 101 10 minecraft:water".into(),
        // Грядка для item_used_on_block.
        "setblock 1684 100 6 minecraft:farmland[moisture=7]".into(),
        "setblock 1684 101 6 minecraft:air".into(),
    ];
    for line in setup {
        p.cmd(&line).await;
    }
    p.pump(500).await;

    // 1. placed_block: ghast на сухом камне — waterlogged=false, достижения нет.
    p.select(0).await;
    p.place_on_top([1680, 100, 6]).await;
    p.pump(400).await;
    assert!(
        !p.done(GHAST, GHAST_CRITERION),
        "place_dried_ghast_in_water сработал на суше: {:?}",
        p.adv.get(GHAST)
    );
    println!("dry ghast skipped ok");
    p.cmd("setblock 1680 101 6 minecraft:air").await;

    // 2. placed_block: ghast в воду — состояние waterlogged=true.
    p.select(0).await;
    p.place_on_top([1680, 100, 10]).await;
    p.pump(500).await;
    assert!(
        p.done(GHAST, GHAST_CRITERION),
        "place_dried_ghast_in_water не сработал в воде: {:?}",
        p.adv.get(GHAST)
    );
    println!("ghast waterlogged state ok");

    // 3. item_used_on_block: мокрая грядка (moisture=7) — достижение есть.
    p.cmd("tp @s 1684.5 100 8.5 0 30").await;
    p.select(1).await;
    p.place_on_top([1684, 100, 6]).await;
    p.pump(500).await;
    assert!(
        p.done(CUSTOM, CUSTOM_CRITERION),
        "item_used_on_block с moisture=7 не сработал: {:?}",
        p.adv.get(CUSTOM)
    );
    println!("wet farmland ok");

    // 4. item_used_on_block: сухая грядка (moisture=0) — не срабатывает.
    p.cmd(&format!("advancement revoke @s only {CUSTOM}")).await;
    p.cmd("setblock 1684 101 6 minecraft:air").await;
    p.cmd("setblock 1684 100 6 minecraft:farmland[moisture=0]")
        .await;
    p.pump(300).await;
    p.select(1).await;
    p.place_on_top([1684, 100, 6]).await;
    p.pump(500).await;
    assert!(
        !p.done(CUSTOM, CUSTOM_CRITERION),
        "item_used_on_block сработал на сухой грядке: {:?}",
        p.adv.get(CUSTOM)
    );
    println!("dry farmland skipped ok");
    println!("adv_block_state: ok");
}
