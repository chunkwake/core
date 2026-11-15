//! Фазы дракона Края: holding-круг, посадка в центре и дыхание только
//! в `SITTING_FLAMING` (несколько облаков подряд).
//!
//! `dragon_phases_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

struct Probe {
    c: Client,
    sb_command: i32,
    cb_add: i32,
    cb_meta: i32,
    cb_chat: i32,
    cloud_type: i32,
    breath_particle: i32,
    clouds: std::collections::HashSet<i32>,
    chat: String,
    breath_clouds: i32,
}

impl Probe {
    fn packet(&self, id: i32) -> Writer {
        let mut w = Writer::new();
        w.var_int(id);
        w
    }

    fn drain(&mut self) {
        for (id, body) in std::mem::take(&mut self.c.captured_by) {
            let mut r = Reader::new(&body);
            if id == self.cb_add {
                let (Ok(entity), Ok(_uuid)) = (r.var_int(), r.bytes(16)) else {
                    continue;
                };
                if r.var_int().ok() == Some(self.cloud_type) {
                    self.clouds.insert(entity);
                }
            } else if id == self.cb_meta {
                let Ok(entity) = r.var_int() else { continue };
                if !self.clouds.contains(&entity) {
                    continue;
                }
                while let Ok(index) = r.u8() {
                    let Ok(kind) = r.var_int() else { break };
                    match kind {
                        0 => {
                            if r.u8().is_err() {
                                break;
                            }
                        }
                        1 => {
                            if r.var_int().is_err() {
                                break;
                            }
                        }
                        3 => {
                            if r.f32().is_err() {
                                break;
                            }
                        }
                        8 => {
                            if r.bool().is_err() {
                                break;
                            }
                        }
                        17 => {
                            if index == 10
                                && r.var_int().ok() == Some(self.breath_particle)
                                && self.clouds.remove(&entity)
                            {
                                self.breath_clouds += 1;
                            }
                        }
                        _ => break,
                    }
                }
            } else if id == self.cb_chat
                && let Ok(component) = text::read_component(&mut r)
            {
                flatten(&component, &mut self.chat);
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
            let _ = ev;
        }
        self.drain();
    }

    async fn cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(250).await;
    }

    async fn dragon_pos(&mut self) -> Option<[f64; 3]> {
        self.chat.clear();
        self.cmd("execute in minecraft:the_end run data get entity @e[type=minecraft:ender_dragon,limit=1]")
            .await;
        let i = self.chat.rfind("Pos:[")? + "Pos:[".len();
        let rest = &self.chat[i..];
        let end = rest.find(']')?;
        let mut it = rest[..end].split(',');
        let x = it.next()?.trim_end_matches('d').parse().ok()?;
        let y = it.next()?.trim_end_matches('d').parse().ok()?;
        let z = it.next()?.trim_end_matches('d').parse().ok()?;
        Some([x, y, z])
    }
}

fn flatten(v: &serde_json::Value, out: &mut String) {
    if let Some(s) = v.as_str() {
        out.push_str(s);
        return;
    }
    if let Some(s) = v.get("text").and_then(serde_json::Value::as_str) {
        out.push_str(s);
    }
    for key in ["with", "extra"] {
        if let Some(args) = v.get(key).and_then(serde_json::Value::as_array) {
            for arg in args {
                flatten(arg, out);
                out.push(' ');
            }
        }
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
    let pid = |reg_name: &str, name: &str| {
        reg[reg_name]["entries"][name]["protocol_id"]
            .as_i64()
            .unwrap() as i32
    };
    let ids = Ids::load(&table).unwrap();
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n| table.id(State::Play, Direction::Clientbound, n).unwrap();
    let cb_add = cb("add_entity");
    let cb_meta = cb("set_entity_data");
    let cb_chat = cb("system_chat");
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    c.capture_also = vec![cb_add, cb_meta, cb_chat];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        cb_add,
        cb_meta,
        cb_chat,
        cloud_type: pid("minecraft:entity_type", "minecraft:area_effect_cloud"),
        breath_particle: pid("minecraft:particle_type", "minecraft:dragon_breath"),
        clouds: std::collections::HashSet::new(),
        chat: String::new(),
        breath_clouds: 0,
    };
    p.pump(1500).await;

    for line in [
        "difficulty easy".to_string(),
        "gamemode creative".to_string(),
        "effect clear @s".to_string(),
        "kill @e[type=!minecraft:player,distance=..96]".to_string(),
        "execute in minecraft:the_end run fill -8 69 -8 8 69 8 minecraft:stone".to_string(),
        "execute in minecraft:the_end run fill -8 70 -8 8 120 8 minecraft:air".to_string(),
        "execute in minecraft:the_end run tp @s 0.5 70 6.5 180 0".to_string(),
        "execute in minecraft:the_end run kill @e[type=minecraft:ender_dragon]".to_string(),
    ] {
        p.cmd(&line).await;
    }
    p.c.confirm_loaded().await.unwrap();

    // Holding pattern: круг вокруг (0.5, 80, 0.5), радиус 24.
    p.cmd("execute in minecraft:the_end run summon minecraft:ender_dragon 0.5 80 0.5 {PersistenceRequired:1b}")
        .await;
    p.pump(3000).await;
    let pos = p.dragon_pos().await.expect("дракон не появился");
    let radius = (pos[0] * pos[0] + pos[2] * pos[2]).sqrt();
    assert!(
        (76.0..=84.0).contains(&pos[1]),
        "высота круга не 80: {pos:?}"
    );
    assert!(radius > 8.0, "дракон не летит по кругу: {pos:?}");
    println!("dragon holding pattern ok (y={}, r={radius:.1})", pos[1]);

    // Посадка и дыхание: облако появляется только в sitting flaming.
    let mut waited = 0;
    while p.breath_clouds == 0 && waited < 400 {
        p.pump(250).await;
        waited += 1;
    }
    assert!(p.breath_clouds > 0, "дракон не дышал после посадки");
    let pos = p.dragon_pos().await.expect("дракон исчез");
    let radius = (pos[0] * pos[0] + pos[2] * pos[2]).sqrt();
    assert!(
        pos[1] <= 74.0 && radius < 5.0,
        "дыхание не в sitting flaming: {pos:?} (r={radius:.1})"
    );
    println!(
        "dragon landing + breath ok (y={}, r={radius:.1}, clouds={})",
        pos[1], p.breath_clouds
    );

    // Следующий цикл scanning → attacking → flaming снова дышит.
    let before = p.breath_clouds;
    let mut waited = 0;
    while p.breath_clouds <= before && waited < 200 {
        p.pump(250).await;
        waited += 1;
    }
    assert!(
        p.breath_clouds > before,
        "повторного дыхания в фазах нет: {}",
        p.breath_clouds
    );
    println!("dragon flaming cycle ok (clouds={})", p.breath_clouds);
    println!("dragon_phases_probe: ok");
}
