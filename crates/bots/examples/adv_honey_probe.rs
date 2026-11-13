//! Триггер `item_used_on_block` с `smokey`: `husbandry/safely_harvest_honey`.
//!
//! `adv_honey_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use std::collections::HashMap;
use std::time::Duration;

const ADV: &str = "minecraft:husbandry/safely_harvest_honey";
const CRITERION: &str = "safely_harvest_honey";
const HIVE: [i32; 3] = [60, 100, 8];
const FIRE: [i32; 3] = [60, 99, 8];

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

    async fn use_on(&mut self, pos: [i32; 3], face: i32) {
        self.seq += 1;
        let mut w = self.packet(self.sb_use_on);
        w.var_int(0)
            .i64(pack_pos(pos))
            .var_int(face)
            .f32(0.5)
            .f32(0.5)
            .f32(0.5)
            .u8(0)
            .u8(0)
            .var_int(self.seq);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(600).await;
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
        sb_use_on: sb("use_item_on"),
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
        "time set noon".into(),
        "fill 56 99 4 64 99 12 minecraft:stone".into(),
        "fill 56 100 4 64 102 12 minecraft:air".into(),
        format!(
            "setblock {} {} {} minecraft:beehive[honey_level=5]",
            HIVE[0], HIVE[1], HIVE[2]
        ),
        format!(
            "setblock {} {} {} minecraft:campfire[lit=true]",
            FIRE[0], FIRE[1], FIRE[2]
        ),
        "tp @s 61.5 100 8.5 -90 20".into(),
        "item replace entity @s hotbar.0 with minecraft:glass_bottle 16".into(),
        format!("advancement revoke @s only {ADV}"),
        "advancement grant @s only minecraft:husbandry/root".into(),
    ];
    for line in setup {
        p.cmd(&line).await;
    }
    p.select(0).await;
    p.pump(600).await;

    // Положительный: улей полный, костёр под ним.
    p.use_on(HIVE, 1).await;
    assert!(
        p.done(),
        "safely_harvest_honey не сработал с костром: {:?}",
        p.adv.get(ADV)
    );
    println!("honey with campfire ok");

    // Отрицательный: убираем костёр, снова полный улей - дыма нет.
    p.cmd(&format!("advancement revoke @s only {ADV}")).await;
    p.cmd(&format!(
        "setblock {} {} {} minecraft:air",
        FIRE[0], FIRE[1], FIRE[2]
    ))
    .await;
    p.cmd(&format!(
        "setblock {} {} {} minecraft:beehive[honey_level=5]",
        HIVE[0], HIVE[1], HIVE[2]
    ))
    .await;
    p.use_on(HIVE, 1).await;
    assert!(
        !p.done(),
        "safely_harvest_honey сработал без костра: {:?}",
        p.adv.get(ADV)
    );
    println!("honey without campfire skipped ok");
    println!("adv_honey: ok");
}
