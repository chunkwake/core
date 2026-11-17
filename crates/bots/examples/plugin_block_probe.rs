//! Регрессия: блок, записанный плагином в `onEnable` (до входа игрока),
//! должен остаться после генерации чанка и пересчёта света.
//!
//! `plugin_block_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer};
use std::time::Duration;

struct Probe {
    c: Client,
    sb_command: i32,
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
            if id == self.cb_chat {
                let mut r = Reader::new(&body);
                if let Ok(component) = chunkwake_protocol::text::read_component(&mut r) {
                    let mut out = String::new();
                    flatten(&component, &mut out);
                    self.chat.push_str(&out);
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
            if let Event::Position(_) = ev {
                continue;
            }
        }
        self.drain();
    }

    async fn cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(400).await;
    }
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

    // Плагин записал лестницу в onEnable, до нашего входа; вход запускает
    // генерацию чанков и `light_ring`, которые раньше затирали правку.
    p.cmd("execute if block 50 100 0 minecraft:oak_stairs run say STAIRS_OK")
        .await;
    let stairs = p.chat.contains("STAIRS_OK");
    println!("block after join: stairs={stairs}");

    // Повторная проверка: чанк уже загружен, свет пересчитан.
    p.pump(3000).await;
    p.chat.clear();
    p.cmd("execute if block 50 100 0 minecraft:oak_stairs run say STAIRS_OK_2")
        .await;
    let again = p.chat.contains("STAIRS_OK_2");
    println!("block after relight: stairs={again}");

    assert!(
        stairs,
        "правка плагина в onEnable потерялась после входа игрока"
    );
    assert!(again, "правка плагина потерялась при повторном пересчёте");
    println!("plugin_block_probe: ok");
}
