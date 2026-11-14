//! Визер: полёт, черепа (`minecraft:wither_skull`), босс-бар и взрыв при
//! смерти (`explode_mob` силой 7).
//!
//! `wither_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

const X: i32 = 2448;
const Z: i32 = 8;
const Y: i32 = 100;

struct Probe {
    c: Client,
    sb_command: i32,
    cb_boss: i32,
    cb_add: i32,
    cb_chat: i32,
    skull_type: i32,
    bar_added: bool,
    bar_removed: bool,
    bar_color: i32,
    bar_progress: f32,
    bar_name: String,
    skull_seen: bool,
    chat: String,
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

fn position_of(chat: &str) -> Option<[f64; 3]> {
    let i = chat.rfind("Pos:[")? + "Pos:[".len();
    let rest = &chat[i..];
    let end = rest.find(']')?;
    let mut it = rest[..end].split(',');
    let x = it.next()?.trim_end_matches('d').parse().ok()?;
    let y = it.next()?.trim_end_matches('d').parse().ok()?;
    let z = it.next()?.trim_end_matches('d').parse().ok()?;
    Some([x, y, z])
}

impl Probe {
    fn packet(&self, id: i32) -> Writer {
        let mut w = Writer::new();
        w.var_int(id);
        w
    }

    fn drain(&mut self) {
        for (id, body) in std::mem::take(&mut self.c.captured_by) {
            if id == self.cb_boss {
                let mut r = Reader::new(&body);
                if r.bytes(16).is_err() {
                    continue;
                }
                let Ok(action) = r.var_int() else { continue };
                match action {
                    0 => {
                        let Ok(name) = text::read_component(&mut r) else {
                            continue;
                        };
                        let Ok(progress) = r.f32() else { continue };
                        let Ok(color) = r.var_int() else { continue };
                        let Ok(_overlay) = r.var_int() else { continue };
                        self.bar_added = true;
                        self.bar_color = color;
                        self.bar_progress = progress.max(self.bar_progress);
                        self.bar_name = name
                            .get("translate")
                            .and_then(serde_json::Value::as_str)
                            .or_else(|| name.get("text").and_then(serde_json::Value::as_str))
                            .map(str::to_owned)
                            .unwrap_or_else(|| flatten(&name));
                    }
                    1 => self.bar_removed = true,
                    2 => {
                        if let Ok(progress) = r.f32() {
                            self.bar_progress = progress.max(self.bar_progress);
                        }
                    }
                    _ => {}
                }
            } else if id == self.cb_add {
                let mut r = Reader::new(&body);
                let Ok(_entity) = r.var_int() else { continue };
                if r.bytes(16).is_err() {
                    continue;
                }
                let Ok(ty) = r.var_int() else { continue };
                if ty == self.skull_type {
                    self.skull_seen = true;
                }
            } else if id == self.cb_chat {
                let mut r = Reader::new(&body);
                if let Ok(component) = text::read_component(&mut r) {
                    self.chat.push_str(&flatten(&component));
                    self.chat.push('\n');
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
            let _ = ev;
        }
        self.drain();
    }

    async fn cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(180).await;
    }

    async fn wither_pos(&mut self) -> [f64; 3] {
        self.chat.clear();
        self.cmd("data get entity @e[type=minecraft:wither,limit=1]")
            .await;
        position_of(&self.chat).unwrap_or_else(|| panic!("нет Pos в ответе: {}", self.chat))
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let reg: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::path::Path::new(&a[2]).with_file_name("registries.json"))
            .unwrap(),
    )
    .unwrap();
    let skull_type =
        reg["minecraft:entity_type"]["entries"]["minecraft:wither_skull"]["protocol_id"]
            .as_i64()
            .unwrap() as i32;
    let ids = Ids::load(&table).unwrap();
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n| table.id(State::Play, Direction::Clientbound, n).unwrap();
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    let cb_boss = cb("boss_event");
    let cb_add = cb("add_entity");
    let cb_chat = cb("system_chat");
    c.capture_also = vec![cb_boss, cb_add, cb_chat];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        cb_boss,
        cb_add,
        cb_chat,
        skull_type,
        bar_added: false,
        bar_removed: false,
        bar_color: -1,
        bar_progress: 0.0,
        bar_name: String::new(),
        skull_seen: false,
        chat: String::new(),
    };
    p.pump(2000).await;

    for line in [
        "difficulty easy".to_string(),
        "gamemode creative".to_string(),
        "effect clear @s".to_string(),
        "kill @e[type=!minecraft:player,distance=..96]".to_string(),
        format!(
            "execute in minecraft:overworld run tp @s {}.5 {Y} {}.5 180 20",
            X + 12,
            Z
        ),
        format!(
            "fill {} 99 {} {} 99 {} minecraft:stone",
            X - 2,
            Z - 6,
            X + 14,
            Z + 6
        ),
        format!(
            "fill {} {Y} {} {} 112 {} minecraft:air",
            X - 2,
            Z - 6,
            X + 14,
            Z + 6
        ),
        format!("summon minecraft:wither {X}.5 {Y} {Z}.5 {{PersistenceRequired:1b}}"),
    ] {
        p.cmd(&line).await;
    }

    for _ in 0..40 {
        if p.bar_added {
            break;
        }
        p.pump(150).await;
    }
    assert!(p.bar_added, "босс-бар визера не появился");
    assert_eq!(p.bar_color, 5, "цвет бара не purple");
    assert!(
        p.bar_progress > 0.99,
        "начальный прогресс бара: {}",
        p.bar_progress
    );
    assert!(
        p.bar_name.to_lowercase().contains("wither"),
        "в имени бара нет wither: {:?}",
        p.bar_name
    );
    println!("wither boss bar ok");

    // Полёт: визер поднимается над целью.
    p.pump(2500).await;
    let pos = p.wither_pos().await;
    assert!(pos[1] > f64::from(Y) + 1.0, "визер не взлетел: {pos:?}");
    println!("wither flight ok (y={})", pos[1]);

    // Череп: сущность `minecraft:wither_skull` в add_entity.
    for _ in 0..40 {
        if p.skull_seen {
            break;
        }
        p.pump(150).await;
    }
    assert!(p.skull_seen, "визер не выпустил череп");
    println!("wither skull ok");

    // Взрыв при смерти: ставим камень в позицию визера и убиваем его.
    let pos = p.wither_pos().await;
    let block = [
        pos[0].floor() as i32,
        pos[1].floor() as i32,
        pos[2].floor() as i32,
    ];
    p.cmd(&format!(
        "setblock {} {} {} minecraft:stone",
        block[0], block[1], block[2]
    ))
    .await;
    p.cmd("kill @e[type=minecraft:wither]").await;
    p.pump(1200).await;
    p.chat.clear();
    p.cmd(&format!(
        "execute if block {} {} {} minecraft:air run say WITHER_BOOM",
        block[0], block[1], block[2]
    ))
    .await;
    assert!(
        p.chat.contains("WITHER_BOOM"),
        "взрыв не разрушил блок {block:?}: {:?}",
        p.chat
    );
    assert!(p.bar_removed, "босс-бар не снят после смерти");
    println!("wither death explosion ok");
    println!("wither_probe: ok");
}
