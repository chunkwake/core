//! Дыхание дракона как `AreaEffectCloud` и флакон `dragon_breath`:
//! облако с частицей `dragon_breath` наносит `instant_damage`, а стеклянный
//! флакон забирает дыхание (радиус уменьшается на 0.5).
//!
//! `dragon_breath_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer};
use std::time::Duration;

const X: i32 = 2560;
const Z: i32 = 8;
const Y: i32 = 100;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_use_item: i32,
    sb_select: i32,
    cb_add: i32,
    cb_meta: i32,
    cb_health: i32,
    cb_sound: i32,
    cb_content: i32,
    cloud_type: i32,
    breath_item: i32,
    bottle_sound: i32,
    cloud: Option<i32>,
    radius: f32,
    particle: Option<i32>,
    health: f32,
    min_health: f32,
    health_at_cloud: f32,
    bottle_sound_seen: bool,
    got_breath: bool,
    radius_after_use: Option<f32>,
    seq: i32,
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
                    self.cloud = Some(entity);
                    self.radius = 0.0;
                    self.particle = None;
                    self.health_at_cloud = self.health;
                }
            } else if id == self.cb_meta {
                let Ok(entity) = r.var_int() else { continue };
                if Some(entity) != self.cloud {
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
                            let Ok(v) = r.f32() else { break };
                            if index == 8 {
                                if self.radius > 0.0 && v < self.radius - 0.4 {
                                    self.radius_after_use = Some(v);
                                }
                                self.radius = v;
                            }
                        }
                        8 => {
                            if r.bool().is_err() {
                                break;
                            }
                        }
                        17 => {
                            let Ok(p) = r.var_int() else { break };
                            if index == 10 {
                                self.particle = Some(p);
                            }
                        }
                        _ => break,
                    }
                }
            } else if id == self.cb_health {
                if let Ok(health) = r.f32() {
                    self.health = health;
                    self.min_health = self.min_health.min(health);
                }
            } else if id == self.cb_sound {
                if r.var_int().ok() == Some(self.bottle_sound + 1) {
                    self.bottle_sound_seen = true;
                }
            } else if id == self.cb_content {
                let (Ok(_window), Ok(_state), Ok(count)) = (r.var_int(), r.var_int(), r.var_int())
                else {
                    continue;
                };
                for _ in 0..count {
                    let Ok(slot) = chunkwake_protocol::inventory::read_stack(&mut r) else {
                        break;
                    };
                    if slot.is_some_and(|s| s.item == self.breath_item) {
                        self.got_breath = true;
                    }
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
        self.pump(200).await;
    }

    async fn select(&mut self, slot: u16) {
        let mut w = self.packet(self.sb_select);
        w.u16(slot);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(100).await;
    }

    async fn use_item(&mut self) {
        self.seq += 1;
        let mut w = self.packet(self.sb_use_item);
        w.var_int(0).var_int(self.seq).f32(0.0).f32(0.0);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(400).await;
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
    let cb_health = cb("set_health");
    let cb_sound = cb("sound");
    let cb_content = cb("container_set_content");
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    c.capture_also = vec![cb_add, cb_meta, cb_health, cb_sound, cb_content];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_use_item: sb("use_item"),
        sb_select: sb("set_carried_item"),
        cb_add,
        cb_meta,
        cb_health,
        cb_sound,
        cb_content,
        cloud_type: pid("minecraft:entity_type", "minecraft:area_effect_cloud"),
        breath_item: pid("minecraft:item", "minecraft:dragon_breath"),
        bottle_sound: pid(
            "minecraft:sound_event",
            "minecraft:item.bottle.fill_dragonbreath",
        ),
        cloud: None,
        radius: 0.0,
        particle: None,
        health: 20.0,
        min_health: 20.0,
        health_at_cloud: 20.0,
        bottle_sound_seen: false,
        got_breath: false,
        radius_after_use: None,
        seq: 0,
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
            X + 32,
            Z + 24
        ),
        format!(
            "fill {} {Y} {} {} 130 {} minecraft:air",
            X - 16,
            Z - 16,
            X + 32,
            Z + 24
        ),
        format!("tp @s {X}.5 {Y} {Z}.5 180 20"),
        "effect give @s minecraft:instant_health 1 10".to_string(),
        "item replace entity @s hotbar.0 with minecraft:glass_bottle 1".to_string(),
        // Дракон в 20 блоках над игроком: дыхание на землю, без контакта.
        format!(
            "summon minecraft:ender_dragon {}.5 120 {}.5 {{PersistenceRequired:1b}}",
            X, Z
        ),
    ] {
        p.cmd(&line).await;
    }
    p.select(0).await;
    p.c.confirm_loaded().await.unwrap();
    p.health = 20.0;
    p.min_health = 20.0;

    // Дыхание — раз в 100 тиков по ближайшему игроку в 24 блоках.
    for _ in 0..80 {
        p.pump(250).await;
        if p.cloud.is_some() {
            break;
        }
    }
    let cloud = p.cloud.expect("дракон не создал AreaEffectCloud дыхания");
    assert_eq!(
        p.particle,
        Some(pid("minecraft:particle_type", "minecraft:dragon_breath")),
        "у облака не частица dragon_breath"
    );
    assert!(
        (p.radius - 5.0).abs() < 0.9,
        "радиус облака не 5: {}",
        p.radius
    );
    println!(
        "dragon breath cloud ok (radius {}, particle {:?})",
        p.radius, p.particle
    );

    // Один удар instant_damage: 3 урона (6 * 0.5).
    for _ in 0..20 {
        p.pump(100).await;
        if p.min_health <= p.health_at_cloud - 2.5 {
            break;
        }
    }
    assert!(
        p.min_health <= p.health_at_cloud - 2.5,
        "облако не нанесло instant_damage: {} -> {}",
        p.health_at_cloud,
        p.min_health
    );
    println!(
        "dragon breath damage ok ({:.1} -> {:.1})",
        p.health_at_cloud, p.min_health
    );

    // Отходим на 6 блоков: вне радиуса урона, но в AABB флакона (+2).
    p.cmd(&format!("tp @s {}.5 {Y} {}.5", X + 6, Z)).await;
    p.c.confirm_loaded().await.unwrap();
    p.pump(150).await;
    let before = p.radius;
    p.use_item().await;
    for _ in 0..20 {
        p.pump(100).await;
        if p.got_breath && p.radius_after_use.is_some() {
            break;
        }
    }
    assert!(p.got_breath, "флакон не превратился в dragon_breath");
    assert!(
        p.bottle_sound_seen,
        "нет звука item.bottle.fill_dragonbreath"
    );
    let after = p.radius_after_use.expect("радиус облака не уменьшился");
    assert!(
        after <= before - 0.4,
        "радиус не уменьшился на 0.5: {before} -> {after}"
    );
    println!("dragon breath bottle ok (radius {before:.2} -> {after:.2}, cloud {cloud})");
    println!("dragon_breath_probe: ok");
}
