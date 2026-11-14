//! Медный голем: метаданные `weather_state` (`DATA_WEATHER_STATE`, id 16,
//! сериализатор 38) и превращение полностью окисленного голема в статую
//! (`oxidized_copper_golem_statue`, случайная поза, 0.58 % за тик).
//!
//! `copper_golem_statue_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use serde_json::Value;
use std::collections::HashSet;
use std::time::Duration;

const X: i32 = 2400;
const Z: i32 = 8;
const Y: i32 = 100;

struct Probe {
    c: Client,
    sb_command: i32,
    cb_meta: i32,
    cb_block: i32,
    cb_chat: i32,
    statue_states: HashSet<i32>,
    weather_meta: bool,
    statue_pos: Option<[i32; 3]>,
    chat: String,
}

fn flatten(v: &Value) -> String {
    if let Some(s) = v.as_str() {
        return s.to_owned();
    }
    if let Some(s) = v.get("text").and_then(Value::as_str) {
        return s.to_owned();
    }
    let mut out = String::new();
    if let Some(args) = v.get("with").and_then(Value::as_array) {
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
            if id == self.cb_meta {
                // entity id, затем (индекс u8, сериализатор varint, значение).
                let mut r = Reader::new(&body);
                if r.var_int().is_err() {
                    continue;
                }
                let Ok(index) = r.u8() else { continue };
                if index != 16 {
                    continue;
                }
                let Ok(serializer) = r.var_int() else {
                    continue;
                };
                if serializer == 38 && matches!(r.var_int(), Ok(1)) {
                    self.weather_meta = true;
                }
            } else if id == self.cb_chat {
                let mut r = Reader::new(&body);
                if let Ok(component) = text::read_component(&mut r) {
                    self.chat.push_str(&flatten(&component));
                    self.chat.push('\n');
                }
            } else if id == self.cb_block {
                let mut r = Reader::new(&body);
                let Ok(packed) = r.i64() else { continue };
                let Ok(state) = r.var_int() else { continue };
                if self.statue_states.contains(&state) && self.statue_pos.is_none() {
                    self.statue_pos = Some(chunkwake_protocol::play::unpack_block_pos(packed));
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
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let reports = std::path::Path::new(&a[2]).with_file_name("blocks.json");
    let blocks: Value = serde_json::from_str(&std::fs::read_to_string(&reports).unwrap()).unwrap();
    let statue_states: HashSet<i32> = blocks["minecraft:oxidized_copper_golem_statue"]["states"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_i64().unwrap() as i32)
        .collect();
    let ids = Ids::load(&table).unwrap();
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n| table.id(State::Play, Direction::Clientbound, n).unwrap();
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    let cb_meta = cb("set_entity_data");
    let cb_block = cb("block_update");
    let cb_chat = cb("system_chat");
    c.capture_also = vec![cb_meta, cb_block, cb_chat];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        cb_meta,
        cb_block,
        cb_chat,
        statue_states,
        weather_meta: false,
        statue_pos: None,
        chat: String::new(),
    };
    p.pump(2000).await;

    for line in [
        "difficulty peaceful".to_string(),
        "gamemode creative".to_string(),
        "effect clear @s".to_string(),
        "kill @e[type=!minecraft:player,distance=..96]".to_string(),
        format!(
            "execute in minecraft:overworld run tp @s {}.5 {Y} {}.5 180 20",
            X + 4,
            Z + 4
        ),
        format!(
            "fill {} 99 {} {} 99 {} minecraft:stone",
            X - 2,
            Z - 2,
            X + 12,
            Z + 8
        ),
        format!(
            "fill {} {Y} {} {} 104 {} minecraft:air",
            X - 2,
            Z - 2,
            X + 12,
            Z + 8
        ),
    ] {
        p.cmd(&line).await;
    }

    // 1. Метаданные окисления: голем окисляется до exposed на следующем тике.
    p.cmd(&format!(
        "summon minecraft:copper_golem {X}.5 {Y} {Z}.5 {{NoAI:1b,PersistenceRequired:1b}}"
    ))
    .await;
    p.cmd("data merge entity @e[type=minecraft:copper_golem,limit=1] {next_weather_age:1}")
        .await;
    for _ in 0..10 {
        if p.weather_meta {
            break;
        }
        p.pump(150).await;
    }
    assert!(
        p.weather_meta,
        "не пришёл set_entity_data с weather_state (16/38/1)"
    );
    println!("copper golem weather metadata ok");

    // 2. Полностью окисленные големы превращаются в статуи.
    p.cmd("kill @e[type=minecraft:copper_golem,distance=..96]")
        .await;
    p.statue_pos = None;
    for dx in 0..8 {
        for dz in 0..5 {
            p.cmd(&format!(
                "summon minecraft:copper_golem {}.5 {Y} {}.5 \
                 {{NoAI:1b,Silent:1b,PersistenceRequired:1b,\
                 weather_state:\"oxidized\",next_weather_age:1}}",
                X + dx,
                Z + dz
            ))
            .await;
        }
    }
    let mut waited = 0;
    while p.statue_pos.is_none() && waited < 20_000 {
        p.pump(300).await;
        waited += 300;
    }
    let at = p.statue_pos.expect("статуя не появилась за 20 секунд");
    // Подтверждение серверной командой: на месте голема реально стоит статуя.
    p.chat.clear();
    p.cmd(&format!(
        "execute if block {} {} {} minecraft:oxidized_copper_golem_statue run say STATUE_OK",
        at[0], at[1], at[2]
    ))
    .await;
    assert!(
        p.chat.contains("STATUE_OK"),
        "на {at:?} нет oxidized_copper_golem_statue: {:?}",
        p.chat
    );
    println!("copper golem statue ok at {at:?}");
    println!("copper_golem_statue_probe: ok");
}
