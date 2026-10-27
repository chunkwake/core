//! Проверка игрока через Mojang session server (`hasJoined`).

use chunkwake_protocol::packets::ProfileProperty;
use serde::Deserialize;
use uuid::Uuid;

pub const MOJANG_SESSION_SERVER: &str = "https://sessionserver.mojang.com";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameProfile {
    pub uuid: Uuid,
    pub name: String,
    pub properties: Vec<ProfileProperty>,
}

#[derive(Deserialize)]
struct RawProfile {
    id: String,
    name: String,
    #[serde(default)]
    properties: Vec<RawProperty>,
}

#[derive(Deserialize)]
struct RawProperty {
    name: String,
    value: String,
    signature: Option<String>,
}

#[derive(Debug)]
pub enum AuthError {
    /// Сервер сессий ответил, что игрок не заходил (HTTP 204).
    NotAuthenticated,
    Http(String),
}

/// `GET {base}/session/minecraft/hasJoined?username=..&serverId=..`
pub async fn has_joined(
    http: &reqwest::Client,
    base: &str,
    username: &str,
    server_hash: &str,
) -> Result<GameProfile, AuthError> {
    let resp = http
        .get(format!("{base}/session/minecraft/hasJoined"))
        .query(&[("username", username), ("serverId", server_hash)])
        .send()
        .await
        .map_err(|e| AuthError::Http(e.to_string()))?;
    if resp.status() == reqwest::StatusCode::NO_CONTENT {
        return Err(AuthError::NotAuthenticated);
    }
    if !resp.status().is_success() {
        return Err(AuthError::Http(format!("HTTP {}", resp.status())));
    }
    let raw: RawProfile = resp
        .json()
        .await
        .map_err(|e| AuthError::Http(e.to_string()))?;
    let uuid = Uuid::parse_str(&raw.id).map_err(|e| AuthError::Http(e.to_string()))?;
    Ok(GameProfile {
        uuid,
        name: raw.name,
        properties: raw
            .properties
            .into_iter()
            .map(|p| ProfileProperty {
                name: p.name,
                value: p.value,
                signature: p.signature,
            })
            .collect(),
    })
}
