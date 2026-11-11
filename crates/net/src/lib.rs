//! chunkwake-net — приём соединений и состояния handshake/status/login.

pub mod anvil;
pub mod arrows;
pub mod auth;
pub mod campfire;
pub mod chunkgen;
pub mod click;
pub mod configuration;
pub mod crypto;
pub mod enchanting;
pub mod fluids;
pub mod furnace;
pub mod grindstone;
pub mod hoppers;
pub mod items;
pub mod live;
pub mod loot;
pub mod maps;
pub mod minecart;
pub mod mobs;
pub mod ops;
pub mod orbs;
pub mod payload;
pub mod pistons;
pub mod placement;
mod plants;
pub mod play;
pub mod potions;
pub mod redstone;
pub mod smithing;
mod soil;
pub mod sounds;
pub mod stack_nbt;
pub mod survival;
pub mod swing;
pub mod ticks;
pub mod weather;
pub mod xp;

use bytes::BytesMut;
use chunkwake_protocol::ProtocolError;
use chunkwake_protocol::frame::FrameCodec;
use chunkwake_protocol::packets::{
    self, Handshake, LoginC2S, NextState, ProfileProperty, StatusC2S,
};
use crypto::{ServerKey, StreamCipher};
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
pub use uuid::Uuid;

/// Версия игры, на которую рассчитан сервер (из `version.json` ванильного jar).
#[derive(Clone, Debug)]
pub struct GameVersion {
    pub name: String,
    pub protocol: i32,
}

#[derive(Clone, Debug)]
pub struct NetConfig {
    pub version: GameVersion,
    pub motd: String,
    pub max_players: usize,
    /// Порог сжатия; отрицательное значение выключает сжатие.
    pub compression_threshold: i32,
    pub online_mode: bool,
    /// Секрет modern Velocity forwarding. `Some` — вход принимается только
    /// от Velocity с подписанным `velocity:player_info`; Mojang session server
    /// при этом не вызывается.
    pub velocity_secret: Option<Vec<u8>>,
    /// Разрешает status ping в списке серверов.
    pub enable_status: bool,
    /// Базовый URL session server (по умолчанию Mojang).
    pub session_server: String,
    /// Фаза configuration; `None` — отдать соединение сразу после login.
    pub configuration: Option<Arc<configuration::ConfigurationPlan>>,
    /// Проверка входа (бан, белый список, места, плагины). `None` —
    /// проверяется только число мест.
    pub admit: Option<Admit>,
}

/// Проверка входа после авторизации: имя, uuid, адрес, свойства профиля и
/// «мест нет». `Some(json причины)` — отказать (JSON-компонент).
pub type AdmitFn =
    Arc<dyn Fn(&str, Uuid, SocketAddr, &[ProfileProperty], bool) -> Option<String> + Send + Sync>;

/// Проверка входа. Отдельный тип, потому что замыкание не реализует `Debug`.
#[derive(Clone)]
pub struct Admit(pub AdmitFn);

impl std::fmt::Debug for Admit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Admit(..)")
    }
}

/// Игрок, прошедший login и перешедший в состояние configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoggedIn {
    pub name: String,
    pub uuid: Uuid,
    pub addr: SocketAddr,
    pub properties: Vec<ProfileProperty>,
    /// Настройки клиента из configuration (обзор, рука, части скина).
    pub client: chunkwake_protocol::configuration::ClientInfo,
    /// Каналы плагинов, объявленные клиентом в configuration.
    pub channels: Vec<String>,
}

#[derive(Debug)]
pub enum NetError {
    Io(std::io::Error),
    Protocol(ProtocolError),
    Timeout,
}

impl From<std::io::Error> for NetError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<ProtocolError> for NetError {
    fn from(e: ProtocolError) -> Self {
        Self::Protocol(e)
    }
}

pub struct Server {
    config: NetConfig,
    /// Входящие и играющие сессии занимают место до закрытия соединения.
    admission: AtomicUsize,
    /// Только играющие сессии показываются в статусе сервера.
    online: AtomicUsize,
    key: Option<ServerKey>,
    http: reqwest::Client,
    /// Случайный UUID сессии сервера; клиент получает его в login_finished.
    session_id: Uuid,
    /// Перевод протокола для клиентов других версий (ставит мост с ViaVersion).
    translator: std::sync::OnceLock<Arc<dyn translate::Translator>>,
    /// Незавершённые входы по адресу: один IP не должен занимать весь
    /// глобальный `MAX_PENDING_LOGINS`.
    pending_by_ip: std::sync::Mutex<std::collections::HashMap<std::net::IpAddr, usize>>,
}

