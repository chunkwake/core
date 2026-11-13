//! Триггер достижений location: биом точки игрока.
//!
//! `adv_location_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::collections::HashMap;
use std::time::Duration;

const BX: i32 = 6000;
const BZ: i32 = 6000;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_move: i32,
    cb_chat: i32,
    chat: String,
    adv: HashMap<String, Vec<String>>,
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
        loop {
            let ev = match tokio::time::timeout_at(deadline, self.c.next_event()).await {
                Ok(Ok(ev)) => ev,
                Ok(Err(e)) => {
                    eprintln!("event error: {e}");
                    break;
                }
                Err(_) => break,
            };
            if let Event::Advancements {
                reset, progress, ..
            } = ev
            {
                if reset {
                    self.adv.clear();
                }
                for (id, criteria) in progress {
                    if criteria.is_empty() {
                        self.adv.remove(&id);
                    } else {
                        self.adv.entry(id).or_default().extend(criteria);
                    }
                }
            }
        }
        self.drain();
    }

    fn done(&self, id: &str, criterion: &str) -> bool {
        self.adv
            .get(id)
            .is_some_and(|v| v.iter().any(|c| c == criterion))
    }

    async fn cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(300).await;
    }

    async fn pose(&mut self, pos: [f64; 3]) {
        let mut w = self.packet(self.sb_move);
        w.f64(pos[0])
            .f64(pos[1])
            .f64(pos[2])
            .f32(0.0)
            .f32(0.0)
            .u8(1);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(200).await;
    }

    async fn wait_for(&mut self, id: &str, criterion: &str, ms: u64) -> bool {
        let steps = (ms / 250).max(1);
        for _ in 0..steps {
            if self.done(id, criterion) {
                return true;
            }
            self.pump(250).await;
        }
        self.done(id, criterion)
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n| table.id(State::Play, Direction::Clientbound, n).unwrap();
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    let cb_chat = cb("system_chat");
    c.capture_also = vec![cb_chat];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_move: sb("move_player_pos_rot"),
        cb_chat,
        chat: String::new(),
        adv: HashMap::new(),
    };
    p.pump(2000).await;

    let by = 64;
    for line in [
        "difficulty peaceful".to_string(),
        "gamemode survival".to_string(),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            BX - 6,
            by - 1,
            BZ - 6,
            BX + 6,
            by - 1,
            BZ + 6
        ),
        format!(
            "tp boater {} {} {} 0 0",
            BX as f64 + 0.5,
            by,
            BZ as f64 + 0.5
        ),
        "effect clear @s".to_string(),
        // Проба сама ставит пустыню ниже; возвращаем равнины, чтобы прогон
        // повторялся на сохранённом мире.
        format!(
            "fillbiome {} {} {} {} {} {} minecraft:plains",
            BX - 8,
            by - 8,
            BZ - 8,
            BX + 8,
            by + 8,
            BZ + 8
        ),
        "advancement revoke @s only minecraft:adventure/adventuring_time".to_string(),
        "advancement revoke @s only minecraft:adventure/minecraft_trials_edition".to_string(),
        "advancement revoke @s only minecraft:end/find_end_city".to_string(),
        "advancement grant @s only minecraft:adventure/root".to_string(),
        "advancement grant @s only minecraft:end/kill_dragon".to_string(),
    ] {
        p.cmd(&line).await;
    }
    // Тик location срабатывает и без движения: ждём без пакетов позиции.
    assert!(
        p.wait_for(
            "minecraft:adventure/adventuring_time",
            "minecraft:plains",
            4000
        )
        .await,
        "location (plains) не сработал: {:?}",
        p.adv.get("minecraft:adventure/adventuring_time")
    );
    println!("location (plains) ok");

    // Другой биом: /fillbiome и шаг внутри области.
    p.cmd(&format!(
        "fillbiome {} {} {} {} {} {} minecraft:desert",
        BX - 4,
        by - 4,
        BZ - 4,
        BX + 4,
        by + 4,
        BZ + 4
    ))
    .await;
    p.pose([BX as f64 + 0.6, by as f64, BZ as f64 + 0.5]).await;
    assert!(
        p.wait_for(
            "minecraft:adventure/adventuring_time",
            "minecraft:desert",
            4000
        )
        .await,
        "location (desert) не сработал: {:?}",
        p.adv.get("minecraft:adventure/adventuring_time")
    );
    println!("location (desert) ok");

    // Структурные предикаты мы не проверяем — они не должны закрываться.
    assert!(
        !p.done("minecraft:end/find_end_city", "in_city"),
        "location со structures не должен срабатывать: {:?}",
        p.adv.get("minecraft:end/find_end_city")
    );
    assert!(
        !p.done(
            "minecraft:adventure/minecraft_trials_edition",
            "minecraft_trials_edition"
        ),
        "location со structures не должен срабатывать: {:?}",
        p.adv.get("minecraft:adventure/minecraft_trials_edition")
    );
    println!("location (structures skipped) ok");

    println!("adv_location: ok");
}
