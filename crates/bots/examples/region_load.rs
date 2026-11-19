//! Базовый нагрузочный замер: N ботов в двух разных регионах чанков.
//!
//! `region_load <addr> <packets.json> <registries.json> [bots] [seconds]`
//!
//! Половина ботов телепортируется в регион (0,0), половина — в (1,1)
//! (регион = 8×8 чанков). Один бот периодически спрашивает `/tps` и
//! печатает MSPT, чтобы можно было сравнить single-thread тик с будущим
//! regionized-тиком.
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer};
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct Bot {
    c: Client,
    sb_command: i32,
    cb_chat: i32,
    chat: String,
}

impl Bot {
    async fn pump(&mut self, ms: u64) {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
        loop {
            match tokio::time::timeout_at(deadline, self.c.next_event()).await {
                Ok(Ok(Event::Position(_))) => {}
                Ok(Ok(_)) => {}
                _ => break,
            }
        }
        self.drain();
    }

    fn drain(&mut self) {
        for (id, body) in std::mem::take(&mut self.c.captured_by) {
            if id != self.cb_chat {
                continue;
            }
            let mut r = Reader::new(&body);
            if let Ok(component) = chunkwake_protocol::text::read_component(&mut r) {
                let mut out = String::new();
                flatten(&component, &mut out);
                self.chat.push_str(&out);
                self.chat.push('\n');
            }
        }
    }

    async fn command(&mut self, line: &str) {
        let mut w = Writer::new();
        w.var_int(self.sb_command).string(line);
        self.c.send_raw(&w.buf).await.unwrap();
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
    if let Some(key) = v.get("translate").and_then(serde_json::Value::as_str) {
        out.push_str(key);
    }
    for key in ["with", "extra"] {
        if let Some(args) = v.get(key).and_then(serde_json::Value::as_array) {
            for arg in args {
                flatten(arg, out);
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let bots: usize = a.get(4).and_then(|v| v.parse().ok()).unwrap_or(8);
    let secs: u64 = a.get(5).and_then(|v| v.parse().ok()).unwrap_or(20);
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let sb_command = table
        .id(State::Play, Direction::Serverbound, "chat_command")
        .unwrap();
    let cb_chat = table
        .id(State::Play, Direction::Clientbound, "system_chat")
        .unwrap();
    let results: Arc<Mutex<Vec<f64>>> = Arc::new(Mutex::new(Vec::new()));
    let mut tasks = Vec::new();

    for i in 0..bots {
        let addr = a[1].clone();
        let reg = Arc::clone(&results);
        tasks.push(tokio::spawn(async move {
            // Небольшая пауза между логинами: сервер обрабатывает вход
            // в основном потоке, одновременный шквал даёт таймауты.
            tokio::time::sleep(Duration::from_millis(400 * i as u64)).await;
            // Имена из ops.json стенда: ботам нужны права на `/tp` и `/tps`.
            let names = ["boater", "framer", "carter", "pather"];
            let name = names
                .get(i)
                .map(|n| n.to_string())
                .unwrap_or_else(|| format!("load{i}"));
            let mut c = match Client::connect(&addr, &name, ids, 777, "26.3").await {
                Ok(c) => c,
                Err(e) => {
                    println!("{name}: connect failed: {e}");
                    return;
                }
            };
            c.capture_also = vec![cb_chat];
            let mut bot = Bot {
                c,
                sb_command,
                cb_chat,
                chat: String::new(),
            };
            // Ждём позицию, затем уводим в свой регион: чётные — (0,0),
            // нечётные — (1,1). Регион = 8 чанков = 128 блоков.
            for _ in 0..80 {
                bot.pump(100).await;
                if bot.chat.contains("spawn") {
                    break;
                }
            }
            let x = if i % 2 == 0 { 8 } else { 200 };
            bot.command(&format!("tp @s {x} -59 8")).await;
            bot.pump(600).await;
            if i == 0 {
                println!("reporter ready at x={x}");
            }
            let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
            while tokio::time::Instant::now() < deadline {
                if i == 0 {
                    bot.command("tick query").await;
                    bot.pump(1500).await;
                    let text = std::mem::take(&mut bot.chat);
                    for line in text.lines() {
                        if let Some(at) = line.find("MSPT:") {
                            let tail = &line[at + 5..];
                            let value = tail
                                .split_whitespace()
                                .next()
                                .and_then(|v| v.parse::<f64>().ok());
                            if let Some(mspt) = value {
                                println!("MSPT {mspt}");
                                reg.lock().unwrap().push(mspt);
                            }
                        }
                    }
                } else {
                    bot.pump(1500).await;
                }
            }
        }));
    }

    for task in tasks {
        let _ = task.await;
    }
    let samples = results.lock().unwrap().clone();
    if !samples.is_empty() {
        let avg = samples.iter().sum::<f64>() / samples.len() as f64;
        let max = samples.iter().cloned().fold(0.0_f64, f64::max);
        println!(
            "region_load: bots={bots} samples={} avg_mspt={avg:.2} max_mspt={max:.2}",
            samples.len()
        );
    } else {
        println!("region_load: no MSPT samples (bots={bots})");
    }
}