const READ_TIMEOUT: Duration = Duration::from_secs(30);
/// Клиент, переставший читать, держал `flush` вечно: сессия не видела ни
/// таймаута keep-alive, ни кика, ни остановки сервера.
const WRITE_TIMEOUT: Duration = Duration::from_secs(30);
/// Срок от соединения до входа в игру (рукопожатие, вход, конфигурация).
const LOGIN_TIMEOUT: Duration = Duration::from_secs(120);
/// Сколько соединений одновременно может находиться на пути к игре.
const MAX_PENDING_LOGINS: usize = 1024;
/// Сколько незавершённых входов допускается с одного IP. Остальные
/// соединения с этого адреса отбрасываются до чтения рукопожатия.
const MAX_PENDING_PER_IP: usize = 8;
/// Первая версия протокола со сжатием (1.8).
const COMPRESSION_SINCE: i32 = 47;

impl Server {
    pub fn new(config: NetConfig) -> Arc<Self> {
        Arc::new(Self {
            key: (config.online_mode && config.velocity_secret.is_none()).then(ServerKey::generate),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("HTTP client"),
            config,
            admission: AtomicUsize::new(0),
            online: AtomicUsize::new(0),
            session_id: uuid::Builder::from_random_bytes(rand::random()).into_uuid(),
            translator: std::sync::OnceLock::new(),
            pending_by_ip: std::sync::Mutex::new(std::collections::HashMap::new()),
        })
    }

    /// Ставит переводчик протокола: клиенты других версий пойдут через него.
    /// Ставится один раз (мост поднимает ViaVersion после старта сети).
    pub fn set_translator(&self, translator: Arc<dyn translate::Translator>) -> bool {
        self.translator.set(translator).is_ok()
    }

    /// Берёт место незавершённого входа для `ip`; `false` — с адреса уже
    /// висит `MAX_PENDING_PER_IP` соединений.
    fn reserve_pending_ip(&self, ip: std::net::IpAddr) -> bool {
        let mut pending = self.pending_by_ip.lock().unwrap();
        let slot = pending.entry(ip).or_insert(0);
        if *slot >= MAX_PENDING_PER_IP {
            return false;
        }
        *slot += 1;
        true
    }

    fn release_pending_ip(&self, ip: std::net::IpAddr) {
        let mut pending = self.pending_by_ip.lock().unwrap();
        if let Some(slot) = pending.get_mut(&ip) {
            *slot -= 1;
            if *slot == 0 {
                pending.remove(&ip);
            }
        }
    }

    /// Принимает соединения бесконечно; каждый вошедший игрок передаётся в `on_login`.
    pub async fn serve<F>(
        self: Arc<Self>,
        listener: TcpListener,
        on_login: F,
    ) -> std::io::Result<()>
    where
        F: Fn(LoggedIn, Connection) + Send + Sync + 'static,
    {
        let (_tx, rx) = tokio::sync::watch::channel(false);
        self.serve_until(listener, rx, on_login).await
    }

