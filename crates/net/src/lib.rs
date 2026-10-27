//! chunkwake-net — приём соединений и состояния handshake/status/login.

use bytes::BytesMut;
use chunkwake_protocol::ProtocolError;
use chunkwake_protocol::frame::FrameCodec;
use chunkwake_protocol::packets::{self, Handshake, LoginC2S, NextState, StatusC2S};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use uuid::Uuid;

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
}

/// Игрок, прошедший login и перешедший в состояние configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoggedIn {
    pub name: String,
    pub uuid: Uuid,
    pub addr: SocketAddr,
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
}

const READ_TIMEOUT: Duration = Duration::from_secs(30);

impl Server {
    pub fn new(config: NetConfig) -> Arc<Self> {
        Arc::new(Self {
            config,
            online: AtomicUsize::new(0),
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
        let on_login = Arc::new(on_login);
        loop {
            let (stream, addr) = listener.accept().await?;
            let _ = stream.set_nodelay(true);
            let server = self.clone();
            let on_login = on_login.clone();
            tokio::spawn(async move {
                match server.handle(stream, addr).await {
                    Ok(Some((player, conn))) => on_login(player, conn),
                    Ok(None) => {}
                    Err(e) => tracing::debug!(%addr, ?e, "connection closed"),
                }
            });
        }
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
                self.status(&mut conn).await?;
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
        if self.config.online_mode {
            // Шифрование и Mojang session server — следующий шаг M1.
            conn.send(&packets::login_disconnect(&text_json(
                "online-mode is not implemented yet",
            )))
            .await?;
            return Ok(None);
        }
        if self.online.load(Ordering::Relaxed) >= self.config.max_players {
            conn.send(&packets::login_disconnect(&text_json(
                "The server is full!",
            )))
            .await?;
            return Ok(None);
        }
        let uuid = packets::offline_uuid(&name);

        if self.config.compression_threshold >= 0 {
            conn.send(&packets::set_compression(self.config.compression_threshold))
                .await?;
            conn.codec.threshold = Some(self.config.compression_threshold as usize);
        }
        conn.send(&packets::login_success(uuid, &name)).await?;

        loop {
            let packet = conn.read_packet().await?;
            match LoginC2S::decode(&packet)? {
                LoginC2S::Acknowledged => break,
                LoginC2S::PluginResponse { .. } | LoginC2S::CookieResponse => continue,
                _ => return Ok(None),
            }
        }
        tracing::info!(%name, %uuid, %addr, "player logged in");
        Ok(Some((LoggedIn { name, uuid, addr }, conn)))
    }
}

fn valid_name(name: &str) -> bool {
    (1..=16).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn text_json(text: &str) -> String {
    serde_json::json!({ "text": text }).to_string()
}

/// Соединение с клиентом: фрейминг и сжатие поверх TCP.
pub struct Connection {
    stream: TcpStream,
    buf: BytesMut,
    pub codec: FrameCodec,
}

impl Connection {
    pub fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            buf: BytesMut::with_capacity(4096),
            codec: FrameCodec::default(),
        }
    }

    pub async fn read_packet(&mut self) -> Result<Vec<u8>, NetError> {
        loop {
            if let Some(packet) = self.codec.decode(&mut self.buf)? {
                return Ok(packet);
            }
            let n = tokio::time::timeout(READ_TIMEOUT, self.stream.read_buf(&mut self.buf))
                .await
                .map_err(|_| NetError::Timeout)??;
            if n == 0 {
                return Err(NetError::Io(std::io::ErrorKind::UnexpectedEof.into()));
            }
        }
    }

    pub async fn send(&mut self, packet: &[u8]) -> Result<(), NetError> {
        let mut out = Vec::with_capacity(packet.len() + 5);
        self.codec.encode(packet, &mut out);
        self.stream.write_all(&out).await?;
        Ok(())
    }
}
