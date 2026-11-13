//! Остальные статистики движения: полёт, подъём, падение, вода, лодка и
//! вагонетка.
//!
//! `stats_ride_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::collections::HashMap;
use std::time::Duration;

const BX: i32 = 5200;
const BZ: i32 = 5200;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_move: i32,
    sb_move_vehicle: i32,
    sb_interact: i32,
    sb_use: i32,
    sb_select: i32,
    sb_input: i32,
    sb_command_act: i32,
    cb_chat: i32,
    boat_ty: i32,
    cart_ty: i32,
    chat: String,
    seq: i32,
    stats: HashMap<(i32, i32), i32>,
    boat: Option<i32>,
    cart: Option<i32>,
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
                } => {
                    if kind == self.boat_ty && (pos[0] - BX as f64).abs() < 12.0 {
                        self.boat = Some(entity_id);
                    } else if kind == self.cart_ty
                        && (pos[0] - BX as f64).abs() < 16.0
                        && (pos[2] - BZ as f64).abs() < 16.0
                    {
                        self.cart = Some(entity_id);
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

    async fn input(&mut self, flags: u8) {
        let mut w = self.packet(self.sb_input);
        w.u8(flags);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(150).await;
    }

    async fn interact(&mut self, entity: i32) {
        let mut w = self.packet(self.sb_interact);
        w.var_int(entity).var_int(0).u8(0).bool(false);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(500).await;
    }

    async fn use_item(&mut self, yaw: f32, pitch: f32) {
        self.seq += 1;
        let mut w = self.packet(self.sb_use);
        w.var_int(0).var_int(self.seq).f32(yaw).f32(pitch);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(400).await;
    }

    async fn drive_boat(&mut self, start: [f64; 3]) {
        let mut pos = start;
        for _ in 0..10 {
            pos[2] += 0.2;
            let mut w = self.packet(self.sb_move_vehicle);
            w.f64(pos[0])
                .f64(pos[1])
                .f64(pos[2])
                .f32(0.0)
                .f32(0.0)
                .bool(true);
            self.c.send_raw(&w.buf).await.unwrap();
            self.pump(80).await;
        }
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
    let (fly, climb, fall) = (
        cstat("minecraft:fly_one_cm"),
        cstat("minecraft:climb_one_cm"),
        cstat("minecraft:fall_one_cm"),
    );
    let (under, on_water, swim) = (
        cstat("minecraft:walk_under_water_one_cm"),
        cstat("minecraft:walk_on_water_one_cm"),
        cstat("minecraft:swim_one_cm"),
    );
    let (boat_cm, cart_cm) = (
        cstat("minecraft:boat_one_cm"),
        cstat("minecraft:minecart_one_cm"),
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
        sb_move_vehicle: sb("move_vehicle"),
        sb_interact: sb("interact"),
        sb_use: sb("use_item"),
        sb_select: sb("set_carried_item"),
        sb_input: sb("player_input"),
        sb_command_act: sb("player_command"),
        cb_chat,
        boat_ty: ent("minecraft:oak_boat"),
        cart_ty: ent("minecraft:minecart"),
        chat: String::new(),
        seq: 0,
        stats: HashMap::new(),
        boat: None,
        cart: None,
    };
    p.pump(2000).await;

    let by = 64;
    for line in [
        "difficulty peaceful".to_string(),
        "gamemode survival".to_string(),
        "kill @e[type=minecraft:oak_boat]".to_string(),
        "kill @e[type=minecraft:minecart]".to_string(),
        format!(
            "fill {} {} {} {} {} {} minecraft:air",
            BX - 8,
            by,
            BZ - 8,
            BX + 16,
            by + 6,
            BZ + 8
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            BX - 8,
            by - 1,
            BZ - 8,
            BX + 16,
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
    ] {
        p.cmd(&line).await;
    }

    // 1. fly_one_cm: в воздухе больше 0.25 блока по горизонтали.
    let x = BX as f64 + 0.5;
    p.move_to([x, by as f64 + 3.0, BZ as f64 + 0.5], false)
        .await;
    let before = p.stat(custom, fly);
    p.move_to([x + 1.0, by as f64 + 3.0, BZ as f64 + 0.5], false)
        .await;
    let flown = p.stat(custom, fly) - before;
    assert!(flown >= 100, "fly_one_cm не вырос: +{flown}");
    println!("fly_one_cm ok (+{flown})");

    // 2. climb_one_cm: лестница и подъём на 0.5.
    p.cmd(&format!(
        "setblock {} {} {} minecraft:ladder[facing=north]",
        BX + 2,
        by,
        BZ
    ))
    .await;
    p.cmd(&format!(
        "tp boater {} {} {} 0 0",
        BX as f64 + 2.5,
        by,
        BZ as f64 + 0.5
    ))
    .await;
    let before = p.stat(custom, climb);
    p.move_to([BX as f64 + 2.5, by as f64 + 0.5, BZ as f64 + 0.5], false)
        .await;
    let climbed = p.stat(custom, climb) - before;
    assert!(climbed >= 50, "climb_one_cm не вырос: +{climbed}");
    println!("climb_one_cm ok (+{climbed})");

    // 3. fall_one_cm: подъём и падение на 3 блока.
    p.cmd(&format!(
        "tp boater {} {} {} 0 0",
        BX as f64 + 3.5,
        by,
        BZ as f64 + 0.5
    ))
    .await;
    p.move_to([BX as f64 + 3.5, by as f64 + 3.0, BZ as f64 + 0.5], false)
        .await;
    let before = p.stat(custom, fall);
    p.move_to([BX as f64 + 3.5, by as f64, BZ as f64 + 0.5], false)
        .await;
    p.move_to([BX as f64 + 3.5, by as f64, BZ as f64 + 0.5], true)
        .await;
    let fallen = p.stat(custom, fall) - before;
    assert!(fallen >= 300, "fall_one_cm не вырос: +{fallen}");
    println!("fall_one_cm ok (+{fallen})");

    // 4. Вода: под водой, по поверхности и вплавь.
    p.cmd(&format!(
        "fill {} {} {} {} {} {} minecraft:water",
        BX + 4,
        by,
        BZ,
        BX + 6,
        by + 1,
        BZ + 2
    ))
    .await;
    p.cmd(&format!(
        "tp boater {} {} {} 0 0",
        BX as f64 + 5.5,
        by,
        BZ as f64 + 0.5
    ))
    .await;
    let before = p.stat(custom, under);
    p.move_to([BX as f64 + 6.2, by as f64, BZ as f64 + 0.5], false)
        .await;
    let under_delta = p.stat(custom, under) - before;
    assert!(
        under_delta >= 50,
        "walk_under_water_one_cm не вырос: +{under_delta}"
    );
    println!("walk_under_water_one_cm ok (+{under_delta})");

    p.cmd(&format!(
        "tp boater {} {} {} 0 0",
        BX as f64 + 5.5,
        by + 1,
        BZ as f64 + 0.5
    ))
    .await;
    let before = p.stat(custom, on_water);
    p.move_to([BX as f64 + 6.2, by as f64 + 1.0, BZ as f64 + 0.5], false)
        .await;
    let on_delta = p.stat(custom, on_water) - before;
    assert!(on_delta >= 50, "walk_on_water_one_cm не вырос: +{on_delta}");
    println!("walk_on_water_one_cm ok (+{on_delta})");

    p.sprint(true).await;
    p.cmd(&format!(
        "tp boater {} {} {} 0 0",
        BX as f64 + 5.5,
        by,
        BZ as f64 + 0.5
    ))
    .await;
    let before = p.stat(custom, swim);
    p.move_to([BX as f64 + 6.2, by as f64, BZ as f64 + 0.5], false)
        .await;
    let swim_delta = p.stat(custom, swim) - before;
    assert!(swim_delta >= 50, "swim_one_cm не вырос: +{swim_delta}");
    p.sprint(false).await;
    println!("swim_one_cm ok (+{swim_delta})");

    // 5. boat_one_cm: поставить лодку, сесть и проехать.
    p.cmd(&format!(
        "tp boater {} {} {} 0 60",
        BX as f64 + 8.5,
        by,
        BZ as f64 + 0.5
    ))
    .await;
    p.cmd("clear boater").await;
    p.cmd("give boater minecraft:oak_boat 1").await;
    p.select(0).await;
    p.boat = None;
    p.use_item(0.0, 60.0).await;
    p.pump(500).await;
    let boat = p.boat.expect("лодка не появилась");
    p.interact(boat).await;
    let start = [BX as f64 + 8.5, by as f64, BZ as f64 + 0.5];
    let before = p.stat(custom, boat_cm);
    p.drive_boat(start).await;
    let boat_delta = p.stat(custom, boat_cm) - before;
    assert!(boat_delta >= 100, "boat_one_cm не вырос: +{boat_delta}");
    println!("boat_one_cm ok (+{boat_delta})");
    p.input(0x20).await;
    p.pump(400).await;
    p.input(0).await;

    // 6. minecart_one_cm: рельсы, вагонетка с движением и езда.
    p.cmd(&format!(
        "fill {} {} {} {} {} {} minecraft:rail",
        BX + 10,
        by - 1,
        BZ - 4,
        BX + 10,
        by - 1,
        BZ + 6
    ))
    .await;
    p.cart = None;
    p.cmd(&format!(
        "summon minecraft:minecart {} {} {} {{Motion:[0.0d,0.0d,0.4d]}}",
        BX as f64 + 10.5,
        by as f64 - 1.0,
        BZ as f64 - 3.5
    ))
    .await;
    p.pump(600).await;
    let cart = p.cart.expect("вагонетка не появилась");
    p.interact(cart).await;
    let before = p.stat(custom, cart_cm);
    p.pump(2500).await;
    let cart_delta = p.stat(custom, cart_cm) - before;
    assert!(cart_delta >= 50, "minecart_one_cm не вырос: +{cart_delta}");
    println!("minecart_one_cm ok (+{cart_delta})");

    p.cmd("kill @e[type=minecraft:oak_boat]").await;
    p.cmd("kill @e[type=minecraft:minecart]").await;
    println!("stats_ride: ok");
}
