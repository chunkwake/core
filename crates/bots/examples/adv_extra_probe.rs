//! Живые проверки: player_kills (второй игрок), slept_in_bed,
//! entity_killed_player и killed_by при смерти от зомби.
//!
//! `adv_extra_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::collections::HashMap;
use std::time::Duration;

const BX: i32 = 4600;
const BZ: i32 = 4600;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_use_on: i32,
    sb_select: i32,
    cb_chat: i32,
    chat: String,
    seq: i32,
    stats: HashMap<(i32, i32), i32>,
    adv: HashMap<String, Vec<String>>,
}

fn flatten(v: &serde_json::Value) -> String {
    if let Some(s) = v.as_str() {
        return s.to_owned();
    }
    if let Some(s) = v.get("text").and_then(serde_json::Value::as_str) {
        return s.to_owned();
    }
    let mut out = String::new();
    if let Some(args) = v.get("with").and_then(serde_json::Value::as_array) {
        for arg in args {
            out.push_str(&flatten(arg));
            out.push(' ');
        }
    }
    out
}

fn pack_pos([x, y, z]: [i32; 3]) -> i64 {
    ((x as i64 & 0x3ffffff) << 38) | ((z as i64 & 0x3ffffff) << 12) | (y as i64 & 0xfff)
}

impl Probe {
    fn packet(&self, id: i32) -> Writer {
        let mut w = Writer::new();
        w.var_int(id);
        w
    }

    fn drain(&mut self) {
        for (id, body) in std::mem::take(&mut self.c.captured_by) {
            if id != self.cb_chat {
                continue;
            }
            let mut r = Reader::new(&body);
            if let Ok(component) = text::read_component(&mut r) {
                self.chat.push_str(&flatten(&component));
                self.chat.push(' ');
            }
        }
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
                Event::Advancements { progress, .. } => {
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
        self.drain();
    }

    fn stat(&self, kind: i32, value: i32) -> i32 {
        self.stats.get(&(kind, value)).copied().unwrap_or(0)
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
        self.pump(350).await;
    }

    async fn select(&mut self, slot: u16) {
        let mut w = self.packet(self.sb_select);
        w.u16(slot);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(150).await;
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
        self.pump(500).await;
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
    let cstat = |n: &str| {
        reg["minecraft:custom_stat"]["entries"][n]["protocol_id"]
            .as_i64()
            .unwrap() as i32
    };
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n| table.id(State::Play, Direction::Clientbound, n).unwrap();
    // Второй игрок для player_kills.
    let table2 = PacketTable::load(a[2].as_ref()).unwrap();
    let mut framer = Client::connect(&a[1], "framer", Ids::load(&table2).unwrap(), 777, "26.3")
        .await
        .unwrap();
    let framer_deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while let Ok(Ok(ev)) = tokio::time::timeout_at(framer_deadline, framer.next_event()).await {
        if let Event::Position(_) = ev {
            break;
        }
    }
    let _framer = framer;
    let mut c = Client::connect(&a[1], "boater", Ids::load(&table).unwrap(), 777, "26.3")
        .await
        .unwrap();
    let cb_chat = cb("system_chat");
    c.capture_also = vec![cb_chat];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_use_on: sb("use_item_on"),
        sb_select: sb("set_carried_item"),
        cb_chat,
        chat: String::new(),
        seq: 0,
        stats: HashMap::new(),
        adv: HashMap::new(),
    };
    p.pump(2000).await;

    let by = 64;
    for line in [
        "difficulty normal".to_string(),
        "gamemode survival".to_string(),
        "time set midnight".to_string(),
        "kill @e[type=minecraft:zombie]".to_string(),
        format!(
            "fill {} {} {} {} {} {} minecraft:air",
            BX - 8,
            by,
            BZ - 8,
            BX + 8,
            by + 5,
            BZ + 8
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            BX - 8,
            by - 1,
            BZ - 8,
            BX + 8,
            by - 1,
            BZ + 8
        ),
        format!("tp boater {} {} {}", BX as f64 + 0.5, by, BZ as f64 + 0.5),
        "clear boater".to_string(),
        "give boater minecraft:iron_sword 1".to_string(),
        "advancement grant @s only minecraft:adventure/root".to_string(),
        "advancement revoke @s only minecraft:adventure/sleep_in_bed".to_string(),
    ] {
        p.cmd(&line).await;
    }
    p.select(0).await;

    // 1. player_kills: второй игрок умирает от /damage by boater.
    let kills = cstat("minecraft:player_kills");
    let before = p.stat(8, kills);
    p.cmd("damage framer 100 minecraft:player_attack by boater")
        .await;
    p.pump(1200).await;
    assert!(
        p.stat(8, kills) > before,
        "player_kills не вырос: {before} -> {}",
        p.stat(8, kills)
    );
    println!("player_kills ok");

    // 2. slept_in_bed: кровать и ночь.
    p.cmd(&format!(
        "setblock {} {} {} minecraft:red_bed[part=foot,facing=east]",
        BX + 1,
        by,
        BZ
    ))
    .await;
    p.cmd(&format!(
        "setblock {} {} {} minecraft:red_bed[part=head,facing=east]",
        BX + 2,
        by,
        BZ
    ))
    .await;
    p.cmd(&format!(
        "tp boater {} {} {}",
        BX as f64 + 0.5,
        by,
        BZ as f64 + 0.5
    ))
    .await;
    p.use_on([BX + 1, by, BZ], 1).await;
    p.pump(800).await;
    assert!(
        p.done("minecraft:adventure/sleep_in_bed", "slept_in_bed"),
        "slept_in_bed не сработал: {:?}",
        p.adv.get("minecraft:adventure/sleep_in_bed")
    );
    println!("slept_in_bed ok");
    p.cmd(&format!(
        "tp boater {} {} {}",
        BX as f64 - 3.5,
        by,
        BZ as f64 + 0.5
    ))
    .await;

    // 3. entity_killed_player: смерть от зомби.
    p.cmd("advancement revoke @s only minecraft:adventure/root")
        .await;
    p.cmd(&format!(
        "summon minecraft:zombie {} {} {} {{Health:20f,NoAI:1b}}",
        BX as f64 - 1.5,
        by as f64,
        BZ as f64 + 0.5
    ))
    .await;
    p.pump(500).await;
    let killed_by = reg["minecraft:stat_type"]["entries"]["minecraft:killed_by"]["protocol_id"]
        .as_i64()
        .unwrap() as i32;
    let zombie_type = reg["minecraft:entity_type"]["entries"]["minecraft:zombie"]["protocol_id"]
        .as_i64()
        .unwrap() as i32;
    let deaths = cstat("minecraft:deaths");
    let before_killed = p.stat(killed_by, zombie_type);
    let before_deaths = p.stat(8, deaths);
    p.cmd("damage @s 100 minecraft:mob_attack by @e[type=minecraft:zombie,limit=1,sort=nearest]")
        .await;
    p.pump(1200).await;
    assert!(
        p.done("minecraft:adventure/root", "killed_by_something"),
        "entity_killed_player не выдал критерий: {:?}",
        p.adv.get("minecraft:adventure/root")
    );
    assert!(
        p.stat(killed_by, zombie_type) > before_killed,
        "killed_by zombie не вырос"
    );
    assert_eq!(p.stat(8, deaths), before_deaths + 1, "deaths не вырос");
    println!("entity_killed_player ok");

    println!("adv_extra: ok");
}
