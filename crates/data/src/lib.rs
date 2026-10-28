//! chunkwake-data — данные ванилы: запуск генератора и таблица пакетов.
//!
//! Данные Mojang не хранятся в репозитории: сервер генерирует их из
//! `server.jar` пользователя и кэширует по имени версии.

pub mod generator;
pub mod packets;
pub mod recipes;
pub mod registries;
pub mod tools;
pub mod world;

#[derive(Debug, thiserror::Error)]
pub enum DataError {
    #[error("{0}")]
    Io(String),
    #[error("data generator failed: {0}")]
    Generator(String),
    #[error("{file}: {msg}")]
    Format { file: String, msg: String },
    #[error("packet {state}/{direction}/{name} is missing in packets.json")]
    MissingPacket {
        state: String,
        direction: String,
        name: String,
    },
    #[error("packet {packet}: chunkwake uses id 0x{ours:02x}, but packets.json has 0x{theirs:02x}")]
    IdMismatch {
        packet: String,
        ours: i32,
        theirs: i32,
    },
}
