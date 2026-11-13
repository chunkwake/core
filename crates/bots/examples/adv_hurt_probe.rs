//! Статистика target_hit и триггеры player_hurt_entity/entity_hurt_player.
//!
//! `adv_hurt_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::collections::HashMap;
use std::time::Duration;

const BX: i32 = 5600;
const BZ: i32 = 5600;

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
                self.chat.push_str(line.trim());
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
    let custom = reg["minecraft:stat_type"]["entries"]["minecraft:custom"]["protocol_id"]
        .as_i64()
        .unwrap() as i32;
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
        stats: HashMap::new(),
        adv: HashMap::new(),
    };
    p.pump(2000).await;

    let by = 64;
    for line in [
        "difficulty easy".to_string(),
        "gamemode survival".to_string(),
        format!(
            "fill {} {} {} {} {} {} minecraft:air",
            BX - 6,
            by,
            BZ - 6,
            BX + 10,
            by + 5,
            BZ + 10
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            BX - 6,
            by - 1,
            BZ - 6,
            BX + 10,
            by - 1,
            BZ + 10
        ),
        "kill @e[type=minecraft:zombie]".to_string(),
        "clear boater".to_string(),
        "effect clear @s".to_string(),
        "advancement grant @s only minecraft:adventure/kill_a_mob".to_string(),
        "advancement grant @s only minecraft:story/obtain_armor".to_string(),
    ] {
        p.cmd(&line).await;
    }

    // 1. target_hit: выстрел из лука в мишень.
    p.cmd(&format!(
        "fill {} {} {} {} {} {} minecraft:target",
        BX + 6,
        by - 1,
        BZ - 3,
        BX + 6,
        by + 4,
        BZ + 3
    ))
    .await;
    p.cmd(&format!(
        "tp boater {} {} {} -90 12",
        BX as f64 + 0.5,
        by,
        BZ as f64 + 0.5
    ))
    .await;
    p.cmd("clear boater").await;
    p.cmd("give boater minecraft:bow 1").await;
    p.cmd("give boater minecraft:arrow 16").await;
    p.select(0).await;
    p.pose([BX as f64 + 0.5, by as f64, BZ as f64 + 0.5], -90.0, 12.0)
        .await;
    let stat = cstat("minecraft:target_hit");
    let before = p.stat(custom, stat);
    p.use_item(-90.0, 12.0).await;
    p.pump(1400).await;
    p.release().await;
    p.pump(1200).await;
    assert!(
        p.stat(custom, stat) > before,
        "target_hit не вырос: {} -> {}",
        before,
        p.stat(custom, stat)
    );
    println!("target_hit ok (+{})", p.stat(custom, stat) - before);

    // 2. player_hurt_entity: стрела по зомби (adventure/shoot_arrow).
    p.cmd("kill @e[type=minecraft:zombie]").await;
    p.cmd(&format!(
        "tp boater {} {} {} 0 10",
        BX as f64 + 0.5,
        by,
        BZ as f64 + 0.5
    ))
    .await;
    p.cmd(&format!(
        "summon minecraft:zombie {} {} {}",
        BX as f64 + 0.5,
        by as f64,
        BZ as f64 + 4.5
    ))
    .await;
    p.cmd("clear boater").await;
    p.cmd("give boater minecraft:bow 1").await;
    p.cmd("give boater minecraft:arrow 16").await;
    p.select(0).await;
    p.pose([BX as f64 + 0.5, by as f64, BZ as f64 + 0.5], 0.0, 9.0)
        .await;
    p.use_item(0.0, 9.0).await;
    p.pump(1400).await;
    p.release().await;
    p.pump(1200).await;
    assert!(
        p.done("minecraft:adventure/shoot_arrow", "shot_arrow"),
        "player_hurt_entity (стрела) не сработал: {:?}",
        p.adv.get("minecraft:adventure/shoot_arrow")
    );
    println!("player_hurt_entity (arrow) ok");

    // 3. player_hurt_entity: трезубец (adventure/throw_trident).
    p.cmd("kill @e[type=minecraft:zombie]").await;
    p.cmd(&format!(
        "tp boater {} {} {} 0 10",
        BX as f64 + 0.5,
        by,
        BZ as f64 + 0.5
    ))
    .await;
    p.cmd(&format!(
        "summon minecraft:zombie {} {} {}",
        BX as f64 + 0.5,
        by as f64,
        BZ as f64 + 4.5
    ))
    .await;
    p.cmd("clear boater").await;
    p.cmd("give boater minecraft:trident 1").await;
    p.select(0).await;
    p.pose([BX as f64 + 0.5, by as f64, BZ as f64 + 0.5], 0.0, 9.0)
        .await;
    p.use_item(0.0, 9.0).await;
    p.pump(900).await;
    p.release().await;
    p.pump(1200).await;
    assert!(
        p.done("minecraft:adventure/throw_trident", "shot_trident"),
        "player_hurt_entity (трезубец) не сработал: {:?}",
        p.adv.get("minecraft:adventure/throw_trident")
    );
    println!("player_hurt_entity (trident) ok");

    // 4. entity_hurt_player: щит блокирует стрелу (story/deflect_arrow).
    p.cmd("kill @e[type=minecraft:zombie]").await;
    p.cmd(&format!(
        "tp boater {} {} {} 0 0",
        BX as f64 + 0.5,
        by,
        BZ as f64 + 0.5
    ))
    .await;
    p.cmd("clear boater").await;
    p.cmd("give boater minecraft:shield 1").await;
    p.select(0).await;
    p.pose([BX as f64 + 0.5, by as f64, BZ as f64 + 0.5], 0.0, 0.0)
        .await;
    p.use_item(0.0, 0.0).await;
    p.pump(500).await;
    p.cmd("damage @s 5 minecraft:arrow at ~ ~ ~2").await;
    p.pump(500).await;
    p.release().await;
    assert!(
        p.done("minecraft:story/deflect_arrow", "deflected_projectile"),
        "entity_hurt_player (щит) не сработал: {:?}",
        p.adv.get("minecraft:story/deflect_arrow")
    );
    println!("entity_hurt_player (blocked) ok");

    p.cmd("kill @e[type=minecraft:zombie,limit=10]").await;
    p.cmd("effect clear @s").await;
    println!("adv_hurt: ok");
}
