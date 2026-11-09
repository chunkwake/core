//! Проба перевода протокола: status-пинг клиентами разных версий.
//!
//! С ViaVersion (и ViaBackwards) в `plugins/` сервер отвечает каждой версии
//! её же протоколом — в списке серверов старый клиент видит, что сервер
//! совместим. Без Via старые версии видят 777 (несовместим).
//!
//! `cargo run -p chunkwake-bots --example via_probe -- 127.0.0.1:25565`

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:25565".into());
    // 26.3, 1.21.4, 1.21.1, 1.20.4, 1.16.5, 1.12.2, 1.8.9, 1.7.10
    let mut failed = false;
    for protocol in [777, 769, 767, 765, 754, 340, 47, 5] {
        match chunkwake_bots::status(&addr, protocol).await {
            Ok(v) => {
                let answered = v["version"]["protocol"].as_i64().unwrap_or(-1);
                let ok = answered == protocol as i64;
                failed |= !ok;
                println!(
                    "{protocol:>4}: {} {} ({})",
                    if ok { "ok  " } else { "FAIL" },
                    answered,
                    v["version"]["name"]
                );
            }
            Err(e) => {
                failed = true;
                println!("{protocol:>4}: FAIL {e}");
            }
        }
    }
    if failed {
        std::process::exit(1);
    }
}
