//! `/tp` не-игровых сущностей: позиция и поворот доходят до сервера и
//! клиента (`teleport_entity`), работают несколько целей и `/execute as`.
//!
//! `teleport_probe <addr> <packets.json> <registries.json>`
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
    cb_teleport: i32,
    chat: String,
    /// Все `teleport_entity`: позиция и отправленный поворот.
    teleports: Vec<([f64; 3], f32, f32)>,
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
            if id == self.cb_teleport {
                let mut r = Reader::new(&body);
                let parsed = (|| {
                    let _id = r.var_int().ok()?;
                    let pos = [r.f64().ok()?, r.f64().ok()?, r.f64().ok()?];
                    // `teleport_entity`: три скорости `Vec3`.
                    for _ in 0..3 {
                        r.f64().ok()?;
                    }
                    Some((pos, r.f32().ok()?, r.f32().ok()?))
                })();
                if let Some((pos, yaw, pitch)) = parsed {
                    self.teleports.push((pos, yaw, pitch));
                }
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
        self.send_cmd(line).await;
        self.pump(250).await;
    }

    async fn send_cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
    }

    async fn data(&mut self, query: &str) -> String {
        self.drain();
        self.chat.clear();
        self.pump(100).await;
        self.cmd(&format!("data get entity {query}")).await;
        self.chat.clone()
    }

    /// Ждёт пакет `teleport_entity` (боты кодируют его как `kind = -1`).
    async fn wait_teleport(&mut self, at: [f64; 3]) -> bool {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout_at(deadline, self.c.next_event()).await {
                Ok(Ok(Event::EntitySpawned { kind: -1, pos, .. })) => {
                    self.drain();
                    if (0..3).all(|i| (pos[i] - at[i]).abs() < 0.01) {
                        return true;
                    }
                }
                Ok(Ok(_)) => {}
                _ => break,
            }
        }
        false
    }

    async fn spawn(&mut self, name: &str, kind: i32, at: [f64; 3]) -> i32 {
        self.send_cmd(&format!(
            "summon minecraft:{name} {} {} {}",
            at[0], at[1], at[2]
        ))
        .await;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout_at(deadline, self.c.next_event()).await {
                Ok(Ok(Event::EntitySpawned {
                    entity_id,
                    kind: k,
                    pos,
                })) if k == kind => {
                    let d = (0..3).map(|i| (pos[i] - at[i]).powi(2)).sum::<f64>();
                    if d <= 2.25 {
                        self.drain();
                        return entity_id;
                    }
                }
                Ok(Ok(_)) => {}
                _ => break,
            }
        }
        panic!("моб {name} не появился у {at:?}");
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

fn near(a: &[f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < 0.01)
}

