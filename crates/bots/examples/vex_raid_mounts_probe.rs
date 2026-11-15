//! Эвокер призывает 3 vexes, а раввагеры в рейде получают наездников
//! (пиллагер на волне 5, эвокер/виндикатор на волне 7).
//!
//! `vex_raid_mounts_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer};
use std::collections::HashMap;
use std::time::Duration;

const X: i32 = 2816;
const Z: i32 = 8;
const Y: i32 = 100;

struct Probe {
    c: Client,
    sb_command: i32,
    cb_add: i32,
    cb_passengers: i32,
    cb_sound: i32,
    evoker_sound: i32,
    evoker_sound_seen: bool,
    kinds: HashMap<i32, i32>,
    positions: HashMap<i32, [f64; 3]>,
    passengers: HashMap<i32, Vec<i32>>,
    vex_type: i32,
    ravager_type: i32,
    pillager_type: i32,
    evoker_type: i32,
    vindicator_type: i32,
    vexes: Vec<[f64; 3]>,
    saw_pillager: bool,
    saw_evoker: bool,
    saw_vindicator: bool,
}

impl Probe {
    fn packet(&self, id: i32) -> Writer {
        let mut w = Writer::new();
        w.var_int(id);
        w
    }

    fn check_passengers(&mut self) {
        for (&vehicle, riders) in &self.passengers {
            if self.kinds.get(&vehicle) != Some(&self.ravager_type) {
                continue;
            }
            for rider in riders {
                match self.kinds.get(rider) {
                    Some(&k) if k == self.pillager_type => self.saw_pillager = true,
                    Some(&k) if k == self.evoker_type => self.saw_evoker = true,
                    Some(&k) if k == self.vindicator_type => self.saw_vindicator = true,
                    _ => {}
                }
            }
        }
    }

    fn apply(&mut self, ev: Event) {
        if let Event::EntityMoved {
            entity_id, delta, ..
        } = ev
            && let Some(pos) = self.positions.get_mut(&entity_id)
        {
            pos[0] += delta[0];
            pos[1] += delta[1];
            pos[2] += delta[2];
        }
    }

    fn drain(&mut self) {
        for (id, body) in std::mem::take(&mut self.c.captured_by) {
            let mut r = Reader::new(&body);
            if id == self.cb_add {
                let (Ok(entity), Ok(_uuid)) = (r.var_int(), r.bytes(16)) else {
                    continue;
                };
                let Ok(kind) = r.var_int() else { continue };
                let (Ok(x), Ok(y), Ok(z)) = (r.f64(), r.f64(), r.f64()) else {
                    continue;
                };
                self.kinds.insert(entity, kind);
                self.positions.insert(entity, [x, y, z]);
                if kind == self.vex_type {
                    self.vexes.push([x, y, z]);
                }
            } else if id == self.cb_passengers {
                let Ok(vehicle) = r.var_int() else { continue };
                let Ok(count) = r.var_int() else { continue };
                let mut riders = Vec::new();
                for _ in 0..count {
                    let Ok(passenger) = r.var_int() else { break };
                    riders.push(passenger);
                }
                self.passengers.insert(vehicle, riders);
            } else if id == self.cb_sound && r.var_int().ok() == Some(self.evoker_sound + 1) {
                self.evoker_sound_seen = true;
            }
        }
        self.check_passengers();
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
            self.apply(ev);
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
    let cb_add = cb("add_entity");
    let cb_passengers = cb("set_passengers");
    let cb_sound = cb("sound");
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    c.capture_also = vec![cb_add, cb_passengers, cb_sound];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        cb_add,
        cb_passengers,
        cb_sound,
        evoker_sound: pid(
            "minecraft:sound_event",
            "minecraft:entity.evoker.prepare_summon",
        ),
        evoker_sound_seen: false,
        kinds: HashMap::new(),
        positions: HashMap::new(),
        passengers: HashMap::new(),
        vex_type: pid("minecraft:entity_type", "minecraft:vex"),
        ravager_type: pid("minecraft:entity_type", "minecraft:ravager"),
        pillager_type: pid("minecraft:entity_type", "minecraft:pillager"),
        evoker_type: pid("minecraft:entity_type", "minecraft:evoker"),
        vindicator_type: pid("minecraft:entity_type", "minecraft:vindicator"),
        vexes: Vec::new(),
        saw_pillager: false,
        saw_evoker: false,
        saw_vindicator: false,
    };
    p.pump(2000).await;

    for line in [
        "difficulty hard".to_string(),
        "gamemode survival".to_string(),
        "effect clear @s".to_string(),
        "effect give @s minecraft:resistance 120 4".to_string(),
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
            "fill {} {Y} {} {} 112 {} minecraft:air",
            X - 16,
            Z - 16,
            X + 32,
            Z + 24
        ),
        format!("tp @s {X}.5 {Y} {Z}.5 180 20"),
    ] {
        p.cmd(&line).await;
    }
    p.c.confirm_loaded().await.unwrap();

    // Эвокер: каст 100 тиков, затем 3 vexes рядом.
    p.cmd(&format!(
        "summon minecraft:evoker {}.5 {Y} {}.5 {{PersistenceRequired:1b}}",
        X + 6,
        Z
    ))
    .await;
    for _ in 0..60 {
        p.pump(200).await;
        if p.vexes.len() >= 3 && p.evoker_sound_seen {
            break;
        }
    }
    assert!(p.evoker_sound_seen, "нет звука prepare_summon");
    assert!(p.vexes.len() >= 3, "эвокер призвал {} vexes", p.vexes.len());
    for at in &p.vexes {
        // Эвокер мог отойти от точки summon: проверяем только уровень
        // призыва (blockPos + 1) и разумный радиус вокруг арены.
        assert!(
            (at[1] - f64::from(Y + 1)).abs() <= 1.5
                && (at[0] - f64::from(X + 6)).abs() <= 16.0
                && (at[2] - f64::from(Z)).abs() <= 16.0,
            "vex вне зоны призыва: {at:?}"
        );
    }
    println!("evoker vexes ok ({} spawned)", p.vexes.len());

    // Рейд Hard: волна 5 — раввагер + пиллагер, волна 7 — +эвокер/виндикатор.
    p.cmd("gamemode creative").await;
    p.cmd("kill @e[type=!minecraft:player,distance=..96]").await;
    for line in [
        format!(
            "setblock {X} 100 {} minecraft:red_bed[part=foot,facing=south]",
            Z
        ),
        format!(
            "setblock {X} 100 {} minecraft:red_bed[part=head,facing=south]",
            Z + 1
        ),
        format!("tp @s {X}.5 {Y} {}.5 180 20", Z + 6),
    ] {
        p.cmd(&line).await;
    }
    p.c.confirm_loaded().await.unwrap();
    p.cmd("effect give @s minecraft:bad_omen 600 0").await;

    for _ in 0..300 {
        p.cmd("kill @e[type=!minecraft:player,distance=..96]").await;
        if p.saw_pillager && p.saw_evoker && p.saw_vindicator {
            break;
        }
        p.pump(300).await;
    }
    assert!(p.saw_pillager, "не найден наездник-пиллагер на волне 5");
    assert!(p.saw_evoker, "не найден наездник-эвокер на волне 7");
    assert!(p.saw_vindicator, "не найден наездник-виндикатор на волне 7");
    println!("ravager jockeys ok (pillager, evoker, vindicator)");
    println!("vex_raid_mounts_probe: ok");
}