    /// Останавливает приём и незавершённые входы при сигнале shutdown.
    pub async fn serve_until<F>(
        self: Arc<Self>,
        listener: TcpListener,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
        on_login: F,
    ) -> std::io::Result<()>
    where
        F: Fn(LoggedIn, Connection) + Send + Sync + 'static,
    {
        let on_login = Arc::new(on_login);
        let mut handshakes = tokio::task::JoinSet::new();
        loop {
            let (stream, addr) = tokio::select! {
                result = listener.accept() => result?,
                _ = shutdown.changed() => {
                    if *shutdown.borrow() { break; }
                    continue;
                }
                Some(_) = handshakes.join_next(), if !handshakes.is_empty() => continue,
            };
            // Незавершённых входов не больше предела: иначе поток соединений,
            // не дошедших до игры, съедал дескрипторы и память.
            if handshakes.len() >= MAX_PENDING_LOGINS {
                tracing::debug!(%addr, "too many pending logins, connection dropped");
                continue;
            }
            if !self.reserve_pending_ip(addr.ip()) {
                tracing::debug!(%addr, "too many pending logins from this IP, connection dropped");
                continue;
            }
            let ip_pending = IpPending {
                server: self.clone(),
                ip: addr.ip(),
            };
            let _ = stream.set_nodelay(true);
            let server = self.clone();
            let on_login = on_login.clone();
            handshakes.spawn(async move {
                let _ip_pending = ip_pending;
                // Срок на весь вход, а не на каждое чтение: байт раз в 29 секунд
                // держал соединение до игры бесконечно.
                match tokio::time::timeout(LOGIN_TIMEOUT, server.handle(stream, addr)).await {
                    Ok(Ok(Some((player, mut conn)))) => {
                        conn.online
                            .as_mut()
                            .expect("login reserved a slot")
                            .activate();
                        on_login(player, conn)
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(e)) => tracing::debug!(%addr, ?e, "connection closed"),
                    Err(_) => tracing::debug!(%addr, "login timed out"),
                }
            });
        }
        handshakes.abort_all();
        while handshakes.join_next().await.is_some() {}
        Ok(())
    }

    async fn handle(
        self: &Arc<Self>,
        stream: TcpStream,
        addr: SocketAddr,
    ) -> Result<Option<(LoggedIn, Connection)>, NetError> {
        let mut conn = Connection::new(stream);
        let mut packet = conn.read_packet().await?;
        let mut hs = Handshake::decode(&packet)?;
        // Клиент другой версии: если переводчик её знает, всё дальнейшее
        // (и сам handshake — его версию переводчик переписывает на нашу)
        // идёт через сессию перевода.
        if hs.protocol_version != self.config.version.protocol
            && let Some(translator) = self.translator.get()
            && let Some(id) = translator.open(hs.protocol_version, addr)
        {
            tracing::debug!(%addr, protocol = hs.protocol_version, "translated connection");
            conn.translate_with(translator.clone(), id, hs.protocol_version);
            conn.receive_translated(&packet)?;
            packet = conn.read_packet().await?;
            hs = Handshake::decode(&packet)?;
        }
        match hs.next_state {
            NextState::Status => {
                if self.config.enable_status {
                    self.status(&mut conn).await?;
                }
                Ok(None)
            }
            NextState::Login | NextState::Transfer => {
                if hs.protocol_version != self.config.version.protocol {
                    let msg = format!(
                        "Unsupported client version. Please use {}",
                        self.config.version.name
                    );
                    conn.send(&packets::login_disconnect(&text_json(&msg)))
                        .await?;
                    return Ok(None);
                }
                self.login(conn, addr).await
            }
        }
    }

    fn status_json(&self) -> String {
        serde_json::json!({
            "version": { "name": self.config.version.name, "protocol": self.config.version.protocol },
            "players": {
                "max": self.config.max_players,
                "online": self.online.load(Ordering::Relaxed),
                "sample": []
            },
            "description": { "text": self.config.motd },
            "enforcesSecureChat": false
        })
        .to_string()
    }

    async fn status(&self, conn: &mut Connection) -> Result<(), NetError> {
        // Ответ на статус один на соединение, как в `ServerStatusPacketListenerImpl`:
        // второй запрос — разрыв, а не бесконечная раздача JSON.
        let mut answered = false;
        loop {
            let packet = conn.read_packet().await?;
            match StatusC2S::decode(&packet)? {
                StatusC2S::Request => {
                    if std::mem::replace(&mut answered, true) {
                        return Ok(());
                    }
                    conn.send(&packets::status_response(&self.status_json()))
                        .await?
                }
                StatusC2S::Ping(payload) => {
                    conn.send(&packets::pong(payload)).await?;
                    return Ok(());
                }
            }
        }
    }

