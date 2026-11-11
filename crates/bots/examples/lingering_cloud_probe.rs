//! Оседающее зелье: `AreaEffectCloud`, эффекты и удаление по сроку.
//!
//! `lingering_cloud_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_use_item: i32,
    sb_set_carried: i32,
    cb_chat: i32,
    chat: String,
    seq: i32,
}

impl Probe {
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
        while let Ok(Ok(_)) = tokio::time::timeout_at(deadline, self.c.next_event()).await {}
        self.drain();
    }

    async fn cmd(&mut self, line: &str) {
        self.send_cmd(line).await;
        self.pump(300).await;
    }

    async fn send_cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
    }

    async fn data(&mut self, query: &str) -> String {
        self.drain();
        self.chat.clear();
        self.pump(120).await;
        self.cmd(&format!("data get entity {query}")).await;
        self.chat.clone()
    }

    async fn spawn_cow(&mut self, at: [f64; 3]) {
        self.send_cmd(&format!(
            "summon minecraft:cow {} {} {}",
            at[0], at[1], at[2]
        ))
        .await;
        let deadline = tokio::time::Instant::now() + Duration::from_millis(2000);
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout_at(deadline, self.c.next_event()).await {
                // Старые коровы приходят отдельными событиями при загрузке
                // чанков — ждём именно призванную рядом с точкой.
                Ok(Ok(Event::EntitySpawned { kind: 30, pos, .. })) => {
                    let d = (0..3).map(|i| (pos[i] - at[i]).powi(2)).sum::<f64>();
                    if d <= 2.25 {
                        break;
                    }
                }
                Ok(Ok(_)) => {}
                _ => break,
            }
        }
        self.drain();
    }

    /// Корова уходит с места призыва: ставим её под облако перед срабатыванием.
    async fn tp_cow(&mut self, at: [f64; 3]) {
        self.cmd(&format!(
            "tp @e[type=minecraft:cow,limit=1,sort=nearest] {} {} {}",
            at[0], at[1], at[2]
        ))
        .await;
    }

    async fn clear_cows(&mut self) {
        self.cmd("kill @e[type=minecraft:cow]").await;
        self.pump(1300).await;
    }

    async fn hold_first(&mut self) {
        let mut s = self.packet(self.sb_set_carried);
        s.u16(0);
        self.c.send_raw(&s.buf).await.unwrap();
        self.pump(100).await;
    }

    async fn throw(&mut self, yaw: f32, pitch: f32) {
        self.seq += 1;
        let mut w = self.packet(self.sb_use_item);
        w.var_int(0).var_int(self.seq).f32(yaw).f32(pitch);
        self.c.send_raw(&w.buf).await.unwrap();
    }

    async fn wait_cloud(&mut self, kind: i32) -> i32 {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            match tokio::time::timeout_at(deadline, self.c.next_event()).await {
                Ok(Ok(Event::EntitySpawned {
                    entity_id, kind: k, ..
                })) if k == kind => {
                    self.drain();
                    return entity_id;
                }
                Ok(Ok(_)) => {}
                _ => panic!("облако {kind} не появилось"),
            }
        }
    }

    async fn wait_removed(&mut self, id: i32, ms: u64) -> bool {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout_at(deadline, self.c.next_event()).await {
                Ok(Ok(Event::EntityRemoved(ids))) if ids.contains(&id) => return true,
                Ok(Ok(_)) => {}
                _ => break,
            }
        }
        false
    }
}

/// Плоский текст компонента: `text`, строки и `with`.
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
    c.capture_also = vec![cb_chat];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_use_item: sb("use_item"),
        sb_set_carried: sb("set_carried_item"),
        cb_chat,
        chat: String::new(),
        seq: 0,
    };
    let (x, z) = (spawn[0].floor() as i32, spawn[2].floor() as i32);
    let (bx, bz) = (x + 40, z + 40);
    let by = -60;
    let cow = [bx as f64 + 2.5, by as f64, bz as f64 + 0.5];
    let cloud_type = ty("minecraft:area_effect_cloud");

    p.cmd("gamemode creative").await;
    p.cmd("difficulty easy").await;
    p.cmd("time set day").await;
    p.cmd("clear @s").await;
    p.cmd(&format!(
        "fill {} {} {} {} {} {} minecraft:air",
        bx - 1,
        by,
        bz - 1,
        bx + 6,
        by + 2,
        bz + 1
    ))
    .await;
    p.cmd(&format!(
        "tp @s {} {} {}",
        bx as f64 + 0.5,
        by as f64,
        bz as f64 + 0.5
    ))
    .await;
    let mut w = Writer::new();
    w.var_int(sb("move_player_pos"));
    for v in [bx as f64 + 0.5, by as f64, bz as f64 + 0.5] {
        w.f64(v);
    }
    w.u8(1);
    p.c.send_raw(&w.buf).await.unwrap();
    p.clear_cows().await;

    p.spawn_cow(cow).await;
    p.pump(200).await;
    p.cmd(
        "item replace entity @s hotbar.0 with minecraft:lingering_potion[minecraft:potion_contents={potion:\"minecraft:poison\"}] 1",
    )
    .await;
    p.hold_first().await;
    p.throw(-90.0, 75.0).await;
    let cloud = p.wait_cloud(cloud_type).await;
    p.tp_cow(cow).await;

    // 10 тиков ожидания, затем яд обязан появиться на корове.
    p.pump(900).await;
    let effects = p
        .data("@e[type=minecraft:cow,limit=1,sort=nearest] active_effects")
        .await;
    assert!(
        effects.contains("minecraft:poison"),
        "облако не наложило яд: {effects}"
    );
    let h1 = last_number(
        &p.data("@e[type=minecraft:cow,limit=1,sort=nearest] Health")
            .await,
    )
    .expect("нет Health");
    p.pump(2000).await;
    let h2 = last_number(
        &p.data("@e[type=minecraft:cow,limit=1,sort=nearest] Health")
            .await,
    )
    .expect("нет Health");
    assert!(h2 < h1, "яд из облака не тикает: было {h1}, стало {h2}");

    // 600 тиков после 10 ожидания: облако удаляется, яд не добивает корову.
    assert!(
        p.wait_removed(cloud, 31_000).await,
        "облако не удалилось по сроку"
    );
    let h3 = last_number(
        &p.data("@e[type=minecraft:cow,limit=1,sort=nearest] Health")
            .await,
    )
    .expect("нет Health");
    assert!(h3 > 0.0, "яд не должен добивать корову, осталось {h3}");

    println!("lingering_cloud: ok");
}
