//! Лодки: посадка мобов, высадка при поломке и толкание лодок.
//!
//! `boat_passenger_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

struct Probe {
    c: Client,
    sb_command: i32,
    cb_chat: i32,
    chat: String,
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
        while let Ok(Ok(_)) = tokio::time::timeout_at(deadline, self.c.next_event()).await {}
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

    async fn pos(&mut self, kind: &str) -> [f64; 3] {
        let query = format!("@e[type={kind},limit=1,sort=nearest] Pos");
        let mut text = self.data(&query).await;
        if vec3(&text).is_none() {
            text = self.data(&query).await;
        }
        vec3(&text).unwrap_or_else(|| panic!("нет Pos у {kind}: {text}"))
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
        cb_chat,
        chat: String::new(),
    };
    p.pump(1200).await;

    // Фиксированная площадка (сохранённые позиции ботов и правки чанков
    // ненадёжны между перезапусками).
    let (bx, bz) = (3300, 3300);
    let by = 64;
    for line in [
        "difficulty peaceful".to_string(),
        "gamemode survival".to_string(),
        "kill @e[type=minecraft:oak_boat]".to_string(),
        "kill @e[type=minecraft:spruce_boat]".to_string(),
        "kill @e[type=minecraft:cow]".to_string(),
        format!(
            "fill {} {} {} {} {} {} minecraft:air",
            bx - 10,
            by,
            bz - 10,
            bx + 10,
            by + 5,
            bz + 10
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            bx - 10,
            by - 1,
            bz - 10,
            bx + 10,
            by - 1,
            bz + 10
        ),
        format!("tp boater {} {} {}", bx as f64 + 0.5, by, bz as f64 + 0.5),
    ] {
        p.cmd(&line).await;
    }

    // 1. Посадка моба: корова задевает пустую лодку и садится в неё.
    let boat = [bx as f64 + 0.5, by as f64, bz as f64 + 0.5];
    p.cmd(&format!(
        "summon minecraft:oak_boat {} {} {}",
        boat[0], boat[1], boat[2]
    ))
    .await;
    p.cmd(&format!(
        "summon minecraft:cow {} {} {} {{NoAI:1b}}",
        boat[0] + 1.1,
        boat[1],
        boat[2]
    ))
    .await;
    p.pump(1200).await;
    let cow = p.pos("minecraft:cow").await;
    let at = p.pos("minecraft:oak_boat").await;
    assert!(
        (cow[1] - (at[1] + 0.1875)).abs() < 0.06,
        "корова не села в лодку: корова {cow:?}, лодка {at:?}"
    );
    assert!(
        (cow[0] - at[0]).abs() < 0.06 && (cow[2] - at[2]).abs() < 0.06,
        "корова не в точке пассажира: корова {cow:?}, лодка {at:?}"
    );
    println!("cow rides the boat at {:?}", cow);

    // 2. Поломка лодки: пассажир высаживается на землю.
    p.cmd("kill @e[type=minecraft:oak_boat]").await;
    p.pump(800).await;
    let cow = p.pos("minecraft:cow").await;
    assert!(
        (cow[1] - by as f64).abs() < 0.2,
        "корова не вышла из лодки: {cow:?}"
    );
    println!("cow dismounts at y={:.3}", cow[1]);
    p.cmd("kill @e[type=minecraft:cow]").await;

    // 3. Две пустые лодки толкают друг друга (`Entity.push`).
    let (ox, oz) = (bx as f64 + 0.5, bz as f64 + 4.5);
    p.cmd(&format!(
        "summon minecraft:oak_boat {} {} {}",
        ox, by as f64, oz
    ))
    .await;
    p.cmd(&format!(
        "summon minecraft:spruce_boat {} {} {}",
        ox + 0.9,
        by as f64,
        oz
    ))
    .await;
    p.pump(1800).await;
    let oak = p.pos("minecraft:oak_boat").await;
    let spruce = p.pos("minecraft:spruce_boat").await;
    assert!(
        (spruce[0] - oak[0]).abs() > 1.3,
        "лодки не разъехались: {oak:?} / {spruce:?}"
    );
    println!("boats pushed apart: dx={:.2}", (spruce[0] - oak[0]).abs());

    p.cmd("kill @e[type=minecraft:oak_boat]").await;
    p.cmd("kill @e[type=minecraft:spruce_boat]").await;
    p.cmd(&format!(
        "tp boater {} 4 {}",
        bx as f64 + 0.5,
        bz as f64 + 0.5
    ))
    .await;
    p.pump(400).await;
    println!("boat_passenger: ok");
}
