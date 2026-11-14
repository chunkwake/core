//! Проводник (`ConduitBlockEntity`): рамка `#minecraft:conduit_effect_block`
//! в воде даёт `minecraft:conduit_power`, полная рамка бьёт враждебных мобов.
//!
//! `conduit_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

const X: i32 = 2248;
const Z: i32 = 8;
const Y: i32 = 96;

struct Probe {
    c: Client,
    sb_command: i32,
    cb_effect: i32,
    cb_chat: i32,
    effect_id: i32,
    effect_seen: bool,
    duration: i32,
    chat: String,
}

fn flatten(v: &serde_json::Value) -> String {
    if let Some(s) = v.as_str() {
        return s.to_owned();
    }
    if let Some(n) = v.as_i64() {
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

fn health_of(chat: &str) -> Option<f32> {
    let i = chat.rfind("Health:")?;
    let rest = &chat[i + "Health:".len()..];
    let end = rest.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))?;
    rest[..end].parse().ok()
}

impl Probe {
    fn packet(&self, id: i32) -> Writer {
        let mut w = Writer::new();
        w.var_int(id);
        w
    }

    fn drain(&mut self) {
        for (id, body) in std::mem::take(&mut self.c.captured_by) {
            if id == self.cb_effect {
                let mut r = Reader::new(&body);
                let Ok(_entity) = r.var_int() else { continue };
                let Ok(effect) = r.var_int() else { continue };
                let Ok(_amplifier) = r.var_int() else {
                    continue;
                };
                let Ok(duration) = r.var_int() else { continue };
                if effect == self.effect_id {
                    self.effect_seen = true;
                    self.duration = duration.max(self.duration);
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
            // Эффекты и чат приходят захваченными пакетами - см. `drain`.
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

    async fn health(&mut self, kind: &str) -> f32 {
        self.chat.clear();
        self.cmd(&format!(
            "data get entity @e[type=minecraft:{kind},limit=1]"
        ))
        .await;
        health_of(&self.chat).unwrap_or_else(|| panic!("нет Health в ответе: {}", self.chat))
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
    let effect_id = pid("minecraft:mob_effect", "minecraft:conduit_power");
    let ids = Ids::load(&table).unwrap();
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n| table.id(State::Play, Direction::Clientbound, n).unwrap();
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    let cb_effect = cb("update_mob_effect");
    let cb_chat = cb("system_chat");
    c.capture_also = vec![cb_effect, cb_chat];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        cb_effect,
        cb_chat,
        effect_id,
        effect_seen: false,
        duration: 0,
        chat: String::new(),
    };
    p.pump(2000).await;

    let mut setup: Vec<String> = vec![
        "difficulty easy".into(),
        "gamemode survival".into(),
        "effect clear @s".into(),
        "kill @e[type=!minecraft:player,distance=..64]".into(),
        // Убрать проводник с прошлого запуска (рамка без него неактивна).
        "setblock 2240 98 8 minecraft:air".into(),
        format!(
            "execute in minecraft:overworld run tp @s {}.5 102 {}.5 180 20",
            X + 5,
            Z + 5
        ),
        // Каменная ванна, внутри вода.
        format!(
            "fill {} {Y} {} {} 101 {} minecraft:stone",
            X - 5,
            Z - 5,
            X + 5,
            Z + 5
        ),
        format!(
            "fill {} {} {} {} 100 {} minecraft:water",
            X - 4,
            Y + 1,
            Z - 4,
            X + 4,
            Z + 4
        ),
    ];
    // Три кольца 5×5: XY, XZ, YZ.
    let rings = [
        // XY (z = Z)
        format!(
            "fill {} {} {Z} {} {} {Z} minecraft:prismarine",
            X - 2,
            Y,
            X + 2,
            Y
        ),
        format!(
            "fill {} 100 {Z} {} 100 {Z} minecraft:prismarine",
            X - 2,
            X + 2
        ),
        format!(
            "fill {} {} {Z} {} 99 {Z} minecraft:prismarine",
            X - 2,
            Y + 1,
            X - 2
        ),
        format!(
            "fill {} {} {Z} {} 99 {Z} minecraft:prismarine",
            X + 2,
            Y + 1,
            X + 2
        ),
        // XZ (y = 98)
        format!(
            "fill {} {} {} {} {} {} minecraft:prismarine",
            X - 2,
            Y + 2,
            Z - 2,
            X + 2,
            Y + 2,
            Z - 2
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:prismarine",
            X - 2,
            Y + 2,
            Z + 2,
            X + 2,
            Y + 2,
            Z + 2
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:prismarine",
            X - 2,
            Y + 2,
            Z - 1,
            X - 2,
            Y + 2,
            Z + 1
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:prismarine",
            X + 2,
            Y + 2,
            Z - 1,
            X + 2,
            Y + 2,
            Z + 1
        ),
        // YZ (x = X)
        format!(
            "fill {X} {} {} {X} {} {} minecraft:prismarine",
            Y,
            Z - 2,
            Y,
            Z + 2
        ),
        format!(
            "fill {X} 100 {} {X} 100 {} minecraft:prismarine",
            Z - 2,
            Z + 2
        ),
        // У dy=±1 кольца YZ только крайние dz=±2, иначе затирается вода.
        format!("setblock {X} {} {} minecraft:prismarine", Y + 1, Z - 2),
        format!("setblock {X} {} {} minecraft:prismarine", Y + 1, Z + 2),
        format!("setblock {X} 99 {} minecraft:prismarine", Z - 2),
        format!("setblock {X} 99 {} minecraft:prismarine", Z + 2),
    ];
    setup.extend(rings);
    // Противник для проверки урона (вне воды, в 8 блоках).
    setup.push(format!(
        "summon minecraft:spider {} 102 {} {{NoAI:1b,PersistenceRequired:1b}}",
        X + 3,
        Z
    ));
    // Проводник ставим последним: `after_block_change` зарегистрирует его.
    setup.push(format!(
        "setblock {X} {} {Z} minecraft:conduit[waterlogged=true]",
        Y + 2
    ));
    for line in &setup {
        p.cmd(line).await;
    }

    // `conduit_power` приходит пакетом `update_mob_effect`.
    for _ in 0..30 {
        if p.effect_seen {
            break;
        }
        p.pump(200).await;
    }
    assert!(p.effect_seen, "эффект conduit_power не пришёл");
    assert!(p.duration >= 200, "слишком короткий эффект: {}", p.duration);
    println!("conduit_power ok (duration {})", p.duration);

    // Полная рамка (42) бьёт враждебного моба: 4 урона раз в 2 секунды.
    let before = p.health("spider").await;
    p.pump(5000).await;
    let after = p.health("spider").await;
    assert!(
        after < before,
        "проводник не нанёс урон: {before} -> {after}"
    );
    println!("conduit attack ok ({before} -> {after})");
    println!("conduit_probe: ok");
}
