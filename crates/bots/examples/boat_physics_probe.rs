//! Физика пустой лодки: падение, плавучесть и дрейф по течению.
//!
//! `boat_physics_probe <addr> <packets.json> <registries.json>`
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
        self.pump(220).await;
    }

    async fn data(&mut self, query: &str) -> String {
        self.pump(150).await;
        self.chat.clear();
        self.cmd(&format!("data get entity {query}")).await;
        // Ответ иногда теряется под нагрузкой — один повтор.
        if self.chat.trim().is_empty() {
            self.pump(300).await;
            self.cmd(&format!("data get entity {query}")).await;
        }
        self.chat.clone()
    }

    async fn pos(&mut self) -> [f64; 3] {
        let query = "@e[type=minecraft:oak_boat,limit=1,sort=nearest] Pos";
        let mut text = self.data(query).await;
        if vec3(&text).is_none() {
            text = self.data(query).await;
        }
        vec3(&text).unwrap_or_else(|| panic!("нет Pos у лодки: {text}"))
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
    let spawn = loop {
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

    let _spawn = spawn;
    // Фиксированная площадка: иначе позиция входа бота (её сохраняет сервер)
    // сдвигалась бы на +72 за прогон и тесты уезжали по миру.
    let (bx, bz) = (3000, 3000);
    let by = 64;

    for line in [
        "difficulty peaceful".to_string(),
        "gamemode creative".to_string(),
        "kill @e[type=minecraft:oak_boat]".to_string(),
        format!("tp boater {} {} {}", bx as f64 + 0.5, by, bz as f64 + 0.5),
    ] {
        p.cmd(&line).await;
    }

    // 1. Падение: лодка встаёт на камень, а не проваливается.
    let fall = (bx, bz);
    for line in [
        format!(
            "fill {} {} {} {} {} {} minecraft:air",
            fall.0 - 20,
            by,
            fall.1 - 20,
            fall.0 + 20,
            by + 5,
            fall.1 + 20
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            fall.0 - 20,
            by - 1,
            fall.1 - 20,
            fall.0 + 20,
            by - 1,
            fall.1 + 20
        ),
        format!(
            "tp boater {} {} {}",
            fall.0 as f64 + 0.5,
            by,
            fall.1 as f64 + 0.5
        ),
        format!(
            "summon minecraft:oak_boat {} {} {}",
            fall.0 as f64 + 0.5,
            by as f64 + 5.0,
            fall.1 as f64 + 0.5
        ),
    ] {
        p.cmd(&line).await;
    }
    p.pump(2600).await;
    let at = p.pos().await;
    assert!(
        (63.9..64.3).contains(&at[1]),
        "лодка не встала на камень: {at:?}"
    );
    println!("boat falls to ground at y={:.3}", at[1]);

    // 2. Плавучесть: лодка не тонет и не лежит на дне.
    let water = (bx + 20, bz);
    for line in [
        format!(
            "fill {} {} {} {} {} {} minecraft:air",
            water.0 - 4,
            by,
            water.1 - 4,
            water.0 + 4,
            by + 5,
            water.1 + 4
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            water.0 - 4,
            by - 1,
            water.1 - 4,
            water.0 + 4,
            by - 1,
            water.1 + 4
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:water",
            water.0 - 3,
            by,
            water.1 - 3,
            water.0 + 3,
            by,
            water.1 + 3
        ),
        format!(
            "tp boater {} {} {}",
            water.0 as f64 + 0.5,
            by,
            water.1 as f64 + 6.0
        ),
        format!(
            "summon minecraft:oak_boat {} {} {}",
            water.0 as f64 + 0.5,
            by as f64 + 4.0,
            water.1 as f64 + 0.5
        ),
    ] {
        p.cmd(&line).await;
    }
    p.pump(2600).await;
    let first = p.pos().await;
    assert!(
        first[1] > by as f64 + 0.15 && first[1] < by as f64 + 1.0,
        "лодка не держится на воде: {first:?}"
    );
    p.pump(1500).await;
    let second = p.pos().await;
    assert!(
        (second[1] - first[1]).abs() < 0.25,
        "лодка не стабилизировалась: {first:?} → {second:?}"
    );
    println!(
        "boat floats at y={:.3} (settled {:.3})",
        first[1], second[1]
    );

    // 3. Течение: вода растекается от источника, лодку несёт вниз по потоку.
    let flow = (bx + 40, bz);
    for line in [
        format!(
            "fill {} {} {} {} {} {} minecraft:air",
            flow.0 - 2,
            by,
            flow.1 - 3,
            flow.0 + 12,
            by + 4,
            flow.1 + 3
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            flow.0 - 2,
            by - 1,
            flow.1 - 3,
            flow.0 + 12,
            by - 1,
            flow.1 + 3
        ),
        // Стенки канала: вода не стекает в пустоту по бокам.
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            flow.0 - 2,
            by,
            flow.1 - 2,
            flow.0 + 12,
            by + 1,
            flow.1 - 2
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            flow.0 - 2,
            by,
            flow.1 + 2,
            flow.0 + 12,
            by + 1,
            flow.1 + 2
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            flow.0 - 2,
            by,
            flow.1 - 2,
            flow.0 - 2,
            by + 1,
            flow.1 + 2
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            flow.0 + 12,
            by,
            flow.1 - 2,
            flow.0 + 12,
            by + 1,
            flow.1 + 2
        ),
        format!(
            "tp boater {} {} {}",
            flow.0 as f64 + 3.5,
            by,
            flow.1 as f64 + 5.0
        ),
        format!("setblock {} {} {} minecraft:water", flow.0, by, flow.1),
    ] {
        p.cmd(&line).await;
    }
    // Дать воде растечься по каналу.
    p.pump(3000).await;
    p.cmd(&format!(
        "summon minecraft:oak_boat {} {} {}",
        flow.0 as f64 + 2.5,
        by as f64 + 1.5,
        flow.1 as f64 + 0.5
    ))
    .await;
    p.pump(700).await;
    let before = p.pos().await;
    p.pump(2500).await;
    let after = p.pos().await;
    assert!(
        after[0] - before[0] > 0.3,
        "лодку не понесло течением: {before:?} → {after:?}"
    );
    println!(
        "boat drifts downstream x {:.2} → {:.2}",
        before[0], after[0]
    );

    // Оставить игрока на естественной земле (y=4): искусственная площадка
    // после перезапуска может исчезнуть, и вход в пустоту убьёт бота.
    p.cmd(&format!(
        "tp boater {} 4 {}",
        fall.0 as f64 + 0.5,
        fall.1 as f64 + 0.5
    ))
    .await;
    p.pump(400).await;
    p.cmd("kill @e[type=minecraft:oak_boat]").await;
    println!("boat_physics: ok");
}
