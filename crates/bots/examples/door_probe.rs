//! Двери мобов: зомби с `CanBreakDoors` идёт к игроку через закрытую
//! деревянную дверь и ломает её на сложной сложности (`BreakDoorGoal`).
//!
//! `door_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_move_pos: i32,
    sb_tick_end: i32,
    cb_chat: i32,
    cb_destruction: i32,
    chat: String,
    destruction: u32,
}

impl Probe {
    fn packet(&self, id: i32) -> Writer {
        let mut w = Writer::new();
        w.var_int(id);
        w
    }

    async fn pump(&mut self, ms: u64) {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
        while let Ok(Ok(_)) = tokio::time::timeout_at(deadline, self.c.next_event()).await {
            self.drain();
        }
    }

    fn drain(&mut self) {
        for (id, body) in std::mem::take(&mut self.c.captured_by) {
            if id == self.cb_destruction {
                self.destruction += 1;
                continue;
            }
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

    async fn cmd(&mut self, line: &str) {
        self.chat.clear();
        self.send_cmd(line).await;
        self.pump(300).await;
    }

    async fn send_cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
    }

    async fn data(&mut self, query: &str) -> String {
        self.chat.clear();
        self.send_cmd(&format!("data get entity {query}")).await;
        self.pump(300).await;
        self.chat.clone()
    }

    async fn say_if_block(&mut self, pos: [i32; 3], block: &str, word: &str) -> bool {
        self.chat.clear();
        self.send_cmd(&format!(
            "execute if block {} {} {} minecraft:{block} run say {word}",
            pos[0], pos[1], pos[2]
        ))
        .await;
        self.pump(300).await;
        self.chat.contains(word)
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

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let sb = |name| table.id(State::Play, Direction::Serverbound, name).unwrap();
    let cb = |name| table.id(State::Play, Direction::Clientbound, name).unwrap();
    let ids = Ids::load(&table).unwrap();
    let mut c = Client::connect(&a[1], "framer", ids, 777, "26.3")
        .await
        .unwrap();
    let spawn = loop {
        if let Event::Position(at) = c.next_event().await.unwrap() {
            break at;
        }
    };
    let cb_chat = cb("system_chat");
    let cb_destruction = cb("block_destruction");
    c.capture_also = vec![cb_chat, cb_destruction];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_move_pos: sb("move_player_pos"),
        sb_tick_end: sb("client_tick_end"),
        cb_chat,
        cb_destruction,
        chat: String::new(),
        destruction: 0,
    };
    let (bx, bz) = (spawn[0].floor() as i32 + 20, spawn[2].floor() as i32 + 20);
    let by = -60;

    p.cmd("gamemode creative").await;
    p.cmd("difficulty hard").await;
    p.cmd("gamerule mob_griefing true").await;
    p.cmd("gamerule doMobSpawning false").await;
    p.cmd("time set 18000").await;
    p.cmd("kill @e[type=minecraft:zombie]").await;
    p.pump(1300).await;

    // Комната с единственным выходом — дверью; окно над ней даёт зомби
    // увидеть игрока, но пройти можно только через дверь.
    p.cmd(&format!(
        "fill {} {} {} {} {} {} minecraft:air",
        bx - 6,
        by,
        bz - 6,
        bx + 6,
        by + 6,
        bz + 6
    ))
    .await;
    for (x0, z0, x1, z1) in [
        (bx - 3, bz - 3, bx + 3, bz - 3),
        (bx - 3, bz + 3, bx + 3, bz + 3),
        (bx - 3, bz - 3, bx - 3, bz + 3),
        (bx + 3, bz - 3, bx + 3, bz + 3),
    ] {
        p.cmd(&format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            x0,
            by,
            z0,
            x1,
            by + 2,
            z1
        ))
        .await;
    }
    p.cmd(&format!(
        "fill {} {} {} {} {} {} minecraft:air",
        bx - 2,
        by,
        bz - 2,
        bx + 2,
        by + 2,
        bz + 2
    ))
    .await;
    let door_z = bz + 3;
    for (dy, half) in [(0, "lower"), (1, "upper")] {
        p.cmd(&format!(
            "setblock {} {} {} minecraft:oak_door[half={half},facing=north,hinge=left,open=false,powered=false]",
            bx,
            by + dy,
            door_z
        ))
        .await;
    }
    p.cmd(&format!(
        "setblock {} {} {} minecraft:air",
        bx,
        by + 2,
        door_z
    ))
    .await;
    p.cmd(&format!(
        "setblock {} {} {} minecraft:stone",
        bx,
        by,
        bz + 6
    ))
    .await;
    let lower = [bx, by, door_z];
    assert!(
        p.say_if_block(lower, "oak_door", "door_ok").await,
        "дверь не поставилась: {}",
        p.chat
    );

    // Игрок снаружи на ступеньке: глаза выше окна над дверью.
    let player = [bx as f64 + 0.5, by as f64 + 1.0, bz as f64 + 6.5];
    p.cmd(&format!("tp @s {} {} {}", player[0], player[1], player[2]))
        .await;
    p.move_ground(player).await;
    p.cmd("gamemode survival").await;
    p.cmd("effect give @s minecraft:resistance 120 4").await;
    p.cmd("effect give @s minecraft:regeneration 120 2").await;

    p.send_cmd(&format!(
        "summon minecraft:zombie {} {} {} {{CanBreakDoors:1b,PersistenceRequired:1b}}",
        bx as f64 + 0.5,
        by as f64,
        bz as f64 + 0.5
    ))
    .await;
    p.pump(500).await;
    let flag = p
        .data(&format!(
            "@e[type=minecraft:zombie,limit=1,sort=nearest,x={},y={},z={},distance=..2] CanBreakDoors",
            bx as f64 + 0.5,
            by as f64,
            bz as f64 + 0.5
        ))
        .await;
    assert!(flag.contains("1.0"), "CanBreakDoors не применился: {flag}");

    let mut broken = false;
    for _ in 0..120 {
        if p.say_if_block(lower, "air", "door_broken").await {
            broken = true;
            break;
        }
        p.pump(200).await;
    }
    assert!(
        broken,
        "зомби не сломал дверь (destruction={})",
        p.destruction
    );
    assert!(p.destruction > 0, "клиенту не пришёл block_destruction");

    p.cmd("kill @e[type=minecraft:zombie]").await;
    println!("door: ok");
}