    async fn login(
        self: &Arc<Self>,
        mut conn: Connection,
        addr: SocketAddr,
    ) -> Result<Option<(LoggedIn, Connection)>, NetError> {
        let packet = conn.read_packet().await?;
        let LoginC2S::Start { name, .. } = LoginC2S::decode(&packet)? else {
            return Ok(None);
        };
        let name = name.to_owned();
        if !valid_name(&name) {
            conn.send(&packets::login_disconnect(&text_json("Invalid username")))
                .await?;
            return Ok(None);
        }
        let (name, uuid, properties) = if let Some(secret) = &self.config.velocity_secret {
            match self.forward_velocity(&mut conn, secret).await? {
                Some(profile) => (profile.name, profile.uuid, profile.properties),
                None => return Ok(None),
            }
        } else {
            match &self.key {
                Some(key) => match self.authenticate(&mut conn, key, &name).await? {
                    Some(profile) => (profile.name, profile.uuid, profile.properties),
                    None => return Ok(None),
                },
                None => {
                    let uuid = packets::offline_uuid(&name);
                    (name, uuid, Vec::new())
                }
            }
        };
        // Как `PlayerList.canPlayerLogin`: бан, белый список, места; решение
        // могут сменить плагины (`AsyncPlayerPreLoginEvent`, `PlayerLoginEvent`).
        // Бронируем место атомарно до асинхронной проверки плагинов: два
        // одновременных входа больше не получают последнее место вместе.
        let slot = OnlineGuard::reserve(self.clone());
        let full = slot.is_none();
        let denied = match &self.config.admit {
            Some(admit) => {
                let admit = admit.0.clone();
                let (n, p) = (name.clone(), properties.clone());
                // Плагины могут думать долго (база данных): поток рантайма не держим.
                match tokio::task::spawn_blocking(move || admit(&n, uuid, addr, &p, full)).await {
                    Ok(decision) => decision,
                    Err(error) => {
                        tracing::error!(%error, "login admission failed");
                        Some(text_json("Internal server error"))
                    }
                }
            }
            None => full.then(|| text_json("The server is full!")),
        };
        if let Some(reason) = denied {
            conn.send(&packets::login_disconnect(&reason)).await?;
            return Ok(None);
        }
        // Плагин вправе снять отказ из-за полного сервера, как PlayerLoginEvent.
        conn.online = Some(slot.unwrap_or_else(|| OnlineGuard::override_full(self.clone())));

        // Клиенты до 1.8 (протокол 47) сжатия не знают: ViaRewind гасит пакет
        // set_compression, и кадры должны остаться несжатыми.
        if self.config.compression_threshold >= 0
            && conn
                .client_protocol()
                .is_none_or(|p| p >= COMPRESSION_SINCE)
        {
            conn.send(&packets::set_compression(self.config.compression_threshold))
                .await?;
            conn.codec.threshold = Some(self.config.compression_threshold as usize);
        }
        conn.send(&packets::login_success(
            uuid,
            &name,
            &properties,
            self.session_id,
        ))
        .await?;

        loop {
            let packet = conn.read_packet().await?;
            match LoginC2S::decode(&packet)? {
                LoginC2S::Acknowledged => break,
                LoginC2S::PluginResponse { .. } | LoginC2S::CookieResponse => continue,
                _ => return Ok(None),
            }
        }
        tracing::info!(%name, %uuid, %addr, "player logged in");
        let mut client = chunkwake_protocol::configuration::ClientInfo::DEFAULT;
        let mut channels = Vec::new();
        if let Some(plan) = &self.config.configuration {
            match plan.run(&mut conn).await? {
                configuration::Outcome::Finished(info, registered) => {
                    tracing::info!(%name, "configuration finished");
                    client = info;
                    channels = registered;
                }
                configuration::Outcome::Rejected => return Ok(None),
            }
        }
        Ok(Some((
            LoggedIn {
                name,
                uuid,
                addr,
                properties,
                client,
                channels,
            },
            conn,
        )))
    }

