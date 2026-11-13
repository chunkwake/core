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
use std::collections::HashMap;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast::error::RecvError;

/// Последний снимок и история, общие для сэмплера и эндпоинтов.
pub use crate::live::Shared;

/// Имя куки с токеном доступа к дашборду.
const COOKIE_NAME: &str = "cw_metrics";

/// Тела меньше этого не сжимаем: заголовки дороже выигрыша.
const GZIP_MIN_BYTES: usize = 1024;

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

/// Настройки доступа к дашборду.
///
/// Токен нужен только тогда, когда сокет слушает не loopback: локально
/// `chunkwake top` и браузер на этой же машине должны работать без
/// настройки, иначе защита мешает больше, чем помогает.
#[derive(Clone, Debug, Default)]
pub struct Access {
    /// Пусто — входящие не с loopback отклоняются на входе.
    pub token: String,
}

impl Access {
    /// Доступ без токена: только loopback.
    pub fn local() -> Self {
        Self::default()
    }

    /// Доступ с токеном для внешних клиентов.
    pub fn with_token(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
        }
    }

    /// Пускает ли такой запрос вообще: без токена — только loopback.
    ///
    /// Это проверка ещё до чтения запроса, по адресу пира: внешний клиент
    /// без настроенного токена не должен тратить наши ресурсы на разбор.
    fn admits(self: &std::sync::Arc<Self>, peer: std::net::IpAddr) -> bool {
        peer.is_loopback() || !self.token.is_empty()
    }

    /// Совпадает ли предъявленный токен с настроенным.
    ///
    /// Сравнение постоянного времени: обычное `==` на строках выходит из
    /// цикла на первом различии, и по задержке можно подбирать токен
    /// побайтово.
    pub fn matches(&self, presented: &str) -> bool {
        let (a, b) = (self.token.as_bytes(), presented.as_bytes());
        if a.is_empty() || a.len() != b.len() {
            return false;
        }
        let mut diff = 0u8;
        for (x, y) in a.iter().zip(b.iter()) {
            diff |= x ^ y;
        }
        diff == 0
    }
}

