//! Статистика `minecraft.custom:sleep_in_bed`: успешный сон в кровати.
//!
//! `stats_sleep_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use std::collections::HashMap;
use std::time::Duration;

const FOOT: [i32; 3] = [10, 100, 8];
const HEAD: [i32; 3] = [10, 100, 7];

fn pack_pos([x, y, z]: [i32; 3]) -> i64 {
    ((x as i64 & 0x3ffffff) << 38) | ((z as i64 & 0x3ffffff) << 12) | (y as i64 & 0xfff)
}

struct Probe {
    c: Client,
    sb_command: i32,
    sb_use_on: i32,
    custom: i32,
    sleep: i32,
    seq: i32,
    stats: HashMap<(i32, i32), i32>,
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
            match ev {
                Event::Stats { stats } => {
                    for (key, n) in stats {
                        self.stats.insert(key, n);
                    }
                }
                Event::Advancements {
                    reset, progress, ..
                } => {
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
                _ => {}
            }
        }
    }

    fn sleep_stat(&self) -> i32 {
        self.stats
            .get(&(self.custom, self.sleep))
            .copied()
            .unwrap_or(0)
    }

    async fn cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(200).await;
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
        self.pump(800).await;
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let reg: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::path::Path::new(&a[2]).with_file_name("registries.json"))
            .unwrap(),
    )
    .unwrap();
    let custom = reg["minecraft:stat_type"]["entries"]["minecraft:custom"]["protocol_id"]
        .as_i64()
        .unwrap() as i32;
    let sleep = reg["minecraft:custom_stat"]["entries"]["minecraft:sleep_in_bed"]["protocol_id"]
        .as_i64()
        .unwrap() as i32;
    let ids = Ids::load(&table).unwrap();
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let c = Client::connect(&a[1], "carter", ids, 777, "26.3")
        .await
        .unwrap();
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_use_on: sb("use_item_on"),
        custom,
        sleep,
        seq: 0,
        stats: HashMap::new(),
        adv: HashMap::new(),
    };
    p.pump(2000).await;

    let setup: Vec<String> = vec![
        "difficulty peaceful".into(),
        "gamemode survival".into(),
        "effect clear @s".into(),
        "time set midnight".into(),
        "advancement revoke @s only minecraft:adventure/sleep_in_bed".into(),
        "advancement grant @s only minecraft:adventure/root".into(),
        // Площадка в воздухе и кровать: ножки на z=8, изголовье на z=7.
        "fill 8 99 6 12 99 10 minecraft:stone".into(),
        "fill 8 100 6 12 100 10 minecraft:air".into(),
        format!(
            "setblock {} {} {} minecraft:red_bed[part=foot,facing=north]",
            FOOT[0], FOOT[1], FOOT[2]
        ),
        format!(
            "setblock {} {} {} minecraft:red_bed[part=head,facing=north]",
            HEAD[0], HEAD[1], HEAD[2]
        ),
        "tp @s 10.5 100 6.5".into(),
    ];
    for line in setup {
        p.cmd(&line).await;
    }
    p.pump(400).await;

    let before = p.sleep_stat();
    p.use_on(HEAD, 1).await;
    p.pump(800).await;
    let after = p.sleep_stat();

    assert_eq!(
        after,
        before + 1,
        "sleep_in_bed не вырос: было {before}, стало {after}"
    );
    println!("sleep_in_bed stat {before} -> {after}");

    let slept = p
        .adv
        .get("minecraft:adventure/sleep_in_bed")
        .is_some_and(|v| v.iter().any(|c| c == "slept_in_bed"));
    assert!(slept, "advancement sleep_in_bed не закрылся: {:?}", p.adv);
    println!("advancement sleep_in_bed ok");

    // Возвращаем день, чтобы кровать не влияла на другие пробы.
    p.cmd("time set day").await;
    println!("stats_sleep: ok");
}
