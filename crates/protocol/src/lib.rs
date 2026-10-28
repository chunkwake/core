//! chunkwake-protocol — сетевой протокол Minecraft 26.3: примитивы, фрейминг, пакеты.

pub mod codec;
pub mod configuration;
pub mod frame;
pub mod inventory;
pub mod packets;
pub mod play;
pub mod text;

pub use codec::{Reader, Writer};

#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("unexpected end of packet")]
    Eof,
    #[error("VarInt is too long")]
    VarIntTooLong,
    #[error("invalid length {0}")]
    BadLength(i32),
    #[error("string longer than {max} chars")]
    StringTooLong { max: usize },
    #[error("invalid UTF-8")]
    BadUtf8,
    #[error("unknown packet 0x{id:02x} in state {state}")]
    UnknownPacket { id: i32, state: &'static str },
    #[error("unknown next state {0}")]
    BadNextState(i32),
    #[error("decompression failed: {0}")]
    Decompress(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, ProtocolError>;
