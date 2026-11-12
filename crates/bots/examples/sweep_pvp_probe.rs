//! PvP-разящий удар: меч бьёт по площади и задевает второго игрока.
//!
//! `sweep_pvp_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

struct Peer {
    c: Client,
    sb_command: i32,
    sb_attack: i32,
    sb_move_pos: i32,
    sb_tick_end: i32,
    sb_carried: i32,
    cb_chat: i32,
    chat: String,
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

    /// Ждёт `n` разных игроков и возвращает их id по возрастанию.
    async fn wait_players(&mut self, kind: i32, n: usize) -> Vec<i32> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            let mut ids: Vec<i32> = self
                .seen
                .iter()
                .filter(|(_, k, _)| *k == kind)
                .map(|(id, _, _)| *id)
                .collect();
            ids.sort_unstable();
            ids.dedup();
            if ids.len() >= n {
                self.drain();
                return ids;
            }
            match tokio::time::timeout_at(deadline, self.c.next_event()).await {
                Ok(Ok(Event::EntitySpawned {
                    entity_id,
                    kind: k,
                    pos,
                })) => self.seen.push((entity_id, k, pos)),
                Ok(Ok(_)) => {}
                _ => panic!("не дождались {n} игроков"),
            }
        }
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
            sb_move_pos: sb("move_player_pos"),
            sb_tick_end: sb("client_tick_end"),
            sb_carried: sb("set_carried_item"),
            cb_chat,
            chat: String::new(),
            seen,
        },
        spawn,
    )
}

async fn health(op: &mut Peer, name: &str) -> f64 {
    let text = op.data(&format!("{name} Health")).await;
    last_number(&text).unwrap_or_else(|| panic!("нет Health у {name}: {text}"))
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
    let (bx, bz) = (spawn[0].floor() as i32 + 70, spawn[2].floor() as i32 + 70);
    let by = -60;
    let base = [bx as f64 + 0.5, by as f64, bz as f64 + 0.5];
    let target_at = [base[0], base[1], base[2] + 2.0];
    let bystander_at = [base[0] + 1.2, base[1], base[2] + 2.0];

    attacker.cmd("difficulty peaceful").await;
    attacker.cmd("gamemode survival").await;
    attacker.cmd("clear @s").await;
    attacker.cmd("effect clear @s").await;
    attacker
        .cmd(&format!(
            "fill {} {} {} {} {} {} minecraft:air",
            bx - 3,
            by,
            bz - 2,
            bx + 3,
            by + 3,
            bz + 4
        ))
        .await;
    attacker
        .cmd(&format!("tp @s {} {} {} 0 0", base[0], base[1], base[2]))
        .await;
    attacker.move_ground(base).await;
    attacker
        .cmd("item replace entity @s hotbar.0 with minecraft:diamond_sword 1")
        .await;
    attacker.select(0).await;

    // Два бота без прав: цель удара и сосед, которого должен задеть размах.
    let (mut defender, _) = connect(&a[1], "defender", &table).await;
    attacker.cmd("gamemode survival defender").await;
    attacker.cmd("clear defender").await;
    attacker
        .cmd(&format!(
            "tp defender {} {} {} 0 0",
            target_at[0], target_at[1], target_at[2]
        ))
        .await;
    attacker
        .cmd("data merge entity defender {Health:20.0f}")
        .await;
    let ids = attacker.wait_players(player, 1).await;
    let target = ids[0];

    let (mut bystander, _) = connect(&a[1], "bystander", &table).await;
    attacker.cmd("gamemode survival bystander").await;
    attacker.cmd("clear bystander").await;
    attacker
        .cmd(&format!(
            "tp bystander {} {} {} 0 0",
            bystander_at[0], bystander_at[1], bystander_at[2]
        ))
        .await;
    attacker
        .cmd("data merge entity bystander {Health:20.0f}")
        .await;
    defender.pump(600).await;
    bystander.pump(600).await;
    let ids = attacker.wait_players(player, 2).await;
    assert_eq!(ids.len(), 2, "должно быть два бота: {ids:?}");

    // Полный заряд меча.
    tokio::time::sleep(Duration::from_millis(1400)).await;
    attacker.pump(200).await;

    attacker.attack(target).await;
    defender.pump(600).await;
    bystander.pump(600).await;
    let h = health(&mut attacker, "defender").await;
    assert!(h < 19.0, "основная цель не получила урон: {h}");
    let h = health(&mut attacker, "bystander").await;
    assert!(
        (18.0..=19.9).contains(&h),
        "разящий удар не задел соседнего игрока: {h}"
    );

    attacker.cmd("clear @s").await;
    attacker.cmd("clear defender").await;
    attacker.cmd("clear bystander").await;
    println!("sweep_pvp: ok");
}
