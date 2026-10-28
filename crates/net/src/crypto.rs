//! Шифрование login: RSA-ключ сервера и потоковый AES-128/CFB8.

use aes::Aes128;
use cfb8::cipher::generic_array::GenericArray;
use cfb8::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use rsa::pkcs8::EncodePublicKey;
use rsa::{Pkcs1v15Encrypt, RsaPrivateKey};

/// RSA-1024 ключ, как у ванильного сервера. Генерируется один раз при старте.
pub struct ServerKey {
    private: RsaPrivateKey,
    public_der: Vec<u8>,
}

impl ServerKey {
    pub fn generate() -> Self {
        let private = RsaPrivateKey::new(&mut rand::thread_rng(), 1024).expect("RSA keygen");
        let public_der = private
            .to_public_key()
            .to_public_key_der()
            .expect("DER encode")
            .into_vec();
        Self {
            private,
            public_der,
        }
    }

    pub fn public_der(&self) -> &[u8] {
        &self.public_der
    }

    pub fn decrypt(&self, data: &[u8]) -> Option<Vec<u8>> {
        self.private.decrypt(Pkcs1v15Encrypt, data).ok()
    }
}

/// Пара шифраторов соединения. Ключ и IV — общий секрет (16 байт).
pub struct StreamCipher {
    enc: cfb8::Encryptor<Aes128>,
    dec: cfb8::Decryptor<Aes128>,
}

impl StreamCipher {
    pub fn new(secret: &[u8; 16]) -> Self {
        Self {
            enc: cfb8::Encryptor::new(secret.into(), secret.into()),
            dec: cfb8::Decryptor::new(secret.into(), secret.into()),
        }
    }

    /// Весь срез одним вызовом: бэкенд AES берётся один раз, а не на байт.
    pub fn encrypt(&mut self, data: &mut [u8]) {
        self.enc.encrypt_blocks_mut(as_blocks(data));
    }

    pub fn decrypt(&mut self, data: &mut [u8]) {
        self.dec.decrypt_blocks_mut(as_blocks(data));
    }
}

/// Блок CFB8 — один байт (`GenericArray<u8, U1>`, та же раскладка, что у `u8`).
fn as_blocks(data: &mut [u8]) -> &mut [GenericArray<u8, cfb8::cipher::consts::U1>] {
    // SAFETY: `GenericArray<u8, U1>` — `repr(transparent)` над `[u8; 1]`.
    unsafe { std::slice::from_raw_parts_mut(data.as_mut_ptr().cast(), data.len()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsa::RsaPublicKey;
    use rsa::pkcs8::DecodePublicKey;

    #[test]
    fn bulk_matches_per_byte() {
        let secret = [3u8; 16];
        let data: Vec<u8> = (0..5000u32).map(|i| (i * 31 % 251) as u8).collect();
        let mut bulk = data.clone();
        let mut c = StreamCipher::new(&secret);
        c.encrypt(&mut bulk[..100]);
        c.encrypt(&mut bulk[100..]);
        let mut one = data.clone();
        let mut e = cfb8::Encryptor::<Aes128>::new((&secret).into(), (&secret).into());
        for b in &mut one {
            let mut block = GenericArray::from([*b]);
            e.encrypt_block_mut(&mut block);
            *b = block[0];
        }
        assert_eq!(bulk, one);
        let mut d = StreamCipher::new(&secret);
        d.decrypt(&mut bulk);
        assert_eq!(bulk, data);
    }

    #[test]
    fn rsa_roundtrip() {
        let key = ServerKey::generate();
        let public = RsaPublicKey::from_public_key_der(key.public_der()).unwrap();
        let secret = [7u8; 16];
        let enc = public
            .encrypt(&mut rand::thread_rng(), Pkcs1v15Encrypt, &secret)
            .unwrap();
        assert_eq!(key.decrypt(&enc).unwrap(), secret);
        assert!(key.decrypt(&[1, 2, 3]).is_none());
    }

    #[test]
    fn cfb8_stream_roundtrip_in_pieces() {
        let secret = [3u8; 16];
        let (mut a, mut b) = (StreamCipher::new(&secret), StreamCipher::new(&secret));
        let plain: Vec<u8> = (0..=255).collect();
        let mut data = plain.clone();
        a.encrypt(&mut data[..100]);
        a.encrypt(&mut data[100..]);
        assert_ne!(data, plain);
        b.decrypt(&mut data[..7]);
        b.decrypt(&mut data[7..]);
        assert_eq!(data, plain);
    }
}
