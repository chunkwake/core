//! Расписание жителя (`timeline/villager_schedule.json` 26.3): днём идёт к
//! рабочему блоку, в `meet` — к колоколу, ночью — к кровати.
//!
//! `villager_schedule_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

const X: i32 = 3456;
const Z: i32 = 8;
const Y: i32 = 100;

struct Probe {
    c: Client,
    sb_command: i32,
    cb_chat: i32,
    cb_meta: i32,
    chat: String,
    /// Последняя поза из `set_entity_data` (индекс 6).
    pose: Option<i32>,
}

impl Probe {
    fn packet(&self, id: i32) -> Writer {
        let mut w = Writer::new();
        w.var_int(id);
        w
    }

    fn drain(&mut self) {
        for (id, body) in std::mem::take(&mut self.c.captured_by) {
            if id == self.cb_chat {
                let mut r = Reader::new(&body);
                if let Ok(component) = text::read_component(&mut r) {
                    flatten(&component, &mut self.chat);
                    self.chat.push(' ');
                }
            } else if id == self.cb_meta {
                let mut r = Reader::new(&body);
                if let (Ok(_entity), Ok(index)) = (r.var_int(), r.u8())
                    && index == 6
                    && let (Ok(kind), Ok(pose)) = (r.var_int(), r.var_int())
                    && kind == 20
                {
                    self.pose = Some(pose);
                }
            }
        }
    }

    async fn pump(&mut self, ms: u64) {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
        loop {
            match tokio::time::timeout_at(deadline, self.c.next_event()).await {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => {
                    eprintln!("event error: {e}");
                    break;
                }
                Err(_) => break,
            }
        }
        self.drain();
    }

    async fn cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(200).await;
    }

    async fn data(&mut self, query: &str) -> String {
        self.pump(120).await;
        self.chat.clear();
        self.cmd(&format!("data get entity {query}")).await;
        self.chat.clone()
    }

    /// `/execute if block`: состояние блока (у кровати нет block entity).
    async fn block_is(&mut self, pos: [i32; 3], predicate: &str) -> bool {
        self.chat.clear();
        self.cmd(&format!(
            "execute if block {} {} {} {predicate} run say BLOCK_YES",
            pos[0], pos[1], pos[2]
        ))
        .await;
        self.chat.contains("BLOCK_YES")
    }

    async fn pos(&mut self) -> Option<[f64; 3]> {
        let text = self
            .data("@e[type=minecraft:villager,limit=1,sort=nearest] Pos")
            .await;
        let mut nums = Vec::new();
        let mut cur = String::new();
        // data get печатает [x, y, z] в конце: до этого в строке есть
        // числовые поля (1.0 и т.п.), их пропускаем.
        let values = text
            .rsplit_once('[')
            .map_or(text.as_str(), |(_, tail)| tail);
        for ch in values.chars() {
            if ch.is_ascii_digit() || ch == '.' || ch == '-' {
                cur.push(ch);
            } else if !cur.is_empty() {
                if let Ok(v) = cur.parse::<f64>() {
                    nums.push(v);
                }
                cur.clear();
            }
        }
        if !cur.is_empty()
            && let Ok(v) = cur.parse::<f64>()
        {
            nums.push(v);
        }
        (nums.len() >= 3).then(|| [nums[0], nums[1], nums[2]])
    }

    async fn wait_near(&mut self, target: [f64; 3], radius: f64, ms: u64) -> Option<[f64; 3]> {
        let start = tokio::time::Instant::now();
        let mut last = None;
        while start.elapsed() < Duration::from_millis(ms) {
            if let Some(pos) = self.pos().await {
                last = Some(pos);
                if dist_xz(pos, target) <= radius {
                    return Some(pos);
                }
            }
            self.pump(200).await;
        }
        last
    }
}

