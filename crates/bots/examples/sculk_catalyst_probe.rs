//! Скалк-катализатор: смерть моба в 8 блоках расползает скалк по
//! `#minecraft:sculk_replaceable`; при выключенном `mobGriefing` — нет.
//!
//! `sculk_catalyst_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

const X: i32 = 2560;
const Z: i32 = 8;
const Y: i32 = 100;

struct Probe {
    c: Client,
    sb_command: i32,
    cb_chat: i32,
    chat: String,
}

fn flatten_into(v: &serde_json::Value, out: &mut String) {
    if let Some(s) = v.as_str() {
        out.push_str(s);
        return;
    }
    if let Some(s) = v.get("text").and_then(serde_json::Value::as_str) {
        out.push_str(s);
    }
    if let Some(args) = v.get("with").and_then(serde_json::Value::as_array) {
        for arg in args {
            flatten_into(arg, out);
            out.push(' ');
        }
    }
}

impl Probe {
    fn drain(&mut self) {
        for (id, body) in std::mem::take(&mut self.c.captured_by) {
            if id != self.cb_chat {
                continue;
            }
            let mut r = Reader::new(&body);
            if let Ok(component) = text::read_component(&mut r) {
                flatten_into(&component, &mut self.chat);
                self.chat.push('\n');
            }
        }
    }

    fn packet(&self, id: i32) -> Writer {
        let mut w = Writer::new();
        w.var_int(id);
        w
    }

    /// Есть ли скалк в `pos` (командой, без разбора section-пакетов).
    async fn is_sculk(&mut self, pos: [i32; 3]) -> bool {
        self.chat.clear();
        self.cmd(&format!(
            "execute if block {} {} {} minecraft:sculk run say SCULK_YES",
            pos[0], pos[1], pos[2]
        ))
        .await;
        self.chat.contains("SCULK_YES")
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
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb_chat = table
        .id(State::Play, Direction::Clientbound, "system_chat")
        .unwrap();
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    c.capture_also = vec![cb_chat];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        cb_chat,
        chat: String::new(),
    };
    p.pump(2000).await;

    for line in [
        "difficulty peaceful".to_string(),
        "gamemode creative".to_string(),
        "effect clear @s".to_string(),
        "kill @e[type=!minecraft:player,distance=..64]".to_string(),
        "gamerule mob_griefing true".to_string(),
        format!("execute in minecraft:overworld run tp @s {X}.5 {Y} {Z}.5 180 20"),
        format!(
            "fill {} 99 {} {} 99 {} minecraft:stone",
            X - 4,
            Z - 4,
            X + 20,
            Z + 6
        ),
        format!(
            "fill {} {Y} {} {} 110 {} minecraft:air",
            X - 4,
            Z - 4,
            X + 20,
            Z + 6
        ),
        format!("setblock {X} {Y} {Z} minecraft:sculk_catalyst"),
        format!("setblock {} {Y} {Z} minecraft:sculk_catalyst", X + 12),
    ] {
        p.cmd(&line).await;
    }

    // 1. mobGriefing=true: скалк расползается (проверяем пол под коровой).
    p.cmd(&format!(
        "summon minecraft:cow {}.5 {Y} {}.5 {{NoAI:1b}}",
        X + 2,
        Z
    ))
    .await;
    p.pump(300).await;
    p.cmd("kill @e[type=minecraft:cow,distance=..16]").await;
    p.pump(1500).await;
    let spots = [
        [X + 2, Y - 1, Z],
        [X + 1, Y - 1, Z],
        [X + 3, Y - 1, Z],
        [X + 2, Y - 1, Z - 1],
        [X + 2, Y - 1, Z + 1],
    ];
    let mut found = 0;
    for at in spots {
        if p.is_sculk(at).await {
            found += 1;
        }
    }
    assert!(found > 0, "скалк не расползся у катализатора");
    println!("sculk catalyst spread ok ({found} floor blocks)");

    // 2. mobGriefing=false: распространения нет.
    p.cmd("gamerule mob_griefing false").await;
    p.cmd(&format!(
        "summon minecraft:cow {}.5 {Y} {}.5 {{NoAI:1b}}",
        X + 14,
        Z
    ))
    .await;
    p.pump(300).await;
    p.cmd("kill @e[type=minecraft:cow,distance=..16]").await;
    p.pump(1500).await;
    let spots = [
        [X + 14, Y - 1, Z],
        [X + 13, Y - 1, Z],
        [X + 15, Y - 1, Z],
        [X + 14, Y - 1, Z - 1],
        [X + 14, Y - 1, Z + 1],
    ];
    for at in spots {
        assert!(
            !p.is_sculk(at).await,
            "скалк расползся при mobGriefing=false"
        );
    }
    p.cmd("gamerule mob_griefing true").await;
    println!("sculk catalyst mobGriefing ok");
    println!("sculk_catalyst_probe: ok");
}
