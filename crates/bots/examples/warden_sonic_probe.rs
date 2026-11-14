//! Страж: тьма вокруг (260 тиков, радиус 20) и звуковой удар
//! (`sonic_boom`: 10 урона в обход брони, отбрасывание, частицы).
//!
//! `warden_sonic_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer};
use std::time::Duration;

const X: i32 = 2432;
const Z: i32 = 8;
const Y: i32 = 100;

struct Probe {
    c: Client,
    sb_command: i32,
    cb_effect: i32,
    cb_particles: i32,
    cb_sound: i32,
    cb_health: i32,
    cb_motion: i32,
    cb_damage: i32,
    darkness_id: i32,
    particle_id: i32,
    boomsound_id: i32,
    darkness: Option<(i32, i32)>,
    particles: i32,
    sound: bool,
    health: f32,
    min_health: f32,
    motion: bool,
    damage: bool,
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
            if id == self.cb_effect {
                let (Ok(_), Ok(effect)) = (r.var_int(), r.var_int()) else {
                    continue;
                };
                if effect == self.darkness_id
                    && let (Ok(amplifier), Ok(duration)) = (r.var_int(), r.var_int())
                {
                    self.darkness = Some((amplifier, duration));
                }
            } else if id == self.cb_particles {
                if r.var_int().ok() == Some(self.particle_id) {
                    self.particles += 1;
                }
            } else if id == self.cb_sound {
                if r.var_int().ok() == Some(self.boomsound_id + 1) {
                    self.sound = true;
                }
            } else if id == self.cb_health {
                if let Ok(health) = r.f32() {
                    self.health = health;
                    self.min_health = self.min_health.min(health);
                }
            } else if id == self.cb_motion {
                if r.var_int().is_err() {
                    continue;
                }
                if let Ok(v) = chunkwake_protocol::play::read_lp_vec3(&mut r)
                    && (v[0].abs() > 1e-6 || v[2].abs() > 1e-6)
                {
                    self.motion = true;
                }
            } else if id == self.cb_damage {
                self.damage = true;
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
        self.pump(200).await;
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
    let cb_effect = cb("update_mob_effect");
    let cb_particles = cb("level_particles");
    let cb_sound = cb("sound");
    let cb_health = cb("set_health");
    let cb_motion = cb("set_entity_motion");
    let cb_damage = cb("damage_event");
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    c.capture_also = vec![
        cb_effect,
        cb_particles,
        cb_sound,
        cb_health,
        cb_motion,
        cb_damage,
    ];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        cb_effect,
        cb_particles,
        cb_sound,
        cb_health,
        cb_motion,
        cb_damage,
        darkness_id: pid("minecraft:mob_effect", "minecraft:darkness"),
        particle_id: pid("minecraft:particle_type", "minecraft:sonic_boom"),
        boomsound_id: pid(
            "minecraft:sound_event",
            "minecraft:entity.warden.sonic_boom",
        ),
        darkness: None,
        particles: 0,
        sound: false,
        health: 20.0,
        min_health: 20.0,
        motion: false,
        damage: false,
    };
    p.pump(2000).await;

    for line in [
        "difficulty hard".to_string(),
        "gamemode survival".to_string(),
        "effect clear @s".to_string(),
        "kill @e[type=!minecraft:player,distance=..96]".to_string(),
        format!("execute in minecraft:overworld run tp @s {X}.5 {Y} {Z}.5 180 20"),
        format!(
            "fill {} 99 {} {} 99 {} minecraft:stone",
            X - 16,
            Z - 16,
            X + 48,
            Z + 24
        ),
        format!(
            "fill {} {Y} {} {} {} {} minecraft:air",
            X - 16,
            Z - 16,
            X + 48,
            112,
            Z + 24
        ),
        format!("tp @s {X}.5 {Y} {Z}.5 180 20"),
        "effect give @s minecraft:instant_health 1 10".to_string(),
        "item replace entity @s armor.head with minecraft:diamond_helmet".to_string(),
        "item replace entity @s armor.chest with minecraft:diamond_chestplate".to_string(),
        "item replace entity @s armor.legs with minecraft:diamond_leggings".to_string(),
        "item replace entity @s armor.feet with minecraft:diamond_boots".to_string(),
        format!(
            "summon minecraft:warden {}.5 {Y} {}.5 {{PersistenceRequired:1b}}",
            X + 10,
            Z
        ),
    ] {
        p.cmd(&line).await;
    }
    p.c.confirm_loaded().await.unwrap();
    p.health = 20.0;
    p.min_health = 20.0;

    // Тьма накладывается в первые 120 тиков, sonic boom — после 200 тиков
    // кулдауна и 34 тиков зарядки. Держим дистанцию 8 блоков: страж не
    // достаёт ближним боем, а линия видимости остаётся чистой.
    for _ in 0..160 {
        p.cmd("execute at @e[type=minecraft:warden,distance=..32,limit=1] run tp @s ~8 ~ ~")
            .await;
        p.c.confirm_loaded().await.unwrap();
        p.pump(120).await;
        if p.darkness.is_some() && p.damage {
            break;
        }
    }
    let (amplifier, duration) = p.darkness.expect("страж не наложил тьму на игрока рядом");
    assert_eq!(amplifier, 0, "усиление тьмы должно быть 0");
    assert!(
        duration >= 250,
        "длительность тьмы не 260 тиков: {duration}"
    );
    println!("warden darkness ok (duration {duration})");

    assert!(p.damage, "не пришёл damage_event от звукового удара");
    assert!(p.particles > 0, "нет частиц sonic_boom");
    assert!(p.sound, "нет звука entity.warden.sonic_boom");
    assert!(p.motion, "нет отбрасывания игрока");
    // Алмазная броня не должна уменьшать 10 урона sonic boom.
    assert!(
        (p.min_health - 10.0).abs() < 0.01,
        "урон sonic boom не 10 в обход брони: здоровье {}",
        p.min_health
    );
    println!(
        "warden sonic boom ok (health {:.1}, particles {}, sound {}, knockback {})",
        p.min_health, p.particles, p.sound, p.motion
    );
    println!("warden_sonic_probe: ok");
}
