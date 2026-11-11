//! Копья: `piercing_weapon` (удар `STAB` на дальности копья),
//! `kinetic_weapon` (урон по скорости при удержании) и `attack_range`.
//!
//! `spear_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::play::STAB;
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_attack: i32,
    sb_use_item: i32,
    sb_player_action: i32,
    sb_move_pos: i32,
    sb_tick_end: i32,
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

    async fn data_health_of(&mut self, name: &str) -> f64 {
        for _ in 0..4 {
            self.pump(200).await;
            self.chat.clear();
            self.cmd(&format!(
                "data get entity @e[type=minecraft:{name},limit=1,sort=nearest] Health"
            ))
            .await;
            if let Some(v) = last_number(&self.chat) {
                return v;
            }
        }
        panic!("нет Health в ответе: {}", self.chat);
    }

    async fn alive(&mut self, name: &str) -> bool {
        self.chat.clear();
        self.send_cmd(&format!(
            "execute if entity @e[type=minecraft:{name},limit=1] run say alive"
        ))
        .await;
        self.pump(300).await;
        self.chat.contains("alive")
    }

    async fn spawn(&mut self, name: &str, kind: i32, at: [f64; 3]) -> i32 {
        self.send_cmd(&format!(
            "summon minecraft:{name} {} {} {}",
            at[0], at[1], at[2]
        ))
        .await;
        let deadline = tokio::time::Instant::now() + Duration::from_millis(2000);
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
        self.drain();
        panic!("моб {name} не появился у {at:?}");
    }

    async fn clear(&mut self, name: &str) {
        self.cmd(&format!("kill @e[type=minecraft:{name}]")).await;
        self.pump(1300).await;
    }

    async fn attack(&mut self, entity: i32) {
        let mut w = self.packet(self.sb_attack);
        w.var_int(entity);
        self.c.send_raw(&w.buf).await.unwrap();
    }

    /// `ServerboundPlayerActionPacket` `STAB` (копьё).
    async fn stab(&mut self) {
        self.seq += 1;
        let mut w = self.packet(self.sb_player_action);
        w.var_int(STAB).i64(0).u8(0).var_int(self.seq);
        self.c.send_raw(&w.buf).await.unwrap();
    }

    /// `use_item` по воздуху с поворотом: начало удержания копья.
    async fn use_air(&mut self, yaw: f32, pitch: f32) {
        self.seq += 1;
        let mut w = self.packet(self.sb_use_item);
        w.var_int(0).var_int(self.seq).f32(yaw).f32(pitch);
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

    /// Шаг вперёд-назад: серверный тик видит скорость ~7 блоков/с.
    async fn shove(&mut self, from: [f64; 3], to: [f64; 3]) {
        self.move_ground(to).await;
        self.pump(70).await;
        self.move_ground(from).await;
        self.pump(70).await;
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
        sb_use_item: sb("use_item"),
        sb_player_action: sb("player_action"),
        sb_move_pos: sb("move_player_pos"),
        sb_tick_end: sb("client_tick_end"),
        cb_chat,
        chat: String::new(),
        seq: 0,
    };
    let (x, z) = (spawn[0].floor() as i32, spawn[2].floor() as i32);
    let (bx, bz) = (x + 40, z + 40);
    let by = -60;
    let spider = ty("minecraft:spider");

    p.cmd("gamemode survival").await;
    p.cmd("difficulty easy").await;
    p.cmd("time set day").await;
    p.cmd("clear @s").await;
    p.cmd("effect give @s minecraft:resistance 60 4 true").await;
    p.cmd(&format!(
        "fill {} {} {} {} {} {} minecraft:air",
        bx - 2,
        by,
        bz - 2,
        bx + 2,
        by + 3,
        bz + 8
    ))
    .await;
    p.cmd(&format!(
        "tp @s {} {} {} 0 15",
        bx as f64 + 0.5,
        by as f64,
        bz as f64 + 0.5
    ))
    .await;
    let base = [bx as f64 + 0.5, by as f64, bz as f64 + 0.5];
    p.move_ground(base).await;
    p.clear("cow").await;
    p.clear("spider").await;
    p.cmd("item replace entity @s hotbar.0 with minecraft:iron_spear 1")
        .await;
    let mut slot = Writer::new();
    slot.var_int(sb("set_carried_item")).u16(0);
    p.c.send_raw(&slot.buf).await.unwrap();
    p.pump(900).await;

    // 1. `PiercingWeapon`: удар `STAB` достаёт дальше обычных 3 блоков.
    let far = [base[0], base[1], base[2] + 3.5];
    p.spawn("cow", ty("minecraft:cow"), far).await;
    p.pump(150).await;
    p.stab().await;
    p.pump(400).await;
    let h = p.data_health_of("cow").await;
    assert!(
        (6.5..=7.5).contains(&h),
        "копьё должно снять 3 HP (ATTACK_DAMAGE 2 + база), осталось {h}"
    );

    // 2. За пределами `attack_range` (4.5 + 0.125) удар не проходит.
    p.clear("cow").await;
    let too_far = [base[0], base[1], base[2] + 6.0];
    p.spawn("cow", ty("minecraft:cow"), too_far).await;
    p.pump(150).await;
    p.stab().await;
    p.pump(400).await;
    let h = p.data_health_of("cow").await;
    assert!(
        (h - 10.0).abs() < 0.1,
        "удар с 6 блоков не должен попадать: {h}"
    );

    // 3. Обычный удар (`attack`) мечом с 4.2 блоков тоже мимо: у меча 3.0.
    p.clear("cow").await;
    p.cmd("item replace entity @s hotbar.0 with minecraft:iron_sword 1")
        .await;
    let mut slot = Writer::new();
    slot.var_int(sb("set_carried_item")).u16(0);
    p.c.send_raw(&slot.buf).await.unwrap();
    p.pump(900).await;
    let sword_far = [base[0], base[1], base[2] + 4.2];
    let cow = p.spawn("cow", ty("minecraft:cow"), sword_far).await;
    p.pump(150).await;
    p.attack(cow).await;
    p.pump(400).await;
    let h = p.data_health_of("cow").await;
    assert!((h - 10.0).abs() < 0.1, "мечом с 4.2 блоков не достать: {h}");

    // 4. `KineticWeapon`: удержание копья на бегу бьёт по скорости.
    p.clear("cow").await;
    p.cmd("item replace entity @s hotbar.0 with minecraft:iron_spear 1")
        .await;
    let mut slot = Writer::new();
    slot.var_int(sb("set_carried_item")).u16(0);
    p.c.send_raw(&slot.buf).await.unwrap();
    p.pump(900).await;
    p.spawn(
        "cow",
        ty("minecraft:cow"),
        [base[0], base[1], base[2] + 3.5],
    )
    .await;
    p.pump(150).await;
    p.use_air(0.0, 15.0).await;
    p.pump(100).await;
    let ahead = [base[0], base[1], base[2] + 0.35];
    for _ in 0..16 {
        p.shove(base, ahead).await;
    }
    p.pump(300).await;
    assert!(
        !p.alive("cow").await || p.data_health_of("cow").await < 10.0,
        "удержание копья на бегу не нанесло урона"
    );
    let _ = spider;
    p.clear("cow").await;
    println!("spear: ok");
}
