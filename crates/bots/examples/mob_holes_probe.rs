//! Дыры у мобов: урон от падения, молния, краска овцы и табличка с именем.
//!
//! `mob_holes_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_interact: i32,
    cb_chat: i32,
    chat: String,
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

    /// Без `pump`: событие спавна нужно поймать в [`Self::spawn`].
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

    /// Ближайший к `at` спавн нужного вида: в мире могут быть старые мобы.
    async fn spawn(&mut self, kind: i32, at: [f64; 3]) -> (i32, [f64; 3]) {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(800);
        let mut best: Option<(i32, [f64; 3], f64)> = None;
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout_at(deadline, self.c.next_event()).await {
                Ok(Ok(Event::EntitySpawned {
                    entity_id,
                    kind: k,
                    pos,
                })) if k == kind => {
                    let d = (0..3).map(|i| (pos[i] - at[i]).powi(2)).sum::<f64>();
                    if best.is_none_or(|(_, _, b)| d < b) {
                        best = Some((entity_id, pos, d));
                    }
                }
                Ok(Ok(_)) => {}
                _ => break,
            }
        }
        self.drain();
        let (id, pos, _) = best.expect("моб не появился");
        (id, pos)
    }

    async fn interact(&mut self, entity: i32) {
        let mut w = self.packet(self.sb_interact);
        w.var_int(entity).var_int(0);
        chunkwake_protocol::play::lp_vec3(&mut w, [0.0, 0.6, 0.0]);
        w.bool(false);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(400).await;
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
        .find_map(|t| t.trim_end_matches('b').parse().ok())
}

fn nbt_health(text: &str) -> Option<f64> {
    text.split("Health:")
        .nth(1)?
        .split('f')
        .next()?
        .parse()
        .ok()
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
        sb_interact: sb("interact"),
        cb_chat,
        chat: String::new(),
    };
    let (x, y, z) = (
        spawn[0].floor() as i32,
        spawn[1].floor() as i32,
        spawn[2].floor() as i32,
    );
    let cow = (x + 4, y, z);
    let creeper = (x - 4, y, z);
    let sheep = (x + 2, y, z);

    p.cmd("gamemode creative").await;
    // Не peaceful: он убирает крипера до удара молнии.
    p.cmd("difficulty easy").await;
    p.cmd("time set day").await;
    p.cmd("effect give @s minecraft:resistance 30 4 true").await;
    p.cmd("clear @s").await;

    // Падение: 8 блоков, безопасно 3 → 5 урона из 10.
    p.send_cmd(&format!(
        "summon minecraft:cow {} {} {}",
        cow.0,
        cow.1 + 8,
        cow.2
    ))
    .await;
    let (cow_id, _) = p
        .spawn(
            ty("minecraft:cow"),
            [cow.0 as f64 + 0.5, cow.1 as f64 + 8.0, cow.2 as f64 + 0.5],
        )
        .await;
    p.pump(3000).await;
    let text = p
        .data("@e[type=minecraft:cow,limit=1,sort=nearest] Health")
        .await;
    let health = last_number(&text).expect("нет Health в ответе");
    assert!(health <= 6.0, "корова не получила урон от падения: {text}");

    // Молния: крипер заряжается.
    p.send_cmd(&format!(
        "summon minecraft:creeper {} {} {}",
        creeper.0, creeper.1, creeper.2
    ))
    .await;
    p.spawn(
        ty("minecraft:creeper"),
        [
            creeper.0 as f64 + 0.5,
            creeper.1 as f64,
            creeper.2 as f64 + 0.5,
        ],
    )
    .await;
    p.cmd(&format!(
        "summon minecraft:lightning_bolt {} {} {}",
        creeper.0, creeper.1, creeper.2
    ))
    .await;
    p.pump(600).await;
    let text = p
        .data("@e[type=minecraft:creeper,limit=1,sort=nearest]")
        .await;
    assert!(
        text.contains("powered:1b"),
        "крипер не зарядился от молнии: {text}"
    );
    let health = nbt_health(&text).expect("нет Health в ответе");
    assert!(health <= 15.0, "молния не нанесла 5 урона криперу: {text}");

    // Краска: цвет овцы меняется на выбранный.
    p.send_cmd(&format!(
        "summon minecraft:sheep {} {} {}",
        sheep.0, sheep.1, sheep.2
    ))
    .await;
    let (sheep_id, _) = p
        .spawn(
            ty("minecraft:sheep"),
            [sheep.0 as f64 + 0.5, sheep.1 as f64, sheep.2 as f64 + 0.5],
        )
        .await;
    let before = p
        .data("@e[type=minecraft:sheep,limit=1,sort=nearest] Color")
        .await;
    let before = last_number(&before).unwrap_or(0.0) as u8;
    let (target, dye) = if before & 15 == 14 {
        (5, "lime_dye")
    } else {
        (14, "red_dye")
    };
    p.cmd(&format!(
        "item replace entity @s hotbar.0 with minecraft:{dye} 1"
    ))
    .await;
    let mut w = Writer::new();
    w.var_int(sb("set_carried_item")).u16(0);
    p.c.send_raw(&w.buf).await.unwrap();
    p.pump(100).await;
    p.interact(sheep_id).await;
    let text = p
        .data("@e[type=minecraft:sheep,limit=1,sort=nearest] Color")
        .await;
    assert_eq!(
        last_number(&text).unwrap_or(-1.0) as u8 & 15,
        target,
        "овца не покрасилась: {text}"
    );

    // Табличка с именем: у коровы появляется CustomName.
    p.cmd(
        "item replace entity @s hotbar.0 with minecraft:name_tag[minecraft:custom_name=\"ProbeCow\"] 1",
    )
    .await;
    let mut w = Writer::new();
    w.var_int(sb("set_carried_item")).u16(0);
    p.c.send_raw(&w.buf).await.unwrap();
    p.pump(100).await;
    p.interact(cow_id).await;
    let text = p.data("@e[type=minecraft:cow,limit=1,sort=nearest]").await;
    assert!(
        text.contains("CustomName:\"ProbeCow\""),
        "табличка с именем не применилась: {text}"
    );

    println!("mob_holes: ok");
}
