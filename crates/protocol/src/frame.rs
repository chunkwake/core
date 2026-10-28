//! Фрейминг пакетов: префикс длины и опциональное zlib-сжатие.

use crate::codec::{Reader, var_int_len, write_var_int};
use crate::{ProtocolError, Result};
use bytes::{Buf, BytesMut};
use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use std::io::{Read, Write};

/// Максимальный размер пакета в протоколе (2^21 - 1).
pub const MAX_PACKET_LEN: usize = 2_097_151;
/// Максимальный размер распакованного тела (как у ванилы).
pub const MAX_UNCOMPRESSED_LEN: usize = 8_388_608;

/// Порог сжатия: `None` — сжатие выключено.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameCodec {
    pub threshold: Option<usize>,
}

impl FrameCodec {
    /// Пытается вынуть один полный пакет (id + данные) из буфера.
    /// Возвращает `Ok(None)`, если данных пока не хватает.
    pub fn decode(&self, buf: &mut BytesMut) -> Result<Option<Vec<u8>>> {
        let mut r = Reader::new(buf);
        let len = match r.var_int() {
            Ok(len) => len,
            Err(ProtocolError::Eof) => return Ok(None),
            Err(e) => return Err(e),
        };
        if len <= 0 || len as usize > MAX_PACKET_LEN {
            return Err(ProtocolError::BadLength(len));
        }
        let header = var_int_len(len);
        let total = header + len as usize;
        if buf.len() < total {
            return Ok(None);
        }
        buf.advance(header);
        let frame = buf.split_to(len as usize);

        let Some(threshold) = self.threshold else {
            return Ok(Some(frame.to_vec()));
        };
        let mut r = Reader::new(&frame);
        let data_len = r.var_int()?;
        let body = r.remaining();
        if data_len == 0 {
            return Ok(Some(body.to_vec()));
        }
        if (data_len as usize) < threshold || data_len as usize > MAX_UNCOMPRESSED_LEN {
            return Err(ProtocolError::BadLength(data_len));
        }
        let mut out = Vec::with_capacity(data_len as usize);
        ZlibDecoder::new(body).read_to_end(&mut out)?;
        if out.len() != data_len as usize {
            return Err(ProtocolError::BadLength(data_len));
        }
        Ok(Some(out))
    }

    /// Кодирует пакет (id + данные) в готовый к отправке фрейм.
    pub fn encode(&self, packet: &[u8], out: &mut Vec<u8>) {
        match self.threshold {
            None => {
                write_var_int(out, packet.len() as i32);
                out.extend_from_slice(packet);
            }
            Some(t) if packet.len() < t => {
                write_var_int(out, packet.len() as i32 + 1);
                out.push(0);
                out.extend_from_slice(packet);
            }
            Some(_) => {
                let mut enc =
                    ZlibEncoder::new(Vec::with_capacity(packet.len() + 64), Compression::fast());
                enc.write_all(packet).expect("write to Vec");
                let compressed = enc.finish().expect("write to Vec");
                let data_len = packet.len() as i32;
                write_var_int(out, (var_int_len(data_len) + compressed.len()) as i32);
                write_var_int(out, data_len);
                out.extend_from_slice(&compressed);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(codec: FrameCodec, packet: &[u8]) {
        let mut out = Vec::new();
        codec.encode(packet, &mut out);
        // Подаём по байту, чтобы проверить работу с неполными данными.
        let mut buf = BytesMut::new();
        for (i, b) in out.iter().enumerate() {
            buf.extend_from_slice(&[*b]);
            let got = codec.decode(&mut buf).unwrap();
            if i + 1 < out.len() {
                assert!(got.is_none());
            } else {
                assert_eq!(got.unwrap(), packet);
            }
        }
        assert!(buf.is_empty());
    }

    #[test]
    fn plain() {
        roundtrip(FrameCodec::default(), &[0x00, 1, 2, 3]);
    }

    #[test]
    fn compressed_small_and_large() {
        let codec = FrameCodec {
            threshold: Some(256),
        };
        roundtrip(codec, &[0x01, 9, 9]);
        roundtrip(codec, &vec![7u8; 10_000]);
    }

    #[test]
    fn rejects_bad_length() {
        let mut buf = BytesMut::from(&[0x00][..]);
        assert!(FrameCodec::default().decode(&mut buf).is_err());
    }
}
