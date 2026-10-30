//! HTTP-эндпоинт метрик: `/metrics` для Prometheus, `/snapshot` для CLI и
//! живой дашборд в браузере с потоком SSE.
//!
//! Свой минимальный HTTP/1.1 вместо `hyper`: нужен один GET без TLS,
//! апгрейдов и тела запроса. Сервер пишет только по запросу, поэтому
//! `chunkwake top` может опросить его в любой момент. Долгоживущее
//! соединение одно — SSE для страницы: она не опрашивает `/snapshot` раз в
//! секунду, а получает кадр сразу после каждой публикации снимка.

use crate::snapshot::{Snapshot, prometheus};
use crate::web;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast::error::RecvError;

/// Последний снимок и история, общие для сэмплера и эндпоинтов.
pub use crate::live::Shared;

/// Сколько ждать запрос, прежде чем закрыть соединение.
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
/// Предел заголовков: запрос больше — рвём соединение.
const MAX_REQUEST: usize = 8 * 1024;
/// Пауза без обновлений, после которой в SSE-поток уходит комментарий:
/// так прокси и браузер не решают, что соединение умерло.
const SSE_KEEPALIVE: std::time::Duration = std::time::Duration::from_secs(15);

/// Сколько портов подряд пробуем, если настроенный занят.
///
/// Второй сервер на той же машине (или забытый прошлый запуск) иначе просто
/// выключал бы метрики: порт занят — и `chunkwake top` нечего опросить.
pub const PORT_ATTEMPTS: u16 = 3;

/// Запускает сервер метрик; возвращает порт, на котором он слушает.
///
/// Если настроенный порт занят, пробуются следующие (`port + 1`, `port + 2`):
/// так эндпоинт продолжает работать, а не отключается из-за соседнего
/// процесса. Вызывающий сравнивает результат с настройкой и сообщает в лог.
pub async fn spawn(bind: std::net::IpAddr, port: u16, snapshot: Shared) -> Result<u16, String> {
    let mut last = String::new();
    for candidate in ports(port) {
        match TcpListener::bind((bind, candidate)).await {
            Ok(listener) => {
                let local = listener.local_addr().map_err(|e| e.to_string())?;
                tokio::spawn(async move {
                    loop {
                        let Ok((stream, peer)) = listener.accept().await else {
                            continue;
                        };
                        // Только свой хост: метрики не должны уходить наружу.
                        if !peer.ip().is_loopback() && !bind.is_unspecified() {
                            continue;
                        }
                        let snapshot = snapshot.clone();
                        tokio::spawn(async move {
                            let _ = handle(stream, snapshot).await;
                        });
                    }
                });
                return Ok(local.port());
            }
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
                last = format!("bind {bind}:{candidate}: {error}");
            }
            Err(error) => return Err(format!("bind {bind}:{candidate}: {error}")),
        }
    }
    Err(format!(
        "{last} (tried ports {})",
        ports(port)
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

/// Порты попыток: настроенный и следующие за ним.
pub fn ports(port: u16) -> impl Iterator<Item = u16> {
    (0..PORT_ATTEMPTS).filter_map(move |offset| port.checked_add(offset))
}

async fn handle(mut stream: TcpStream, snapshot: Shared) -> Result<(), std::io::Error> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    let request = loop {
        if buf.len() > MAX_REQUEST {
            return Ok(());
        }
        let read = tokio::time::timeout(REQUEST_TIMEOUT, stream.read(&mut chunk))
            .await
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "request"))??;
        if read == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&chunk[..read]);
        if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break String::from_utf8_lossy(&buf[..end]).into_owned();
        }
    };
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");
    let path = path.split('?').next().unwrap_or("/");

    // Поток событий отвечает сам: у него нет `Content-Length` и конец ответа
    // определяется закрытием соединения.
    if path == "/api/events" {
        return events(stream, snapshot).await;
    }

    let (status, content_type, body) = match path {
        "/" | "/index.html" => ("200 OK", "text/html; charset=utf-8", web::INDEX.into()),
        "/metrics" => {
            let text = prometheus(&snapshot.read().unwrap().snapshot());
            ("200 OK", "text/plain; version=0.0.4; charset=utf-8", text)
        }
        "/snapshot" | "/snapshot.json" | "/api/snapshot" => {
            let text = serde_json::to_string_pretty(&snapshot.read().unwrap().snapshot())
                .unwrap_or_else(|_| "{}".into());
            ("200 OK", "application/json", text)
        }
        "/api/history" => {
            let text = serde_json::to_string(&snapshot.read().unwrap().history())
                .unwrap_or_else(|_| "[]".into());
            ("200 OK", "application/json", text)
        }
        "/health" => ("200 OK", "text/plain; charset=utf-8", "ok\n".into()),
        _ => (
            "404 Not Found",
            "text/plain; charset=utf-8",
            "not found\n".into(),
        ),
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.write_all(body.as_bytes()).await?;
    stream.flush().await
}

