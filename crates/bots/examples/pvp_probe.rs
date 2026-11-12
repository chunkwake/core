//! PvP: удар игрока по игроку, блокировка щитом и отключение щита топором.
//!
//! `pvp_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

struct Peer {
    c: Client,
    sb_command: i32,
    sb_attack: i32,
    sb_use_item: i32,
    sb_move_pos: i32,
    sb_tick_end: i32,
    sb_carried: i32,
    cb_chat: i32,
    chat: String,
    seq: i32,
    /// Появившиеся до `player_position` сущности (их `next_event` уже съел).
    seen: Vec<(i32, i32, [f64; 3])>,
}

impl Peer {
    fn packet(&self, id: i32) -> Writer {
        let mut w = Writer::new();
        w.var_int(id);
        w
    }

    fn drain(&mut self) {
        for (id, body) in std::mem::take(&mut self.c.captured_by) {
            if id != self.cb_chat {
                continue;
            }
            let mut r = Reader::new(&body);
            if let Ok(component) = text::read_component(&mut r) {
                self.chat.push_str(&flatten(&component));
                self.chat.push(' ');
            }
        }
    }

    async fn pump(&mut self, ms: u64) {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
        while let Ok(Ok(event)) = tokio::time::timeout_at(deadline, self.c.next_event()).await {
            if let Event::EntitySpawned {
                entity_id,
                kind,
                pos,
            } = event
            {
                self.seen.push((entity_id, kind, pos));
            }
        }
        self.drain();
    }

    async fn cmd(&mut self, line: &str) {
        self.send_cmd(line).await;
        self.pump(350).await;
    }

    async fn send_cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
    }

    async fn data(&mut self, query: &str) -> String {
        self.pump(150).await;
        self.chat.clear();
        self.cmd(&format!("data get entity {query}")).await;
        self.chat.clone()
    }

    async fn select(&mut self, slot: u16) {
        let mut w = self.packet(self.sb_carried);
        w.u16(slot);
        self.c.send_raw(&w.buf).await.unwrap();
    }

    async fn move_ground(&mut self, pos: [f64; 3]) {
        let mut w = self.packet(self.sb_move_pos);
        for v in pos {
            w.f64(v);
        }
        w.u8(1);
        self.c.send_raw(&w.buf).await.unwrap();
        let end = self.packet(self.sb_tick_end);
        self.c.send_raw(&end.buf).await.unwrap();
    }

    async fn attack(&mut self, entity: i32) {
        let mut w = self.packet(self.sb_attack);
        w.var_int(entity);
        self.c.send_raw(&w.buf).await.unwrap();
    }

    async fn use_air(&mut self, yaw: f32, pitch: f32) {
        self.seq += 1;
        let mut w = self.packet(self.sb_use_item);
        w.var_int(0).var_int(self.seq).f32(yaw).f32(pitch);
        self.c.send_raw(&w.buf).await.unwrap();
    }

    /// Ждёт появления сущности нужного типа (в мире один другой игрок).
    async fn wait_spawn(&mut self, kind: i32) -> i32 {
        if let Some(&(id, _, _)) = self.seen.iter().find(|(_, k, _)| *k == kind) {
            return id;
        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout_at(deadline, self.c.next_event()).await {
                Ok(Ok(Event::EntitySpawned {
                    entity_id,
                    kind: k,
                    pos,
                })) if k == kind => {
                    self.seen.push((entity_id, k, pos));
                    self.drain();
                    return entity_id;
                }
                Ok(Ok(_)) => {}
                _ => break,
            }
        }
        panic!("сущность {kind} не появилась");
    }
}

