//! Профессии по точкам интереса: безработный житель занимает ближайший
//! рабочий блок, теряет профессию при разрушении и освобождает точку.
//!
//! `poi_profession_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

const BX: i32 = 4900;
const BZ: i32 = 4900;

struct Probe {
    c: Client,
    sb_command: i32,
    cb_chat: i32,
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
        self.pump(250).await;
    }

    async fn query(&mut self, path: &str) -> String {
        self.pump(120).await;
        self.chat.clear();
        self.cmd(&format!(
            "data get entity @e[type=minecraft:villager,limit=1,sort=nearest] {path}"
        ))
        .await;
        if self.chat.trim().is_empty() {
            self.pump(300).await;
            self.cmd(&format!(
                "data get entity @e[type=minecraft:villager,limit=1,sort=nearest] {path}"
            ))
            .await;
        }
        self.chat.clone()
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
        cb_chat,
        chat: String::new(),
    };
    p.pump(2000).await;

    let by = 64;
    for line in [
        "difficulty peaceful".to_string(),
        format!(
            "fill {} {} {} {} {} {} minecraft:air",
            BX - 6,
            by,
            BZ - 6,
            BX + 6,
            by + 4,
            BZ + 6
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            BX - 6,
            by - 1,
            BZ - 6,
            BX + 6,
            by - 1,
            BZ + 6
        ),
        format!("tp boater {} {} {}", BX as f64 + 0.5, by, BZ as f64 + 0.5),
        "kill @e[type=minecraft:villager]".to_string(),
        format!("setblock {} {} {} minecraft:composter", BX + 2, by, BZ),
        format!(
            "summon minecraft:villager {} {} {} {{VillagerData:{{type:\"minecraft:plains\",profession:\"minecraft:none\",level:1}}}}",
            BX as f64 + 0.5,
            by as f64,
            BZ as f64 + 0.5
        ),
    ] {
        p.cmd(&line).await;
    }
    p.pump(2500).await;

    // 1. Безработный занимает компостер и становится фермером.
    let data = p.query("VillagerData").await;
    assert!(
        data.contains("minecraft:farmer"),
        "житель не стал фермером: {data}"
    );
    let offers = p.query("Offers").await;
    assert!(
        offers.contains("minecraft:wheat"),
        "у фермера нет сделок: {offers}"
    );
    println!("poi profession gain ok");

    // 2. Разрушение компостера сбрасывает профессию и сделки.
    p.cmd(&format!("setblock {} {} {} minecraft:air", BX + 2, by, BZ))
        .await;
    p.pump(2500).await;
    let data = p.query("VillagerData").await;
    assert!(
        data.contains("minecraft:none"),
        "профессия не сброшена после разрушения: {data}"
    );
    let offers = p.query("Offers").await;
    assert!(offers.contains("Recipes:[]"), "сделки не очищены: {offers}");
    println!("poi profession loss ok");

    // 3. Новый блок меняет профессию (лектерн -> библиотекарь).
    p.cmd(&format!(
        "setblock {} {} {} minecraft:lectern",
        BX + 2,
        by,
        BZ
    ))
    .await;
    p.pump(2500).await;
    let data = p.query("VillagerData").await;
    assert!(
        data.contains("minecraft:librarian"),
        "житель не стал библиотекарем: {data}"
    );
    p.pump(2000).await;
    let data = p.query("VillagerData").await;
    assert!(
        data.contains("minecraft:librarian"),
        "профессия нестабильна: {data}"
    );
    println!("poi profession change ok");

    // 4. Убийство жителя освобождает точку: новый житель занимает лектерн.
    p.cmd("kill @e[type=minecraft:villager]").await;
    p.pump(1500).await;
    p.cmd(&format!(
        "summon minecraft:villager {} {} {} {{VillagerData:{{type:\"minecraft:plains\",profession:\"minecraft:none\",level:1}}}}",
        BX as f64 + 1.5,
        by as f64,
        BZ as f64 + 0.5
    ))
    .await;
    p.pump(2500).await;
    let data = p.query("VillagerData").await;
    assert!(
        data.contains("minecraft:librarian"),
        "новая профессия не получена после освобождения точки: {data}"
    );
    println!("poi claim release ok");

    p.cmd("kill @e[type=minecraft:villager]").await;
    println!("poi_profession: ok");
}
