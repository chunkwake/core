//! Изолированная проба размножения (`VillagerMakeLove`): пара жителей с
//! едой, свободные кровати и таймер 275–325 тиков.
//!
//! `villager_breed_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer};
use std::time::Duration;

const X: i32 = 5400;
const Z: i32 = 5400;
const Y: i32 = 100;

struct Probe {
    c: Client,
    sb_command: i32,
    cb_chat: i32,
    chat: String,
    villager_type: i32,
    spawned: Vec<i32>,
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
            && kind == self.villager_type
            && (pos[0] - f64::from(X)).abs() < 20.0
            && (pos[2] - f64::from(Z)).abs() < 20.0
        {
            self.spawned.push(entity_id);
            println!("villager {entity_id} at {pos:?}");
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
        spawned: Vec::new(),
    };
    p.pump(1500).await;

    for line in [
        "difficulty peaceful".to_string(),
        "gamemode creative".to_string(),
        format!(
            "execute in minecraft:overworld run tp @s {} {} {}",
            X + 4,
            Y,
            Z + 2
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            X - 6,
            Y - 1,
            Z - 6,
            X + 10,
            Y - 1,
            Z + 6
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:air",
            X - 6,
            Y,
            Z - 6,
            X + 10,
            Y + 4,
            Z + 6
        ),
        "kill @e[type=!minecraft:player,distance=..96]".to_string(),
        "time set 1000".to_string(),
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
    }
    for bx in [X + 1, X + 3] {
        p.cmd(&format!(
            "summon minecraft:villager {}.5 {Y} {}.5 {{Age:0,FoodLevel:12b,PersistenceRequired:1b,VillagerData:{{type:\"minecraft:plains\",profession:\"minecraft:none\",level:1}}}}",
            bx,
            Z + 2
        ))
        .await;
    }
    p.cmd(&format!(
        "tag @e[type=minecraft:villager,x={},y={Y},z={},distance=..2] add probe_pair",
        X + 2,
        Z
    ))
    .await;
    let start = tokio::time::Instant::now();
    let mut tick = 0;
    while start.elapsed() < Duration::from_millis(90_000) {
        if tick % 10 == 0 {
            p.cmd(&format!(
                "tp @e[tag=probe_pair,limit=1,sort=nearest] {} {Y} {Z}.5",
                X + 1
            ))
            .await;
            p.cmd(&format!(
                "tp @e[tag=probe_pair,limit=1,sort=furthest] {} {Y} {Z}.5",
                X + 3
            ))
            .await;
        }
        tick += 1;
        if p.spawned.len() >= 3 {
            break;
        }
        p.pump(300).await;
    }
    assert!(
        p.spawned.len() >= 3,
        "ребёнок не родился: spawned {:?}",
        p.spawned
    );
    println!("villager_breed_probe: ok");
}
