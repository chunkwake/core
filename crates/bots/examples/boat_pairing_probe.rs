//! Лодка: повторная отправка пассажиров, когда моб входит в обзор позже.
//!
//! Нужен `view-distance` больше 8: лодка видна с 10 чанков, зомби — с 8,
//! поэтому на 140 блоках видна только лодка, на 100 — оба.
//!
//! `boat_pairing_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

struct Probe {
    c: Client,
    sb_command: i32,
    cb_chat: i32,
    boat_ty: i32,
    zombie_ty: i32,
    chat: String,
    boat_id: Option<i32>,
    zombie_id: Option<i32>,
    passengers: Vec<(i32, Vec<i32>)>,
    removed: Vec<i32>,
    zombie_spawns: u32,
    boat_spawns: u32,
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
                    entity_id,
                    kind,
                    pos,
                } => {
                    // Берём только сущности у тестовой площадки: при входе
                    // могут прийти сохранённые лодки/зомби из прошлых прогонов.
                    let near = (pos[0] - 3500.5).abs() < 4.0 && (pos[2] - 3500.5).abs() < 4.0;
                    if !near {
                        continue;
                    }
                    if kind == self.boat_ty {
                        self.boat_id.get_or_insert(entity_id);
                        self.boat_spawns += 1;
                    } else if kind == self.zombie_ty {
                        self.zombie_id.get_or_insert(entity_id);
                        self.zombie_spawns += 1;
                    }
                }
                Event::EntityRemoved(ids) => self.removed.extend(ids),
                Event::EntityPassengers {
                    vehicle,
                    passengers,
                } => self.passengers.push((vehicle, passengers)),
                _ => {}
            }
        }
        self.drain();
    }

    async fn settle(&mut self) {
        // Команды будят тик сессии: без работы сервер может простаивать и
        // не пересчитывать слежение после телепорта.
        for _ in 0..4 {
            self.cmd("time query gametime").await;
        }
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
    let ty = |n: &str| {
        reg["minecraft:entity_type"]["entries"][n]["protocol_id"]
            .as_i64()
            .unwrap() as i32
    };
    let (boat_ty, zombie_ty) = (ty("minecraft:oak_boat"), ty("minecraft:zombie"));
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
        boat_ty,
        zombie_ty,
        chat: String::new(),
        boat_id: None,
        zombie_id: None,
        passengers: Vec::new(),
        removed: Vec::new(),
        zombie_spawns: 0,
        boat_spawns: 0,
    };
    p.pump(1200).await;

    let (bx, bz) = (3500, 3500);
    let by = 64;
    for line in [
        "difficulty easy".to_string(),
        "time midnight".to_string(),
        "gamemode survival".to_string(),
        "kill @e[type=minecraft:oak_boat]".to_string(),
        "kill @e[type=minecraft:zombie]".to_string(),
        format!(
            "fill {} {} {} {} {} {} minecraft:air",
            bx - 6,
            by,
            bz - 6,
            bx + 6,
            by + 6,
            bz + 6
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            bx - 6,
            by - 1,
            bz - 6,
            bx + 6,
            by - 1,
            bz + 6
        ),
        format!("tp boater {} {} {}", bx as f64 + 0.5, by, bz as f64 + 0.5),
        // Ещё раз после подгрузки площадки: старые сущности могли прийти с диска.
        "kill @e[type=minecraft:oak_boat]".to_string(),
        "kill @e[type=minecraft:zombie]".to_string(),
    ] {
        p.cmd(&line).await;
    }
    p.boat_id = None;
    p.zombie_id = None;
    p.boat_spawns = 0;
    p.zombie_spawns = 0;
    p.passengers.clear();
    p.removed.clear();
    p.pump(400).await;

    // Лодка + зомби рядом: он садится (NoAI — чтобы не убежал).
    p.cmd(&format!(
        "summon minecraft:oak_boat {} {} {}",
        bx as f64 + 0.5,
        by,
        bz as f64 + 0.5
    ))
    .await;
    p.cmd(&format!(
        "summon minecraft:zombie {} {} {} {{NoAI:1b}}",
        bx as f64 + 1.2,
        by,
        bz as f64 + 0.5
    ))
    .await;
    p.pump(1200).await;
    let boat = p.boat_id.expect("лодка не появилась");
    let zombie = p.zombie_id.expect("зомби не появился");
    let at = p
        .pos("@e[type=minecraft:oak_boat,limit=1,sort=nearest] Pos")
        .await;
    let z = p
        .pos("@e[type=minecraft:zombie,limit=1,sort=nearest] Pos")
        .await;
    assert!(
        (z[1] - (at[1] + 0.1875)).abs() < 0.06,
        "зомби не сел в лодку: зомби {z:?}, лодка {at:?}"
    );

    // 140 блоков (по 3D с высотой ~152): лодка (10 чанков) видна,
    // зомби (8 чанков) — нет.
    p.removed.clear();
    p.cmd(&format!(
        "tp boater {} 4 {}",
        bx as f64 + 140.5,
        bz as f64 + 0.5
    ))
    .await;
    p.settle().await;
    p.pump(1500).await;
    assert!(
        !p.removed.contains(&boat),
        "лодка ушла из обзора на 140 блоках — нужен view-distance > 8"
    );

    // 100 блоков: зомби входит в обзор, лодка остаётся видимой.
    let before_pairs = p.passengers.len();
    let boat_spawns = p.boat_spawns;
    let zombie_spawns = p.zombie_spawns;
    p.cmd(&format!(
        "tp boater {} 4 {}",
        bx as f64 + 100.5,
        bz as f64 + 0.5
    ))
    .await;
    p.settle().await;
    p.pump(2000).await;
    let paired = p.passengers[before_pairs..]
        .iter()
        .any(|(v, list)| *v == boat && list.contains(&zombie));
    assert!(
        p.zombie_spawns > zombie_spawns,
        "зомби не вошёл в обзор отдельно от лодки"
    );
    assert!(
        paired,
        "пассажир не пришёл в обзор отдельно: boat={boat} zombie={zombie}, пары={:?}",
        p.passengers
    );
    println!(
        "pairing re-sent for zombie {zombie} on boat {boat} (boat spawns {boat_spawns}→{})",
        p.boat_spawns
    );

    p.cmd("kill @e[type=minecraft:zombie]").await;
    p.cmd("kill @e[type=minecraft:oak_boat]").await;
    p.cmd(&format!(
        "tp boater {} 4 {}",
        bx as f64 + 0.5,
        bz as f64 + 0.5
    ))
    .await;
    p.pump(400).await;
    println!("boat_pairing: ok");
}