    /// Modern Velocity forwarding: запрос `velocity:player_info` и проверка
    /// HMAC-SHA256 от общего секрета. `Ok(None)` — прокси/клиент отключён.
    async fn forward_velocity(
        &self,
        conn: &mut Connection,
        secret: &[u8],
    ) -> Result<Option<auth::GameProfile>, NetError> {
        const MESSAGE_ID: i32 = 0;
        conn.send(&packets::login_plugin_request(
            MESSAGE_ID,
            velocity::CHANNEL,
            &[],
        ))
        .await?;
        loop {
            let packet = conn.read_packet().await?;
            match LoginC2S::decode(&packet)? {
                LoginC2S::PluginResponse { message_id, data } if message_id == MESSAGE_ID => {
                    let Some(data) = data else {
                        tracing::info!("velocity forwarding: proxy declined the request");
                        conn.send(&packets::login_disconnect(&text_json(
                            "Invalid proxy forwarding data",
                        )))
                        .await?;
                        return Ok(None);
                    };
                    return match velocity::parse(secret, data) {
                        Ok(profile) => Ok(Some(profile)),
                        Err(error) => {
                            tracing::info!(%error, "velocity forwarding rejected");
                            conn.send(&packets::login_disconnect(&text_json(
                                "Invalid proxy forwarding data",
                            )))
                            .await?;
                            Ok(None)
                        }
                    };
                }
                // Cookie-ответы не относятся к профилю; остальные пакеты в
                // этом состоянии означают, что вход идёт не через Velocity.
                LoginC2S::CookieResponse => continue,
                _ => {
                    tracing::info!("velocity forwarding: unexpected login packet");
                    conn.send(&packets::login_disconnect(&text_json(
                        "This server requires Velocity player info forwarding",
                    )))
                    .await?;
                    return Ok(None);
                }
            }
        }
    }

    /// Обмен ключами, включение шифрования и проверка через session server.
    /// `Ok(None)` — игрок отключён с сообщением.
    async fn authenticate(
        &self,
        conn: &mut Connection,
        key: &ServerKey,
        name: &str,
    ) -> Result<Option<auth::GameProfile>, NetError> {
        let verify_token: [u8; 4] = rand::random();
        conn.send(&packets::encryption_request(
            key.public_der(),
            &verify_token,
            true,
        ))
        .await?;
        let packet = conn.read_packet().await?;
        let LoginC2S::EncryptionResponse {
            shared_secret,
            verify_token: token,
        } = LoginC2S::decode(&packet)?
        else {
            return Ok(None);
        };
        let secret = key
            .decrypt(shared_secret)
            .and_then(|s| <[u8; 16]>::try_from(s).ok());
        let token_ok = key.decrypt(token).is_some_and(|t| t == verify_token);
        let (Some(secret), true) = (secret, token_ok) else {
            return Ok(None);
        };
        // С этого момента весь трафик (в т.ч. disconnect) шифруется.
        conn.enable_encryption(&secret);

        let hash = packets::server_hash("", &secret, key.public_der());
        match auth::has_joined(&self.http, &self.config.session_server, name, &hash).await {
            Ok(profile) => Ok(Some(profile)),
            Err(e) => {
                tracing::info!(%name, ?e, "authentication failed");
                let msg = match e {
                    auth::AuthError::NotAuthenticated => "Failed to verify username!",
                    auth::AuthError::Http(_) => {
                        "Authentication servers are down. Please try again later, sorry!"
                    }
                };
                conn.send(&packets::login_disconnect(&text_json(msg)))
                    .await?;
                Ok(None)
            }
        }
    }
}

fn valid_name(name: &str) -> bool {
    (1..=16).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn text_json(text: &str) -> String {
    serde_json::json!({ "text": text }).to_string()
}

/// Соединение с клиентом: шифрование, фрейминг и сжатие поверх TCP.
pub struct Connection {
    stream: TcpStream,
    buf: BytesMut,
    pub codec: FrameCodec,
    cipher: Option<StreamCipher>,
    /// Закодированные, ещё не отправленные кадры (шифруются при `flush`).
    out: Vec<u8>,
    /// Начало `out`, записанное до включения шифрования: уходит открытым.
    plain: usize,
    /// Сколько байт уже ушло в сокет: по разнице видно объём записи.
    written_bytes: u64,
    /// Перевод протокола для клиента другой версии.
    via: Option<translate::Session>,
    /// Переведённые пакеты от клиента, которые сервер ещё не прочитал.
    inbox: VecDeque<Vec<u8>>,
    /// Место игрока на сервере: занято, пока жива сессия (`max-players`, статус).
    online: Option<OnlineGuard>,
}

/// Место незавершённого входа для одного IP; освобождается при завершении
/// задачи рукопожатия/входа.
struct IpPending {
    server: Arc<Server>,
    ip: std::net::IpAddr,
}

impl Drop for IpPending {
    fn drop(&mut self) {
        self.server.release_pending_ip(self.ip);
    }
}

/// Бронь места на время входа и игры; освобождается при закрытии соединения.
struct OnlineGuard {
    server: Arc<Server>,
    active: bool,
}

impl OnlineGuard {
    fn reserve(server: Arc<Server>) -> Option<Self> {
        server
            .admission
            .try_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < server.config.max_players).then_some(count + 1)
            })
            .ok()?;
        Some(Self {
            server,
            active: false,
        })
    }

    fn override_full(server: Arc<Server>) -> Self {
        server.admission.fetch_add(1, Ordering::AcqRel);
        Self {
            server,
            active: false,
        }
    }

    fn activate(&mut self) {
        debug_assert!(!self.active);
        self.active = true;
        self.server.online.fetch_add(1, Ordering::Relaxed);
    }
}

