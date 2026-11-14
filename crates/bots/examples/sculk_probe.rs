//! `gameEvent`: вибрация доходит до скалк-сенсора за `distance` тиков, шерсть
//! (`#minecraft:occludes_vibration_signals`) её затеняет, а к сенсору летит
//! частица `minecraft:vibration`.
//!
//! `sculk_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

const X: i32 = 2312;
const Z: i32 = 8;
const Y: i32 = 100;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_select: i32,
    sb_use_on: i32,
    cb_chat: i32,
    cb_particles: i32,
    vibration: i32,
    seq: i32,
    chat: String,
    /// (позиция сенсора, тики) из частиц `vibration`.
    particles: Vec<([i32; 3], i32)>,
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

impl Probe {
    fn packet(&self, id: i32) -> Writer {
        let mut w = Writer::new();
        w.var_int(id);
        w
    }

    fn drain(&mut self) {
        for (id, body) in std::mem::take(&mut self.c.captured_by) {
            if id == self.cb_chat {
                let mut r = Reader::new(&body);
                if let Ok(component) = text::read_component(&mut r) {
                    self.chat.push_str(&flatten(&component));
                    self.chat.push('\n');
                }
            } else if id == self.cb_particles {
                let mut r = Reader::new(&body);
                let Ok(particle) = r.var_int() else { continue };
                if particle != self.vibration {
                    continue;
                }
                let Ok(kind) = r.var_int() else { continue };
                if kind != 0 {
                    continue;
                }
                let Ok(packed) = r.i64() else { continue };
                let Ok(ticks) = r.var_int() else { continue };
                self.particles
                    .push((chunkwake_protocol::play::unpack_block_pos(packed), ticks));
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
        self.cmd_for(line, 180).await;
    }

    async fn cmd_for(&mut self, line: &str, ms: u64) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(ms).await;
    }

    async fn select(&mut self, slot: u16) {
        let mut w = self.packet(self.sb_select);
        w.u16(slot);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(120).await;
    }

    /// Поставить блок над `pos` (грань `face`), не дожидаясь доставки.
    async fn use_on(&mut self, pos: [i32; 3], face: i32, ms: u64) {
        self.seq += 1;
        let mut w = self.packet(self.sb_use_on);
        w.var_int(0)
            .i64(chunkwake_protocol::play::pack_block_pos(pos))
            .var_int(face)
            .f32(0.5)
            .f32(0.5)
            .f32(0.5)
            .u8(0)
            .u8(0)
            .var_int(self.seq);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(ms).await;
    }

    /// Фаза сенсора через `/execute if block`: `say SENSOR_ACTIVE` при совпадении.
    async fn phase(&mut self, phase: &str) -> bool {
        self.chat.clear();
        self.cmd(&format!(
            "execute if block {X} {Y} {Z} minecraft:sculk_sensor[sculk_sensor_phase={phase}] run say SENSOR_ACTIVE"
        ))
        .await;
        self.chat.contains("SENSOR_ACTIVE")
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
    let vibration = pid("minecraft:particle_type", "minecraft:vibration");
    let ids = Ids::load(&table).unwrap();
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n| table.id(State::Play, Direction::Clientbound, n).unwrap();
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    let cb_chat = cb("system_chat");
    let cb_particles = cb("level_particles");
    c.capture_also = vec![cb_chat, cb_particles];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_select: sb("set_carried_item"),
        sb_use_on: sb("use_item_on"),
        cb_chat,
        cb_particles,
        vibration,
        seq: 0,
        chat: String::new(),
        particles: Vec::new(),
    };
    p.pump(2000).await;

    let setup = [
        "difficulty peaceful".to_string(),
        "gamemode creative".to_string(),
        "effect clear @s".to_string(),
        "kill @e[type=!minecraft:player,distance=..64]".to_string(),
        format!(
            "execute in minecraft:overworld run tp @s {}.5 {Y} {}.5 180 20",
            X + 6,
            Z + 1
        ),
        format!(
            "fill {} 99 {} {} 99 {} minecraft:stone",
            X - 2,
            Z - 2,
            X + 10,
            Z + 2
        ),
        format!(
            "fill {} {Y} {} {} 102 {} minecraft:air",
            X - 2,
            Z - 2,
            X + 10,
            Z + 2
        ),
        format!("setblock {X} {Y} {Z} minecraft:sculk_sensor[sculk_sensor_phase=inactive]"),
        "item replace entity @s hotbar.0 with minecraft:stone 16".to_string(),
    ];
    for line in &setup {
        p.cmd(line).await;
    }
    p.select(0).await;
    assert!(p.phase("inactive").await, "сенсор не в фазе inactive");

    // 1. Задержка: блок ставится в 8 блоках, вибрация идёт 8 тиков.
    p.use_on([X + 8, 99, Z], 1, 60).await;
    let early = p.phase("inactive").await;
    assert!(early, "сенсор сработал раньше задержки");
    println!("sculk delay ok (сразу после установки ещё inactive)");
    p.pump(900).await;
    assert!(p.phase("active").await, "сенсор не сработал после задержки");
    println!("sculk activation ok");
    let particle = p
        .particles
        .iter()
        .find(|(pos, ticks)| *pos == [X, Y, Z] && *ticks == 8);
    assert!(
        particle.is_some(),
        "нет частицы vibration к сенсору: {:?}",
        p.particles
    );
    println!("sculk vibration particle ok");

    // 2. Шерсть между источником и сенсором затеняет вибрацию.
    p.pump(2500).await;
    assert!(p.phase("inactive").await, "сенсор не вернулся в inactive");
    p.cmd(&format!("setblock {} {Y} {Z} minecraft:white_wool", X + 4))
        .await;
    let before = p.particles.len();
    p.use_on([X + 7, 99, Z], 1, 60).await;
    p.pump(1200).await;
    let active = p.phase("active").await;
    assert!(!active, "шерсть не затеняет вибрацию");
    assert_eq!(
        p.particles.len(),
        before,
        "частица появилась сквозь шерсть: {:?}",
        &p.particles[before..]
    );
    println!("sculk occlusion ok");

    // 3. Убираем шерсть — вибрация снова проходит.
    p.cmd(&format!("setblock {} {Y} {Z} minecraft:air", X + 4))
        .await;
    p.pump(500).await;
    let before = p.particles.len();
    p.use_on([X + 7, 99, Z + 1], 1, 60).await;
    p.pump(1200).await;
    assert!(p.phase("active").await, "без шерсти вибрация не дошла");
    assert!(
        p.particles.len() > before,
        "без шерсти нет частицы vibration"
    );
    println!("sculk unoccluded ok");
    println!("sculk_probe: ok");
}
