//! Дракон Края: полёт по кругу вокруг портала, контактный урон, босс-бар
//! `entity.minecraft.ender_dragon` и анимация возрождения (100 тиков).
//!
//! `dragon_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

const Y: i32 = 100;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_select: i32,
    sb_use_on: i32,
    cb_boss: i32,
    cb_chat: i32,
    seq: i32,
    bar_adds: i32,
    bar_removed: bool,
    bar_color: i32,
    bar_progress: f32,
    bar_name: String,
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

fn number_after(chat: &str, key: &str) -> Option<f64> {
    let i = chat.rfind(key)? + key.len();
    let rest = &chat[i..];
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == 'd' || c == 'f'))
        .unwrap_or(rest.len());
    rest[..end].trim_end_matches(['d', 'f']).parse().ok()
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
                        self.bar_adds += 1;
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

    async fn select(&mut self, slot: u16) {
        let mut w = self.packet(self.sb_select);
        w.u16(slot);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(120).await;
    }

    async fn use_on(&mut self, pos: [i32; 3]) {
        self.seq += 1;
        let mut w = self.packet(self.sb_use_on);
        w.var_int(0)
            .i64(chunkwake_protocol::play::pack_block_pos(pos))
            .var_int(1)
            .f32(0.5)
            .f32(0.5)
            .f32(0.5)
            .u8(0)
            .u8(0)
            .var_int(self.seq);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(300).await;
    }

    async fn dragon_pos(&mut self) -> Option<[f64; 3]> {
        self.chat.clear();
        self.cmd("data get entity @e[type=minecraft:ender_dragon,limit=1]")
            .await;
        position_of(&self.chat)
    }

    async fn dragon_exists(&mut self) -> bool {
        self.chat.clear();
        self.cmd("execute if entity @e[type=minecraft:ender_dragon] run say DRAGON_ALIVE")
            .await;
        self.chat.contains("DRAGON_ALIVE")
    }

    async fn health(&mut self) -> f64 {
        self.chat.clear();
        self.cmd("data get entity @s").await;
        number_after(&self.chat, "Health:").unwrap_or_else(|| panic!("нет Health: {}", self.chat))
    }
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
    let cb_boss = cb("boss_event");
    let cb_chat = cb("system_chat");
    c.capture_also = vec![cb_boss, cb_chat];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_select: sb("set_carried_item"),
        sb_use_on: sb("use_item_on"),
        cb_boss,
        cb_chat,
        seq: 0,
        bar_adds: 0,
        bar_removed: false,
        bar_color: -1,
        bar_progress: 0.0,
        bar_name: String::new(),
        chat: String::new(),
    };
    p.pump(2000).await;

    for line in [
        "difficulty easy".to_string(),
        "gamemode survival".to_string(),
        "effect clear @s".to_string(),
        "kill @e[type=!minecraft:player,distance=..96]".to_string(),
        "execute in minecraft:the_end run tp @s 0.5 100 8.5 180 0".to_string(),
        "execute in minecraft:the_end run fill -6 99 -6 6 99 6 minecraft:stone".to_string(),
        "execute in minecraft:the_end run fill -6 100 -6 6 120 6 minecraft:air".to_string(),
    ] {
        p.cmd(&line).await;
    }

    // Дракон появляется рядом с игроком: контактный урон 6.
    p.cmd(&format!(
        "execute in minecraft:the_end run summon minecraft:ender_dragon 0.5 {Y} 5.5 {{PersistenceRequired:1b}}"
    ))
    .await;
    p.pump(600).await;
    let health = p.health().await;
    assert!(health < 20.0, "дракон не нанёс контактный урон: {health}");
    println!("dragon contact damage ok ({health})");
    // Дальше дракон не должен убить бота до проверки возрождения.
    p.cmd("gamemode creative").await;
    p.cmd("effect clear @s").await;

    for _ in 0..40 {
        if p.bar_adds > 0 {
            break;
        }
        p.pump(150).await;
    }
    assert!(p.bar_adds > 0, "босс-бар дракона не появился");
    assert_eq!(p.bar_color, 0, "цвет бара не pink");
    assert!(p.bar_progress > 0.99, "прогресс бара: {}", p.bar_progress);
    assert!(
        p.bar_name.contains("ender_dragon"),
        "имя бара: {:?}",
        p.bar_name
    );
    println!("dragon boss bar ok");

    // Полёт по кругу: y стремится к 80, радиус от центра растёт.
    p.pump(6500).await;
    let pos = p.dragon_pos().await.expect("дракон исчез");
    assert!(pos[1] < 95.0, "дракон не снизился: {pos:?}");
    let radius = (pos[0] * pos[0] + pos[2] * pos[2]).sqrt();
    assert!(radius > 5.0, "дракон не улетел по кругу: {pos:?}");
    println!("dragon flight ok (y={}, r={radius:.1})", pos[1]);

    // Смерть и анимация возрождения: 4 кристалла, пауза, новый дракон.
    p.cmd("execute in minecraft:the_end run kill @e[type=minecraft:ender_dragon]")
        .await;
    p.pump(1800).await;
    assert!(!p.dragon_exists().await, "дракон не умер");
    assert!(p.bar_removed, "босс-бар не снят после смерти");
    p.cmd("execute in minecraft:the_end run item replace entity @s hotbar.0 with minecraft:end_crystal 4")
        .await;
    p.select(0).await;
    for (x, z) in [(3, 0), (-3, 0), (0, 3), (0, -3)] {
        p.cmd(&format!(
            "execute in minecraft:the_end run setblock {x} {Y} {z} minecraft:obsidian"
        ))
        .await;
    }
    p.cmd(&format!(
        "execute in minecraft:the_end run setblock 0 {Y} 0 minecraft:obsidian"
    ))
    .await;
    p.cmd(&format!(
        "execute in minecraft:the_end run tp @s 0.5 {} 0.5 0 0",
        Y + 1
    ))
    .await;
    // После телепорта сервер заново включает load-gate: подтверждаем загрузку.
    p.pump(600).await;
    p.c.confirm_loaded().await.unwrap();
    p.pump(400).await;
    let before = p.bar_adds;
    for (x, z) in [(3, 0), (-3, 0), (0, 3), (0, -3)] {
        // Сервер может заново включить load-gate после телепорта: подтверждаем.
        p.c.confirm_loaded().await.unwrap();
        p.pump(200).await;
        p.use_on([x, Y, z]).await;
    }

    p.pump(800).await;
    assert!(
        !p.dragon_exists().await,
        "дракон появился без анимации возрождения"
    );
    p.pump(5500).await;
    assert!(p.dragon_exists().await, "дракон не возродился");
    assert!(
        p.bar_adds > before,
        "босс-бар не вернулся после возрождения"
    );
    println!("dragon respawn animation ok");
    println!("dragon_probe: ok");
}
