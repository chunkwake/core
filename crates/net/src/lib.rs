//! chunkwake-net — приём соединений и состояния handshake/status/login.

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
pub mod mobs;
pub mod ops;
pub mod orbs;
pub mod pistons;
pub mod placement;
pub mod play;
pub mod potions;
pub mod redstone;
pub mod sounds;
pub mod stack_nbt;
pub mod survival;
pub mod ticks;
pub mod xp;

use bytes::BytesMut;
use chunkwake_protocol::ProtocolError;
use chunkwake_protocol::frame::FrameCodec;
use chunkwake_protocol::packets::{
    self, Handshake, LoginC2S, NextState, ProfileProperty, StatusC2S,
};
use crypto::{ServerKey, StreamCipher};
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
    /// Разрешает status ping в списке серверов.
    pub enable_status: bool,
    /// Базовый URL session server (по умолчанию Mojang).
    pub session_server: String,
    /// Фаза configuration; `None` — отдать соединение сразу после login.
    pub configuration: Option<Arc<configuration::ConfigurationPlan>>,
}

/// Игрок, прошедший login и перешедший в состояние configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoggedIn {
    pub name: String,
    pub uuid: Uuid,
    pub addr: SocketAddr,
    pub properties: Vec<ProfileProperty>,
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
    online: AtomicUsize,
    key: Option<ServerKey>,
    http: reqwest::Client,
    /// Случайный UUID сессии сервера; клиент получает его в login_finished.
    session_id: Uuid,
}

const READ_TIMEOUT: Duration = Duration::from_secs(30);

impl Server {
    pub fn new(config: NetConfig) -> Arc<Self> {
        Arc::new(Self {
            key: config.online_mode.then(ServerKey::generate),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("HTTP client"),
            config,
            online: AtomicUsize::new(0),
            session_id: uuid::Builder::from_random_bytes(rand::random()).into_uuid(),
        })
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
            let _ = stream.set_nodelay(true);
            let server = self.clone();
            let on_login = on_login.clone();
            handshakes.spawn(async move {
                match server.handle(stream, addr).await {
                    Ok(Some((player, conn))) => on_login(player, conn),
                    Ok(None) => {}
                    Err(e) => tracing::debug!(%addr, ?e, "connection closed"),
                }
            });
        }
        handshakes.abort_all();
        while handshakes.join_next().await.is_some() {}
        Ok(())
    }

    async fn handle(
        &self,
        stream: TcpStream,
        addr: SocketAddr,
    ) -> Result<Option<(LoggedIn, Connection)>, NetError> {
        let mut conn = Connection::new(stream);
        let packet = conn.read_packet().await?;
        let hs = Handshake::decode(&packet)?;
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
        loop {
            let packet = conn.read_packet().await?;
            match StatusC2S::decode(&packet)? {
                StatusC2S::Request => {
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
        &self,
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
        let (name, uuid, properties) = match &self.key {
            Some(key) => match self.authenticate(&mut conn, key, &name).await? {
                Some(profile) => (profile.name, profile.uuid, profile.properties),
                None => return Ok(None),
            },
            None => {
                let uuid = packets::offline_uuid(&name);
                (name, uuid, Vec::new())
            }
        };
        if self.online.load(Ordering::Relaxed) >= self.config.max_players {
            conn.send(&packets::login_disconnect(&text_json(
                "The server is full!",
            )))
            .await?;
            return Ok(None);
        }

        if self.config.compression_threshold >= 0 {
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
        if let Some(plan) = &self.config.configuration {
            match plan.run(&mut conn).await? {
                configuration::Outcome::Finished => {
                    tracing::info!(%name, "configuration finished");
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
            },
            conn,
        )))
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
}

/// Неотправленный хвост (причина отключения после `send` и `return`) —
/// без ожидания, сколько примет сокет.
impl Drop for Connection {
    fn drop(&mut self) {
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
        }
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

    pub async fn read_packet(&mut self) -> Result<Vec<u8>, NetError> {        // Ответ ждут на отправленное: сначала оно должно уйти.
        self.flush().await?;
        loop {
            if let Some(packet) = self.codec.decode(&mut self.buf)? {
                return Ok(packet);
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
    pub async fn send_frames(&mut self, frames: &[u8]) -> Result<(), NetError> {
        if self.out.is_empty() && self.cipher.is_none() {
            self.stream.write_all(frames).await?;
            self.written_bytes += frames.len() as u64;
            return Ok(());
        }
        self.out.extend_from_slice(frames);
        self.flush().await
    }

    /// Кадр в буфер; уходит при `flush` (конец итерации сессии, чтение ответа)
    /// или при переполнении — один syscall на пачку, а не на пакет.
    pub async fn send(&mut self, packet: &[u8]) -> Result<(), NetError> {
        self.codec.encode(packet, &mut self.out);
        if self.out.len() >= OUT_LIMIT {
            self.flush().await?;
        }
        Ok(())
    }

    /// Шифрует буфер на месте и пишет его целиком.
    pub async fn flush(&mut self) -> Result<(), NetError> {
        if self.out.is_empty() {
            return Ok(());
        }
        if let Some(c) = &mut self.cipher {
            c.encrypt(&mut self.out[self.plain..]);
        }
        let written = self.out.len() as u64;
        let res = self.stream.write_all(&self.out).await;
        self.out.clear();
        self.plain = 0;
        if self.out.capacity() > OUT_KEEP {
            self.out.shrink_to(OUT_KEEP);
        }
        res?;
        self.written_bytes += written;
        Ok(())
    }
}