/// Запускает сервер метрик; возвращает порт, на котором он слушает.
///
/// Если настроенный порт занят, пробуются следующие (`port + 1`, `port + 2`):
/// так эндпоинт продолжает работать, а не отключается из-за соседнего
/// процесса. Вызывающий сравнивает результат с настройкой и сообщает в лог.
pub async fn spawn(
    bind: std::net::IpAddr,
    port: u16,
    snapshot: Shared,
    access: Access,
) -> Result<u16, String> {
    if !bind.is_loopback() && access.token.is_empty() {
        return Err(format!(
            "metrics-bind={bind} is reachable from the network, but metrics-token is empty; \
             set one with `chunkwake config set metrics-token <long random string>` \
             or bind to 127.0.0.1"
        ));
    }
    let access = std::sync::Arc::new(access);
    let mut last = String::new();
    for candidate in ports(port) {
        match TcpListener::bind((bind, candidate)).await {
            Ok(listener) => {
                let local = listener.local_addr().map_err(|e| e.to_string())?;
                let access_loop = access.clone();
                tokio::spawn(async move {
                    loop {
                        let Ok((stream, peer)) = listener.accept().await else {
                            continue;
                        };
                        if !access_loop.admits(peer.ip()) {
                            continue;
                        }
                        let snapshot = snapshot.clone();
                        let access = access_loop.clone();
                        let loopback = peer.ip().is_loopback();
                        tokio::spawn(async move {
                            let _ = handle(stream, snapshot, access, loopback).await;
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

/// Разобранный запрос: минимальный набор, который нужен этому серверу.
struct Request {
    method: String,
    path: String,
    query: HashMap<String, String>,
    headers: HashMap<String, String>,
}

impl Request {
    /// Разбирает строку заголовков (без тела: сервер принимает только GET).
    fn parse(raw: &str) -> Self {
        let mut lines = raw.lines();
        let mut first = lines.next().unwrap_or("").split_whitespace();
        let method = first.next().unwrap_or("GET").to_owned();
        let target = first.next().unwrap_or("/");
        let (path, query) = match target.split_once('?') {
            Some((path, query)) => (path, parse_pairs(query)),
            None => (target, HashMap::new()),
        };
        let headers = lines
            .filter_map(|line| line.split_once(':'))
            .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
            .collect();
        Self {
            method,
            path: path.to_owned(),
            query,
            headers,
        }
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(String::as_str)
    }

    /// Токен из запроса: кука (для страницы), query (вход по ссылке) или
    /// `Authorization: Bearer` (CLI и Prometheus).
    fn presented_token(&self) -> Option<String> {
        if let Some(cookie) = self.header("cookie") {
            for part in cookie.split(';') {
                if let Some(value) = part
                    .trim()
                    .strip_prefix(COOKIE_NAME)
                    .and_then(|rest| rest.strip_prefix('='))
                {
                    return Some(value.to_owned());
                }
            }
        }
        if let Some(token) = self.query.get("token") {
            return Some(token.clone());
        }
        self.header("authorization")
            .and_then(|value| value.strip_prefix("Bearer "))
            .map(|value| value.trim().to_owned())
    }
}

/// `a=1&b=2` с декодированием процентов.
fn parse_pairs(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(key, value)| (percent_decode(key), percent_decode(value)))
        .collect()
}

/// Процентное декодирование; битые последовательности остаются как есть.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(byte) => {
                        out.push(byte);
                        i += 3;
                    }
                    None => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Считается ли запрос безопасным для обработки.
///
/// Токен нужен только там, где сервер слушает сеть: локально ничего не
/// меняется. Отдельно проверяем `Origin`: иначе чужая страница может
/// дёрнуть дашборд из браузера жертвы и прочитать ответ через rebound-DNS.
fn authorize(req: &Request, access: &Access, peer_loopback: bool) -> Auth {
    if peer_loopback && access.token.is_empty() {
        return Auth::Ok;
    }
    // Origin, если он есть, должен указывать на тот же хост: иначе чужая
    // страница дёргает дашборд из браузера жертвы. Порт сверять не нужно —
    // браузер шлёт `Origin` с портом, а `Host` может прийти без него.
    if let Some(origin) = req.header("origin") {
        let host = req.header("host").unwrap_or("");
        if !host.is_empty() && host_only(origin_host(origin)) != host_only(host) {
            return Auth::Forbidden;
        }
    }
    match req.presented_token() {
        Some(token) if access.matches(&token) => Auth::Ok,
        Some(_) => Auth::BadToken,
        None => Auth::Missing,
    }
}

/// Хост из `Origin`: `http://a:1` -> `a:1`.
fn origin_host(origin: &str) -> &str {
    origin
        .split_once("://")
        .map_or(origin, |(_, rest)| rest)
        .split('/')
        .next()
        .unwrap_or("")
}

/// Хост без порта: `127.0.0.1:25585` -> `127.0.0.1`.
fn host_only(authority: &str) -> &str {
    match authority.rsplit_once(':') {
        Some((host, port)) if port.chars().all(|c| c.is_ascii_digit()) => host,
        _ => authority,
    }
}

/// Итог проверки доступа.
enum Auth {
    Ok,
    /// Токена нет: показать форму входа.
    Missing,
    /// Токен есть, но неверный: ответ тот же, что при отсутствии, плюс 401.
    BadToken,
    /// Запрос пришёл с чужого origin.
    Forbidden,
}

async fn handle(
    mut stream: TcpStream,
    snapshot: Shared,
    access: std::sync::Arc<Access>,
    peer_loopback: bool,
) -> Result<(), std::io::Error> {
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
    let req = Request::parse(&request);

    // Сервер отвечает только на GET: другие методы бессмысленны, а
    // разбирать тела запросов — лишняя поверхность атаки.
    if req.method != "GET" {
        return respond(
            &mut stream,
            "405 Method Not Allowed",
            "text/plain; charset=utf-8",
            b"method not allowed\n",
            false,
        )
        .await;
    }

    // Вход по ссылке: токен в query обменивается на куку, и редирект уводит
    // на чистый адрес — иначе токен остался бы в истории браузера и логах.
    let login_token = if req.path.starts_with("/api/") {
        None
    } else {
        req.query
            .get("token")
            .filter(|t| access.matches(t))
            .cloned()
    };
    if let Some(token) = login_token {
        let cookie = format!(
            "Set-Cookie: {COOKIE_NAME}={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age=604800\r\n"
        );
        let response = format!(
            "HTTP/1.1 302 Found\r\nLocation: {}\r\n{cookie}Cache-Control: no-store\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            req.path
        );
        stream.write_all(response.as_bytes()).await?;
        return stream.flush().await;
    }

    match authorize(&req, &access, peer_loopback) {
        Auth::Ok => {}
        Auth::Forbidden => {
            return respond(
                &mut stream,
                "403 Forbidden",
                "text/plain; charset=utf-8",
                "forbidden\n".as_bytes(),
                false,
            )
            .await;
        }
        Auth::Missing | Auth::BadToken => {
            // Форма входа вместо данных: тот же стиль, без внешних ресурсов.
            return respond(
                &mut stream,
                "401 Unauthorized",
                "text/html; charset=utf-8",
                web::LOGIN.as_bytes(),
                accepts_gzip(&req),
            )
            .await;
        }
    }

    // Поток событий отвечает сам: у него нет `Content-Length` и конец ответа
    // определяется закрытием соединения.
    if req.path == "/api/events" {
        return events(stream, snapshot).await;
    }

    let gzip = accepts_gzip(&req);
    let (status, content_type, body) = match req.path.as_str() {
        "/" | "/index.html" => {
            // Сжатая страница лежит готовой: второй раз её не жмём, только
            // ставим `Content-Encoding`.
            return match gzip {
                true => {
                    respond_gzip(&mut stream, "text/html; charset=utf-8", &web::index_gzip()).await
                }
                false => {
                    respond(
                        &mut stream,
                        "200 OK",
                        "text/html; charset=utf-8",
                        web::index_raw(),
                        false,
                    )
                    .await
                }
            };
        }
        "/metrics" => {
            let text = prometheus(&snapshot.read().unwrap().snapshot());
            (
                "200 OK",
                "text/plain; version=0.0.4; charset=utf-8",
                text.into_bytes(),
            )
        }
        "/snapshot" | "/snapshot.json" | "/api/snapshot" => {
            let text = serde_json::to_string_pretty(&snapshot.read().unwrap().snapshot())
                .unwrap_or_else(|_| "{}".into());
            ("200 OK", "application/json", text.into_bytes())
        }
        "/api/history" => {
            let text = serde_json::to_string(&snapshot.read().unwrap().history())
                .unwrap_or_else(|_| "[]".into());
            ("200 OK", "application/json", text.into_bytes())
        }
        "/api/profile" => {
            let text = serde_json::to_string(&snapshot.read().unwrap().profile())
                .unwrap_or_else(|_| "[]".into());
            ("200 OK", "application/json", text.into_bytes())
        }
        "/health" => ("200 OK", "text/plain; charset=utf-8", b"ok\n".to_vec()),
        _ => (
            "404 Not Found",
            "text/plain; charset=utf-8",
            b"not found\n".to_vec(),
        ),
    };
    respond(&mut stream, status, content_type, &body, gzip).await
}

/// Клиент принимает gzip.
fn accepts_gzip(req: &Request) -> bool {
    req.header("accept-encoding")
        .is_some_and(|value| value.to_ascii_lowercase().contains("gzip"))
}

/// Отправляет ответ; крупные тела сжимает, если клиент это умеет.
///
/// `allow_gzip` — можно ли сжимать здесь; `precompressed` — тело уже сжато
/// заранее (страница), тогда остаётся только поставить заголовок.
async fn respond(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
    gzip_wanted: bool,
) -> Result<(), std::io::Error> {
    let gzipped = match gzip_wanted && body.len() >= GZIP_MIN_BYTES {
        true => Some(gzip(body)),
        false => None,
    };
    let (encoding, payload) = match &gzipped {
        Some(packed) => ("Content-Encoding: gzip\r\n", packed.as_slice()),
        None => ("", body),
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\n{encoding}Content-Length: {}\r\nVary: Accept-Encoding\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        payload.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.write_all(payload).await?;
    stream.flush().await
}

/// Отправляет уже сжатое тело: заголовок ставится сразу, сжатия нет.
async fn respond_gzip(
    stream: &mut TcpStream,
    content_type: &str,
    body: &[u8],
) -> Result<(), std::io::Error> {
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\nVary: Accept-Encoding\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(body).await?;
    stream.flush().await
}

/// Сжимает тело gzip: страница отдаётся в 3.4 раза меньше.
fn gzip(body: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    // Сжатие в память не падает; при ошибке уходим без сжатия.
    match encoder.write_all(body).and_then(|()| encoder.finish()) {
        Ok(packed) => packed,
        Err(_) => body.to_vec(),
    }
}

/// SSE для дашборда: история сразу целиком, дальше по кадру на публикацию.
///
/// Снимок сериализуется под коротким read-локом и никогда — через `.await`:
/// иначе медленный браузер останавливал бы сэмплер. Канал передаёт сигнал без
/// данных, поэтому отставший подписчик получает `Lagged` и просто берёт свежий
/// снимок, а не догоняет очередь из ста секунд.
async fn events(mut stream: TcpStream, live: Shared) -> Result<(), std::io::Error> {
    let (history, profile, mut rx) = {
        let guard = live.read().unwrap();
        (guard.history(), guard.profile(), guard.subscribe())
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
    // Профиль приходит отдельным кадром: это не снимок, у него своя
    // секундная сетка и своя ёмкость.
    write_frame(
        &mut stream,
        "profile",
        &serde_json::to_string(&profile).unwrap_or_else(|_| "[]".into()),
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
        // Лок берём только на время клонирования: держать его через
        // `.await` нельзя — будущее перестаёт быть `Send`, а медленный
        // браузер останавливал бы сэмплер.
        let (snapshot, latest) = {
            let guard = live.read().unwrap();
            (guard.snapshot(), guard.latest_profile())
        };
        let json = serde_json::to_string(&snapshot).unwrap_or_else(|_| "{}".into());
        write_frame(&mut stream, "tick", &json).await?;
        // Отдаём только последнюю закрытую секунду профиля: страница уже
        // держит историю и дописывает её сама, гонять 900 корзин раз в
        // секунду незачем.
        if let Some(bucket) = latest {
            let json = serde_json::to_string(&bucket).unwrap_or_else(|_| "{}".into());
            write_frame(&mut stream, "prof", &json).await?;
        }
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
///
/// `token` — то же значение, что в `metrics-token`: без него сервер,
/// открытый в сеть, ответит 401.
pub async fn fetch(host: &str, port: u16, token: Option<&str>) -> Result<Snapshot, String> {
    let mut last = String::new();
    for candidate in ports(port) {
        match fetch_json::<Snapshot>(host, candidate, "/api/snapshot", token).await {
            Ok(snapshot) => return Ok(snapshot),
            Err(error) => last = error,
        }
    }
    Err(last)
}

/// Последняя закрытая секунда профиля: её печатает `chunkwake top`.
///
/// Отдельно от [`fetch`], потому что профиль необязателен: старый сервер его
/// не отдаёт, и `top` должен работать без него, а не падать.
pub async fn fetch_profile(
    host: &str,
    port: u16,
    token: Option<&str>,
) -> Result<crate::profile::ProfileBucket, String> {
    let mut last = String::new();
    for candidate in ports(port) {
        match fetch_json::<Vec<crate::profile::ProfileBucket>>(
            host,
            candidate,
            "/api/profile",
            token,
        )
        .await
        {
            Ok(mut buckets) => {
                return buckets
                    .pop()
                    .ok_or_else(|| "профиль ещё не набран".to_owned());
            }
            Err(error) => last = error,
        }
    }
    Err(last)
}

/// GET JSON с одного порта: общий код для снимка и профиля.
///
/// `token` уходит в `Authorization: Bearer`: `chunkwake top` читает его из
/// `server.properties`, поэтому работает и на сервере, открытом в сеть.
async fn fetch_json<T: serde::de::DeserializeOwned>(
    host: &str,
    port: u16,
    path: &str,
    token: Option<&str>,
) -> Result<T, String> {
    let connect = tokio::time::timeout(
        std::time::Duration::from_millis(700),
        TcpStream::connect((host, port)),
    )
    .await
    .map_err(|_| format!("connect {host}:{port}: таймаут"))?
    .map_err(|e| format!("connect {host}:{port}: {e}"))?;
    let mut stream = connect;
    let auth = match token {
        Some(token) if !token.is_empty() => format!("Authorization: Bearer {token}\r\n"),
        _ => String::new(),
    };
    let request = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\n{auth}Connection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    let mut body = String::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        stream.read_to_string(&mut body),
    )
    .await
    .map_err(|_| format!("{host}:{port}: нет ответа"))?
    .map_err(|e| e.to_string())?;
    let (head, payload) = body.split_once("\r\n\r\n").ok_or("ответ без заголовков")?;
    if !head.starts_with("HTTP/1.1 200") {
        // 401 означает именно то, что написано: токен не тот или не задан.
        let hint = if head.starts_with("HTTP/1.1 401") {
            "; set metrics-token in server.properties"
        } else {
            ""
        };
        return Err(format!(
            "сервер метрик ответил: {}{hint}",
            head.lines().next().unwrap_or("")
        ));
    }
    serde_json::from_str(payload).map_err(|e| format!("{path}: {e}"))
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
            Access::local(),
        )
        .await
        .unwrap();
        let fetched = fetch("127.0.0.1", port, None).await.unwrap();
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
            Access::local(),
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
            Access::local(),
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
            Access::local(),
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
            Access::local(),
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

        // Второй кадр — профиль: пустой на старте, но отдельным событием.
        let mut prof = String::new();
        loop {
            let mut chunk = [0u8; 512];
            let read =
                tokio::time::timeout(std::time::Duration::from_secs(2), stream.read(&mut chunk))
                    .await
                    .expect("кадр профиля")
                    .unwrap();
            assert!(read > 0, "соединение закрылось");
            prof.push_str(&String::from_utf8_lossy(&chunk[..read]));
            if prof.contains("event: profile") {
                break;
            }
        }

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

    /// Профиль по фазам: пустой на старте, наполняется отдельно от снимка.
    #[tokio::test]
    async fn profile_endpoint_follows_tick_marks() {
        let live = shared();
        let port = spawn(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            live.clone(),
            Access::local(),
        )
        .await
        .unwrap();
        let body = get(port, "/api/profile").await;
        assert!(body.ends_with("[]"), "профиль пуст до первых тиков");

        let mut marks = [0.0_f32; crate::profile::PHASE_COUNT];
        marks[crate::profile::PH_MOBS] = 2.5;
        marks[crate::profile::PH_FLUIDS] = 0.5;
        {
            let mut guard = live.write().unwrap();
            guard.publish_profile(77, &marks);
            guard.publish_profile(77, &marks);
            guard.publish_profile(78, &marks);
        }

        let body = get(port, "/api/profile").await;
        let (_, payload) = body.split_once("\r\n\r\n").unwrap();
        let buckets: Vec<crate::profile::ProfileBucket> = serde_json::from_str(payload).unwrap();
        assert_eq!(buckets.len(), 1, "закрыта только секунда 77");
        assert_eq!(buckets[0].at, 77);
        assert_eq!(buckets[0].ticks, 2);
        assert!((buckets[0].total[crate::profile::PH_MOBS] - 5.0).abs() < 1e-4);
        assert!((buckets[0].share(crate::profile::PH_MOBS) - 5.0 / 6.0).abs() < 1e-4);
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
        // `from_std` требует неблокирующий сокет, иначе tokio паникует при
        // регистрации. Порт от этого не освобождается — он всё равно занят.
        busy.set_nonblocking(true).unwrap();
        let busy = TcpListener::from_std(busy).unwrap();

        let snapshot = shared();
        let bound = spawn(localhost, port, snapshot, Access::local())
            .await
            .unwrap();
        assert_eq!(bound, port + 1, "занятый порт пропущен");

        // `fetch` с настройкой на занятый порт тоже находит сервер.
        let fetched = fetch("127.0.0.1", port, None).await.unwrap();
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

    /// Запрос с заголовками и query, как его пришлёт браузер.
    async fn request(port: u16, raw_head: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let request = format!("{raw_head}\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut body = String::new();
        stream.read_to_string(&mut body).await.unwrap();
        body
    }

    /// Без токена и без loopback сервер не стартует, а не открывает всё.
    #[tokio::test]
    async fn network_bind_without_token_is_refused() {
        let error = spawn(
            std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
            0,
            shared(),
            Access::local(),
        )
        .await
        .expect_err("внешний bind без токена должен быть отклонён");
        assert!(error.contains("metrics-token"), "{error}");
    }

    /// Loopback без токена работает как раньше: настройка не мешает.
    #[tokio::test]
    async fn loopback_does_not_need_token() {
        let port = spawn(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            shared(),
            Access::local(),
        )
        .await
        .unwrap();
        let body = get(port, "/api/snapshot").await;
        assert!(body.starts_with("HTTP/1.1 200"));
    }

    /// С токеном: без него 401 и форма входа, с ним — данные.
    #[tokio::test]
    async fn token_gates_every_route() {
        let port = spawn(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            shared(),
            Access::with_token("0123456789abcdef"),
        )
        .await
        .unwrap();

        let denied = request(port, "GET /api/snapshot HTTP/1.1\r\nHost: 127.0.0.1").await;
        assert!(denied.starts_with("HTTP/1.1 401"), "{}", &denied[..40]);
        assert!(denied.contains("<form"), "должна быть форма входа");

        let bearer = request(
            port,
            "GET /api/snapshot HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer 0123456789abcdef",
        )
        .await;
        assert!(bearer.starts_with("HTTP/1.1 200"));

        let cookie = request(
            port,
            "GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nCookie: cw_metrics=0123456789abcdef",
        )
        .await;
        assert!(cookie.starts_with("HTTP/1.1 200"));
        assert!(cookie.contains("<title>Chunkwake"));

        // Неверный токен — тот же отказ, что и отсутствие.
        let wrong = request(
            port,
            "GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer 0123456789abcdee",
        )
        .await;
        assert!(wrong.starts_with("HTTP/1.1 401"));
    }

    /// Токен в ссылке меняется на куку и уводит на адрес без токена: иначе
    /// он остался бы в истории браузера и в логах прокси.
    #[tokio::test]
    async fn token_in_link_becomes_cookie() {
        let port = spawn(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            shared(),
            Access::with_token("0123456789abcdef"),
        )
        .await
        .unwrap();
        let body = request(
            port,
            "GET /?token=0123456789abcdef HTTP/1.1\r\nHost: 127.0.0.1",
        )
        .await;
        assert!(body.starts_with("HTTP/1.1 302"), "{}", &body[..40]);
        assert!(body.contains("Location: /\r\n"), "{body}");
        assert!(body.contains("Set-Cookie: cw_metrics=0123456789abcdef"));
        assert!(body.contains("HttpOnly"));
        assert!(body.contains("SameSite=Strict"));
    }

    /// Чужой Origin отклоняется даже с верным токеном: иначе страница на
    /// другом сайте дёргала бы дашборд из браузера жертвы.
    #[tokio::test]
    async fn foreign_origin_is_forbidden() {
        let port = spawn(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            shared(),
            Access::with_token("0123456789abcdef"),
        )
        .await
        .unwrap();
        let body = request(
            port,
            "GET /api/snapshot HTTP/1.1\r\nHost: 127.0.0.1\r\nOrigin: http://evil.example\r\n\
             Authorization: Bearer 0123456789abcdef",
        )
        .await;
        assert!(body.starts_with("HTTP/1.1 403"), "{}", &body[..40]);

        let same = request(
            port,
            "GET /api/snapshot HTTP/1.1\r\nHost: 127.0.0.1\r\nOrigin: http://127.0.0.1:25585\r\n\
             Authorization: Bearer 0123456789abcdef",
        )
        .await;
        assert!(same.starts_with("HTTP/1.1 200"), "{}", &same[..40]);
    }

    /// Страница сжимается по запросу клиента и отдаётся как есть без него.
    #[tokio::test]
    async fn page_is_gzipped_when_accepted() {
        let port = spawn(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            shared(),
            Access::local(),
        )
        .await
        .unwrap();

        let plain = request(port, "GET / HTTP/1.1\r\nHost: x").await;
        assert!(!plain.contains("Content-Encoding"));

        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        stream
            .write_all(
                b"GET / HTTP/1.1\r\nHost: x\r\nAccept-Encoding: gzip, deflate\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).await.unwrap();
        let split = raw
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .expect("заголовки");
        let head = String::from_utf8_lossy(&raw[..split]);
        assert!(head.contains("Content-Encoding: gzip"), "{head}");
        assert!(head.contains("Vary: Accept-Encoding"));
        // Тело действительно gzip: распаковывается в исходную страницу.
        use std::io::Read;
        let mut decoded = String::new();
        flate2::read::GzDecoder::new(&raw[split + 4..])
            .read_to_string(&mut decoded)
            .expect("gzip");
        assert!(decoded.contains("<title>Chunkwake"));
        assert!(
            raw.len() < web::index_raw().len(),
            "сжатие должно уменьшать"
        );
    }

    /// Сравнение токена не выдаёт длину совпадения по времени.
    #[test]
    fn token_comparison_is_exact() {
        let access = Access::with_token("0123456789abcdef");
        assert!(access.matches("0123456789abcdef"));
        assert!(!access.matches("0123456789abcde"));
        assert!(!access.matches("0123456789abcdef0"));
        assert!(!access.matches(""));
        // Пустой настроенный токен не совпадает ни с чем: иначе доступ был
        // бы открыт пустой строкой.
        assert!(!Access::local().matches(""));
        assert!(!Access::local().matches("x"));
    }
}
