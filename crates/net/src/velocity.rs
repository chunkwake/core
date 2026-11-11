//! Modern Velocity player-info forwarding.
//!
//! Прокси присылает профиль в ответ на `LoginPluginRequest` по каналу
//! `velocity:player_info`: сначала 32 байта HMAC-SHA256, затем данные
//! (версия, адрес, UUID, имя и свойства). Подпись считается по данным.
//! Секрет общий с `velocity.toml` прокси; без него сервер данные не принимает.

use crate::auth::GameProfile;
use chunkwake_protocol::packets::ProfileProperty;
use chunkwake_protocol::{ProtocolError, Reader};
use sha2::{Digest, Sha256};

/// Канал modern-forwarding в Velocity.
pub const CHANNEL: &str = "velocity:player_info";
/// Максимальная известная версия формата (1 — базовая, дальше добавляются
/// ключи подписи чата; нужный префикс у них общий).
const MAX_VERSION: i32 = 4;
/// Сколько свойств профиля принимаем. У ванильного профиля обычно одно
/// (`textures`), но моды могут добавить несколько.
const MAX_PROPERTIES: i32 = 64;

/// Проверяет подпись и разбирает ответ прокси.
pub fn parse(secret: &[u8], data: &[u8]) -> Result<GameProfile, String> {
    if secret.is_empty() {
        return Err("velocity secret is empty".into());
    }
    if data.len() < 32 {
        return Err("velocity forwarding data is too short".into());
    }
    let (signature, payload) = data.split_at(32);
    let expected = sign(secret, payload);
    if !constant_time_eq(signature, &expected) {
        return Err("velocity forwarding signature mismatch".into());
    }

    let mut r = Reader::new(payload);
    let version = r.var_int().map_err(protocol)?;
    if !(1..=MAX_VERSION).contains(&version) {
        return Err(format!("unsupported velocity forwarding version {version}"));
    }
    // Адрес в профиле нужен прокси для проверки; серверу он не нужен, но
    // поле обязательное и входит в подпись.
    let _address = r.string(255).map_err(protocol)?;
    let uuid = r.uuid().map_err(protocol)?;
    let name = r.string(16).map_err(protocol)?;
    if !valid_name(name) {
        return Err("velocity forwarded an invalid player name".into());
    }
    let count = r.var_int().map_err(protocol)?;
    if !(0..=MAX_PROPERTIES).contains(&count) {
        return Err(format!("invalid velocity property count {count}"));
    }
    let mut properties = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let name = r.string(64).map_err(protocol)?.to_owned();
        let value = r.string(32767).map_err(protocol)?.to_owned();
        let signature = r
            .bool()
            .map_err(protocol)?
            .then(|| r.string(32767).map(str::to_owned))
            .transpose()
            .map_err(protocol)?;
        properties.push(ProfileProperty {
            name,
            value,
            signature,
        });
    }
    // В версиях 2–4 после свойств идёт ключ чата и/или UUID подписанта.
    // Он уже покрыт подписью; для входа достаточно профиля выше.
    Ok(GameProfile {
        uuid,
        name: name.to_owned(),
        properties,
    })
}

/// HMAC-SHA256. Velocity кладёт его первыми 32 байтами ответа.
pub fn sign(secret: &[u8], payload: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut key = [0u8; BLOCK];
    if secret.len() > BLOCK {
        let digest = Sha256::digest(secret);
        key[..digest.len()].copy_from_slice(&digest);
    } else {
        key[..secret.len()].copy_from_slice(secret);
    }
    let mut inner = Sha256::new();
    for b in &key {
        inner.update([b ^ 0x36]);
    }
    inner.update(payload);
    let inner = inner.finalize();
    let mut outer = Sha256::new();
    for b in &key {
        outer.update([b ^ 0x5c]);
    }
    outer.update(inner);
    outer.finalize().into()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
}

fn protocol(error: ProtocolError) -> String {
    format!("velocity forwarding data: {error}")
}

fn valid_name(name: &str) -> bool {
    (1..=16).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use chunkwake_protocol::Writer;
    use uuid::Uuid;

    fn payload(version: i32, name: &str, uuid: Uuid, key: bool) -> Vec<u8> {
        let mut w = Writer::new();
        w.var_int(version)
            .string("203.0.113.7")
            .uuid(uuid)
            .string(name)
            .var_int(1)
            .string("textures")
            .string("dGV4")
            .bool(true)
            .string("c2ln");
        if key {
            // Версия 2+: произвольный ключ чата, подпись всё равно покрывает
            // весь payload, а разбор профиля останавливается на свойствах.
            w.bool(true).uuid(Uuid::from_u128(7));
        }
        w.buf
    }

    fn packet(secret: &[u8], payload: &[u8]) -> Vec<u8> {
        let mut out = sign(secret, payload).to_vec();
        out.extend_from_slice(payload);
        out
    }

    #[test]
    fn hmac_matches_rfc4231() {
        let mac = sign(b"Jefe", b"what do ya want for nothing?");
        let hex: String = mac.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn parses_signed_profile_and_ignores_key_tail() {
        let uuid = Uuid::from_u128(0x0123456789abcdef0123456789abcdef);
        for (version, key) in [(1, false), (2, true), (4, false)] {
            let body = payload(version, "Steve", uuid, key);
            let profile = parse(b"secret", &packet(b"secret", &body)).unwrap();
            assert_eq!(profile.uuid, uuid);
            assert_eq!(profile.name, "Steve");
            assert_eq!(profile.properties.len(), 1);
            assert_eq!(profile.properties[0].value, "dGV4");
            assert_eq!(profile.properties[0].signature.as_deref(), Some("c2ln"));
        }
    }

    #[test]
    fn rejects_bad_signature_and_short_data() {
        let body = payload(1, "Steve", Uuid::from_u128(1), false);
        let mut data = packet(b"secret", &body);
        let last = data.len() - 1;
        data[last] ^= 1;
        assert!(parse(b"secret", &data).is_err());
        assert!(parse(b"other", &packet(b"secret", &body)).is_err());
        assert!(parse(b"secret", &data[..31]).is_err());
    }

    #[test]
    fn rejects_bad_name_and_version() {
        let uuid = Uuid::from_u128(1);
        let body = payload(0, "Steve", uuid, false);
        assert!(parse(b"secret", &packet(b"secret", &body)).is_err());
        let body = payload(1, "bad name", uuid, false);
        assert!(parse(b"secret", &packet(b"secret", &body)).is_err());
    }
}