/// `[90f,20f]` из вывода `data get … Rotation`.
fn parse_rotation(text: &str) -> Option<(f32, f32)> {
    let start = text.find('[')?;
    let end = text[start..].find(']')? + start;
    let mut it = text[start + 1..end].split(',');
    let yaw = it.next()?.trim().trim_end_matches('f').parse().ok()?;
    let pitch = it.next()?.trim().trim_end_matches('f').parse().ok()?;
    Some((yaw, pitch))
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
    let reg: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&a[3]).unwrap()).unwrap();
    let ty = |n: &str| {
        reg["minecraft:entity_type"]["entries"][n]["protocol_id"]
            .as_i64()
            .unwrap() as i32
    };
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
    let cb_teleport = cb("teleport_entity");
    c.capture_also = vec![cb_chat, cb_teleport];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_move_pos: sb("move_player_pos"),
        sb_tick_end: sb("client_tick_end"),
        cb_chat,
        cb_teleport,
        chat: String::new(),
        teleports: Vec::new(),
    };
    let (bx, bz) = (spawn[0].floor() as i32 + 20, spawn[2].floor() as i32 + 20);
    let by = -60;
    let skeleton = ty("minecraft:skeleton");

    p.cmd("gamemode creative").await;
    p.cmd(&format!(
        "fill {} {} {} {} {} {} minecraft:air",
        bx - 2,
        by,
        bz - 2,
        bx + 24,
        by + 2,
        bz + 12
    ))
    .await;
    p.cmd(&format!(
        "tp @s {} {} {}",
        bx as f64 + 0.5,
        by as f64,
        bz as f64 + 0.5
    ))
    .await;
    p.move_ground([bx as f64 + 0.5, by as f64, bz as f64 + 0.5])
        .await;
    p.cmd("kill @e[type=minecraft:skeleton]").await;
    p.pump(1300).await;

    // Один моб: позиция и явный поворот.
    let a_pos = [bx as f64 + 2.5, by as f64, bz as f64 + 0.5];
    let b_pos = [bx as f64 + 8.5, by as f64, bz as f64 + 0.5];
    p.spawn("skeleton", skeleton, a_pos).await;
    p.teleports.clear();
    p.send_cmd(&format!(
        "tp @e[type=minecraft:skeleton,limit=1] {} {} {} 90 20",
        b_pos[0], b_pos[1], b_pos[2]
    ))
    .await;
    assert!(
        p.wait_teleport(b_pos).await,
        "клиент не получил teleport_entity для моба"
    );
    assert!(
        p.teleports
            .iter()
            .any(|&(pos, yaw, pitch)| near(&pos, b_pos)
                && (yaw - 90.0).abs() < 0.01
                && (pitch - 20.0).abs() < 0.01),
        "в teleport_entity не ушли явные углы: {:?}",
        p.teleports
    );
    let pos = p
        .data("@e[type=minecraft:skeleton,limit=1,sort=nearest] Pos")
        .await;
    assert!(
        pos.contains(&format!("{}", b_pos[0])) && pos.contains(&format!("{}", b_pos[2])),
        "позиция моба не изменилась: {pos}"
    );

    // Без углов поворот сохраняется, а не сбрасывается в ноль.
    let before = p
        .data("@e[type=minecraft:skeleton,limit=1,sort=nearest] Rotation")
        .await;
    let (yaw0, pitch0) =
        parse_rotation(&before).unwrap_or_else(|| panic!("нет Rotation: {before}"));
    let c_pos = [bx as f64 + 12.5, by as f64, bz as f64 + 0.5];
    p.teleports.clear();
    p.send_cmd(&format!(
        "tp @e[type=minecraft:skeleton,limit=1] {} {} {}",
        c_pos[0], c_pos[1], c_pos[2]
    ))
    .await;
    assert!(p.wait_teleport(c_pos).await, "нет второго teleport_entity");
    assert!(
        p.teleports
            .iter()
            .any(|&(pos, yaw, pitch)| near(&pos, c_pos)
                && (yaw - yaw0).abs() < 5.0
                && (pitch - pitch0).abs() < 5.0),
        "без углов ушёл не текущий поворот ({yaw0} {pitch0}): {:?}",
        p.teleports
    );

    // Несколько целей: пакет каждой.
    let d_pos = [bx as f64 + 16.5, by as f64, bz as f64 + 6.5];
    p.spawn(
        "skeleton",
        skeleton,
        [bx as f64 + 2.5, by as f64, bz as f64 + 6.5],
    )
    .await;
    p.send_cmd(&format!(
        "tp @e[type=minecraft:skeleton] {} {} {} 0 0",
        d_pos[0], d_pos[1], d_pos[2]
    ))
    .await;
    let mut seen = 0;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while seen < 2 && tokio::time::Instant::now() < deadline {
        match tokio::time::timeout_at(deadline, p.c.next_event()).await {
            Ok(Ok(Event::EntitySpawned { kind: -1, pos, .. }))
                if (0..3).all(|i| (pos[i] - d_pos[i]).abs() < 0.01) =>
            {
                seen += 1
            }
            Ok(Ok(_)) => {}
            _ => break,
        }
    }
    p.drain();
    assert_eq!(seen, 2, "не все цели получили teleport_entity");

    // `/execute as <моб> run tp @s`: исполнитель — сам моб.
    let e_pos = [bx as f64 + 20.5, by as f64, bz as f64 + 6.5];
    p.send_cmd(&format!(
        "execute as @e[type=minecraft:skeleton,limit=1] run tp @s {} {} {}",
        e_pos[0], e_pos[1], e_pos[2]
    ))
    .await;
    assert!(
        p.wait_teleport(e_pos).await,
        "execute as моб не телепортировал @s"
    );

    p.cmd("kill @e[type=minecraft:skeleton]").await;
    println!("teleport: ok");
}