impl Drop for OnlineGuard {
    fn drop(&mut self) {
        if self.active {
            self.server.online.fetch_sub(1, Ordering::Relaxed);
        }
        self.server.admission.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Неотправленный хвост (причина отключения после `send` и `return`) —
/// без ожидания, сколько примет сокет.
impl Drop for Connection {
    fn drop(&mut self) {
        if let Some(s) = &self.via {
            s.translator.close(s.id);
        }
        if self.out.is_empty() {
            return;
        }
        if let Some(c) = &mut self.cipher {
            c.encrypt(&mut self.out[self.plain..]);
        }
        let mut rest = &self.out[..];
        while !rest.is_empty() {
            match self.stream.try_write(rest) {
                Ok(n) if n > 0 => rest = &rest[n..],
                _ => break,
            }
        }
    }
}

/// Буфер вывода сбрасывается, дорастая до этого размера.
const OUT_LIMIT: usize = 64 * 1024;
/// Сколько памяти буфера держать после сброса.
const OUT_KEEP: usize = 256 * 1024;
/// Выше этого буфер ужимается: пачка чанков при входе игрока — единицы
/// мегабайт, и держать их за каждым соединением до конца сессии незачем.
///
/// Раньше ужимали всё, что больше `OUT_KEEP`, то есть на каждой пачке
/// 257 КиБ — а это переезд буфера и `memcpy`. Обычная пачка чанков при
/// входе (96 × ~4.7 КиБ) в новый порог влезает и остаётся на месте.
const OUT_MAX_KEEP: usize = 256 * 1024;

impl Connection {
    pub fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            buf: BytesMut::with_capacity(4096),
            codec: FrameCodec::default(),
            cipher: None,
            out: Vec::new(),
            plain: 0,
            written_bytes: 0,
            via: None,
            inbox: VecDeque::new(),
            online: None,
        }
    }

    /// Дальше пакеты соединения идут через переводчик (сессия `id`).
    pub fn translate_with(
        &mut self,
        translator: Arc<dyn translate::Translator>,
        id: u64,
        protocol: i32,
    ) {
        self.via = Some(translate::Session {
            id,
            translator,
            protocol,
        });
    }

    /// Версия протокола клиента, если соединение переводится.
    pub fn client_protocol(&self) -> Option<i32> {
        self.via.as_ref().map(|s| s.protocol)
    }

    /// Пакет от клиента — через переводчик; результат ждёт в `inbox`
    /// (серверу) и в буфере записи (ответы переводчика клиенту).
    pub fn receive_translated(&mut self, packet: &[u8]) -> Result<(), NetError> {
        self.translate(translate::Dir::Serverbound, Some(packet))
    }

    /// Один вызов переводчика: `packet` — перевести, `None` — забрать то,
    /// что переводчик отправил сам.
    fn translate(&mut self, dir: translate::Dir, packet: Option<&[u8]>) -> Result<(), NetError> {
        let Some(s) = &self.via else {
            if let Some(p) = packet {
                match dir {
                    translate::Dir::Serverbound => self.inbox.push_back(p.to_vec()),
                    translate::Dir::Clientbound => self.codec.encode(p, &mut self.out),
                }
            }
            return Ok(());
        };
        let (translator, id) = (s.translator.clone(), s.id);
        let mut out = Vec::new();
        let result = match packet {
            Some(p) => translator.translate(id, dir, p, &mut out),
            None => translator.poll(id, &mut out),
        };
        for (dir, p) in out {
            match dir {
                translate::Dir::Serverbound => self.inbox.push_back(p),
                translate::Dir::Clientbound => self.codec.encode(&p, &mut self.out),
            }
        }
        // Причину отключения переводчик положил в буфер: её допишет `Drop`.
        result.map_err(|reason| {
            NetError::Io(std::io::Error::new(
                std::io::ErrorKind::ConnectionAborted,
                reason,
            ))
        })
    }

