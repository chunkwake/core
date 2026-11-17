//! Изолированная проба `Villager.spawnGolemIfNeeded`: пять жителей спят
//! (получают `LAST_SLEPT`), затем паника от зомби — трое согласных зовут
//! железного голема.
//!
//! `villager_golem_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer};
use std::time::Duration;

const X: i32 = 5200;
const Z: i32 = 5200;
const Y: i32 = 100;

struct Probe {
    c: Client,
    sb_command: i32,
    cb_chat: i32,
    chat: String,
    villager_type: i32,
    golem_type: i32,
    villagers: usize,
    golem: bool,
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
                if let Ok(component) = chunkwake_protocol::text::read_component(&mut r) {
                    let mut out = String::new();
                    flatten(&component, &mut out);
                    self.chat.push_str(&out);
                }
            }
        }
    }

    fn apply(&mut self, ev: Event) {
        if let Event::EntitySpawned {
            kind,
            entity_id,
            pos,
        } = ev
        {
            if (pos[0] - f64::from(X)).abs() > 40.0 || (pos[2] - f64::from(Z)).abs() > 40.0 {
                return;
            }
            if kind == self.villager_type {
                self.villagers += 1;
                println!("villager spawned: {entity_id} at {pos:?}");
            }
            if kind == self.golem_type {
                self.golem = true;
                println!("golem spawned: {entity_id} at {pos:?}");
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
            self.apply(ev);
        }
        self.drain();
    }

    async fn cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(250).await;
    }

    async fn check(&mut self, predicate: &str) -> bool {
        self.chat.clear();
        self.cmd(&format!("execute if {predicate} run say YES"))
            .await;
        self.chat.contains("YES")
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
    let cb_chat = cb("system_chat");
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    c.capture_also = vec![cb_chat];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        cb_chat,
        chat: String::new(),
        villager_type: pid("minecraft:entity_type", "minecraft:villager"),
        golem_type: pid("minecraft:entity_type", "minecraft:iron_golem"),
        villagers: 0,
        golem: false,
    };
    p.pump(1500).await;

    for line in [
        "difficulty peaceful".to_string(),
        "gamemode creative".to_string(),
        format!(
            "execute in minecraft:overworld run tp @s {} {} {}",
            X + 6,
            Y,
            Z + 3
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            X - 8,
            Y - 1,
            Z - 8,
            X + 20,
            Y - 1,
            Z + 8
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:air",
            X - 8,
            Y,
            Z - 8,
            X + 20,
            Y + 4,
            Z + 8
        ),
        "kill @e[type=!minecraft:player,distance=..96]".to_string(),
    ] {
        p.cmd(&line).await;
    }
    for i in 0..5 {
        let bx = X + i * 3;
        for (dz, part) in [(0, "foot"), (1, "head")] {
            p.cmd(&format!(
                "setblock {bx} {Y} {} minecraft:red_bed[part={part},facing=south]",
                Z + dz
            ))
            .await;
        }
        p.cmd(&format!(
            "summon minecraft:villager {}.5 {Y} {}.5 {{PersistenceRequired:1b,VillagerData:{{type:\"minecraft:plains\",profession:\"minecraft:none\",level:1}}}}",
            bx,
            Z + 2
        ))
        .await;
    }
    p.cmd(&format!(
        "tag @e[type=minecraft:villager,x={},y={Y},z={},distance=..10] add probe_go",
        X + 6,
        Z,
    ))
    .await;
    println!(
        "setup: beds + 5 villagers, villagers spawned={}",
        p.villagers
    );

    p.cmd("time set 13000").await;
    for step in 0..8 {
        p.pump(5_000).await;
        let sleeping = p
            .check(&format!(
                "entity @e[type=minecraft:villager,x={},y={Y},z={},distance=..10]",
                X + 6,
                Z,
            ))
            .await;
        println!("sleep wait {}0s: villagers present={sleeping}", step + 1);
    }
    p.cmd("time set 1000").await;
    p.pump(2_000).await;

    // Монстры на peaceful удаляются, а на easy днём горят:
    // ставим easy и крышу над зомби.
    p.cmd("difficulty easy").await;
    p.cmd(&format!(
        "setblock {} {Y2} {} minecraft:stone",
        X + 6,
        Z + 5,
        Y2 = Y + 2
    ))
    .await;
    p.cmd(&format!(
        "summon minecraft:zombie {}.5 {Y} {}.5 {{NoAI:1b,PersistenceRequired:1b}}",
        X + 6,
        Z + 5
    ))
    .await;
    for step in 0..12 {
        p.pump(5_000).await;
        if p.golem {
            break;
        }
        println!("panic wait {}0s: golem={}", step + 1, p.golem);
    }
    assert!(
        p.golem,
        "железный голем не появился (5 спящих жителей + паника)"
    );
    println!("villager_golem_probe: ok");
}