/// SSE для дашборда: история сразу целиком, дальше по кадру на публикацию.
///
/// Снимок сериализуется под коротким read-локом и никогда — через `.await`:
/// иначе медленный браузер останавливал бы сэмплер. Канал передаёт сигнал без
/// данных, поэтому отставший подписчик получает `Lagged` и просто берёт свежий
/// снимок, а не догоняет очередь из ста секунд.
async fn events(mut stream: TcpStream, live: Shared) -> Result<(), std::io::Error> {
    let (history, mut rx) = {
        let guard = live.read().unwrap();
        (guard.history(), guard.subscribe())
    };
    let head = concat!(
        "HTTP/1.1 200 OK\r\n",
        "Content-Type: text/event-stream; charset=utf-8\r\n",
        "Cache-Control: no-store\r\n",
        "X-Accel-Buffering: no\r\n",
        "Connection: keep-alive\r\n\r\n",
    );
    stream.write_all(head.as_bytes()).await?;
    write_frame(
        &mut stream,
        "history",
        &serde_json::to_string(&history).unwrap_or_else(|_| "[]".into()),
    )
    .await?;

    loop {
        match tokio::time::timeout(SSE_KEEPALIVE, rx.recv()).await {
            Ok(Ok(())) | Ok(Err(RecvError::Lagged(_))) => {}
            Ok(Err(RecvError::Closed)) => return Ok(()),
            // Тишина: держим соединение живым комментарием.
            Err(_) => {
                stream.write_all(b": keepalive\n\n").await?;
                stream.flush().await?;
                continue;
            }
        }
        let snapshot = live.read().unwrap().snapshot();
        let json = serde_json::to_string(&snapshot).unwrap_or_else(|_| "{}".into());
        write_frame(&mut stream, "tick", &json).await?;
    }
}

/// Один кадр SSE: JSON без переводов строк, поэтому `data:` — одна строка.
async fn write_frame(
    stream: &mut TcpStream,
    event: &str,
    json: &str,
) -> Result<(), std::io::Error> {
    let frame = format!("event: {event}\ndata: {json}\n\n");
    stream.write_all(frame.as_bytes()).await?;
    stream.flush().await
}

/// GET `/snapshot` с локального адреса: так CLI получает снимок.
///
/// Порт настройки мог быть занят, и сервер слушает следующий: перебираем те
/// же адреса, что и [`spawn`], чтобы `chunkwake top` нашёл его без флагов.
pub async fn fetch(host: &str, port: u16) -> Result<Snapshot, String> {
    let mut last = String::new();
    for candidate in ports(port) {
        match fetch_port(host, candidate).await {
            Ok(snapshot) => return Ok(snapshot),
            Err(error) => last = error,
        }
    }
    Err(last)
}

