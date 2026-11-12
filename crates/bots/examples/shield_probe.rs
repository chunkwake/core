//! Щит и тотем: блокировка урона с износом и спасение от смерти.
//!
//! `shield_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::play::RELEASE_USE_ITEM;
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_use_item: i32,
    sb_player_action: i32,
    sb_move_pos: i32,
    sb_tick_end: i32,
    cb_chat: i32,
    cb_effect: i32,
    chat: String,
    seq: i32,
    /// ID эффектов из `update_mob_effect`.
    effects: Vec<i32>,
}

impl Probe {
    fn packet(&self, id: i32) -> Writer {
        let mut w = Writer::new();
        w.var_int(id);
        w
    }

    fn drain(&mut self) {
        for (id, body) in std::mem::take(&mut self.c.captured_by) {
            if id == self.cb_effect {
                let mut r = Reader::new(&body);
                if r.var_int().is_ok()
                    && let Ok(effect) = r.var_int()
                {
                    self.effects.push(effect);
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

    async fn pump(&mut self, ms: u64) {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
        while let Ok(Ok(_)) = tokio::time::timeout_at(deadline, self.c.next_event()).await {}
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

    async fn health(&mut self) -> f64 {
        let text = self.data("@s Health").await;
        last_number(&text).unwrap_or_else(|| panic!("нет Health: {text}"))
    }

    async fn use_air(&mut self, yaw: f32, pitch: f32) {
        self.seq += 1;
        let mut w = self.packet(self.sb_use_item);
        w.var_int(0).var_int(self.seq).f32(yaw).f32(pitch);
        self.c.send_raw(&w.buf).await.unwrap();
    }

    async fn release(&mut self) {
        self.seq += 1;
        let mut w = self.packet(self.sb_player_action);
        w.var_int(RELEASE_USE_ITEM).i64(0).u8(0).var_int(self.seq);
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

    async fn damage_at(&mut self, amount: f32, cause: &str, at: [f64; 3]) -> String {
        self.chat.clear();
        self.send_cmd(&format!(
            "damage @s {amount} minecraft:{cause} at {} {} {}",
            at[0], at[1], at[2]
        ))
        .await;
        self.pump(400).await;
        self.chat.clone()
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
    let effect_id = |n: &str| {
        reg["minecraft:mob_effect"]["entries"][n]["protocol_id"]
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
    let cb_effect = cb("update_mob_effect");
    c.capture_also = vec![cb_chat, cb_effect];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_use_item: sb("use_item"),
        sb_player_action: sb("player_action"),
        sb_move_pos: sb("move_player_pos"),
        sb_tick_end: sb("client_tick_end"),
        cb_chat,
        cb_effect,
        chat: String::new(),
        seq: 0,
        effects: Vec::new(),
    };
    let (x, z) = (spawn[0].floor() as i32, spawn[2].floor() as i32);
    let (bx, bz) = (x + 50, z + 50);
    let by = -60;
    let base = [bx as f64 + 0.5, by as f64, bz as f64 + 0.5];

    p.cmd("gamemode survival").await;
    p.cmd("difficulty easy").await;
    p.cmd("clear @s").await;
    p.cmd("effect clear @s").await;
    p.cmd(&format!(
        "fill {} {} {} {} {} {} minecraft:air",
        bx - 1,
        by,
        bz - 2,
        bx + 1,
        by + 3,
        bz + 4
    ))
    .await;
    p.cmd(&format!("tp @s {} {} {} 0 0", base[0], base[1], base[2]))
        .await;
    p.move_ground(base).await;
    p.cmd("data merge entity @s {Health:20.0f}").await;
    p.pump(300).await;

    // 1. Поднятый щит полностью блокирует удар спереди и теряет прочность.
    p.cmd("item replace entity @s hotbar.0 with minecraft:shield 1")
        .await;
    let mut slot = Writer::new();
    slot.var_int(sb("set_carried_item")).u16(0);
    p.c.send_raw(&slot.buf).await.unwrap();
    p.pump(900).await;
    p.use_air(0.0, 0.0).await;
    p.pump(500).await;
    let front = [base[0], base[1] + 1.0, base[2] + 3.0];
    let reply = p.damage_at(6.0, "arrow", front).await;
    assert!(!reply.contains("error"), "damage не выполнился: {reply}");
    let h = p.health().await;
    assert!((h - 20.0).abs() < 0.1, "щит не заблокировал удар: {h}");
    let inv = p.data("@s Inventory").await;
    assert!(
        inv.contains("minecraft:damage:7"),
        "щит должен потерять 7 прочности: {inv}"
    );

    // 2. Сзади щит не работает.
    let back = [base[0], base[1] + 1.0, base[2] - 3.0];
    p.damage_at(6.0, "arrow", back).await;
    let h = p.health().await;
    // Регенерация от полной сытости добавляет ~0.17 HP за тик.
    assert!(
        (13.5..=14.5).contains(&h),
        "удар в спину должен пройти: {h}"
    );

    // 3. После отпускания щит больше не блокирует.
    p.release().await;
    p.pump(300).await;
    p.cmd("data merge entity @s {Health:20.0f}").await;
    p.pump(200).await;
    p.damage_at(6.0, "arrow", front).await;
    let h = p.health().await;
    assert!(
        (13.5..=14.5).contains(&h),
        "опущенный щит не должен блокировать: {h}"
    );

    // 4. Тотем: смертельный урон оставляет 1 HP, даёт эффекты и исчезает.
    p.cmd("data merge entity @s {Health:20.0f}").await;
    p.cmd("item replace entity @s weapon.offhand with minecraft:totem_of_undying 1")
        .await;
    p.pump(500).await;
    p.damage_at(30.0, "generic", [base[0], base[1], base[2]])
        .await;
    let h = p.health().await;
    // Регенерация II уже успевает вылечить одно сердце.
    assert!(
        (1.0..=4.0).contains(&h),
        "тотем не спас (ожидалось 1–4 HP): {h}"
    );
    assert!(
        p.effects.contains(&effect_id("minecraft:regeneration"))
            && p.effects.contains(&effect_id("minecraft:absorption"))
            && p.effects.contains(&effect_id("minecraft:fire_resistance")),
        "тотем не выдал эффекты: {:?}",
        p.effects
    );
    let inv = p.data("@s Inventory").await;
    assert!(
        !inv.contains("totem_of_undying"),
        "тотем не израсходован: {inv}"
    );

    p.cmd("clear @s").await;
    println!("shield: ok");
}
