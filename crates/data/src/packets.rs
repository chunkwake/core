//! Таблица ID пакетов из `reports/packets.json`.

use crate::DataError;
use std::collections::HashMap;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum State {
    Handshake,
    Status,
    Login,
    Configuration,
    Play,
}

impl State {
    fn key(self) -> &'static str {
        match self {
            Self::Handshake => "handshake",
            Self::Status => "status",
            Self::Login => "login",
            Self::Configuration => "configuration",
            Self::Play => "play",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    Clientbound,
    Serverbound,
}

impl Direction {
    fn key(self) -> &'static str {
        match self {
            Self::Clientbound => "clientbound",
            Self::Serverbound => "serverbound",
        }
    }
}

/// (состояние, направление, имя без `minecraft:`) → protocol id.
#[derive(Debug, Default)]
pub struct PacketTable {
    ids: HashMap<(State, Direction, String), i32>,
}

/// ID, которые захардкожены в `chunkwake-protocol` (стабильны с 1.20.2).
/// При старте сверяются с данными версии, чтобы расхождение было видно сразу.
pub const HARDCODED: &[(State, Direction, &str, i32)] = {
    use Direction::*;
    use State::*;
    &[
        (Handshake, Serverbound, "intention", 0x00),
        (Status, Serverbound, "status_request", 0x00),
        (Status, Serverbound, "ping_request", 0x01),
        (Status, Clientbound, "status_response", 0x00),
        (Status, Clientbound, "pong_response", 0x01),
        (Login, Serverbound, "hello", 0x00),
        (Login, Serverbound, "key", 0x01),
        (Login, Serverbound, "custom_query_answer", 0x02),
        (Login, Serverbound, "login_acknowledged", 0x03),
        (Login, Serverbound, "cookie_response", 0x04),
        (Login, Clientbound, "login_disconnect", 0x00),
        (Login, Clientbound, "hello", 0x01),
        (Login, Clientbound, "login_finished", 0x02),
        (Login, Clientbound, "login_compression", 0x03),
    ]
};

impl PacketTable {
    pub fn load(path: &Path) -> Result<Self, DataError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| DataError::Io(format!("read {}: {e}", path.display())))?;
        Self::parse(&text).map_err(|msg| DataError::Format {
            file: path.display().to_string(),
            msg,
        })
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let root: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let mut ids = HashMap::new();
        for state in [
            State::Handshake,
            State::Status,
            State::Login,
            State::Configuration,
            State::Play,
        ] {
            for dir in [Direction::Clientbound, Direction::Serverbound] {
                let Some(packets) = root[state.key()][dir.key()].as_object() else {
                    continue;
                };
                for (name, v) in packets {
                    let id = v["protocol_id"]
                        .as_i64()
                        .ok_or_else(|| format!("{name}: no protocol_id"))?;
                    let name = name.strip_prefix("minecraft:").unwrap_or(name);
                    ids.insert((state, dir, name.to_owned()), id as i32);
                }
            }
        }
        if ids.is_empty() {
            return Err("no packets found".into());
        }
        Ok(Self { ids })
    }

    pub fn id(&self, state: State, dir: Direction, name: &str) -> Result<i32, DataError> {
        self.ids
            .get(&(state, dir, name.to_owned()))
            .copied()
            .ok_or_else(|| DataError::MissingPacket {
                state: state.key().into(),
                direction: dir.key().into(),
                name: name.into(),
            })
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Проверяет захардкоженные ID против данных версии.
    pub fn verify_hardcoded(&self) -> Result<(), DataError> {
        for &(state, dir, name, ours) in HARDCODED {
            let theirs = self.id(state, dir, name)?;
            if theirs != ours {
                return Err(DataError::IdMismatch {
                    packet: format!("{}/{}/{name}", state.key(), dir.key()),
                    ours,
                    theirs,
                });
            }
        }
        Ok(())
    }
}
