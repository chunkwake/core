//! Сжатие чанков в регионе (`RegionFileVersion` ванили): байт вида перед
//! payload — 1 gzip, 2 zlib, 3 без сжатия, 4 lz4; бит 128 — данные во
//! внешнем файле `c.X.Z.mcc` рядом с регионом.
//!
//! lz4 у ванили — поток `LZ4BlockOutputStream` из lz4-java: кадры
//! `LZ4Block` по 64 КиБ с xxhash32 каждого кадра и пустым кадром в конце.

use flate2::Compression;
use flate2::read::{GzDecoder, ZlibDecoder};
use flate2::write::ZlibEncoder;
use std::io::{Read, Write};

/// Вид сжатия при записи (`region-file-compression`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Codec {
    /// zlib, уровень 6 — как `DeflaterOutputStream` ванили.
    #[default]
    Deflate,
    /// lz4-java `LZ4Block`: быстрее в обе стороны, файл больше.
    Lz4,
    /// Без сжатия.
    None,
}

pub(crate) const GZIP: u8 = 1;
pub(crate) const ZLIB: u8 = 2;
pub(crate) const RAW: u8 = 3;
pub(crate) const LZ4: u8 = 4;
/// Флаг вида: payload во внешнем файле `.mcc`.
pub(crate) const EXTERNAL: u8 = 128;

impl Codec {
    /// Значение настройки `region-file-compression`: `deflate`, `lz4`, `none`.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "deflate" => Some(Self::Deflate),
            "lz4" => Some(Self::Lz4),
            "none" => Some(Self::None),
            _ => None,
        }
    }

    fn id(self) -> u8 {
        match self {
            Self::Deflate => ZLIB,
            Self::Lz4 => LZ4,
            Self::None => RAW,
        }
    }

    /// Байт вида и сжатый NBT одним буфером — payload чанка в регионе.
    pub fn encode(self, raw: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(raw.len() / 3 + 64);
        out.push(self.id());
        match self {
            Self::Deflate => {
                let mut enc = ZlibEncoder::new(out, Compression::default());
                enc.write_all(raw).expect("write to Vec");
                enc.finish().expect("write to Vec")
            }
            Self::Lz4 => {
                lz4_write(raw, &mut out);
                out
            }
            Self::None => {
                out.extend_from_slice(raw);
                out
            }
        }
    }
}

/// Распаковать данные вида `kind` (без бита внешнего файла) в NBT.
pub(crate) fn decode(kind: u8, data: &[u8]) -> Result<Vec<u8>, String> {
    // Запас в четыре сжатых размера покрывает обычный коэффициент чанка:
    // без него буфер растёт реаллокациями.
    let mut raw = Vec::with_capacity(data.len().saturating_mul(4).max(4096));
    match kind {
        ZLIB => ZlibDecoder::new(data).read_to_end(&mut raw).map_err(|e| e.to_string())?,
        GZIP => GzDecoder::new(data).read_to_end(&mut raw).map_err(|e| e.to_string())?,
        RAW => return Ok(data.to_vec()),
        LZ4 => {
            lz4_read(data, &mut raw)?;
            raw.len()
        }
        other => return Err(format!("unknown compression {other}")),
    };
    Ok(raw)
}

const MAGIC: &[u8; 8] = b"LZ4Block";
/// Кадр lz4-java по умолчанию — 64 КиБ, уровень в токене `log2 - 10`.
const BLOCK: usize = 1 << 16;
const LEVEL: u8 = 6;
const METHOD_RAW: u8 = 0x10;
const METHOD_LZ4: u8 = 0x20;
/// `XXHashFactory.newStreamingHash32(DEFAULT_SEED)` у `LZ4BlockOutputStream`.
const SEED: u32 = 0x9747_b28c;

/// Сумма кадра: `asChecksum()` у lz4-java обрезает её до 28 бит.
fn check(block: &[u8]) -> u32 {
    xxh32(block, SEED) & 0x0FFF_FFFF
}

fn lz4_write(raw: &[u8], out: &mut Vec<u8>) {
    let header = |out: &mut Vec<u8>, method: u8, packed: usize, orig: usize, sum: u32| {
        out.extend_from_slice(MAGIC);
        out.push(method | LEVEL);
        out.extend_from_slice(&(packed as u32).to_le_bytes());
        out.extend_from_slice(&(orig as u32).to_le_bytes());
        out.extend_from_slice(&sum.to_le_bytes());
    };
    for block in raw.chunks(BLOCK) {
        let packed = lz4_flex::block::compress(block);
        // Сжатое не короче исходного — кадр пишется как есть, как у lz4-java.
        if packed.len() >= block.len() {
            header(out, METHOD_RAW, block.len(), block.len(), check(block));
            out.extend_from_slice(block);
        } else {
            header(out, METHOD_LZ4, packed.len(), block.len(), check(block));
            out.extend_from_slice(&packed);
        }
    }
    // `finish()`: пустой кадр — конец потока.
    header(out, METHOD_RAW, 0, 0, 0);
}

