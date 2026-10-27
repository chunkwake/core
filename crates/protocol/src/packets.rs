//! Пакеты состояний handshake, status и login.
//!
//! ID этих состояний стабильны с 1.20.2. Для play/configuration ID будут
//! генерироваться из `reports/packets.json` ванильного генератора данных.

use crate::{ProtocolError, Reader, Result, Writer};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NextState {
    Status,
    Login,
    Transfer,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Handshake<'a> {
    pub protocol_version: i32,
    pub server_address: &'a str,
    pub server_port: u16,
    pub next_state: NextState,
}

impl<'a> Handshake<'a> {
    pub fn decode(packet: &'a [u8]) -> Result<Self> {
        let mut r = Reader::new(packet);
        let id = r.var_int()?;
        if id != 0x00 {
            return Err(ProtocolError::UnknownPacket {
                id,
                state: "handshake",
            });
        }
        let protocol_version = r.var_int()?;
        let server_address = r.string(255)?;
        let server_port = r.u16()?;
        let next_state = match r.var_int()? {
            1 => NextState::Status,
            2 => NextState::Login,
            3 => NextState::Transfer,
            n => return Err(ProtocolError::BadNextState(n)),
        };
        Ok(Self {
            protocol_version,
            server_address,
            server_port,
            next_state,
        })
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.var_int(0x00)
            .var_int(self.protocol_version)
            .string(self.server_address)
            .u16(self.server_port)
            .var_int(match self.next_state {
                NextState::Status => 1,
                NextState::Login => 2,
                NextState::Transfer => 3,
            });
        w.buf
    }
}

/// Serverbound-пакеты состояния status.
#[derive(Debug, PartialEq, Eq)]
pub enum StatusC2S {
    Request,
    Ping(i64),
}

impl StatusC2S {
    pub fn decode(packet: &[u8]) -> Result<Self> {
        let mut r = Reader::new(packet);
        match r.var_int()? {
            0x00 => Ok(Self::Request),
            0x01 => Ok(Self::Ping(r.i64()?)),
            id => Err(ProtocolError::UnknownPacket {
                id,
                state: "status",
            }),
        }
    }
}

pub fn status_response(json: &str) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(0x00).string(json);
    w.buf
}

pub fn pong(payload: i64) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(0x01).i64(payload);
    w.buf
}

/// Serverbound-пакеты состояния login.
#[derive(Debug, PartialEq, Eq)]
pub enum LoginC2S<'a> {
    Start {
        name: &'a str,
        uuid: Uuid,
    },
    EncryptionResponse {
        shared_secret: &'a [u8],
        verify_token: &'a [u8],
    },
    PluginResponse {
        message_id: i32,
        data: Option<&'a [u8]>,
    },
    Acknowledged,
    CookieResponse,
}

impl<'a> LoginC2S<'a> {
    pub fn decode(packet: &'a [u8]) -> Result<Self> {
        let mut r = Reader::new(packet);
        match r.var_int()? {
            0x00 => Ok(Self::Start {
                name: r.string(16)?,
                uuid: r.uuid()?,
            }),
            0x01 => {
                let n = r.var_int()?;
                let shared_secret =
                    r.bytes(usize::try_from(n).map_err(|_| ProtocolError::BadLength(n))?)?;
                let n = r.var_int()?;
                let verify_token =
                    r.bytes(usize::try_from(n).map_err(|_| ProtocolError::BadLength(n))?)?;
                Ok(Self::EncryptionResponse {
                    shared_secret,
                    verify_token,
                })
            }
            0x02 => {
                let message_id = r.var_int()?;
                let data = if r.bool()? { Some(r.remaining()) } else { None };
                Ok(Self::PluginResponse { message_id, data })
            }
            0x03 => Ok(Self::Acknowledged),
            0x04 => Ok(Self::CookieResponse),
            id => Err(ProtocolError::UnknownPacket { id, state: "login" }),
        }
    }
}

/// Disconnect в состоянии login: причина — JSON текстового компонента.
pub fn login_disconnect(reason_json: &str) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(0x00).string(reason_json);
    w.buf
}

/// Свойство игрового профиля (например, `textures`) от session server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileProperty {
    pub name: String,
    pub value: String,
    pub signature: Option<String>,
}

pub fn login_success(uuid: Uuid, name: &str, properties: &[ProfileProperty]) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(0x02)
        .uuid(uuid)
        .string(name)
        .var_int(properties.len() as i32);
    for p in properties {
        w.string(&p.name)
            .string(&p.value)
            .bool(p.signature.is_some());
        if let Some(sig) = &p.signature {
            w.string(sig);
        }
    }
    w.buf
}

/// Encryption Request: пустой server id, публичный ключ (DER SPKI), verify token.
pub fn encryption_request(
    public_key_der: &[u8],
    verify_token: &[u8],
    authenticate: bool,
) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(0x01)
        .string("")
        .byte_array(public_key_der)
        .byte_array(verify_token)
        .bool(authenticate);
    w.buf
}

