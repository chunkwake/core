//! HTTP-эндпоинт метрик: `/metrics` для Prometheus и `/snapshot` для CLI.
//!
//! Свой минимальный HTTP/1.1 вместо `hyper`: нужен один GET без TLS,
//! апгрейдов и тела запроса. Сервер пишет только по запросу, поэтому
//! `chunkwake top` может опросить его в любой момент.

use crate::snapshot::{prometheus, Snapshot};
use std::sync::{Arc, RwLock};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
/// Сколько ждать запрос, прежде чем закрыть соединение.
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
/// Предел заголовков: запрос больше — рвём соединение.
const MAX_REQUEST: usize = 8 * 1024;

/// Последний снимок, который отдают эндпоинты.
pub type Shared = Arc<RwLock<Snapshot>>;

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
    let (status, content_type, body) = match path {
        "/metrics" => {
            let text = prometheus(&snapshot.read().unwrap());
            ("200 OK", "text/plain; version=0.0.4; charset=utf-8", text)
        }
        "/snapshot" | "/snapshot.json" => {
            let text = serde_json::to_string_pretty(&*snapshot.read().unwrap())
                .unwrap_or_else(|_| "{}".into());
            ("200 OK", "application/json", text)
        }
        "/" | "/health" => ("200 OK", "text/plain; charset=utf-8", "ok\n".into()),
        _ => ("404 Not Found", "text/plain; charset=utf-8", "not found\n".into()),
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.write_all(body.as_bytes()).await?;
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
    let request = format!("GET /snapshot HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
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
    let (head, payload) = body
        .split_once("\r\n\r\n")
        .ok_or("ответ без заголовков")?;
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

    #[tokio::test]
    async fn serves_metrics_and_snapshot() {
        let snapshot: Shared = Arc::new(RwLock::new(Snapshot::default()));
        let port = spawn(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 0, snapshot)
            .await
            .unwrap();
        let fetched = fetch("127.0.0.1", port).await.unwrap();
        assert_eq!(fetched.tick.rate, 0.0);
        // `/metrics` содержит обязательные имена.
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        stream
            .write_all(b"GET /metrics HTTP/1.1\r\nHost: x\r\n\r\n")
            .await
            .unwrap();
        let mut body = String::new();
        stream.read_to_string(&mut body).await.unwrap();
        assert!(body.contains("chunkwake_up 1"));
        assert!(body.contains("chunkwake_tps{window=\"1m\"}"));
    }

    #[tokio::test]
    async fn unknown_path_is_404() {
        let snapshot: Shared = Arc::new(RwLock::new(Snapshot::default()));
        let port = spawn(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 0, snapshot)
            .await
            .unwrap();
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        stream
            .write_all(b"GET /nope HTTP/1.1\r\nHost: x\r\n\r\n")
            .await
            .unwrap();
        let mut body = String::new();
        stream.read_to_string(&mut body).await.unwrap();
        assert!(body.starts_with("HTTP/1.1 404"));
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

        let snapshot: Shared = Arc::new(RwLock::new(Snapshot::default()));
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
