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

/// `registry_data` с данными записей (сетевой NBT): для клиента без общего
/// с сервером пака, как ванила шлёт клиентам других версий и модам.
pub fn registry_data_full(id: i32, registry: &str, entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).string(registry).var_int(entries.len() as i32);
    for (e, nbt) in entries {
        w.string(e).bool(true);
        w.buf.extend_from_slice(nbt);
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

/// Настройки клиента (`ClientInformation`): приходят в configuration и в play.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClientInfo {
    /// Запрошенная дальность обзора в чанках (`requestedViewDistance`).
    pub view_distance: u8,
    /// Видимые части скина (`modelCustomisation`): бит 0 — плащ, 6 — шляпа.
    pub skin_parts: u8,
    /// Основная рука: `HumanoidArm` 0 — левая, 1 — правая.
    pub main_hand: u8,
    pub allows_listing: bool,
}

impl ClientInfo {
    /// `PlayerModelPart.HAT`: бит 6.
    pub const HAT: u8 = 1 << 6;

    /// `ClientInformation.createDefault`: обзор 2, скин скрыт, правая рука.
    pub const DEFAULT: Self = Self {
        view_distance: 2,
        skin_parts: 0,
        main_hand: 1,
        allows_listing: false,
    };

    pub fn shows_hat(&self) -> bool {
        self.skin_parts & Self::HAT != 0
    }
}

/// Тело `client_information` после ID (`ClientInformation(FriendlyByteBuf)`):
/// язык (до 16 символов), обзор, видимость чата, цвета чата, части скина,
/// рука, фильтр текста, показ в списке, частицы.
pub fn read_client_information(r: &mut Reader) -> Result<ClientInfo> {
    let _language = r.string(16)?;
    let view_distance = r.u8()?;
    let _chat_visibility = r.var_int()?;
    let _chat_colors = r.bool()?;
    let skin_parts = r.u8()?;
    let main_hand = r.var_int()?.clamp(0, 1) as u8;
    let _text_filtering = r.bool()?;
    let allows_listing = r.bool()?;
    let _particles = r.var_int()?;
    Ok(ClientInfo {
        view_distance,
        skin_parts,
        main_hand,
        allows_listing,
    })
}

/// `client_information` клиента (для ботов): язык `en_us`, чат виден и цветной,
/// фильтра текста нет, частицы все.
pub fn client_information(id: i32, info: &ClientInfo) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id)
        .string("en_us")
        .u8(info.view_distance)
        .var_int(0)
        .bool(true)
        .u8(info.skin_parts)
        .var_int(i32::from(info.main_hand))
        .bool(false)
        .bool(info.allows_listing)
        .var_int(0);
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

    #[test]
    fn client_information_round_trip() {
        let info = ClientInfo {
            view_distance: 12,
            skin_parts: 0x7f,
            main_hand: 0,
            allows_listing: true,
        };
        let buf = client_information(3, &info);
        let mut r = Reader::new(&buf);
        assert_eq!(r.var_int().unwrap(), 3);
        assert_eq!(read_client_information(&mut r).unwrap(), info);
        assert!(r.remaining().is_empty());
        assert!(info.shows_hat());
        assert!(!ClientInfo::DEFAULT.shows_hat());
    }
}