/// Хэш для session server: SHA-1 как знаковое big-endian число в hex (как в ваниле).
pub fn server_hash(server_id: &str, shared_secret: &[u8], public_key_der: &[u8]) -> String {
    let mut sha = sha1_smol::Sha1::new();
    sha.update(server_id.as_bytes());
    sha.update(shared_secret);
    sha.update(public_key_der);
    let mut digest = sha.digest().bytes();
    let negative = digest[0] & 0x80 != 0;
    if negative {
        // Дополнение до двух: инвертировать и прибавить 1.
        let mut carry = true;
        for b in digest.iter_mut().rev() {
            *b = !*b;
            if carry {
                let (v, c) = b.overflowing_add(1);
                *b = v;
                carry = c;
            }
        }
    }
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    let trimmed = hex.trim_start_matches('0');
    let trimmed = if trimmed.is_empty() { "0" } else { trimmed };
    if negative {
        format!("-{trimmed}")
    } else {
        trimmed.to_owned()
    }
}

pub fn set_compression(threshold: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(0x03).var_int(threshold);
    w.buf
}

/// UUID офлайн-игрока: UUIDv3 от `"OfflinePlayer:" + name`, как в ваниле.
pub fn offline_uuid(name: &str) -> Uuid {
    let digest = md5_compat(format!("OfflinePlayer:{name}").as_bytes());
    let mut b = digest;
    b[6] = (b[6] & 0x0f) | 0x30;
    b[8] = (b[8] & 0x3f) | 0x80;
    Uuid::from_bytes(b)
}

fn md5_compat(data: &[u8]) -> [u8; 16] {
    // UUIDv3 из крейта uuid = MD5 по (namespace || name); ванила использует
    // MD5 без namespace, поэтому считаем MD5 сами.
    md5::compute(data)
}

mod md5 {
    //! Минимальная реализация MD5 (RFC 1321) — только для офлайн-UUID.
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];

    pub fn compute(input: &[u8]) -> [u8; 16] {
        let k: [u32; 64] =
            std::array::from_fn(|i| ((i as f64 + 1.0).sin().abs() * 4294967296.0) as u32);
        let mut msg = input.to_vec();
        let bit_len = (input.len() as u64).wrapping_mul(8);
        msg.push(0x80);
        while msg.len() % 64 != 56 {
            msg.push(0);
        }
        msg.extend_from_slice(&bit_len.to_le_bytes());

        let mut h = [0x67452301u32, 0xefcdab89, 0x98badcfe, 0x10325476];
        for chunk in msg.as_chunks::<64>().0 {
            let m: [u32; 16] = std::array::from_fn(|i| {
                u32::from_le_bytes(chunk[i * 4..i * 4 + 4].try_into().unwrap())
            });
            let [mut a, mut b, mut c, mut d] = h;
            for i in 0..64 {
                let (f, g) = match i / 16 {
                    0 => ((b & c) | (!b & d), i),
                    1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                    2 => (b ^ c ^ d, (3 * i + 5) % 16),
                    _ => (c ^ (b | !d), (7 * i) % 16),
                };
                let f = f.wrapping_add(a).wrapping_add(k[i]).wrapping_add(m[g]);
                a = d;
                d = c;
                c = b;
                b = b.wrapping_add(f.rotate_left(S[i]));
            }
            h[0] = h[0].wrapping_add(a);
            h[1] = h[1].wrapping_add(b);
            h[2] = h[2].wrapping_add(c);
            h[3] = h[3].wrapping_add(d);
        }
        let mut out = [0u8; 16];
        for (i, v) in h.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handshake_roundtrip() {
        let hs = Handshake {
            protocol_version: 1234,
            server_address: "localhost",
            server_port: 25565,
            next_state: NextState::Login,
        };
        assert_eq!(Handshake::decode(&hs.encode()).unwrap(), hs);
    }

    #[test]
    fn md5_vectors() {
        let hex = |d: [u8; 16]| d.iter().map(|b| format!("{b:02x}")).collect::<String>();
        assert_eq!(hex(md5::compute(b"")), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(
            hex(md5::compute(b"The quick brown fox jumps over the lazy dog")),
            "9e107d9d372bb6826bd81d3542a419d6"
        );
    }

    #[test]
    fn offline_uuid_is_v3() {
        let u = offline_uuid("Notch");
        assert_eq!(u.get_version_num(), 3);
        assert_eq!(u, offline_uuid("Notch"));
        assert_ne!(u, offline_uuid("notch"));
    }

    #[test]
    fn server_hash_vectors() {
        // Эталонные значения из документации протокола (wiki.vg).
        let h = |s: &str| server_hash(s, &[], &[]);
        assert_eq!(h("Notch"), "4ed1f46bbe04bc756bcb17c0c7ce3e4632f06a48");
        assert_eq!(h("jeb_"), "-7c9d5b0044c130109a5d7b5fb5c317c02b4e28c1");
        assert_eq!(h("simon"), "88e16a1019277b15d58faf0541e11910eb756f6");
    }

    #[test]
    fn login_start_decode() {
        let id = Uuid::from_u128(42);
        let mut w = Writer::new();
        w.var_int(0x00).string("Steve").uuid(id);
        assert_eq!(
            LoginC2S::decode(&w.buf).unwrap(),
            LoginC2S::Start {
                name: "Steve",
                uuid: id
            }
        );
    }
}
