//! Пакеты состояния configuration.
//!
//! ID передаются параметром: они берутся из `packets.json` версии,
//! а не захардкожены.

use crate::{Reader, Result, Writer};

/// Пак данных, который клиент и сервер могут знать оба (`minecraft:core:26.3`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnownPack {
    pub namespace: String,
    pub id: String,
    pub version: String,
}

impl KnownPack {
    pub fn vanilla(version: &str) -> Self {
        Self {
            namespace: "minecraft".into(),
            id: "core".into(),
            version: version.into(),
        }
    }
}

pub fn select_known_packs(id: i32, packs: &[KnownPack]) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(packs.len() as i32);
    for p in packs {
        w.string(&p.namespace).string(&p.id).string(&p.version);
    }
    w.buf
}

/// Тело serverbound `select_known_packs` (после ID пакета).
pub fn read_known_packs(r: &mut Reader) -> Result<Vec<KnownPack>> {
    let n = r.var_int()?;
    // Ванила ограничивает список 64 паками.
    if !(0..=64).contains(&n) {
        return Err(crate::ProtocolError::BadLength(n));
    }
    (0..n)
        .map(|_| {
            Ok(KnownPack {
                namespace: r.string(32767)?.to_owned(),
                id: r.string(32767)?.to_owned(),
                version: r.string(32767)?.to_owned(),
            })
        })
        .collect()
}

pub fn update_enabled_features(id: i32, features: &[&str]) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(features.len() as i32);
    for f in features {
        w.string(f);
    }
    w.buf
}

/// `registry_data` без NBT-тел: клиент берёт данные записей из своего пака
/// `minecraft:core`, поэтому передаются только ID записей в сетевом порядке.
pub fn registry_data(id: i32, registry: &str, entries: &[String]) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).string(registry).var_int(entries.len() as i32);
    for e in entries {
        w.string(e).bool(false);
    }
    w.buf
}

/// `update_tags`: реестр → (тег → сетевые ID записей).
pub fn update_tags<'a>(
    id: i32,
    registries: impl ExactSizeIterator<Item = (&'a str, &'a [(String, Vec<i32>)])>,
) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(registries.len() as i32);
    for (registry, tags) in registries {
        w.string(registry).var_int(tags.len() as i32);
        for (tag, ids) in tags {
            w.string(tag).var_int(ids.len() as i32);
            for &i in ids {
                w.var_int(i);
            }
        }
    }
    w.buf
}

pub fn finish_configuration(id: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id);
    w.buf
}

/// Disconnect в configuration/play: причина — текстовый компонент в сетевом NBT.
/// Простой текст кодируется как корневой TAG_String.
pub fn disconnect(id: i32, text: &str) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id);
    write_nbt_string_root(&mut w, text);
    w.buf
}

/// Сетевой NBT (без имени корня): тип 8 (String) + u16 длина + modified UTF-8.
pub(crate) fn write_nbt_string_root(w: &mut Writer, s: &str) {
    let bytes = modified_utf8(s);
    w.u8(8).u16(bytes.len() as u16);
    w.buf.extend_from_slice(&bytes);
}

/// Java modified UTF-8: NUL как 2 байта, символы вне BMP — суррогатными парами.
pub(crate) fn modified_utf8(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for unit in s.encode_utf16() {
        match unit {
            0x0001..=0x007f => out.push(unit as u8),
            0x0000 | 0x0080..=0x07ff => {
                out.push(0xc0 | (unit >> 6) as u8);
                out.push(0x80 | (unit & 0x3f) as u8);
            }
            _ => {
                out.push(0xe0 | (unit >> 12) as u8);
                out.push(0x80 | ((unit >> 6) & 0x3f) as u8);
                out.push(0x80 | (unit & 0x3f) as u8);
            }
        }
    }
    out.truncate(u16::MAX as usize);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_packs_roundtrip() {
        let packs = vec![KnownPack::vanilla("26.3")];
        let buf = select_known_packs(0x0e, &packs);
        let mut r = Reader::new(&buf);
        assert_eq!(r.var_int().unwrap(), 0x0e);
        assert_eq!(read_known_packs(&mut r).unwrap(), packs);
    }

    #[test]
    fn registry_data_layout() {
        let buf = registry_data(
            7,
            "minecraft:dimension_type",
            &["minecraft:overworld".into()],
        );
        let mut r = Reader::new(&buf);
        assert_eq!(r.var_int().unwrap(), 7);
        assert_eq!(r.string(256).unwrap(), "minecraft:dimension_type");
        assert_eq!(r.var_int().unwrap(), 1);
        assert_eq!(r.string(256).unwrap(), "minecraft:overworld");
        assert!(!r.bool().unwrap());
        assert!(r.remaining().is_empty());
    }

    #[test]
    fn nbt_text() {
        let buf = disconnect(2, "hi\0");
        assert_eq!(buf, [2, 8, 0, 4, b'h', b'i', 0xc0, 0x80]);
        assert_eq!(modified_utf8("😀").len(), 6);
    }
}