    /// Кадры, закодированные для всех (сжатые общим порогом), — по одному
    /// пакету через переводчик.
    fn send_frames_translated(&mut self, frames: &[u8]) -> Result<(), NetError> {
        let mut buf = BytesMut::from(frames);
        while let Some(packet) = self.codec.decode(&mut buf)? {
            self.translate(translate::Dir::Clientbound, Some(&packet))?;
        }
        Ok(())
    }

    /// Включает AES/CFB8 для обоих направлений.
    pub fn enable_encryption(&mut self, secret: &[u8; 16]) {
        self.plain = self.out.len();
        self.cipher = Some(StreamCipher::new(secret));
    }

    /// Сколько байт уже ушло в сокет: по разнице видно объём записи.
    pub fn written(&self) -> u64 {
        self.written_bytes
    }

    pub async fn read_packet(&mut self) -> Result<Vec<u8>, NetError> {
        loop {
            if let Some(packet) = self.inbox.pop_front() {
                return Ok(packet);
            }
            // Ответ ждут на отправленное: сначала оно должно уйти.
            self.flush().await?;
            if let Some(packet) = self.codec.decode(&mut self.buf)? {
                if self.via.is_none() {
                    return Ok(packet);
                }
                // Переводчик мог съесть пакет (0 на выходе) или ответить
                // клиенту сам — тогда снова на круг.
                self.receive_translated(&packet)?;
                continue;
            }
            let start = self.buf.len();
            let n = tokio::time::timeout(READ_TIMEOUT, self.stream.read_buf(&mut self.buf))
                .await
                .map_err(|_| NetError::Timeout)??;
            if n == 0 {
                return Err(NetError::Io(std::io::ErrorKind::UnexpectedEof.into()));
            }
            if let Some(c) = &mut self.cipher {
                c.decrypt(&mut self.buf[start..]);
            }
        }
    }

    /// Отправляет уже закодированные `codec` кадры (общие для многих соединений):
    /// сжатие сделано один раз, здесь только шифрование и запись.
    ///
    /// Кадры пишутся по одному, без сборки в промежуточный буфер: пачка
    /// чанков — это сотни килобайт, и склеивать её только ради того, чтобы
    /// тут же скопировать в буфер соединения (под шифрование), значило
    /// платить двумя копиями вместо одной.
    pub async fn send_frames(&mut self, frames: &[u8]) -> Result<(), NetError> {
        if frames.is_empty() {
            return Ok(());
        }
        if self.via.is_some() {
            self.send_frames_translated(frames)?;
            return self.flush().await;
        }
        if self.out.is_empty() && self.cipher.is_none() {
            tokio::time::timeout(WRITE_TIMEOUT, self.stream.write_all(frames))
                .await
                .map_err(|_| NetError::Timeout)??;
            self.written_bytes += frames.len() as u64;
            return Ok(());
        }
        // Копия всё равно нужна: шифрование идёт на месте, по буферу записи.
        // Но она одна — та, что делает `extend_from_slice`, без промежуточной
        // сборки у вызывающего.
        self.out.extend_from_slice(frames);
        self.flush().await
    }

    /// Пишет кадр, который уже лежит готовым у отправителя: буфер соединения
    /// освобождается до копирования, чтобы не держать две копии пачки.
    pub async fn send_owned_frames(&mut self, mut frame: Vec<u8>) -> Result<(), NetError> {
        if self.via.is_some() {
            self.send_frames_translated(&frame)?;
            return self.flush().await;
        }
        if self.out.is_empty() && self.cipher.is_none() {
            tokio::time::timeout(WRITE_TIMEOUT, self.stream.write_all(&frame))
                .await
                .map_err(|_| NetError::Timeout)??;
            self.written_bytes += frame.len() as u64;
            return Ok(());
        }
        self.flush().await?;
        std::mem::swap(&mut self.out, &mut frame);
        self.flush().await
    }

