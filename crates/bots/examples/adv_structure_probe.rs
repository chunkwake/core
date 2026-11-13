//! Триггер `location` по структурам (`location.structures`).
//!
//! `adv_structure_probe <addr> <packets.json> <registries.json> [case]`
//!
//! `case`: `stronghold` (по умолчанию), `fortress`, `bastion`, `end_city`,
//! `trial`. Для каждого случая ищется структура, игрок переносится в её кусок
//! и проверяется критерий достижения.
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::collections::HashMap;
use std::time::Duration;

struct Probe {
    c: Client,
    sb_command: i32,
    cb_chat: i32,
    chat: String,
    adv: HashMap<String, Vec<String>>,
}

fn flatten(v: &serde_json::Value) -> String {
    if let Some(s) = v.as_str() {
        return s.to_owned();
    }
    if let Some(n) = v.as_i64() {
        return n.to_string();
    }
    if let Some(n) = v.as_f64() {
        return n.to_string();
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

/// Первые два целых числа из текста (`The nearest ... is at [x, ~, z]`).
fn first_two_ints(s: &str) -> Option<(i32, i32)> {
    let mut nums = Vec::new();
    let mut cur = String::new();
    for ch in s.chars() {
        if ch.is_ascii_digit() || (ch == '-' && cur.is_empty()) {
            cur.push(ch);
        } else if !cur.is_empty() {
            if let Ok(v) = cur.parse::<i32>() {
                nums.push(v);
            }
            cur.clear();
            if nums.len() == 2 {
                break;
            }
        }
    }
    if nums.len() < 2
        && !cur.is_empty()
        && let Ok(v) = cur.parse::<i32>()
    {
        nums.push(v);
    }
    (nums.len() >= 2).then(|| (nums[0], nums[1]))
}

struct Case {
    structure: &'static str,
    advancement: &'static str,
    criterion: &'static str,
    /// Биом-родитель, который надо выдать до проверки.
    parent: &'static str,
    /// Измерение для `execute in` (крепость и испытания — в оверворлде).
    dimension: Option<&'static str>,
    /// Диапазон высот для перебора.
    y: (i32, i32),
}

fn case(name: &str) -> Case {
    match name {
        "fortress" => Case {
            structure: "minecraft:fortress",
            advancement: "minecraft:nether/find_fortress",
            criterion: "fortress",
            parent: "minecraft:nether/root",
            dimension: Some("minecraft:the_nether"),
            y: (0, 120),
        },
        "bastion" => Case {
            structure: "minecraft:bastion_remnant",
            advancement: "minecraft:nether/find_bastion",
            criterion: "bastion",
            parent: "minecraft:nether/root",
            dimension: Some("minecraft:the_nether"),
            y: (0, 120),
        },
        "end_city" => Case {
            structure: "minecraft:end_city",
            advancement: "minecraft:end/find_end_city",
            criterion: "in_city",
            parent: "minecraft:end/enter_end_gateway",
            dimension: Some("minecraft:the_end"),
            y: (40, 120),
        },
        "trial" => Case {
            structure: "minecraft:trial_chambers",
            advancement: "minecraft:adventure/minecraft_trials_edition",
            criterion: "minecraft_trials_edition",
            parent: "minecraft:adventure/root",
            dimension: Some("minecraft:overworld"),
            y: (-60, 20),
        },
        _ => Case {
            structure: "minecraft:stronghold",
            advancement: "minecraft:story/follow_ender_eye",
            criterion: "in_stronghold",
            parent: "minecraft:story/enter_the_nether",
            dimension: Some("minecraft:overworld"),
            y: (0, 56),
        },
    }
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
                self.chat.push('\n');
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
        self.pump(150).await;
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let c = case(a.get(4).map(String::as_str).unwrap_or("stronghold"));
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n| table.id(State::Play, Direction::Clientbound, n).unwrap();
    let mut client = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    let cb_chat = cb("system_chat");
    client.capture_also = vec![cb_chat];
    let mut p = Probe {
        c: client,
        sb_command: sb("chat_command"),
        cb_chat,
        chat: String::new(),
        adv: HashMap::new(),
    };
    p.pump(2000).await;

    for line in [
        "difficulty peaceful".to_owned(),
        "gamemode creative".to_owned(),
        "effect clear @s".to_owned(),
        format!("advancement revoke @s only {}", c.advancement),
        format!("advancement grant @s only {}", c.parent),
    ] {
        p.cmd(&line).await;
    }

    assert!(
        !p.done(c.advancement, c.criterion),
        "критерий {} уже закрыт до телепорта: {:?}",
        c.advancement,
        p.adv.get(c.advancement)
    );

    let locate = match c.dimension {
        Some(dim) => format!("execute in {dim} run locate structure {}", c.structure),
        None => format!("locate structure {}", c.structure),
    };
    p.chat.clear();
    p.cmd(&locate).await;
    p.pump(800).await;
    let reply = p.chat.clone();
    let Some((sx, sz)) = first_two_ints(&reply) else {
        panic!("locate structure не вернул координаты: {reply}");
    };
    println!("{} near {sx} {sz}", c.structure);

    // `/locate` отдаёт угол куска, поэтому сканируем смещения внутри чанка и
    // высоты, пока точка не попадёт в рамку куска.
    let offsets = [(0, 0), (7, 7), (15, 15), (15, 0), (0, 15)];
    let mut found = false;
    let (lo, hi) = c.y;
    let mut first = true;
    'scan: for y in (lo..=hi).rev().step_by(6) {
        for (dx, dz) in offsets {
            let x = sx + dx;
            let z = sz + dz;
            match c.dimension {
                Some(dim) => {
                    p.cmd(&format!("execute in {dim} run tp @s {x} {y} {z}"))
                        .await
                }
                None => p.cmd(&format!("tp @s {x} {y} {z}")).await,
            }
            // Первый раз ждём подгрузку кусков и биома под ногами.
            p.pump(if first { 1200 } else { 250 }).await;
            first = false;
            if p.done(c.advancement, c.criterion) {
                println!("inside {} at {x} {y} {z}", c.structure);
                found = true;
                break 'scan;
            }
        }
    }

    assert!(
        found,
        "location.structures не сработал для {}: {:?}",
        c.structure,
        p.adv.get(c.advancement)
    );
    println!("location ({}) ok", c.structure);
}
