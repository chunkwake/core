//! Riptide (спин-атака в воде) и Channeling (молния при попадании).
//!
//! `trident_enchants_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

const BX: i32 = 4500;
const BZ: i32 = 4500;
const DRY: i32 = 4480;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_use: i32,
    sb_action: i32,
    sb_select: i32,
    cb_chat: i32,
    zombie_ty: i32,
    drowned_ty: i32,
    lightning_ty: i32,
    chat: String,
    seq: i32,
    zombies: Vec<i32>,
    drowned: Vec<i32>,
    removed: HashSet<i32>,
    lightning: Vec<[f64; 3]>,
    stats: HashMap<(i32, i32), i32>,
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
                Event::EntitySpawned {
                    entity_id,
                    kind,
                    pos,
                } => {
                    if kind == self.zombie_ty {
                        self.zombies.push(entity_id);
                    } else if kind == self.drowned_ty {
                        self.drowned.push(entity_id);
                    } else if kind == self.lightning_ty {
                        self.lightning.push(pos);
                    }
                }
                Event::EntityRemoved(ids) => self.removed.extend(ids),
                Event::Stats { stats } => {
                    for (key, n) in stats {
                        self.stats.insert(key, n);
                    }
                }
                _ => {}
            }
        }
        self.drain();
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

    async fn use_item(&mut self, yaw: f32, pitch: f32) {
        self.seq += 1;
        let mut w = self.packet(self.sb_use);
        w.var_int(0).var_int(self.seq).f32(yaw).f32(pitch);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(150).await;
    }

    /// Отпустить трезубец; до 3 попыток, пока `target` не исчезнет.
    async fn throw_until_gone(&mut self, target: i32) {
        for _ in 0..3 {
            self.use_item(-90.0, 0.0).await;
            self.pump(900).await;
            self.release().await;
            self.pump(1200).await;
            if self.removed.contains(&target) {
                return;
            }
        }
    }

    async fn release(&mut self) {
        self.seq += 1;
        let mut w = self.packet(self.sb_action);
        w.var_int(6)
            .i64(pack_pos([0, 0, 0]))
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
    let reg: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::path::Path::new(&a[2]).with_file_name("registries.json"))
            .unwrap(),
    )
    .unwrap();
    let ent = |n: &str| {
        reg["minecraft:entity_type"]["entries"][n]["protocol_id"]
            .as_i64()
            .unwrap() as i32
    };
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
        sb_use: sb("use_item"),
        sb_action: sb("player_action"),
        sb_select: sb("set_carried_item"),
        cb_chat,
        zombie_ty: ent("minecraft:zombie"),
        drowned_ty: ent("minecraft:drowned"),
        lightning_ty: ent("minecraft:lightning_bolt"),
        chat: String::new(),
        seq: 0,
        zombies: Vec::new(),
        drowned: Vec::new(),
        removed: HashSet::new(),
        lightning: Vec::new(),
        stats: HashMap::new(),
    };
    p.pump(2000).await;

    let by = 64;
    for line in [
        "difficulty normal".to_string(),
        "gamemode survival".to_string(),
        "kill @e[type=minecraft:zombie]".to_string(),
        "time set midnight".to_string(),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            DRY - 5,
            by - 1,
            BZ - 5,
            BX + 6,
            by - 1,
            BZ + 5
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:air",
            DRY - 5,
            by,
            BZ - 5,
            BX + 6,
            by + 4,
            BZ + 5
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:water",
            BX - 2,
            by,
            BZ - 2,
            BX + 3,
            by,
            BZ + 2
        ),
        format!(
            "tp boater {} {} {} 0 0",
            DRY as f64 + 0.5,
            by,
            BZ as f64 + 0.5
        ),
        "clear boater".to_string(),
        "give boater minecraft:trident 1".to_string(),
        "enchant boater minecraft:riptide 1".to_string(),
    ] {
        p.cmd(&line).await;
    }
    p.select(0).await;

    // 1. Riptide без воды: спин-атаки нет, зомби жив.
    p.zombies.clear();
    p.cmd(&format!(
        "summon minecraft:zombie {} {} {} {{Health:5f,NoAI:1b}}",
        DRY as f64 + 2.5,
        by as f64,
        BZ as f64 + 0.5
    ))
    .await;
    p.pump(500).await;
    let dry_zombie = *p.zombies.last().expect("зомби для суши не появился");
    p.removed.clear();
    p.use_item(0.0, 0.0).await;
    p.pump(700).await;
    p.release().await;
    p.pump(500).await;
    assert!(
        !p.removed.contains(&dry_zombie),
        "Riptide сработал без воды"
    );
    println!("riptide dry ok");

    // 2. Riptide в воде: спин-атака убивает зомби.
    p.cmd(&format!(
        "tp boater {} {} {} 0 0",
        BX as f64 + 0.5,
        by,
        BZ as f64 + 0.5
    ))
    .await;
    p.zombies.clear();
    p.cmd(&format!(
        "summon minecraft:zombie {} {} {} {{Health:5f,NoAI:1b}}",
        BX as f64 + 2.5,
        by as f64,
        BZ as f64 + 0.5
    ))
    .await;
    p.pump(500).await;
    let wet_zombie = *p.zombies.last().expect("зомби в воде не появился");
    p.removed.clear();
    p.throw_until_gone(wet_zombie).await;
    assert!(
        p.removed.contains(&wet_zombie),
        "Riptide не убил зомби в воде: removed={:?}",
        p.removed
    );
    println!("riptide water ok");

    // 3. Channeling: гроза + попадание трезубца -> молния.
    p.cmd("clear boater").await;
    p.cmd("give boater minecraft:trident 1").await;
    p.cmd("enchant boater minecraft:channeling 1").await;
    p.select(0).await;
    p.cmd("weather thunder").await;
    p.cmd(&format!(
        "tp boater {} {} {} -90 0",
        BX as f64 + 0.5,
        by,
        BZ as f64 + 0.5
    ))
    .await;
    p.zombies.clear();
    p.cmd(&format!(
        "summon minecraft:zombie {} {} {} {{Health:20f,NoAI:1b}}",
        BX as f64 + 3.5,
        by as f64,
        BZ as f64 + 0.5
    ))
    .await;
    p.pump(500).await;
    let target = *p.zombies.last().expect("зомби для Channeling не появился");
    p.lightning.clear();
    p.removed.clear();
    p.use_item(-90.0, 0.0).await;
    p.pump(700).await;
    p.release().await;
    p.pump(1500).await;
    assert!(
        !p.lightning.is_empty(),
        "Channeling не вызвал молнию (цель {target})"
    );
    println!("channeling ok ({} lightning)", p.lightning.len());

    // 4. Impaling: +2.5×уровень по водным мобам (drowned).
    p.cmd("weather clear").await;
    p.cmd("clear boater").await;
    p.cmd("give boater minecraft:trident 1").await;
    p.cmd("enchant boater minecraft:impaling 3").await;
    p.cmd(&format!(
        "tp boater {} {} {} -90 0",
        BX as f64 + 0.5,
        by,
        BZ as f64 + 0.5
    ))
    .await;
    p.select(0).await;
    p.drowned.clear();
    p.cmd(&format!(
        "summon minecraft:drowned {} {} {} {{Health:12f,NoAI:1b}}",
        BX as f64 + 3.5,
        by as f64,
        BZ as f64 + 0.5
    ))
    .await;
    p.pump(600).await;
    let impaled = *p.drowned.last().expect("drowned для Impaling не появился");
    p.removed.clear();
    p.throw_until_gone(impaled).await;
    assert!(
        p.removed.contains(&impaled),
        "Impaling 3 не убил drowned с 12 HP: removed={:?}",
        p.removed
    );
    println!("impaling ok");

    // 5. Без Impaling тот же удар (8 урона) drowned с 12 HP не убивает.
    p.cmd("clear boater").await;
    p.cmd("give boater minecraft:trident 1").await;
    p.select(0).await;
    p.drowned.clear();
    p.cmd(&format!(
        "summon minecraft:drowned {} {} {} {{Health:12f,NoAI:1b}}",
        BX as f64 + 3.5,
        by as f64,
        BZ as f64 + 0.5
    ))
    .await;
    p.pump(600).await;
    let plain = *p.drowned.last().expect("drowned для контроля не появился");
    p.removed.clear();
    p.use_item(-90.0, 0.0).await;
    p.pump(700).await;
    p.release().await;
    p.pump(1500).await;
    assert!(
        !p.removed.contains(&plain),
        "обычный трезубец убил drowned с 12 HP"
    );
    println!("impaling control ok");

    println!("trident_enchants: ok");
}
