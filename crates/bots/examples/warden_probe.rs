//! Скалк-визгун и страж: 4 предупреждения от вибраций у визгуна с
//! `can_summon=true` поднимают стража (`minecraft:warden`).
//!
//! `warden_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

const X: i32 = 2624;
const Z: i32 = 8;
const Y: i32 = 100;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_select: i32,
    sb_use_on: i32,
    cb_add: i32,
    cb_chat: i32,
    warden_type: i32,
    warden_seen: bool,
    seq: i32,
    chat: String,
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

impl Probe {
    fn packet(&self, id: i32) -> Writer {
        let mut w = Writer::new();
        w.var_int(id);
        w
    }

    fn drain(&mut self) {
        for (id, body) in std::mem::take(&mut self.c.captured_by) {
            if id == self.cb_add {
                let mut r = Reader::new(&body);
                let Ok(_entity) = r.var_int() else { continue };
                if r.bytes(16).is_err() {
                    continue;
                }
                let Ok(ty) = r.var_int() else { continue };
                if ty == self.warden_type {
                    self.warden_seen = true;
                }
            } else if id == self.cb_chat {
                let mut r = Reader::new(&body);
                if let Ok(component) = text::read_component(&mut r) {
                    self.chat.push_str(&flatten(&component));
                    self.chat.push('\n');
                }
            }
        }
    }

    async fn pump(&mut self, ms: u64) {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
        loop {
            let ev = match tokio::time::timeout_at(deadline, self.c.next_event()).await {
                Ok(Ok(ev)) => ev,
                Ok(Err(e)) => {
                    eprintln!("event error: {e}");
                    break;
                }
                Err(_) => break,
            };
            let _ = ev;
        }
        self.drain();
    }

    async fn cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(180).await;
    }

    async fn select(&mut self, slot: u16) {
        let mut w = self.packet(self.sb_select);
        w.u16(slot);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(100).await;
    }

    async fn use_on(&mut self, pos: [i32; 3]) {
        self.seq += 1;
        let mut w = self.packet(self.sb_use_on);
        w.var_int(0)
            .i64(chunkwake_protocol::play::pack_block_pos(pos))
            .var_int(1)
            .f32(0.5)
            .f32(0.5)
            .f32(0.5)
            .u8(0)
            .u8(0)
            .var_int(self.seq);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(250).await;
    }

    async fn shrieking(&mut self) -> bool {
        self.chat.clear();
        self.cmd(&format!(
            "execute if block {X} {Y} {Z} minecraft:sculk_shrieker[shrieking=true] run say SHRIEK"
        ))
        .await;
        self.chat.contains("SHRIEK")
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let reg: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::path::Path::new(&a[2]).with_file_name("registries.json"))
            .unwrap(),
    )
    .unwrap();
    let warden_type = reg["minecraft:entity_type"]["entries"]["minecraft:warden"]["protocol_id"]
        .as_i64()
        .unwrap() as i32;
    let ids = Ids::load(&table).unwrap();
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n| table.id(State::Play, Direction::Clientbound, n).unwrap();
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    let cb_add = cb("add_entity");
    let cb_chat = cb("system_chat");
    c.capture_also = vec![cb_add, cb_chat];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_select: sb("set_carried_item"),
        sb_use_on: sb("use_item_on"),
        cb_add,
        cb_chat,
        warden_type,
        warden_seen: false,
        seq: 0,
        chat: String::new(),
    };
    p.pump(2000).await;

    for line in [
        "difficulty easy".to_string(),
        "gamemode creative".to_string(),
        "effect clear @s".to_string(),
        "kill @e[type=!minecraft:player,distance=..96]".to_string(),
        format!("execute in minecraft:overworld run tp @s {X}.5 {Y} {Z}.5 180 20"),
        format!(
            "fill {} 99 {} {} 99 {} minecraft:stone",
            X - 4,
            Z - 4,
            X + 10,
            Z + 4
        ),
        format!(
            "fill {} {Y} {} {} 110 {} minecraft:air",
            X - 4,
            Z - 4,
            X + 10,
            Z + 4
        ),
        format!(
            "setblock {X} {Y} {Z} minecraft:sculk_shrieker[can_summon=true,shrieking=false,waterlogged=false]"
        ),
        "item replace entity @s hotbar.0 with minecraft:stone 64".to_string(),
    ] {
        p.cmd(&line).await;
    }
    p.select(0).await;
    p.pump(600).await;
    p.c.confirm_loaded().await.unwrap();
    p.pump(300).await;

    for i in 0..4 {
        // Каждый поставленный блок даёт вибрацию block_place у визгуна.
        p.c.confirm_loaded().await.unwrap();
        p.pump(150).await;
        p.use_on([X + 4, 99, Z + i]).await;
        p.pump(600).await;
        if i == 0 {
            assert!(p.shrieking().await, "визгун не перешёл в shrieking");
        }
        // Ждём окончания 90-тикового крика перед следующей вибрацией.
        p.pump(5000).await;
    }
    for _ in 0..20 {
        if p.warden_seen {
            break;
        }
        p.pump(200).await;
    }
    assert!(p.warden_seen, "страж не появился после 4 предупреждений");
    p.chat.clear();
    p.cmd("execute if entity @e[type=minecraft:warden] run say WARDEN_ALIVE")
        .await;
    assert!(p.chat.contains("WARDEN_ALIVE"), "страж не найден");
    println!("warden_probe: ok");
}
