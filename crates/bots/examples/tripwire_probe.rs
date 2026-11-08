//! Живой прогон разрыва растяжки рукой и ножницами, включая вибрации крюков.
//! `tripwire_probe <addr> <packets.json> <blocks.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use std::collections::HashMap;
use std::time::Duration;

const HOOKS: [[i32; 3]; 2] = [[8, 200, 8], [11, 200, 8]];
const WIRE: [i32; 3] = [9, 200, 8];
const SENSOR: [i32; 3] = [8, 200, 10];
const FILTER: [i32; 3] = [7, 200, 10];

fn pack(p: [i32; 3]) -> i64 {
    ((p[0] as i64 & 0x3ff_ffff) << 38) | ((p[2] as i64 & 0x3ff_ffff) << 12) | (p[1] as i64 & 0xfff)
}

struct Probe {
    c: Client,
    table: PacketTable,
    states: HashMap<i32, String>,
    hooks: HashMap<[i32; 3], i32>,
    seq: i32,
    changes: Vec<String>,
    sensor_changes: Vec<String>,
    filter_changes: Vec<String>,
}

impl Probe {
    fn sb(&self, name: &str) -> i32 {
        self.table
            .id(State::Play, Direction::Serverbound, name)
            .unwrap()
    }

    fn name(&self, state: i32) -> &str {
        self.states
            .get(&state)
            .map(String::as_str)
            .unwrap_or("unknown")
    }

    async fn pump(&mut self, ms: u64) {
        let until = tokio::time::Instant::now() + Duration::from_millis(ms);
        while let Ok(Ok(event)) = tokio::time::timeout_at(until, self.c.next_event()).await {
            let updates = match event {
                Event::BlockUpdate(pos, state) => vec![(pos, state)],
                Event::BlockUpdates(updates) => updates,
                _ => continue,
            };
            for (pos, state) in updates {
                if HOOKS.contains(&pos) {
                    self.hooks.insert(pos, state);
                    self.changes.push(format!("{pos:?} {}", self.name(state)));
                } else if pos == SENSOR {
                    self.sensor_changes.push(self.name(state).to_owned());
                } else if pos == FILTER {
                    self.filter_changes.push(self.name(state).to_owned());
                }
            }
        }
    }

    async fn cmd(&mut self, command: &str) {
        let mut w = Writer::new();
        w.var_int(self.sb("chat_command")).string(command);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(200).await;
    }

    async fn dig(&mut self) {
        for status in [0, 2] {
            self.seq += 1;
            let mut w = Writer::new();
            w.var_int(self.sb("player_action"))
                .var_int(status)
                .i64(pack(WIRE))
                .u8(1)
                .var_int(self.seq);
            self.c.send_raw(&w.buf).await.unwrap();
            self.pump(150).await;
        }
        self.pump(500).await;
    }

    async fn chain(&mut self) {
        for command in [
            "fill 6 199 6 13 199 10 minecraft:stone",
            "fill 6 200 6 13 202 10 minecraft:air",
            "setblock 7 200 8 minecraft:stone",
            "setblock 12 200 8 minecraft:stone",
            "setblock 8 200 8 minecraft:tripwire_hook[facing=east,attached=true,powered=false]",
            "setblock 11 200 8 minecraft:tripwire_hook[facing=west,attached=true,powered=false]",
            "setblock 9 200 8 minecraft:tripwire[attached=true,disarmed=false,powered=false]",
            "setblock 10 200 8 minecraft:tripwire[attached=true,disarmed=false,powered=false]",
        ] {
            self.cmd(command).await;
        }
        self.pump(500).await;
        self.changes.clear();
    }

    async fn sensor(&mut self, frequency: i32) {
        self.cmd("fill 0 199 10 8 199 10 minecraft:stone").await;
        self.cmd("fill 0 200 10 8 200 10 minecraft:air").await;
        let source = if frequency == 9 { 0 } else { 1 };
        self.cmd(&format!(
            "setblock {source} 200 10 minecraft:redstone_block"
        ))
        .await;
        self.filter_changes.clear();
        self.cmd(&format!(
            "fill {} 200 10 7 200 10 minecraft:redstone_wire",
            source + 1
        ))
        .await;
        self.cmd("setblock 8 200 10 minecraft:calibrated_sculk_sensor[facing=east]")
            .await;
        self.pump(400).await;
        assert!(
            self.filter_changes
                .last()
                .is_some_and(|s| s.contains(&format!("power={frequency}"))),
            "фильтр сенсора: ожидалась частота {frequency}, обновления {:?}",
            self.filter_changes
        );
        self.sensor_changes.clear();
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<_> = std::env::args().collect();
    let table = PacketTable::load(args[2].as_ref()).unwrap();
    let blocks: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&args[3]).unwrap()).unwrap();
    let mut states = HashMap::new();
    for (name, block) in blocks.as_object().unwrap() {
        for state in block["states"].as_array().unwrap() {
            let mut props: Vec<_> = state["properties"]
                .as_object()
                .map(|properties| {
                    properties
                        .iter()
                        .map(|(key, value)| format!("{key}={}", value.as_str().unwrap()))
                        .collect()
                })
                .unwrap_or_default();
            props.sort();
            states.insert(
                state["id"].as_i64().unwrap() as i32,
                format!("{name}[{}]", props.join(",")),
            );
        }
    }
    let ids = Ids::load(&table).unwrap();
    let mut c = Client::connect(&args[1], "framer", ids, 777, "26.3")
        .await
        .unwrap();
    while !matches!(c.next_event().await.unwrap(), Event::Position(_)) {}
    let mut probe = Probe {
        c,
        table,
        states,
        hooks: HashMap::new(),
        seq: 0,
        changes: Vec::new(),
        sensor_changes: Vec::new(),
        filter_changes: Vec::new(),
    };
    probe.pump(500).await;
    probe.cmd("gamemode survival").await;
    probe.cmd("tp framer 9.5 200 10.5").await;
    probe.pump(800).await;

    for (label, shears) in [("hand", false), ("shears", true)] {
        probe.chain().await;
        probe.cmd("clear framer").await;
        if shears {
            probe.cmd("give framer minecraft:shears").await;
        }
        probe.sensor(if shears { 9 } else { 10 }).await;
        probe.changes.clear();
        probe.dig().await;
        println!("== {label}");
        for change in &probe.changes {
            println!("{change}");
        }
        assert!(
            probe
                .sensor_changes
                .iter()
                .any(|state| state.contains("sculk_sensor_phase=active")),
            "{label}: крюк не включил сенсор; обновления {:?}",
            probe.sensor_changes
        );
        println!("{label}: калиброванный сенсор включился");
        for hook in HOOKS {
            let pulse = probe.changes.iter().any(|line| {
                line.starts_with(&format!("{hook:?}")) && line.contains("powered=true")
            });
            assert_eq!(pulse, !shears, "{label}: неверный импульс крюка {hook:?}");
            let final_state = probe.hooks[&hook];
            assert!(
                probe.name(final_state).contains("attached=false")
                    && probe.name(final_state).contains("powered=false"),
                "{label}: крюк {hook:?} не отпустил нить"
            );
        }
    }
}