    /// Кадр в буфер; уходит при `flush` (конец итерации сессии, чтение ответа)
    /// или при переполнении — один syscall на пачку, а не на пакет.
    pub async fn send(&mut self, packet: &[u8]) -> Result<(), NetError> {
        if self.via.is_some() {
            self.translate(translate::Dir::Clientbound, Some(packet))?;
        } else {
            self.codec.encode(packet, &mut self.out);
        }
        if self.out.len() >= OUT_LIMIT {
            self.flush().await?;
        }
        Ok(())
    }

    /// Шифрует буфер на месте и пишет его целиком.
    pub async fn flush(&mut self) -> Result<(), NetError> {
        if self.via.is_some() {
            // Что переводчик отправил сам (по таймеру) — с этой же записью.
            self.translate(translate::Dir::Clientbound, None)?;
        }
        if self.out.is_empty() {
            return Ok(());
        }
        if let Some(c) = &mut self.cipher {
            c.encrypt(&mut self.out[self.plain..]);
        }
        let written = self.out.len() as u64;
        let res = tokio::time::timeout(WRITE_TIMEOUT, self.stream.write_all(&self.out))
            .await
            .map_err(|_| NetError::Timeout);
        self.out.clear();
        self.plain = 0;
        if self.out.capacity() > OUT_MAX_KEEP {
            self.out.shrink_to(OUT_KEEP);
        }
        res??;
        self.written_bytes += written;
        Ok(())
    }
}
pub mod brewing;
pub mod environment;
pub mod fire;
pub mod tnt;
pub mod transform;
pub mod translate;
pub mod velocity;

#[cfg(test)]
mod admission_tests {
    use super::*;

    fn server(max_players: usize) -> Arc<Server> {
        Server::new(NetConfig {
            version: GameVersion {
                name: "26.3".into(),
                protocol: 777,
            },
            motd: String::new(),
            max_players,
            compression_threshold: -1,
            online_mode: false,
            velocity_secret: None,
            enable_status: false,
            session_server: String::new(),
            configuration: None,
            admit: None,
        })
    }

    #[test]
    fn concurrent_logins_share_only_one_free_slot() {
        let server = server(1);
        let release = Arc::new(std::sync::Barrier::new(17));
        let (tx, rx) = std::sync::mpsc::channel();
        let threads: Vec<_> = (0..16)
            .map(|_| {
                let server = server.clone();
                let release = release.clone();
                let tx = tx.clone();
                std::thread::spawn(move || {
                    let slot = OnlineGuard::reserve(server);
                    tx.send(slot.is_some()).unwrap();
                    release.wait();
                })
            })
            .collect();
        let accepted = (0..16).filter(|_| rx.recv().unwrap()).count();
        assert_eq!(accepted, 1);
        assert_eq!(server.admission.load(Ordering::Relaxed), 1);
        release.wait();
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(server.admission.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn plugin_override_and_disconnect_update_counters() {
        let server = server(1);
        let reserved = OnlineGuard::reserve(server.clone()).unwrap();
        assert!(OnlineGuard::reserve(server.clone()).is_none());
        let mut overridden = OnlineGuard::override_full(server.clone());
        overridden.activate();
        assert_eq!(server.online.load(Ordering::Relaxed), 1);
        assert_eq!(server.admission.load(Ordering::Relaxed), 2);
        drop(overridden);
        drop(reserved);
        assert_eq!(server.online.load(Ordering::Relaxed), 0);
        assert_eq!(server.admission.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn pending_logins_are_limited_per_ip() {
        let server = server(10);
        let ip: std::net::IpAddr = "192.0.2.1".parse().unwrap();
        let other: std::net::IpAddr = "192.0.2.2".parse().unwrap();
        let guards: Vec<IpPending> = (0..MAX_PENDING_PER_IP)
            .map(|_| {
                assert!(server.reserve_pending_ip(ip));
                IpPending {
                    server: server.clone(),
                    ip,
                }
            })
            .collect();

        assert!(!server.reserve_pending_ip(ip));
        assert!(server.reserve_pending_ip(other), "другой IP не затронут");

        drop(guards);
        assert!(
            server.reserve_pending_ip(ip),
            "после входа место снова свободно"
        );
    }
}