fn flatten(v: &serde_json::Value) -> String {
    if let Some(s) = v.as_str() {
        return s.to_owned();
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

fn last_number(text: &str) -> Option<f64> {
    text.split_whitespace()
        .rev()
        .find_map(|t| t.trim_end_matches('f').trim_end_matches('b').parse().ok())
}

async fn connect(addr: &str, name: &str, table: &PacketTable) -> (Peer, [f64; 3]) {
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n| table.id(State::Play, Direction::Clientbound, n).unwrap();
    let ids = Ids::load(table).unwrap();
    let mut c = Client::connect(addr, name, ids, 777, "26.3").await.unwrap();
    let mut seen = Vec::new();
    let spawn = loop {
        match c.next_event().await.unwrap() {
            Event::Position(at) => break at,
            Event::EntitySpawned {
                entity_id,
                kind,
                pos,
            } => seen.push((entity_id, kind, pos)),
            _ => {}
        }
    };
    let cb_chat = cb("system_chat");
    c.capture_also = vec![cb_chat];
    (
        Peer {
            c,
            sb_command: sb("chat_command"),
            sb_attack: sb("attack"),
            sb_use_item: sb("use_item"),
            sb_move_pos: sb("move_player_pos"),
            sb_tick_end: sb("client_tick_end"),
            sb_carried: sb("set_carried_item"),
            cb_chat,
            chat: String::new(),
            seq: 0,
            seen,
        },
        spawn,
    )
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let reg: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&a[3]).unwrap()).unwrap();
    let player = reg["minecraft:entity_type"]["entries"]["minecraft:player"]["protocol_id"]
        .as_i64()
        .unwrap() as i32;

    let (mut attacker, spawn) = connect(&a[1], "framer", &table).await;
    let (bx, bz) = (spawn[0].floor() as i32 + 60, spawn[2].floor() as i32 + 60);
    let by = -60;
    let base = [bx as f64 + 0.5, by as f64, bz as f64 + 0.5];
    // Атакующий севернее защитника; оба смотрят на +Z (атакующий впереди).
    let from = [base[0], base[1], base[2] + 2.0];

    attacker.cmd("difficulty peaceful").await;
    attacker.cmd("gamemode survival").await;
    attacker.cmd("clear @s").await;
    attacker.cmd("effect clear @s").await;
    attacker
        .cmd(&format!(
            "fill {} {} {} {} {} {} minecraft:air",
            bx - 2,
            by,
            bz - 2,
            bx + 2,
            by + 3,
            bz + 2
        ))
        .await;
    attacker
        .cmd(&format!("tp @s {} {} {} 0 0", from[0], from[1], from[2]))
        .await;
    attacker.move_ground(from).await;
    attacker
        .cmd("item replace entity @s hotbar.0 with minecraft:iron_axe 1")
        .await;
    attacker.select(0).await;

    // Второй бот без прав: всё снаряжение и позицию ему выдаёт оператор.
    let (mut defender, _) = connect(&a[1], "defender", &table).await;
    attacker.cmd("gamemode survival defender").await;
    attacker.cmd("clear defender").await;
    attacker
        .cmd(&format!(
            "tp defender {} {} {} 0 0",
            base[0], base[1], base[2]
        ))
        .await;
    attacker
        .cmd("item replace entity defender hotbar.0 with minecraft:shield 1")
        .await;
    attacker
        .cmd("data merge entity defender {Health:20.0f}")
        .await;
    defender.pump(800).await;
    let target = attacker.wait_spawn(player).await;
    defender.use_air(0.0, 0.0).await;
    defender.pump(600).await;

    // 1. Щит блокирует удар топором, но теряет 10 прочности.
    attacker.attack(target).await;
    defender.pump(500).await;
    let h = attacker.data("defender Health").await;
    let h = last_number(&h).unwrap_or_else(|| panic!("нет Health: {h}"));
    assert!((h - 20.0).abs() < 0.1, "щит не заблокировал PvP-удар: {h}");
    let inv = attacker.data("defender Inventory").await;
    assert!(
        inv.contains("minecraft:damage:10"),
        "щит должен потерять 10 прочности: {inv}"
    );

    // 2. Топор отключает щит на 5 секунд: следующий удар проходит.
    // Пауза — чтобы заряд атаки успел восстановиться (у топора ~1.1 с).
    tokio::time::sleep(Duration::from_millis(1400)).await;
    attacker.attack(target).await;
    defender.pump(500).await;
    let h = attacker.data("defender Health").await;
    let h = last_number(&h).unwrap_or_else(|| panic!("нет Health: {h}"));
    assert!(
        (11.0..=12.0).contains(&h),
        "после отключения щита удар должен пройти (ожидалось ~11 HP): {h}"
    );

    // 3. Пока идёт откат, щит не поднимается.
    defender.use_air(0.0, 0.0).await;
    defender.pump(400).await;
    tokio::time::sleep(Duration::from_millis(600)).await;
    attacker.attack(target).await;
    defender.pump(500).await;
    let h = attacker.data("defender Health").await;
    let h = last_number(&h).unwrap_or_else(|| panic!("нет Health: {h}"));
    assert!(
        (1.0..=3.0).contains(&h),
        "щит поднялся во время отката: {h}"
    );

    // Износ топора: 2 за удар.
    let inv = attacker.data("@s Inventory").await;
    assert!(
        inv.contains("minecraft:damage:6"),
        "топор должен износиться на 6: {inv}"
    );

    attacker.cmd("clear @s").await;
    attacker.cmd("clear defender").await;
    println!("pvp: ok");
}
