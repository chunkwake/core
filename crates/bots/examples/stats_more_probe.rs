//! Остальные custom-статистики: walk/sprint_one_cm, jump, damage_dealt,
//! damage_taken и open_chest.
//!
//! `stats_more_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::collections::HashMap;
use std::time::Duration;

const BX: i32 = 5000;
const BZ: i32 = 5000;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_move: i32,
    sb_command_act: i32,
    sb_attack: i32,
    sb_use_on: i32,
    sb_select: i32,
    cb_chat: i32,
    zombie_ty: i32,
    chat: String,
    seq: i32,
    stats: HashMap<(i32, i32), i32>,
    zombie: Option<i32>,
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
                Event::EntitySpawned {
                    entity_id,
                    kind,
                    pos,
                } if kind == self.zombie_ty
                    && (pos[0] - BX as f64 - 5.5).abs() < 4.0
                    && (pos[2] - BZ as f64 - 0.5).abs() < 4.0 =>
                {
                    self.zombie = Some(entity_id);
                }
                _ => {}
            }
        }
        self.drain();
    }

    fn stat(&self, kind: i32, value: i32) -> i32 {
        self.stats.get(&(kind, value)).copied().unwrap_or(0)
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
        self.pump(150).await;
    }

    async fn move_to(&mut self, pos: [f64; 3], on_ground: bool) {
        let mut w = self.packet(self.sb_move);
        w.f64(pos[0])
            .f64(pos[1])
            .f64(pos[2])
            .u8(u8::from(on_ground));
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(150).await;
    }

    async fn sprint(&mut self, on: bool) {
        self.seq += 1;
        let mut w = self.packet(self.sb_command_act);
        w.var_int(0).var_int(if on { 1 } else { 2 }).var_int(0);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(200).await;
    }

    async fn attack(&mut self, entity: i32) {
        let mut w = self.packet(self.sb_attack);
        w.var_int(entity);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(600).await;
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
    let cstat = |n: &str| {
        reg["minecraft:custom_stat"]["entries"][n]["protocol_id"]
            .as_i64()
            .unwrap() as i32
    };
    let custom = reg["minecraft:stat_type"]["entries"]["minecraft:custom"]["protocol_id"]
        .as_i64()
        .unwrap() as i32;
    let (walk, sprint, jump) = (
        cstat("minecraft:walk_one_cm"),
        cstat("minecraft:sprint_one_cm"),
        cstat("minecraft:jump"),
    );
    let (dealt, taken, chest) = (
        cstat("minecraft:damage_dealt"),
        cstat("minecraft:damage_taken"),
        cstat("minecraft:open_chest"),
    );
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
        sb_move: sb("move_player_pos"),
        sb_command_act: sb("player_command"),
        sb_attack: sb("attack"),
        sb_use_on: sb("use_item_on"),
        sb_select: sb("set_carried_item"),
        cb_chat,
        zombie_ty: ent("minecraft:zombie"),
        chat: String::new(),
        seq: 0,
        stats: HashMap::new(),
        zombie: None,
    };
    p.pump(2000).await;

    let by = 64;
    for line in [
        "difficulty normal".to_string(),
        "gamemode survival".to_string(),
        "kill @e[type=minecraft:zombie]".to_string(),
        format!(
            "fill {} {} {} {} {} {} minecraft:air",
            BX - 6,
            by,
            BZ - 6,
            BX + 6,
            by + 4,
            BZ + 6
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            BX - 6,
            by - 1,
            BZ - 6,
            BX + 6,
            by - 1,
            BZ + 6
        ),
        format!(
            "tp boater {} {} {} 0 0",
            BX as f64 + 0.5,
            by,
            BZ as f64 + 0.5
        ),
        "clear boater".to_string(),
        "give boater minecraft:iron_sword 1".to_string(),
    ] {
        p.cmd(&line).await;
    }
    p.select(0).await;
    p.pump(200).await;

    // 1. walk_one_cm: 2 блока шагом.
    let before = p.stat(custom, walk);
    for step in 1..=4 {
        p.move_to(
            [
                BX as f64 + 0.5 + step as f64 * 0.5,
                by as f64,
                BZ as f64 + 0.5,
            ],
            true,
        )
        .await;
    }
    let walked = p.stat(custom, walk) - before;
    assert!(
        (150..=250).contains(&walked),
        "walk_one_cm не вырос на ~200: +{walked}"
    );
    println!("walk_one_cm ok (+{walked})");

    // 2. sprint_one_cm: спринт и 2 блока.
    p.sprint(true).await;
    let before = p.stat(custom, sprint);
    let x = BX as f64 + 2.5;
    for step in 1..=4 {
        p.move_to([x + step as f64 * 0.5, by as f64, BZ as f64 + 0.5], true)
            .await;
    }
    let sprinted = p.stat(custom, sprint) - before;
    assert!(
        (150..=250).contains(&sprinted),
        "sprint_one_cm не вырос на ~200: +{sprinted}"
    );
    p.sprint(false).await;
    println!("sprint_one_cm ok (+{sprinted})");

    // 3. jump: отрыв от земли вверх.
    let x = BX as f64 + 4.5;
    let ground = [x, by as f64, BZ as f64 + 0.5];
    p.move_to(ground, true).await;
    let before = p.stat(custom, jump);
    p.move_to([x, by as f64 + 1.2, BZ as f64 + 0.5], false)
        .await;
    let jumped = p.stat(custom, jump) - before;
    assert_eq!(jumped, 1, "jump не вырос ровно на 1: +{jumped}");
    p.move_to(ground, true).await;
    println!("jump ok");
    p.pump(300).await;

    // 4. damage_taken: 5 единиц урона.
    let before = p.stat(custom, taken);
    p.cmd("damage @s 5 minecraft:generic").await;
    p.pump(600).await;
    let taken_delta = p.stat(custom, taken) - before;
    assert!(
        taken_delta >= 40,
        "damage_taken не вырос на ~50: +{taken_delta}"
    );
    println!("damage_taken ok (+{taken_delta})");

    // 5. damage_dealt: удар железным мечом по зомби.
    p.cmd(&format!(
        "summon minecraft:zombie {} {} {} {{Health:20f,NoAI:1b}}",
        BX as f64 + 5.5,
        by as f64,
        BZ as f64 + 0.5
    ))
    .await;
    p.pump(600).await;
    p.cmd(&format!(
        "tp boater {} {} {} -90 0",
        BX as f64 + 3.5,
        by as f64,
        BZ as f64 + 0.5
    ))
    .await;
    let zombie = p.zombie.expect("зомби не появился");
    let before = p.stat(custom, dealt);
    p.attack(zombie).await;
    p.pump(600).await;
    let dealt_delta = p.stat(custom, dealt) - before;
    assert!(
        dealt_delta >= 40,
        "damage_dealt не вырос на удар мечом: +{dealt_delta}"
    );
    println!("damage_dealt ok (+{dealt_delta})");

    // 6. open_chest: открыть сундук.
    p.cmd(&format!(
        "setblock {} {} {} minecraft:chest",
        BX + 1,
        by,
        BZ + 1
    ))
    .await;
    let before = p.stat(custom, chest);
    p.use_on([BX + 1, by, BZ + 1], 1).await;
    let opened = p.stat(custom, chest) - before;
    assert_eq!(opened, 1, "open_chest не вырос на 1: +{opened}");
    println!("open_chest ok");

    println!("stats_more: ok");
}
