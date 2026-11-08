//! Живой прогон вибраций от урона, приземления и смены пластинки.
//! `sculk_events_probe <addr> <packets.json> <blocks.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use std::collections::HashMap;
use std::time::Duration;

const SENSOR: [i32; 3] = [8, 200, 8];
const JUKEBOX: [i32; 3] = [10, 200, 8];

fn pack(pos: [i32; 3]) -> i64 {
    ((pos[0] as i64 & 0x3ff_ffff) << 38)
        | ((pos[2] as i64 & 0x3ff_ffff) << 12)
        | (pos[1] as i64 & 0xfff)
}

struct Probe {
    client: Client,
    table: PacketTable,
    states: HashMap<i32, String>,
    changes: Vec<String>,
    jukebox_changes: Vec<String>,
    seq: i32,
}

impl Probe {
    fn sb(&self, name: &str) -> i32 {
        self.table
            .id(State::Play, Direction::Serverbound, name)
            .unwrap()
    }

    async fn pump(&mut self, ms: u64) {
        let until = tokio::time::Instant::now() + Duration::from_millis(ms);
        while let Ok(Ok(event)) = tokio::time::timeout_at(until, self.client.next_event()).await {
            let updates = match event {
                Event::BlockUpdate(pos, state) => vec![(pos, state)],
                Event::BlockUpdates(updates) => updates,
                _ => continue,
            };
            for (pos, state) in updates {
                if pos == SENSOR {
                    self.changes
                        .push(self.states.get(&state).cloned().unwrap_or_default());
                } else if pos == JUKEBOX {
                    self.jukebox_changes
                        .push(self.states.get(&state).cloned().unwrap_or_default());
                }
            }
        }
    }

    async fn cmd(&mut self, command: &str) {
        let mut w = Writer::new();
        w.var_int(self.sb("chat_command")).string(command);
        self.client.send_raw(&w.buf).await.unwrap();
        self.pump(180).await;
    }

    async fn move_to(&mut self, pos: [f64; 3], on_ground: bool) {
        let mut w = Writer::new();
        w.var_int(self.sb("move_player_pos"));
        for value in pos {
            w.f64(value);
        }
        w.u8(u8::from(on_ground));
        self.client.send_raw(&w.buf).await.unwrap();
        let mut end = Writer::new();
        end.var_int(self.sb("client_tick_end"));
        self.client.send_raw(&end.buf).await.unwrap();
        self.pump(100).await;
    }

    async fn hold(&mut self, slot: i16) {
        let mut w = Writer::new();
        w.var_int(self.sb("set_carried_item")).u16(slot as u16);
        self.client.send_raw(&w.buf).await.unwrap();
        self.pump(100).await;
    }

    async fn use_jukebox(&mut self) {
        self.seq += 1;
        let mut w = Writer::new();
        w.var_int(self.sb("use_item_on"))
            .var_int(0)
            .i64(pack(JUKEBOX))
            .var_int(1)
            .f32(0.5)
            .f32(0.5)
            .f32(0.5)
            .u8(0)
            .u8(0)
            .var_int(self.seq);
        self.client.send_raw(&w.buf).await.unwrap();
        self.pump(400).await;
    }

    async fn drop_one(&mut self) {
        self.seq += 1;
        let mut w = Writer::new();
        w.var_int(self.sb("player_action"))
            .var_int(4)
            .i64(0)
            .u8(0)
            .var_int(self.seq);
        self.client.send_raw(&w.buf).await.unwrap();
        self.pump(200).await;
    }

    async fn reset_sensor(&mut self) {
        self.cmd("setblock 8 200 8 minecraft:air").await;
        self.cmd("setblock 8 200 8 minecraft:sculk_sensor").await;
        self.pump(300).await;
        self.changes.clear();
    }

    fn assert_active(&self, label: &str) {
        assert!(
            self.changes
                .iter()
                .any(|state| state.contains("sculk_sensor")
                    && state.contains("sculk_sensor_phase=active")),
            "{label}: сенсор не включился; обновления: {:?}",
            self.changes
        );
        println!("{label}: сенсор включился");
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
    let mut client = Client::connect(&args[1], "framer", ids, 777, "26.3")
        .await
        .unwrap();
    while !matches!(client.next_event().await.unwrap(), Event::Position(_)) {}
    let mut probe = Probe {
        client,
        table,
        states,
        changes: Vec::new(),
        jukebox_changes: Vec::new(),
        seq: 0,
    };
    probe.pump(400).await;
    probe.cmd("gamemode survival").await;
    probe.cmd("fill 6 199 6 13 199 13 minecraft:stone").await;
    probe.cmd("fill 6 200 6 13 205 13 minecraft:air").await;
    probe.cmd("tp framer 10.5 200 10.5").await;
    probe.reset_sensor().await;
    probe.cmd("damage framer 1 minecraft:generic").await;
    probe.assert_active("урон");

    probe.pump(2600).await;
    probe.reset_sensor().await;
    probe.cmd("tp framer 10.5 201.5 10.5").await;
    probe.pump(400).await;
    probe.changes.clear();
    probe.move_to([10.5, 201.0, 10.5], false).await;
    probe.move_to([10.5, 200.0, 10.5], true).await;
    probe.assert_active("приземление без урона");

    probe.pump(2600).await;
    probe.reset_sensor().await;
    probe.cmd("summon minecraft:cow 10.5 204 10.5").await;
    probe.pump(1800).await;
    probe.assert_active("приземление моба");

    probe.cmd("kill @e[type=minecraft:cow]").await;
    probe.cmd("kill @e[type=minecraft:item]").await;
    probe.cmd("clear framer").await;
    probe.cmd("give framer minecraft:music_disc_cat").await;
    probe.cmd("setblock 10 200 8 minecraft:jukebox").await;
    probe.cmd("tp framer 10.5 200 10.5 -90 0").await;
    probe.pump(2600).await;
    probe.reset_sensor().await;
    probe.hold(0).await;
    probe.use_jukebox().await;
    probe.assert_active("пластинка в проигрыватель рукой");
    assert!(
        probe
            .jukebox_changes
            .iter()
            .any(|state| state.contains("has_record=true"))
    );

    probe.pump(2600).await;
    probe.reset_sensor().await;
    probe.hold(5).await;
    probe.use_jukebox().await;
    probe.assert_active("пластинка из проигрывателя рукой");
    assert!(
        probe
            .jukebox_changes
            .iter()
            .any(|state| state.contains("has_record=false"))
    );

    probe.cmd("kill @e[type=minecraft:item]").await;
    probe.cmd("clear framer").await;
    probe.cmd("give framer minecraft:music_disc_13").await;
    probe
        .cmd("setblock 10 201 8 minecraft:hopper[facing=down]")
        .await;
    probe.cmd("tp framer 10.5 202 8.5 0 90").await;
    probe.pump(2600).await;
    probe.reset_sensor().await;
    probe.jukebox_changes.clear();
    probe.hold(0).await;
    probe.drop_one().await;
    probe.pump(3000).await;
    probe.assert_active("пластинка в проигрыватель из воронки");
    assert!(
        probe
            .jukebox_changes
            .iter()
            .any(|state| state.contains("has_record=true"))
    );
}
