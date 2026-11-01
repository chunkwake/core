//! chunkwake-data — данные ванилы: образ запуска и загрузчики по дереву данных.
//!
//! Данные Mojang не хранятся в репозитории: `build.rs` упаковывает кэш
//! `generated/<версия>` (или собирает его из `server.jar`) в архив и встраивает
//! его в бинарь. При первом запуске архив распаковывается, дальше читаются
//! обычные файлы.

pub mod boot;
pub mod bundle;
pub mod enchanting;
pub mod packets;
pub mod recipes;
pub mod registries;
pub mod tools;
pub mod world;

pub use chunkwake_datacache as cache;

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
