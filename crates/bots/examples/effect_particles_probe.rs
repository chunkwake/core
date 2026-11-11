//! Частицы эффектов: моб с `active_effects` получает
//! `DATA_EFFECT_PARTICLES`/`DATA_EFFECT_AMBIENCE_ID` в `set_entity_data`.
//!
//! `effect_particles_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

struct Probe {
    c: Client,
    sb_command: i32,
    cb_chat: i32,
    cb_data: i32,
    chat: String,
    /// `(entity_id, цвета частиц, ambient)` из `set_entity_data`.
    effects: Vec<(i32, Vec<i32>, bool)>,
}

impl Probe {
    fn packet(&self, id: i32) -> Writer {
        let mut w = Writer::new();
        w.var_int(id);
        w
    }

    async fn pump(&mut self, ms: u64) {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
        while let Ok(Ok(_)) = tokio::time::timeout_at(deadline, self.c.next_event()).await {
            self.drain();
        }
    }

    fn drain(&mut self) {
        for (id, body) in std::mem::take(&mut self.c.captured_by) {
            if id == self.cb_data {
                if let Some(entry) = parse_effect_meta(&body) {
                    self.effects.push(entry);
                }
                continue;
            }
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

    async fn cmd(&mut self, line: &str) {
        self.chat.clear();
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(300).await;
    }
}

/// Разбирает `set_entity_data`, если в нём есть `DATA_EFFECT_PARTICLES` (10).
fn parse_effect_meta(body: &[u8]) -> Option<(i32, Vec<i32>, bool)> {
    let mut r = Reader::new(body);
    let entity = r.var_int().ok()?;
    let mut colors = None;
    let mut ambient = false;
    loop {
        let index = r.u8().ok()?;
        if index == 0xff {
            break;
        }
        let kind = r.var_int().ok()?;
        match kind {
            17 => {
                let count = r.var_int().ok()?;
                let mut list = Vec::with_capacity(count.max(0) as usize);
                for _ in 0..count {
                    let _particle = r.var_int().ok()?;
                    list.push(i32::from_be_bytes(r.bytes(4).ok()?.try_into().ok()?));
                }
                if index == 10 {
                    colors = Some(list);
                }
            }
            8 => {
                let value = r.bool().ok()?;
                if index == 11 {
                    ambient = value;
                }
            }
            _ => return None,
        }
    }
    Some((entity, colors?, ambient))
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

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let sb = |name| table.id(State::Play, Direction::Serverbound, name).unwrap();
    let cb = |name| table.id(State::Play, Direction::Clientbound, name).unwrap();
    let ids = Ids::load(&table).unwrap();
    let mut c = Client::connect(&a[1], "framer", ids, 777, "26.3")
        .await
        .unwrap();
    let spawn = loop {
        if let Event::Position(at) = c.next_event().await.unwrap() {
            break at;
        }
    };
    let cb_chat = cb("system_chat");
    let cb_data = cb("set_entity_data");
    c.capture_also = vec![cb_chat, cb_data];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        cb_chat,
        cb_data,
        chat: String::new(),
        effects: Vec::new(),
    };
    let at = [spawn[0].floor() + 0.5, spawn[1], spawn[2].floor() + 0.5];

    p.cmd("gamemode creative").await;
    p.cmd("kill @e[type=minecraft:cow]").await;
    p.pump(1300).await;
    p.effects.clear();
    p.cmd(&format!(
        "summon minecraft:cow {} {} {} {{active_effects:[{{id:\"minecraft:jump_boost\",amplifier:2,duration:600,show_particles:1b}}],PersistenceRequired:1b}}",
        at[0], at[1], at[2]
    ))
    .await;
    p.pump(800).await;

    let (_, colors, ambient) = p
        .effects
        .iter()
        .find(|(_, colors, _)| colors.contains(&(0xFFFD_FF84u32 as i32)))
        .unwrap_or_else(|| panic!("нет DATA_EFFECT_PARTICLES с jump_boost: {:?}", p.effects));
    assert!(!ambient, "эффект не ambient, а флаг выставлен");
    assert_eq!(colors.len(), 1, "{colors:?}");

    // Невидимые (ambient) частицы: альфа 38.
    p.effects.clear();
    p.cmd("kill @e[type=minecraft:cow]").await;
    p.pump(1300).await;
    p.cmd(&format!(
        "summon minecraft:cow {} {} {} {{active_effects:[{{id:\"minecraft:speed\",amplifier:0,duration:600,ambient:1b,show_particles:1b}}],PersistenceRequired:1b}}",
        at[0], at[1], at[2]
    ))
    .await;
    p.pump(800).await;
    assert!(
        p.effects
            .iter()
            .any(|(_, colors, ambient)| *ambient && colors.contains(&(0x2633_EBFFu32 as i32))),
        "нет ambient-частиц speed: {:?}",
        p.effects
    );

    p.cmd("kill @e[type=minecraft:cow]").await;
    println!("effect_particles: ok");
}
