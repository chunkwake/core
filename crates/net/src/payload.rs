//! Каналы плагинов: пакет `custom_payload` и его перенос в мост строкой.
//!
//! Данные канала — произвольные байты, а мост передаёт строки, поэтому
//! они кодируются шестнадцатерично. `minecraft:register`/`unregister` —
//! имена каналов через `\0` (`DiscardedPayload` в ванили, обработка в Paper).

use chunkwake_protocol::Writer;

/// Предел данных `ServerboundCustomPayloadPacket` (32767 байт).
pub const MAX_SERVERBOUND: usize = 32767;
/// Предел `ClientboundCustomPayloadPacket` (1 МиБ).
pub const MAX_CLIENTBOUND: usize = 1 << 20;

/// Имена каналов из тела `minecraft:register`.
pub fn register_names(data: &[u8]) -> Vec<String> {
    data.split(|&b| b == 0)
        .filter(|n| !n.is_empty())
        .filter_map(|n| std::str::from_utf8(n).ok())
        .map(str::to_owned)
        .collect()
}

pub fn to_hex(data: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(data.len() * 2);
    for b in data {
        let _ = write!(out, "{b:02x}");
    }
    out
}

pub fn from_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

/// `ClientboundCustomPayloadPacket`: канал и сырые байты без префикса длины.
pub fn custom_payload(id: i32, channel: &str, data: &[u8]) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).string(channel);
    w.buf.extend_from_slice(data);
    w.buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_splits_on_nul() {
        assert_eq!(
            register_names(b"voicechat:a\0bluemap:b\0"),
            vec!["voicechat:a".to_owned(), "bluemap:b".to_owned()]
        );
        assert!(register_names(b"").is_empty());
    }

    #[test]
    fn hex_round_trip() {
        let data = [0u8, 1, 0x7f, 0xff];
        assert_eq!(to_hex(&data), "00017fff");
        assert_eq!(from_hex("00017fff").unwrap(), data);
        assert!(from_hex("0").is_none());
        assert!(from_hex("zz").is_none());
    }
}