fn lz4_read(mut data: &[u8], out: &mut Vec<u8>) -> Result<(), String> {
    let le = |b: &[u8]| u32::from_le_bytes(b.try_into().unwrap()) as usize;
    while !data.is_empty() {
        if data.len() < 21 || &data[..8] != MAGIC {
            return Err("lz4: bad block header".into());
        }
        let method = data[8] & 0xF0;
        let (packed, orig, sum) = (le(&data[9..13]), le(&data[13..17]), le(&data[17..21]) as u32);
        data = &data[21..];
        if orig == 0 && packed == 0 {
            return Ok(());
        }
        if packed > data.len() || orig > 1 << 25 {
            return Err("lz4: truncated block".into());
        }
        let start = out.len();
        match method {
            METHOD_RAW if packed == orig => out.extend_from_slice(&data[..packed]),
            METHOD_LZ4 => {
                let block = lz4_flex::block::decompress(&data[..packed], orig)
                    .map_err(|e| format!("lz4: {e}"))?;
                out.extend_from_slice(&block);
            }
            _ => return Err("lz4: bad block method".into()),
        }
        if check(&out[start..]) != sum {
            return Err("lz4: checksum mismatch".into());
        }
        data = &data[packed..];
    }
    // Поток без пустого кадра: lz4-java читает это как обрыв.
    Err("lz4: stream ends without the final block".into())
}

/// xxHash32.
fn xxh32(data: &[u8], seed: u32) -> u32 {
    const P1: u32 = 2_654_435_761;
    const P2: u32 = 2_246_822_519;
    const P3: u32 = 3_266_489_917;
    const P4: u32 = 668_265_263;
    const P5: u32 = 374_761_393;
    let word = |i: usize| u32::from_le_bytes(data[i..i + 4].try_into().unwrap());
    let round = |acc: u32, w: u32| acc.wrapping_add(w.wrapping_mul(P2)).rotate_left(13).wrapping_mul(P1);
    let mut i = 0;
    let mut h = if data.len() >= 16 {
        let mut v = [
            seed.wrapping_add(P1).wrapping_add(P2),
            seed.wrapping_add(P2),
            seed,
            seed.wrapping_sub(P1),
        ];
        while i + 16 <= data.len() {
            for (k, acc) in v.iter_mut().enumerate() {
                *acc = round(*acc, word(i + k * 4));
            }
            i += 16;
        }
        v[0].rotate_left(1)
            .wrapping_add(v[1].rotate_left(7))
            .wrapping_add(v[2].rotate_left(12))
            .wrapping_add(v[3].rotate_left(18))
    } else {
        seed.wrapping_add(P5)
    };
    h = h.wrapping_add(data.len() as u32);
    while i + 4 <= data.len() {
        h = h.wrapping_add(word(i).wrapping_mul(P3)).rotate_left(17).wrapping_mul(P4);
        i += 4;
    }
    for &b in &data[i..] {
        h = h.wrapping_add((b as u32).wrapping_mul(P5)).rotate_left(11).wrapping_mul(P1);
    }
    h ^= h >> 15;
    h = h.wrapping_mul(P2);
    h ^= h >> 13;
    h = h.wrapping_mul(P3);
    h ^ (h >> 16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xxh32_reference_vectors() {
        assert_eq!(xxh32(b"", 0), 0x02CC_5D05);
        assert_eq!(xxh32(b"abc", 0), 0x32D1_53FF);
        assert_eq!(xxh32(b"Nobody inspects the spammish repetition", 0), 0xE229_3B2F);
    }

    #[test]
    fn every_codec_roundtrips() {
        // Больше одного кадра lz4 и с несжимаемым хвостом.
        let mut raw: Vec<u8> = (0..200_000u32).map(|i| (i / 7 % 13) as u8).collect();
        raw.extend((0..5000u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8));
        for codec in [Codec::Deflate, Codec::Lz4, Codec::None] {
            let p = codec.encode(&raw);
            assert_eq!(decode(p[0], &p[1..]).unwrap(), raw, "{codec:?}");
        }
    }

    #[test]
    fn lz4_needs_the_final_block() {
        let p = Codec::Lz4.encode(b"hello hello hello hello");
        assert!(decode(LZ4, &p[1..p.len() - 21]).is_err());
    }
}
