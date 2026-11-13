//! Триггеры достижений: shot_crossbow, used_totem, nether_travel и
//! channeled_lightning.
//!
//! `adv_misc_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::collections::HashMap;
use std::time::Duration;

const BX: i32 = 5700;
const BZ: i32 = 5700;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_select: i32,
    sb_use: i32,
    sb_move: i32,
    sb_action: i32,
    cb_chat: i32,
    seq: i32,
    chat: String,
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
                let line = flatten(&component);
                if !line.trim().is_empty() {
                    self.chat.push_str(line.trim());
                    self.chat.push(' ');
                }
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
        self.drain();
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
        self.pump(300).await;
    }

    async fn select(&mut self, slot: u16) {
        let mut w = self.packet(self.sb_select);
        w.u16(slot);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(120).await;
    }

    async fn pose(&mut self, pos: [f64; 3], yaw: f32, pitch: f32) {
        let mut w = self.packet(self.sb_move);
        w.f64(pos[0])
            .f64(pos[1])
            .f64(pos[2])
            .f32(yaw)
            .f32(pitch)
            .u8(1);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(120).await;
    }

    async fn use_item(&mut self, yaw: f32, pitch: f32) {
        self.seq += 1;
        let mut w = self.packet(self.sb_use);
        w.var_int(0).var_int(self.seq).f32(yaw).f32(pitch);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(200).await;
    }

    async fn release(&mut self) {
        self.seq += 1;
        let mut w = self.packet(self.sb_action);
        w.var_int(6).i64(0).u8(0).var_int(self.seq);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(900).await;
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n| table.id(State::Play, Direction::Clientbound, n).unwrap();
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    let cb_chat = cb("system_chat");
    c.capture_also = vec![cb_chat];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_select: sb("set_carried_item"),
        sb_use: sb("use_item"),
        sb_move: sb("move_player_pos_rot"),
        sb_action: sb("player_action"),
        cb_chat,
        seq: 0,
        chat: String::new(),
        adv: HashMap::new(),
    };
    p.pump(2000).await;

    let by = 64;
    for line in [
        "difficulty easy".to_string(),
        "gamemode survival".to_string(),
        "weather clear".to_string(),
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
        format!(
            "tp boater {} {} {} 0 0",
            BX as f64 + 0.5,
            by,
            BZ as f64 + 0.5
        ),
        "clear boater".to_string(),
        "effect clear @s".to_string(),
        "kill @e[type=minecraft:zombie,limit=10]".to_string(),
        "kill @e[type=minecraft:villager,limit=10]".to_string(),
        "advancement grant @s only minecraft:adventure/root".to_string(),
        "advancement grant @s only minecraft:adventure/kill_a_mob".to_string(),
        "advancement grant @s only minecraft:adventure/throw_trident".to_string(),
        "advancement grant @s only minecraft:nether/root".to_string(),
    ] {
        p.cmd(&line).await;
    }

    // 1. shot_crossbow: adventure/ol_betsy.
    p.cmd("clear boater").await;
    p.cmd("give boater minecraft:crossbow 1").await;
    p.cmd("give boater minecraft:arrow 8").await;
    p.select(0).await;
    p.pose([BX as f64 + 0.5, by as f64, BZ as f64 + 0.5], 0.0, 0.0)
        .await;
    p.use_item(0.0, 0.0).await;
    p.pump(3000).await;
    p.release().await;
    p.use_item(0.0, 0.0).await;
    p.pump(800).await;
    assert!(
        p.done("minecraft:adventure/ol_betsy", "shot_crossbow"),
        "shot_crossbow не сработал: {:?}",
        p.adv.get("minecraft:adventure/ol_betsy")
    );
    println!("shot_crossbow ok");

    // 2. used_totem: adventure/totem_of_undying.
    p.cmd("clear boater").await;
    p.cmd("give boater minecraft:totem_of_undying 1").await;
    p.select(0).await;
    p.pose([BX as f64 + 0.5, by as f64, BZ as f64 + 0.5], 0.0, 0.0)
        .await;
    p.cmd("damage @s 100 minecraft:generic").await;
    p.pump(800).await;
    assert!(
        p.done("minecraft:adventure/totem_of_undying", "used_totem"),
        "used_totem не сработал: {:?}",
        p.adv.get("minecraft:adventure/totem_of_undying")
    );
    println!("used_totem ok");
    p.cmd("effect give @s minecraft:regeneration 10 4").await;

    // 3. nether_travel: nether/fast_travel (7000+ блоков по горизонтали).
    p.cmd(&format!(
        "tp boater {} {} {} 0 0",
        BX as f64 + 0.5,
        by,
        BZ as f64 + 0.5
    ))
    .await;
    p.cmd("execute in minecraft:the_nether run tp @s 0 64 0")
        .await;
    p.pump(600).await;
    p.cmd("execute in minecraft:overworld run tp @s 0 64 0")
        .await;
    p.pump(800).await;
    assert!(
        p.done("minecraft:nether/fast_travel", "travelled"),
        "nether_travel не сработал: {:?}",
        p.adv.get("minecraft:nether/fast_travel")
    );
    println!("nether_travel ok");

    // 4. channeled_lightning: adventure/very_very_frightening.
    p.cmd(&format!(
        "execute in minecraft:overworld run tp @s {} {} {}",
        BX as f64 + 0.5,
        by,
        BZ as f64 + 0.5
    ))
    .await;
    p.cmd("weather thunder").await;
    p.cmd("clear boater").await;
    p.cmd("give boater minecraft:trident 1").await;
    p.select(0).await;
    p.cmd("enchant boater minecraft:channeling 1").await;
    p.cmd(&format!(
        "summon minecraft:zombie {} {} {}",
        BX as f64 + 0.5,
        by as f64,
        BZ as f64 + 4.5
    ))
    .await;
    p.cmd(&format!(
        "summon minecraft:villager {} {} {}",
        BX as f64 + 2.5,
        by as f64,
        BZ as f64 + 4.5
    ))
    .await;
    p.pose([BX as f64 + 0.5, by as f64, BZ as f64 + 0.5], 0.0, 9.0)
        .await;
    p.use_item(0.0, 9.0).await;
    p.pump(1200).await;
    p.release().await;
    p.pump(1500).await;
    assert!(
        p.done(
            "minecraft:adventure/very_very_frightening",
            "struck_villager"
        ),
        "channeled_lightning не сработал: {:?}",
        p.adv.get("minecraft:adventure/very_very_frightening")
    );
    println!("channeled_lightning ok");

    p.cmd("weather clear").await;
    p.cmd("kill @e[type=minecraft:zombie,limit=10]").await;
    p.cmd("kill @e[type=minecraft:villager,limit=10]").await;
    p.cmd("effect clear @s").await;
    println!("adv_misc: ok");
}
