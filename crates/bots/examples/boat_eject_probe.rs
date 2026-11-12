//! Лодка: серверная высадка игрока после 60 тиков под водой.
//!
//! `boat_eject_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_move_vehicle: i32,
    cb_chat: i32,
    boat_ty: i32,
    chat: String,
    boat_id: Option<i32>,
    passengers: Vec<(i32, Vec<i32>)>,
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
        while let Ok(Ok(ev)) = tokio::time::timeout_at(deadline, self.c.next_event()).await {
            match ev {
                Event::EntitySpawned {
                    entity_id, kind, ..
                } if kind == self.boat_ty => {
                    self.boat_id.get_or_insert(entity_id);
                }
                Event::EntityPassengers {
                    vehicle,
                    passengers,
                } => self.passengers.push((vehicle, passengers)),
                _ => {}
            }
        }
        self.drain();
    }

    async fn cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(250).await;
    }

    async fn data(&mut self, query: &str) -> String {
        self.pump(150).await;
        self.chat.clear();
        self.cmd(&format!("data get entity {query}")).await;
        if self.chat.trim().is_empty() {
            self.pump(300).await;
            self.cmd(&format!("data get entity {query}")).await;
        }
        self.chat.clone()
    }

    async fn pos(&mut self, query: &str) -> [f64; 3] {
        let mut text = self.data(query).await;
        if vec3(&text).is_none() {
            text = self.data(query).await;
        }
        vec3(&text).unwrap_or_else(|| panic!("нет Pos у {query}: {text}"))
    }

    async fn move_vehicle(&mut self, pos: [f64; 3], yaw: f32) {
        let mut w = self.packet(self.sb_move_vehicle);
        for v in pos {
            w.f64(v);
        }
        w.f32(yaw).f32(0.0).bool(true);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(400).await;
    }
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

fn numbers(text: &str) -> Vec<f64> {
    let body = match (text.find('['), text.rfind(']')) {
        (Some(a), Some(b)) if b > a => &text[a + 1..b],
        _ => return Vec::new(),
    };
    body.split(',')
        .filter_map(|t| {
            t.trim()
                .trim_end_matches('d')
                .trim_end_matches('f')
                .parse()
                .ok()
        })
        .collect()
}

fn vec3(text: &str) -> Option<[f64; 3]> {
    let v = numbers(text);
    (v.len() >= 3).then(|| [v[0], v[1], v[2]])
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
    let boat_ty = reg["minecraft:entity_type"]["entries"]["minecraft:oak_boat"]["protocol_id"]
        .as_i64()
        .unwrap() as i32;
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n| table.id(State::Play, Direction::Clientbound, n).unwrap();
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    let _spawn = loop {
        if let Event::Position(p) = c.next_event().await.unwrap() {
            break p;
        }
    };
    let cb_chat = cb("system_chat");
    c.capture_also = vec![cb_chat];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_move_vehicle: sb("move_vehicle"),
        cb_chat,
        boat_ty,
        chat: String::new(),
        boat_id: None,
        passengers: Vec::new(),
    };
    p.pump(1200).await;

    let (bx, bz) = (3400, 3400);
    let by = 64;
    for line in [
        "difficulty easy".to_string(),
        "gamemode survival".to_string(),
        "kill @e[type=minecraft:oak_boat]".to_string(),
        format!(
            "fill {} {} {} {} {} {} minecraft:air",
            bx - 4,
            by,
            bz - 4,
            bx + 4,
            by + 15,
            bz + 4
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            bx - 4,
            by - 1,
            bz - 4,
            bx + 4,
            by - 1,
            bz + 4
        ),
        // Стенки бассейна: вода не уходит в стороны.
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            bx - 4,
            by,
            bz - 4,
            bx - 4,
            by + 15,
            bz + 4
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            bx + 4,
            by,
            bz - 4,
            bx + 4,
            by + 15,
            bz + 4
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            bx - 4,
            by,
            bz - 4,
            bx + 4,
            by + 15,
            bz - 4
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            bx - 4,
            by,
            bz + 4,
            bx + 4,
            by + 15,
            bz + 4
        ),
        format!("tp boater {} {} {}", bx as f64 + 0.5, by, bz as f64 + 0.5),
    ] {
        p.cmd(&line).await;
    }

    // Садим игрока на лодку на сухом полу.
    p.cmd(&format!(
        "summon minecraft:oak_boat {} {} {}",
        bx as f64 + 0.5,
        by,
        bz as f64 + 0.5
    ))
    .await;
    p.pump(600).await;
    let boat = p.boat_id.expect("лодка не появилась");
    let mut w = Writer::new();
    w.var_int(sb("interact"))
        .var_int(boat)
        .var_int(0)
        .u8(0)
        .bool(false);
    p.c.send_raw(&w.buf).await.unwrap();
    p.pump(500).await;

    // Наливаем воду на лодку: она остаётся под водой (игроком не правится).
    p.cmd(&format!(
        "fill {} {} {} {} {} {} minecraft:water",
        bx - 3,
        by,
        bz - 3,
        bx + 3,
        by + 13,
        bz + 3
    ))
    .await;
    let before = p
        .pos("@e[type=minecraft:oak_boat,limit=1,sort=nearest] Pos")
        .await;

    // `outOfControlTicks` растёт, на 60 пассажиры сходят (`ejectPassengers`).
    p.pump(4000).await;
    let empty = p
        .passengers
        .iter()
        .any(|(v, list)| *v == boat && list.is_empty());
    assert!(
        empty,
        "нет set_passengers с пустым списком: {:?}",
        p.passengers
    );

    // Если игрок ещё в лодке, `move_vehicle` утащил бы её на 5 блоков.
    p.move_vehicle([before[0] + 5.0, before[1], before[2]], 0.0)
        .await;
    let after = p
        .pos("@e[type=minecraft:oak_boat,limit=1,sort=nearest] Pos")
        .await;
    assert!(
        (after[0] - before[0]).abs() < 1.5,
        "лодка уехала с игроком: {before:?} → {after:?}"
    );
    println!("player ejected, boat stays: {before:?} → {after:?}");

    p.cmd("kill @e[type=minecraft:oak_boat]").await;
    p.cmd(&format!(
        "tp boater {} 4 {}",
        bx as f64 + 0.5,
        bz as f64 + 0.5
    ))
    .await;
    p.pump(400).await;
    println!("boat_eject: ok");
}
