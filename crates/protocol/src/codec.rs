//! Чтение и запись базовых типов протокола: VarInt, строки, UUID и т.д.

use crate::{ProtocolError, Result};
use uuid::Uuid;

/// Курсор чтения поверх тела пакета. Не аллоцирует.
pub struct Reader<'a> {
    buf: &'a [u8],
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf }
    }

    pub fn remaining(&self) -> &'a [u8] {
        self.buf
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.buf.len() < n {
            return Err(ProtocolError::Eof);
        }
        let (head, tail) = self.buf.split_at(n);
        self.buf = tail;
        Ok(head)
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }

    pub fn bool(&mut self) -> Result<bool> {
        Ok(self.u8()? != 0)
    }

    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.bytes(2)?.try_into().unwrap()))
    }

    pub fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_be_bytes(self.bytes(8)?.try_into().unwrap()))
    }

    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_bits(u32::from_be_bytes(
            self.bytes(4)?.try_into().unwrap(),
        )))
    }

    pub fn f64(&mut self) -> Result<f64> {
        Ok(f64::from_bits(self.i64()? as u64))
    }

    pub fn var_int(&mut self) -> Result<i32> {
        let mut value = 0u32;
        for i in 0..5 {
            let b = self.u8()?;
            value |= ((b & 0x7f) as u32) << (7 * i);
            if b & 0x80 == 0 {
                return Ok(value as i32);
            }
        }
        Err(ProtocolError::VarIntTooLong)
    }

    pub fn string(&mut self, max_chars: usize) -> Result<&'a str> {
        let len = self.var_int()?;
        if len < 0 || len as usize > max_chars * 3 {
            return Err(ProtocolError::BadLength(len));
        }
        let s =
            std::str::from_utf8(self.bytes(len as usize)?).map_err(|_| ProtocolError::BadUtf8)?;
        if s.chars().count() > max_chars {
            return Err(ProtocolError::StringTooLong { max: max_chars });
        }
        Ok(s)
    }

    pub fn uuid(&mut self) -> Result<Uuid> {
        Ok(Uuid::from_slice(self.bytes(16)?).unwrap())
    }
}

/// Буфер записи тела пакета.
#[derive(Default)]
pub struct Writer {
    pub buf: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.buf.push(v);
        self
    }

    pub fn bool(&mut self, v: bool) -> &mut Self {
        self.u8(v as u8)
    }

    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.buf.extend_from_slice(&v.to_be_bytes());
        self
    }

    pub fn i64(&mut self, v: i64) -> &mut Self {
        self.buf.extend_from_slice(&v.to_be_bytes());
        self
    }

    pub fn i32(&mut self, v: i32) -> &mut Self {
        self.buf.extend_from_slice(&v.to_be_bytes());
        self
    }

    pub fn f32(&mut self, v: f32) -> &mut Self {
        self.buf.extend_from_slice(&v.to_be_bytes());
        self
    }

    pub fn f64(&mut self, v: f64) -> &mut Self {
        self.buf.extend_from_slice(&v.to_be_bytes());
        self
    }

    pub fn var_int(&mut self, v: i32) -> &mut Self {
        write_var_int(&mut self.buf, v);
        self
    }

    pub fn byte_array(&mut self, b: &[u8]) -> &mut Self {
        self.var_int(b.len() as i32);
        self.buf.extend_from_slice(b);
        self
    }

    pub fn string(&mut self, s: &str) -> &mut Self {
        self.var_int(s.len() as i32);
        self.buf.extend_from_slice(s.as_bytes());
        self
    }

    pub fn uuid(&mut self, u: Uuid) -> &mut Self {
        self.buf.extend_from_slice(u.as_bytes());
        self
    }
}

pub fn write_var_int(out: &mut Vec<u8>, v: i32) {
    let mut v = v as u32;
    loop {
        if v & !0x7f == 0 {
            out.push(v as u8);
            return;
        }
        out.push((v as u8 & 0x7f) | 0x80);
        v >>= 7;
    }
}

pub fn var_int_len(v: i32) -> usize {
    match v as u32 {
        0..=0x7f => 1,
        0x80..=0x3fff => 2,
        0x4000..=0x1f_ffff => 3,
        0x20_0000..=0x0fff_ffff => 4,
        _ => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn var_int_roundtrip() {
        for v in [0, 1, 127, 128, 255, 25565, 2097151, i32::MAX, -1, i32::MIN] {
            let mut w = Writer::new();
            w.var_int(v);
            assert_eq!(w.buf.len(), var_int_len(v));
            assert_eq!(Reader::new(&w.buf).var_int().unwrap(), v);
        }
    }

    #[test]
    fn var_int_known_encodings() {
        let mut w = Writer::new();
        w.var_int(-1);
        assert_eq!(w.buf, [0xff, 0xff, 0xff, 0xff, 0x0f]);
        assert!(Reader::new(&[0xff; 6]).var_int().is_err());
    }

    #[test]
    fn string_limits() {
        let mut w = Writer::new();
        w.string("Steve");
        assert_eq!(Reader::new(&w.buf).string(16).unwrap(), "Steve");
        assert!(Reader::new(&w.buf).string(4).is_err());
    }
}