fn dist_xz(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn flatten(v: &serde_json::Value, out: &mut String) {
    if let Some(s) = v.as_str() {
        out.push_str(s);
        return;
    }
    if let Some(s) = v.get("text").and_then(serde_json::Value::as_str) {
        out.push_str(s);
    }
    for key in ["with", "extra"] {
        if let Some(args) = v.get(key).and_then(serde_json::Value::as_array) {
            for arg in args {
                flatten(arg, out);
                out.push(' ');
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n| table.id(State::Play, Direction::Clientbound, n).unwrap();
    let cb_chat = cb("system_chat");
    let cb_meta = cb("set_entity_data");
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    c.capture_also = vec![cb_chat, cb_meta];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        cb_chat,
        cb_meta,
        chat: String::new(),
        pose: None,
    };
    p.pump(1500).await;

    for line in [
        "difficulty peaceful".to_string(),
        "gamemode creative".to_string(),
        "effect clear @s".to_string(),
        "kill @e[type=!minecraft:player,distance=..96]".to_string(),
        format!("execute in minecraft:overworld run tp @s {X}.5 {Y} {Z}.5 180 20"),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            X - 10,
            Y - 1,
            Z - 8,
            X + 16,
            Y - 1,
            Z + 10
        ),
        format!(
            "fill {} {Y} {} {} {} {} minecraft:air",
            X - 10,
            Z - 8,
            X + 16,
            Y + 5,
            Z + 10
        ),
        "kill @e[type=minecraft:item]".to_string(),
        "time set 3000".to_string(),
    ] {
        p.cmd(&line).await;
    }
    p.c.confirm_loaded().await.unwrap();

    // Рабочая точка: компостер, кровать и колокол.
    p.cmd(&format!("setblock {} {Y} {} minecraft:composter", X + 6, Z))
        .await;
    p.cmd(&format!(
        "setblock {X} {Y} {} minecraft:bell[attachment=floor,facing=north]",
        Z + 6
    ))
    .await;
    p.cmd(&format!(
        "setblock {} {Y} {} minecraft:red_bed[part=foot,facing=south]",
        X + 8,
        Z - 4
    ))
    .await;
    p.cmd(&format!(
        "setblock {} {Y} {} minecraft:red_bed[part=head,facing=south]",
        X + 8,
        Z - 3
    ))
    .await;
    // Безработный житель сам займёт компостер и станет фермером.
    p.cmd(&format!(
        "summon minecraft:villager {X}.5 {Y} {Z}.5 {{VillagerData:{{type:\"minecraft:plains\",profession:\"minecraft:none\",level:1}},Xp:0}}"
    ))
    .await;
    p.pump(800).await;

    let data = p
        .data("@e[type=minecraft:villager,limit=1,sort=nearest] VillagerData")
        .await;
    assert!(data.contains("farmer"), "житель не занял компостер: {data}");

    // 1. Работа: 2000..9000 — идёт к компостеру.
    p.cmd("time set 3000").await;
    let start = p
        .wait_near([X as f64 + 6.5, f64::from(Y), Z as f64 + 0.5], 2.0, 12_000)
        .await
        .unwrap_or_else(|| panic!("нет позиции жителя"));
    let near = dist_xz(start, [X as f64 + 6.5, f64::from(Y), Z as f64 + 0.5]);
    assert!(near <= 2.0, "житель не дошёл до компостера: {start:?}");
    println!("work: villager at composter ({near:.2})");

    // 2. Встреча: 9000..11000 — идёт к колоколу.
    p.cmd("time set 9600").await;
    let at_bell = p
        .wait_near([X as f64 + 0.5, f64::from(Y), Z as f64 + 6.5], 2.0, 12_000)
        .await
        .unwrap_or_else(|| panic!("нет позиции жителя у колокола"));
    let near = dist_xz(at_bell, [X as f64 + 0.5, f64::from(Y), Z as f64 + 6.5]);
    assert!(near <= 2.0, "житель не дошёл до колокола: {at_bell:?}");
    println!("meet: villager at bell ({near:.2})");

    // 3. Отдых: 12000.. — идёт к своей кровати.
    p.cmd("time set 13000").await;
    let bed_foot = [X as f64 + 8.5, f64::from(Y), Z as f64 - 3.5];
    let bed_head = [X as f64 + 8.5, f64::from(Y), Z as f64 - 2.5];
    let mut at_bed = None;
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(15_000) {
        if let Some(pos) = p.pos().await {
            at_bed = Some(pos);
            if dist_xz(pos, bed_foot).min(dist_xz(pos, bed_head)) <= 2.0 {
                break;
            }
        }
        p.pump(200).await;
    }
    let at_bed = at_bed.unwrap_or_else(|| panic!("нет позиции жителя у кровати"));
    let near = dist_xz(at_bed, bed_foot).min(dist_xz(at_bed, bed_head));
    assert!(near <= 2.0, "житель не дошёл до кровати: {at_bed:?}");
    println!("rest: villager at bed ({near:.2})");

    // Сон: поза SLEEPING, кровать занята, позиция на кровати.
    let mut slept = None;
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(8_000) {
        if p.pose == Some(2)
            && p.block_is([X + 8, Y, Z - 4], "minecraft:red_bed[occupied=true]")
                .await
        {
            slept = p.pos().await;
            break;
        }
        p.pump(200).await;
    }
    let slept = slept.unwrap_or_else(|| panic!("житель не лёг спать (pose {:?})", p.pose));
    assert!(
        slept[1] > f64::from(Y) + 0.4,
        "позиция во сне не на кровати: {slept:?}"
    );
    println!("sleep: pose sleeping, bed occupied (y {:.3})", slept[1]);

    // Пробуждение: вне `rest` поза STANDING и кровать свободна.
    p.cmd("time set 1000").await;
    let mut woken = false;
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_millis(8_000) {
        if p.pose == Some(0)
            && p.block_is([X + 8, Y, Z - 4], "minecraft:red_bed[occupied=false]")
                .await
        {
            woken = true;
            break;
        }
        p.pump(200).await;
    }
    let bed_busy = p
        .block_is([X + 8, Y, Z - 4], "minecraft:red_bed[occupied=true]")
        .await;
    assert!(
        woken,
        "житель не проснулся (pose {:?}, bed занята: {bed_busy})",
        p.pose
    );
    println!("wake: pose standing, bed free");

    println!("villager_schedule_probe: ok");
}
