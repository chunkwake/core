//! Живой прогон вибраций от урона, приземления, пластинки, контейнеров и редстоуна.
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

    async fn use_on(&mut self, pos: [i32; 3]) {
        self.seq += 1;
        let mut w = Writer::new();
        w.var_int(self.sb("use_item_on"))
            .var_int(0)
            .i64(pack(pos))
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

    async fn use_item(&mut self) {
        self.seq += 1;
        let mut w = Writer::new();
        w.var_int(self.sb("use_item"))
            .var_int(0)
            .var_int(self.seq)
            .f32(0.0)
            .f32(0.0);
        self.client.send_raw(&w.buf).await.unwrap();
        self.pump(400).await;
    }

    async fn close_window(&mut self) {
        let mut w = Writer::new();
        w.var_int(self.sb("container_close")).var_int(1);
        self.client.send_raw(&w.buf).await.unwrap();
        self.pump(400).await;
    }

    async fn hit_block(&mut self, pos: [i32; 3]) {
        self.seq += 1;
        let mut w = Writer::new();
        w.var_int(self.sb("player_action"))
            .var_int(0)
            .i64(pack(pos))
            .u8(1)
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
    probe.use_on(JUKEBOX).await;
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
    probe.use_on(JUKEBOX).await;
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

    probe.cmd("setblock 10 201 8 minecraft:air").await;
    probe.cmd("setblock 10 200 8 minecraft:chest").await;
    probe.cmd("tp framer 10.5 200 10.5 -90 0").await;
    probe.pump(2600).await;
    probe.reset_sensor().await;
    probe.use_on(JUKEBOX).await;
    probe.assert_active("открытие сундука");
    probe.pump(2600).await;
    probe.reset_sensor().await;
    probe.close_window().await;
    probe.assert_active("закрытие сундука");

    probe.cmd("setblock 10 200 8 minecraft:barrel").await;
    probe.pump(2600).await;
    probe.reset_sensor().await;
    probe.use_on(JUKEBOX).await;
    probe.assert_active("открытие бочки");
    probe.pump(2600).await;
    probe.reset_sensor().await;
    probe.close_window().await;
    probe.assert_active("закрытие бочки");

    probe.cmd("setblock 10 200 8 minecraft:hopper").await;
    probe.pump(2600).await;
    probe.reset_sensor().await;
    probe.use_on(JUKEBOX).await;
    assert!(
        !probe
            .changes
            .iter()
            .any(|state| state.contains("sculk_sensor_phase=active")),
        "открытие воронки ошибочно включило сенсор: {:?}",
        probe.changes
    );
    println!("открытие воронки: сенсор молчит");
    probe.close_window().await;

    probe.cmd("setblock 10 200 8 minecraft:note_block").await;
    probe.pump(2600).await;
    probe.reset_sensor().await;
    probe.use_on(JUKEBOX).await;
    probe.assert_active("нотный блок: клик");

    probe.cmd("setblock 10 201 8 minecraft:stone").await;
    probe.pump(2600).await;
    probe.reset_sensor().await;
    probe.use_on(JUKEBOX).await;
    assert!(
        !probe
            .changes
            .iter()
            .any(|state| state.contains("sculk_sensor_phase=active")),
        "заглушенный нотный блок включил сенсор: {:?}",
        probe.changes
    );
    println!("нотный блок под камнем: сенсор молчит");

    probe.cmd("setblock 10 201 8 minecraft:air").await;
    probe.pump(2600).await;
    probe.reset_sensor().await;
    probe.hit_block(JUKEBOX).await;
    probe.assert_active("нотный блок: удар");

    probe.pump(2600).await;
    probe.reset_sensor().await;
    probe
        .cmd("setblock 11 200 8 minecraft:redstone_block")
        .await;
    probe.pump(500).await;
    probe.assert_active("нотный блок: фронт сигнала");

    probe.cmd("setblock 11 200 8 minecraft:air").await;
    probe
        .cmd("setblock 10 200 8 minecraft:lever[face=floor,facing=north,powered=false]")
        .await;
    probe.pump(2600).await;
    probe.reset_sensor().await;
    probe.use_on(JUKEBOX).await;
    probe.assert_active("рычаг: включение");
    assert!(
        probe
            .jukebox_changes
            .iter()
            .any(|state| state.contains("powered=true"))
    );

    probe.pump(2600).await;
    probe.reset_sensor().await;
    probe.jukebox_changes.clear();
    probe.use_on(JUKEBOX).await;
    probe.assert_active("рычаг: выключение");
    assert!(
        probe
            .jukebox_changes
            .iter()
            .any(|state| state.contains("powered=false"))
    );

    probe
        .cmd("setblock 10 200 8 minecraft:oak_button[face=floor,facing=north,powered=false]")
        .await;
    probe.pump(2600).await;
    probe.reset_sensor().await;
    probe.jukebox_changes.clear();
    probe.use_on(JUKEBOX).await;
    probe.assert_active("кнопка: нажатие");
    assert!(
        probe
            .jukebox_changes
            .iter()
            .any(|state| state.contains("powered=true"))
    );
    // Старый сенсор должен закончить цикл: его отложенный тик переживает `/setblock`.
    probe.pump(2600).await;
    probe.cmd("setblock 8 200 8 minecraft:air").await;
    probe.cmd("tick rate 5").await;
    probe.jukebox_changes.clear();
    probe.use_on(JUKEBOX).await;
    assert!(
        probe
            .jukebox_changes
            .iter()
            .any(|state| state.contains("powered=true"))
    );
    probe.reset_sensor().await;
    probe.pump(7000).await;
    probe.assert_active("кнопка: отпускание");
    assert!(
        probe
            .jukebox_changes
            .iter()
            .any(|state| state.contains("powered=false"))
    );
    probe.cmd("tick rate 20").await;

    probe
        .cmd("setblock 10 200 8 minecraft:stone_pressure_plate[powered=false]")
        .await;
    probe.cmd("tp framer 10.5 200 8.5").await;
    probe.pump(2600).await;
    probe.reset_sensor().await;
    probe.jukebox_changes.clear();
    // `/setblock` сам не вибрирует; стоящий на плите игрок нажмёт её на тике.
    probe
        .cmd("setblock 10 200 8 minecraft:stone_pressure_plate[powered=false]")
        .await;
    probe.assert_active("плита: нажатие");
    assert!(
        probe
            .jukebox_changes
            .iter()
            .any(|state| state.contains("powered=true"))
    );

    // Остановить тики, чтобы успеть сбросить сенсор до проверки отпущенной плиты.
    probe.cmd("tick freeze").await;
    probe.cmd("tp framer 10.5 200 10.5").await;
    probe.reset_sensor().await;
    probe.jukebox_changes.clear();
    probe.cmd("tick unfreeze").await;
    probe.pump(1400).await;
    probe.assert_active("плита: отпускание");
    assert!(
        probe
            .jukebox_changes
            .iter()
            .any(|state| state.contains("powered=false"))
    );

    probe
        .cmd("setblock 10 200 8 minecraft:light_weighted_pressure_plate[power=0]")
        .await;
    probe.cmd("tp framer 10.5 200 8.5").await;
    probe.pump(2600).await;
    probe.reset_sensor().await;
    probe.jukebox_changes.clear();
    probe
        .cmd("setblock 10 200 8 minecraft:light_weighted_pressure_plate[power=2]")
        .await;
    probe.pump(500).await;
    assert!(
        probe
            .jukebox_changes
            .iter()
            .any(|state| state.contains("power=1")),
        "утяжелённая плита не пересчитала силу: {:?}",
        probe.jukebox_changes
    );
    assert!(
        !probe
            .changes
            .iter()
            .any(|state| state.contains("sculk_sensor_phase=active")),
        "изменение ненулевой силы плиты включило сенсор: {:?}",
        probe.changes
    );
    println!("утяжелённая плита: сила 2 → 1 без вибрации");

    probe.cmd("tp framer 10.5 200 10.5").await;
    probe
        .cmd("setblock 10 200 8 minecraft:oak_fence_gate[facing=north,open=false,powered=false]")
        .await;
    probe.pump(2600).await;
    probe.reset_sensor().await;
    probe.jukebox_changes.clear();
    probe
        .cmd("setblock 11 200 8 minecraft:redstone_block")
        .await;
    probe.assert_active("калитка: открытие редстоуном");
    assert!(
        probe
            .jukebox_changes
            .iter()
            .any(|state| state.contains("open=true"))
    );

    probe.pump(2600).await;
    probe.reset_sensor().await;
    probe.jukebox_changes.clear();
    probe.cmd("setblock 11 200 8 minecraft:air").await;
    probe.assert_active("калитка: закрытие редстоуном");
    assert!(
        probe
            .jukebox_changes
            .iter()
            .any(|state| state.contains("open=false"))
    );

    probe.cmd("setblock 10 200 8 minecraft:oak_door[half=lower,facing=north,hinge=left,open=false,powered=false]").await;
    probe.cmd("setblock 10 201 8 minecraft:oak_door[half=upper,facing=north,hinge=left,open=false,powered=false]").await;
    probe.pump(2600).await;
    probe.reset_sensor().await;
    probe.jukebox_changes.clear();
    probe
        .cmd("setblock 11 200 8 minecraft:redstone_block")
        .await;
    probe.assert_active("дверь: открытие редстоуном");
    assert!(
        probe
            .jukebox_changes
            .iter()
            .any(|state| state.contains("open=true"))
    );

    probe.pump(2600).await;
    probe.reset_sensor().await;
    probe.jukebox_changes.clear();
    probe.cmd("setblock 11 200 8 minecraft:air").await;
    probe.assert_active("дверь: закрытие редстоуном");
    assert!(
        probe
            .jukebox_changes
            .iter()
            .any(|state| state.contains("open=false"))
    );

    probe.cmd("setblock 10 201 8 minecraft:air").await;
    probe.cmd("setblock 10 200 8 minecraft:oak_trapdoor[facing=north,half=bottom,open=false,powered=false,waterlogged=false]").await;
    probe.pump(2600).await;
    probe.reset_sensor().await;
    probe.jukebox_changes.clear();
    probe
        .cmd("setblock 11 200 8 minecraft:redstone_block")
        .await;
    assert!(
        probe
            .jukebox_changes
            .iter()
            .any(|state| state.contains("open=true"))
    );
    assert!(
        !probe
            .changes
            .iter()
            .any(|state| state.contains("sculk_sensor_phase=active")),
        "люк от редстоуна включил сенсор: {:?}",
        probe.changes
    );
    println!("люк: открытие редстоуном без вибрации");

    // Сигнал 3 сзади калиброванного сенсора: снаряд при запуске даёт
    // `projectile_shoot`, а его приземление имеет другую частоту (2).
    probe.cmd("tp framer 10.5 200 10.5").await;
    probe.cmd("fill -6 199 8 8 199 8 minecraft:stone").await;
    probe.cmd("fill -6 200 8 8 200 8 minecraft:air").await;
    probe
        .cmd("setblock -6 200 8 minecraft:redstone_block")
        .await;
    probe
        .cmd("fill -5 200 8 7 200 8 minecraft:redstone_wire")
        .await;
    probe
        .cmd("setblock 8 200 8 minecraft:calibrated_sculk_sensor[facing=east]")
        .await;
    probe.cmd("clear framer").await;
    probe.cmd("give framer minecraft:snowball").await;
    probe.hold(0).await;
    probe.pump(2600).await;
    probe.changes.clear();
    probe.use_item().await;
    probe.assert_active("снежок: запуск снаряда (частота 3)");

    // Следующий снежок ударится в стену; фильтр 2 отсечёт событие запуска.
    probe.pump(2600).await;
    probe.cmd("fill -7 199 8 8 199 8 minecraft:stone").await;
    probe.cmd("fill -7 200 8 8 200 8 minecraft:air").await;
    probe
        .cmd("setblock -7 200 8 minecraft:redstone_block")
        .await;
    probe
        .cmd("fill -6 200 8 7 200 8 minecraft:redstone_wire")
        .await;
    probe
        .cmd("setblock 8 200 8 minecraft:calibrated_sculk_sensor[facing=east]")
        .await;
    probe.cmd("fill 10 200 12 10 204 12 minecraft:stone").await;
    probe.cmd("give framer minecraft:snowball").await;
    probe.hold(0).await;
    probe.pump(2600).await;
    probe.changes.clear();
    probe.use_item().await;
    probe.pump(900).await;
    probe.assert_active("снежок: попадание в блок (частота 2)");

    // Калиброванный сенсор с сигналом 15 слышит смерть, но отсекает урон (7).
    probe.pump(2600).await;
    probe.cmd("fill -7 200 8 8 200 8 minecraft:air").await;
    probe.cmd("setblock 7 200 8 minecraft:redstone_block").await;
    probe
        .cmd("setblock 8 200 8 minecraft:calibrated_sculk_sensor[facing=east]")
        .await;
    probe.cmd("tp framer 12.5 200 10.5").await;
    probe.cmd("fill 9 200 9 11 203 11 minecraft:glass").await;
    probe.cmd("fill 10 200 10 10 202 10 minecraft:air").await;
    probe.cmd("summon minecraft:cow 10.5 200 10.5").await;
    probe.pump(2600).await;
    probe.changes.clear();
    probe.cmd("setblock 10 200 10 minecraft:lava").await;
    probe.pump(6000).await;
    probe.assert_active("смерть коровы (частота 15)");

    probe.cmd("fill 9 200 9 11 203 11 minecraft:air").await;
    probe.cmd("tp framer 10.5 200 10.5").await;
    probe.pump(2600).await;
    probe.cmd("setblock 8 200 8 minecraft:air").await;
    probe
        .cmd("setblock 8 200 8 minecraft:calibrated_sculk_sensor[facing=east]")
        .await;
    probe.changes.clear();
    probe.cmd("damage framer 100 minecraft:generic").await;
    probe.assert_active("смерть игрока (частота 15)");
}