/// Один конкретный порт.
///
/// Таймаут здесь короткий: занятый порт может принадлежать чужому процессу,
/// который принимает соединение и молчит. Долгое ожидание не даёт перейти
/// к следующему порту, где сервер действительно слушает.
async fn fetch_port(host: &str, port: u16) -> Result<Snapshot, String> {
    let connect = tokio::time::timeout(
        std::time::Duration::from_millis(700),
        TcpStream::connect((host, port)),
    )
    .await
    .map_err(|_| format!("connect {host}:{port}: таймаут"))?
    .map_err(|e| format!("connect {host}:{port}: {e}"))?;
    let mut stream = connect;
    let request =
        format!("GET /api/snapshot HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    let mut body = String::new();
    // Ждём ответ ограниченно: порт мог достаться чужому процессу, который
    // принял соединение и молчит — тогда нужен следующий порт, а не 5 с сна.
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        stream.read_to_string(&mut body),
    )
    .await
    .map_err(|_| format!("{host}:{port}: нет ответа"))?
    .map_err(|e| e.to_string())?;
    let (head, payload) = body.split_once("\r\n\r\n").ok_or("ответ без заголовков")?;
    if !head.starts_with("HTTP/1.1 200") {
        return Err(format!(
            "сервер метрик ответил: {}",
            head.lines().next().unwrap_or("")
        ));
    }
    serde_json::from_str(payload).map_err(|e| format!("snapshot: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::Live;
    use std::sync::{Arc, RwLock};

    fn shared() -> Shared {
        Arc::new(RwLock::new(Live::new()))
    }

    /// Один запрос к серверу: возвращает весь ответ целиком.
    async fn get(port: u16, path: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let request = format!("GET {path} HTTP/1.1\r\nHost: x\r\n\r\n");
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut body = String::new();
        stream.read_to_string(&mut body).await.unwrap();
        body
    }

    #[tokio::test]
    async fn serves_metrics_and_snapshot() {
        let snapshot = shared();
        let port = spawn(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            snapshot,
        )
        .await
        .unwrap();
        let fetched = fetch("127.0.0.1", port).await.unwrap();
        assert_eq!(fetched.tick.rate, 0.0);
        // `/metrics` содержит обязательные имена.
        let body = get(port, "/metrics").await;
        assert!(body.contains("chunkwake_up 1"));
        assert!(body.contains("chunkwake_tps{window=\"1m\"}"));
    }

    #[tokio::test]
    async fn unknown_path_is_404() {
        let snapshot = shared();
        let port = spawn(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            snapshot,
        )
        .await
        .unwrap();
        let body = get(port, "/nope").await;
        assert!(body.starts_with("HTTP/1.1 404"));
    }

    /// Дашборд отдаётся с корня, `/health` остаётся простым «ok».
    #[tokio::test]
    async fn root_serves_dashboard() {
        let snapshot = shared();
        let port = spawn(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            snapshot,
        )
        .await
        .unwrap();
        let body = get(port, "/").await;
        assert!(body.starts_with("HTTP/1.1 200"));
        assert!(body.contains("text/html"));
        assert!(body.contains("<title>Chunkwake"));
        assert!(body.contains("/api/events"));

        let health = get(port, "/health").await;
        assert!(health.ends_with("ok\n"));
    }

    #[tokio::test]
    async fn history_starts_empty_and_follows_publications() {
        let live = shared();
        let port = spawn(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            live.clone(),
        )
        .await
        .unwrap();
        let body = get(port, "/api/history").await;
        assert!(body.ends_with("[]"), "история пуста до первой публикации");

        live.write().unwrap().publish(Snapshot {
            at: "1700000000".into(),
            tick: crate::snapshot::TickReport {
                tps_now: 19.5,
                ..Default::default()
            },
            ..Snapshot::default()
        });

        let body = get(port, "/api/history").await;
        let (_, payload) = body.split_once("\r\n\r\n").unwrap();
        let points: Vec<crate::live::HistoryPoint> = serde_json::from_str(payload).unwrap();
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].at, 1_700_000_000);
    }

    /// Поток присылает историю сразу и кадр на каждую публикацию.
    #[tokio::test]
    async fn events_stream_history_then_ticks() {
        let live = shared();
        let port = spawn(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            live.clone(),
        )
        .await
        .unwrap();
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        stream
            .write_all(b"GET /api/events HTTP/1.1\r\nHost: x\r\n\r\n")
            .await
            .unwrap();

        // Первый кадр — история; читаем до пустой строки, отделяющей её.
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            let read =
                tokio::time::timeout(std::time::Duration::from_secs(2), stream.read(&mut byte))
                    .await
                    .expect("кадр истории")
                    .unwrap();
            assert!(read > 0, "соединение закрылось");
            head.push(byte[0]);
            if head.ends_with(b"\n\n") {
                break;
            }
        }
        let head = String::from_utf8(head).unwrap();
        assert!(head.contains("text/event-stream"));
        assert!(head.contains("event: history"));

        // Публикация снимка превращается в кадр `tick` с полным содержимым.
        live.write().unwrap().publish(Snapshot {
            at: "1700000001".into(),
            tick: crate::snapshot::TickReport {
                tps_now: 18.75,
                ..Default::default()
            },
            ..Snapshot::default()
        });
        let mut frame = String::new();
        loop {
            let mut chunk = [0u8; 512];
            let read =
                tokio::time::timeout(std::time::Duration::from_secs(2), stream.read(&mut chunk))
                    .await
                    .expect("кадр тика")
                    .unwrap();
            assert!(read > 0, "соединение закрылось");
            frame.push_str(&String::from_utf8_lossy(&chunk[..read]));
            if frame.contains("event: tick") && frame.contains("\"at\":\"1700000001\"") {
                break;
            }
        }
        assert!(frame.contains("18.75"));
    }

    /// Настроенный порт занят — сервер слушает следующий, а клиент его находит.
    #[tokio::test]
    async fn busy_port_falls_back_to_the_next_one() {
        let localhost = std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST);
        // Порт из эфемерного диапазона не годится: Windows выдаёт их блоками,
        // и соседние тоже оказываются заняты. Ищем пару в обычном диапазоне.
        let (busy, port) = (40000..50000u16)
            .find_map(|base| {
                let busy = std::net::TcpListener::bind((localhost, base)).ok()?;
                // Следующий порт должен быть свободен: тогда перебор очевиден.
                std::net::TcpListener::bind((localhost, base + 1)).ok()?;
                Some((busy, base))
            })
            .expect("свободная пара портов");
        let busy = TcpListener::from_std(busy).unwrap();

        let snapshot = shared();
        let bound = spawn(localhost, port, snapshot).await.unwrap();
        assert_eq!(bound, port + 1, "занятый порт пропущен");

        // `fetch` с настройкой на занятый порт тоже находит сервер.
        let fetched = fetch("127.0.0.1", port).await.unwrap();
        assert_eq!(fetched.tick.rate, 0.0);
        drop(busy);
    }

    /// Перебор не уходит за пределы диапазона портов.
    #[test]
    fn port_attempts_stay_in_range() {
        assert_eq!(ports(25585).collect::<Vec<_>>(), vec![25585, 25586, 25587]);
        assert_eq!(
            ports(u16::MAX - 1).collect::<Vec<_>>(),
            vec![u16::MAX - 1, u16::MAX]
        );
    }
}
