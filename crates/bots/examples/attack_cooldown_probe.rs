//! Перезарядка удара игрока и условия крита.
//!
//! `attack_cooldown_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_attack: i32,
    sb_move_pos: i32,
    sb_tick_end: i32,
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

    async fn send_cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
    }

    /// Здоровье ближайшего паука: в мире после `clear` он один.
    async fn data_health(&mut self) -> f64 {
        self.data_health_of("spider").await
    }

    async fn data_health_of(&mut self, name: &str) -> f64 {
        self.pump(150).await;
        self.chat.clear();
        self.cmd(&format!(
            "data get entity @e[type=minecraft:{name},limit=1,sort=nearest] Health"
        ))
        .await;
        last_number(&self.chat).expect("нет Health в ответе")
    }

    async fn spawn(&mut self, name: &str, kind: i32, at: [f64; 3]) -> i32 {
        self.send_cmd(&format!(
            "summon minecraft:{name} {} {} {}",
            at[0], at[1], at[2]
        ))
        .await;
        let deadline = tokio::time::Instant::now() + Duration::from_millis(800);
        let mut best: Option<(i32, f64)> = None;
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout_at(deadline, self.c.next_event()).await {
                Ok(Ok(Event::EntitySpawned {
                    entity_id,
                    kind: k,
                    pos,
                })) if k == kind => {
                    let d = (0..3).map(|i| (pos[i] - at[i]).powi(2)).sum::<f64>();
                    if best.is_none_or(|(_, b)| d < b) {
                        best = Some((entity_id, d));
                    }
                }
                Ok(Ok(_)) => {}
                _ => break,
            }
        }
        self.drain();
        best.expect("моб не появился").0
    }

    /// `/kill` оставляет тушу на 20 тиков — ждём, чтобы `@e` её не увидел.
    async fn clear(&mut self, name: &str) {
        self.cmd(&format!("kill @e[type=minecraft:{name}]")).await;
        self.pump(1300).await;
    }

    /// Клетка 1×2 вокруг блока: цель не убегает от паники и не выходит
    /// из радиуса удара между проверками.
    async fn cage(&mut self, c: [i32; 3]) {
        for dy in 0..2 {
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                self.cmd(&format!(
                    "setblock {} {} {} minecraft:stone",
                    c[0] + dx,
                    c[1] + dy,
                    c[2] + dz
                ))
                .await;
            }
        }
    }

    async fn attack(&mut self, entity: i32) {
        let mut w = self.packet(self.sb_attack);
        w.var_int(entity);
        self.c.send_raw(&w.buf).await.unwrap();
    }

    /// Движение клиента без опоры: сервер копит высоту падения.
    async fn move_down(&mut self, pos: [f64; 3]) {
        let mut w = self.packet(self.sb_move_pos);
        for v in pos {
            w.f64(v);
        }
        w.u8(0);
        self.c.send_raw(&w.buf).await.unwrap();
        let end = self.packet(self.sb_tick_end);
        self.c.send_raw(&end.buf).await.unwrap();
    }

    /// Клиент стоит на земле: `/tp` сам ставит `on_ground = false`.
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
        sb_attack: sb("attack"),
        sb_move_pos: sb("move_player_pos"),
        sb_tick_end: sb("client_tick_end"),
        cb_chat,
        chat: String::new(),
    };
    let (x, z) = (spawn[0].floor() as i32, spawn[2].floor() as i32);
    // Пол ровного мира: bedrock, 2×dirt, трава — верх на -60.
    let (bx, bz) = (x + 40, z + 40);
    let by = -60;
    let spot = [bx as f64 + 2.5, by as f64, bz as f64 + 0.5];
    let spider = ty("minecraft:spider");

    p.cmd("gamemode creative").await;
    p.cmd("difficulty easy").await;
    p.cmd("time set day").await;
    p.cmd("effect give @s minecraft:resistance 30 4 true").await;
    p.cmd("clear @s").await;
    // Своя площадка: прошлые прогоны оставляли камни и туши.
    p.cmd(&format!(
        "fill {} {} {} {} {} {} minecraft:air",
        bx - 1,
        by,
        bz - 1,
        bx + 7,
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
    p.move_ground([bx as f64 + 0.5, by as f64, bz as f64 + 0.5])
        .await;
    p.clear("spider").await;
    p.clear("cow").await;
    p.cage([bx + 2, by, bz]).await;
    // Топор: 7 урона, скорость 0.8 (25 тиков до полного заряда).
    p.cmd("item replace entity @s hotbar.0 with minecraft:wooden_axe 1")
        .await;
    let mut slot = Writer::new();
    slot.var_int(sb("set_carried_item")).u16(0);
    p.c.send_raw(&slot.buf).await.unwrap();
    p.pump(1400).await;

    // Первый удар полный (7), через 12 тиков (заряд ~0.5) — ещё ~2.8.
    let s_id = p.spawn("spider", spider, spot).await;
    p.pump(200).await;
    p.attack(s_id).await;
    p.pump(750).await;
    p.attack(s_id).await;
    p.pump(300).await;
    let h1 = p.data_health().await;
    let dealt = 16.0 - h1;
    assert!(
        (8.5..=11.5).contains(&dealt),
        "полный удар + неполный должны снять ~10 HP, снято {dealt}"
    );

    // Крит и разящий удар: открытая площадка, клетку убираем.
    p.cmd(&format!(
        "fill {} {} {} {} {} {} minecraft:air",
        bx + 1,
        by,
        bz - 1,
        bx + 3,
        by + 1,
        bz + 1
    ))
    .await;
    p.clear("spider").await;
    p.clear("cow").await;
    // Меч в тегах `minecraft:swords`: при полном заряде бьёт по площади.
    p.cmd("item replace entity @s hotbar.0 with minecraft:iron_sword 1")
        .await;
    let mut slot = Writer::new();
    slot.var_int(sb("set_carried_item")).u16(0);
    p.c.send_raw(&slot.buf).await.unwrap();
    p.pump(900).await;

    // Цель — паук, рядом корова: разящий удар задевает и её (1 × заряд).
    let s2 = p
        .spawn(
            "spider",
            spider,
            [bx as f64 + 3.5, by as f64, bz as f64 + 0.5],
        )
        .await;
    p.spawn(
        "cow",
        ty("minecraft:cow"),
        [bx as f64 + 2.5, by as f64, bz as f64 + 0.5],
    )
    .await;
    p.pump(100).await;
    p.attack(s2).await;
    p.pump(300).await;
    let cow_h = p.data_health_of("cow").await;
    let spider_h = p.data_health().await;
    assert!(
        (8.5..=9.5).contains(&cow_h),
        "разящий удар должен снять ~1 HP с коровы, осталось {cow_h}"
    );
    assert!(
        (9.3..=10.7).contains(&spider_h),
        "цель должна получить ~6 HP, осталось {spider_h}"
    );

    // Крит: падение с высоты, удар в воздухе — 9 урона.
    p.clear("spider").await;
    p.clear("cow").await;
    let s3 = p.spawn("spider", spider, spot).await;
    p.pump(900).await;
    p.cmd(&format!("tp @s {} {} {}", spot[0], spot[1] + 4.0, spot[2]))
        .await;
    p.pump(200).await;
    for dy in [3.0, 2.0, 1.5] {
        p.move_down([spot[0], spot[1] + dy, spot[2]]).await;
        p.pump(80).await;
    }
    p.attack(s3).await;
    p.pump(300).await;
    let h3 = p.data_health().await;
    let crit = 16.0 - h3;
    assert!(
        (crit - 9.0).abs() < 1.0,
        "крит в падении должен снять ~9 HP, снято {crit}"
    );

    println!("attack_cooldown: ok");
}
