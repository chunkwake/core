//! Минимальный NBT (Java Edition, big-endian) для чанков Anvil.

use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub enum Tag {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<i8>),
    String(String),
    List(Vec<Tag>),
    Compound(Compound),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

pub type Compound = BTreeMap<String, Tag>;

impl Tag {
    pub fn id(&self) -> u8 {
        match self {
            Tag::Byte(_) => 1,
            Tag::Short(_) => 2,
            Tag::Int(_) => 3,
            Tag::Long(_) => 4,
            Tag::Float(_) => 5,
            Tag::Double(_) => 6,
            Tag::ByteArray(_) => 7,
            Tag::String(_) => 8,
            Tag::List(_) => 9,
            Tag::Compound(_) => 10,
            Tag::IntArray(_) => 11,
            Tag::LongArray(_) => 12,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match *self {
            Tag::Byte(v) => Some(v as i64),
            Tag::Short(v) => Some(v as i64),
            Tag::Int(v) => Some(v as i64),
            Tag::Long(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Tag::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_compound(&self) -> Option<&Compound> {
        match self {
            Tag::Compound(c) => Some(c),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[Tag]> {
        match self {
            Tag::List(l) => Some(l),
            _ => None,
        }
    }

    pub fn as_byte_array(&self) -> Option<&[i8]> {
        match self {
            Tag::ByteArray(b) => Some(b),
            _ => None,
        }
    }

    pub fn as_long_array(&self) -> Option<&[i64]> {
        match self {
            Tag::LongArray(l) => Some(l),
            _ => None,
        }
    }

    pub fn as_int_array(&self) -> Option<&[i32]> {
        match self {
            Tag::IntArray(v) => Some(v),
            _ => None,
        }
    }
}

/// Compound в сетевом NBT: байт типа и тело без имени. Пустой — `10 0`,
/// как пакет чанка пишет блок-сущность без данных.
pub fn write_network(c: &Compound) -> Vec<u8> {
    let mut out = vec![10];
    write_compound(&mut out, c);
    out
}

/// Любой тег в сетевом NBT: байт типа и тело без имени.
pub fn write_network_tag(t: &Tag) -> Vec<u8> {
    let mut out = vec![t.id()];
    write_payload(&mut out, t);
    out
}

/// Корневой compound с пустым именем (формат файлов чанков).
pub fn write_root(root: &Compound) -> Vec<u8> {
    let mut out = vec![10];
    write_str(&mut out, "");
    write_compound(&mut out, root);
    out
}

/// Предел распакованного NBT из файла: `level.dat`, `playerdata` и карты
/// пишет сам сервер, но импортированный мир может принести gzip-бомбу.
const MAX_NBT_FILE_BYTES: usize = 64 << 20;
/// Предел самого сжатого файла: читать гигабайты в память до распаковки
/// незачем.
const MAX_NBT_GZIP_BYTES: u64 = 64 << 20;

/// Читает gzip-NBT файл (`playerdata`, `level.dat`); нет файла — `None`.
pub fn read_gzip_file(path: &std::path::Path) -> Result<Option<Compound>, String> {
    read_gzip_file_capped(path, MAX_NBT_FILE_BYTES)
}

fn read_gzip_file_capped(path: &std::path::Path, limit: usize) -> Result<Option<Compound>, String> {
    use std::io::Read;
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("open {}: {e}", path.display())),
    };
    let mut raw = Vec::new();
    file.take(MAX_NBT_GZIP_BYTES + 1)
        .read_to_end(&mut raw)
        .map_err(|e| format!("read {}: {e}", path.display()))?;
    if raw.len() as u64 > MAX_NBT_GZIP_BYTES {
        return Err(format!("read {}: file is too large", path.display()));
    }
    let mut data = Vec::new();
    flate2::read::GzDecoder::new(&raw[..])
        .take(limit as u64 + 1)
        .read_to_end(&mut data)
        .map_err(|e| format!("gunzip {}: {e}", path.display()))?;
    if data.len() > limit {
        return Err(format!("gunzip {}: NBT is too large", path.display()));
    }
    read_root(&data)
        .map(Some)
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Пишет gzip-NBT через временный файл: оборванная запись не портит старый.
pub fn write_gzip_file(path: &std::path::Path, root: &Compound) -> Result<(), String> {
    use std::io::Write;
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(&write_root(root)).map_err(|e| e.to_string())?;
    let bytes = gz.finish().map_err(|e| e.to_string())?;
    let tmp = path.with_extension("dat_tmp");
    std::fs::write(&tmp, bytes).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    std::fs::File::options()
        .write(true)
        .open(&tmp)
        .and_then(|f| f.sync_all())
        .map_err(|e| format!("sync {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("rename {}: {e}", path.display()))
}

pub fn read_root(mut data: &[u8]) -> Result<Compound, String> {
    let r = &mut data;
    if take(r, 1)?[0] != 10 {
        return Err("root is not a compound".into());
    }
    read_str(r)?;
    read_compound(r, 0)
}

fn write_str(out: &mut Vec<u8>, s: &str) {
    // NBT пишется Java `DataOutput.writeUTF`: modified UTF-8, где NUL —
    // `C0 80`, а символы вне BMP — пара суррогатов по три байта. Сырой
    // UTF-8 с эмодзи Java прочитает как мусор, а NUL оборвёт строку.
    let units: Vec<u16> = s.encode_utf16().collect();
    let mut bytes = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < units.len() {
        let unit = units[i];
        let next = if (0xd800..=0xdbff).contains(&unit)
            && units
                .get(i + 1)
                .is_some_and(|u| (0xdc00..=0xdfff).contains(u))
        {
            Some(units[i + 1])
        } else {
            None
        };
        let len = match unit {
            0x0001..=0x007f => 1,
            0x0000 | 0x0080..=0x07ff => 2,
            _ => 3,
        };
        // Длина NBT-строки — u16. Режем по границе символа: половину
        // суррогатной пары записывать нельзя.
        if bytes.len() + len + next.map_or(0, |_| 3) > u16::MAX as usize {
            break;
        }
        write_utf_unit(&mut bytes, unit);
        if let Some(low) = next {
            write_utf_unit(&mut bytes, low);
            i += 1;
        }
        i += 1;
    }
    out.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
    out.extend_from_slice(&bytes);
}

fn write_utf_unit(out: &mut Vec<u8>, unit: u16) {
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

fn decode_utf(bytes: &[u8]) -> String {
    let mut units = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let first = bytes[i];
        if first & 0x80 == 0 {
            units.push(first as u16);
            i += 1;
        } else if first & 0xe0 == 0xc0 {
            let Some(&second) = bytes.get(i + 1) else {
                units.push(0xfffd);
                break;
            };
            units.push((((first & 0x1f) as u16) << 6) | (second & 0x3f) as u16);
            i += 2;
        } else if first & 0xf0 == 0xe0 {
            let Some(&second) = bytes.get(i + 1) else {
                units.push(0xfffd);
                break;
            };
            let Some(&third) = bytes.get(i + 2) else {
                units.push(0xfffd);
                break;
            };
            units.push(
                (((first & 0x0f) as u16) << 12)
                    | (((second & 0x3f) as u16) << 6)
                    | (third & 0x3f) as u16,
            );
            i += 3;
        } else {
            units.push(0xfffd);
            i += 1;
        }
    }
    String::from_utf16_lossy(&units)
}

/// `ListTag.isWrapper`: словарь из одного ключа `""`.
fn is_wrapper(c: &Compound) -> bool {
    c.len() == 1 && c.contains_key("")
}

fn write_compound(out: &mut Vec<u8>, c: &Compound) {
    for (k, v) in c {
        out.push(v.id());
        write_str(out, k);
        write_payload(out, v);
    }
    out.push(0);
}

fn write_payload(out: &mut Vec<u8>, t: &Tag) {
    match t {
        Tag::Byte(v) => out.push(*v as u8),
        Tag::Short(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::Int(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::Long(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::Float(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::Double(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::ByteArray(v) => {
            out.extend_from_slice(&(v.len() as i32).to_be_bytes());
            out.extend(v.iter().map(|&b| b as u8));
        }
        Tag::String(s) => write_str(out, s),
        Tag::List(l) => {
            out.extend_from_slice(&[0]);
            let at = out.len() - 1;
            out.extend_from_slice(&(l.len() as i32).to_be_bytes());
            let first = l.first().map_or(0, Tag::id);
            if l.iter().all(|t| t.id() == first) {
                out[at] = first;
                for t in l {
                    write_payload(out, t);
                }
            } else {
                // Разнотипный список (`ListTag.wrapIfNeeded`, 1.21.5+): список
                // словарей, прочее — в обёртке `{"": значение}`.
                out[at] = 10;
                for t in l {
                    match t {
                        Tag::Compound(c) if !is_wrapper(c) => write_compound(out, c),
                        t => write_compound(out, &Compound::from([(String::new(), t.clone())])),
                    }
                }
            }
        }
        Tag::Compound(c) => write_compound(out, c),
        Tag::IntArray(v) => {
            out.extend_from_slice(&(v.len() as i32).to_be_bytes());
            for x in v {
                out.extend_from_slice(&x.to_be_bytes());
            }
        }
        Tag::LongArray(v) => {
            out.extend_from_slice(&(v.len() as i32).to_be_bytes());
            for x in v {
                out.extend_from_slice(&x.to_be_bytes());
            }
        }
    }
}

fn take<'a>(r: &mut &'a [u8], n: usize) -> Result<&'a [u8], String> {
    if r.len() < n {
        return Err("unexpected end of NBT".into());
    }
    let (head, tail) = r.split_at(n);
    *r = tail;
    Ok(head)
}

fn be<const N: usize>(r: &mut &[u8]) -> Result<[u8; N], String> {
    Ok(take(r, N)?.try_into().unwrap())
}

fn read_len(r: &mut &[u8]) -> Result<usize, String> {
    let n = i32::from_be_bytes(be(r)?);
    // Защита от мусора: массив не может быть длиннее оставшихся данных.
    if n < 0 || n as usize > r.len() * 8 + 64 {
        return Err(format!("bad NBT length {n}"));
    }
    Ok(n as usize)
}

fn read_str(r: &mut &[u8]) -> Result<String, String> {
    let n = u16::from_be_bytes(be(r)?) as usize;
    Ok(decode_utf(take(r, n)?))
}

const MAX_DEPTH: usize = 512;

fn read_compound(r: &mut &[u8], depth: usize) -> Result<Compound, String> {
    let mut c = Compound::new();
    loop {
        let id = take(r, 1)?[0];
        if id == 0 {
            return Ok(c);
        }
        let name = read_str(r)?;
        c.insert(name, read_payload(r, id, depth + 1)?);
    }
}

fn read_payload(r: &mut &[u8], id: u8, depth: usize) -> Result<Tag, String> {
    if depth > MAX_DEPTH {
        return Err("NBT too deep".into());
    }
    Ok(match id {
        1 => Tag::Byte(take(r, 1)?[0] as i8),
        2 => Tag::Short(i16::from_be_bytes(be(r)?)),
        3 => Tag::Int(i32::from_be_bytes(be(r)?)),
        4 => Tag::Long(i64::from_be_bytes(be(r)?)),
        5 => Tag::Float(f32::from_be_bytes(be(r)?)),
        6 => Tag::Double(f64::from_be_bytes(be(r)?)),
        7 => {
            let n = read_len(r)?;
            Tag::ByteArray(take(r, n)?.iter().map(|&b| b as i8).collect())
        }
        8 => Tag::String(read_str(r)?),
        9 => {
            let elem = take(r, 1)?[0];
            let n = read_len(r)?;
            let mut l = Vec::with_capacity(n.min(4096));
            for _ in 0..n {
                let t = read_payload(r, elem, depth + 1)?;
                // `ListTag`: обёртка `{"": значение}` разнотипного списка снимается.
                l.push(match t {
                    Tag::Compound(mut c) if is_wrapper(&c) => {
                        c.remove("").unwrap_or(Tag::Compound(c))
                    }
                    t => t,
                });
            }
            Tag::List(l)
        }
        10 => Tag::Compound(read_compound(r, depth)?),
        11 => {
            let n = read_len(r)?;
            Tag::IntArray(
                take(r, n * 4)?
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|b| i32::from_be_bytes(*b))
                    .collect(),
            )
        }
        12 => {
            let n = read_len(r)?;
            Tag::LongArray(
                take(r, n * 8)?
                    .as_chunks::<8>()
                    .0
                    .iter()
                    .map(|b| i64::from_be_bytes(*b))
                    .collect(),
            )
        }
        _ => return Err(format!("unknown NBT tag {id}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_list_is_wrapped_like_vanilla() {
        let red = Tag::Compound(Compound::from([("text".into(), Tag::String("x".into()))]));
        let mixed = Tag::List(vec![Tag::String("a".into()), red.clone(), Tag::Int(3)]);
        let root = Compound::from([("l".into(), mixed.clone())]);
        let mut out = Vec::new();
        write_payload(&mut out, &Tag::Compound(root.clone()));
        // Тип элементов — словарь (10).
        assert_eq!(out[1 + 2 + 1], 10);
        let back = read_payload(&mut &out[..], 10, 0).unwrap();
        assert_eq!(back, Tag::Compound(root));
    }

    #[test]
    fn roundtrip() {
        let mut inner = Compound::new();
        inner.insert("Name".into(), Tag::String("minecraft:stone".into()));
        let mut root = Compound::new();
        root.insert("DataVersion".into(), Tag::Int(5023));
        root.insert("Y".into(), Tag::Byte(-4));
        root.insert("data".into(), Tag::LongArray(vec![1, -1]));
        root.insert(
            "palette".into(),
            Tag::List(vec![Tag::Compound(inner.clone())]),
        );
        root.insert("empty".into(), Tag::List(vec![]));
        let bytes = write_root(&root);
        assert_eq!(&bytes[..3], &[10, 0, 0]);
        assert_eq!(read_root(&bytes).unwrap(), root);
        assert!(read_root(&bytes[..bytes.len() - 1]).is_err());
    }

    #[test]
    fn modified_utf8_roundtrip() {
        for text in [
            "Steve",
            "Привет",
            "\u{1F600}",
            "a\0b",
            "Привет \u{1F600}\0",
        ] {
            let mut out = Vec::new();
            write_str(&mut out, text);
            let mut slice = out.as_slice();
            assert_eq!(read_str(&mut slice).unwrap(), text, "{text:?}");
        }
        // Java `writeUTF` для U+1F600: пара суррогатов, каждый по 3 байта.
        let mut out = Vec::new();
        write_str(&mut out, "\u{1F600}");
        assert_eq!(out, [0, 6, 0xED, 0xA0, 0xBD, 0xED, 0xB8, 0x80]);
        // NUL в modified UTF-8 — два байта, а не нулевой байт.
        let mut out = Vec::new();
        write_str(&mut out, "\0");
        assert_eq!(out, [0, 2, 0xC0, 0x80]);
    }

    /// Файл мира мог прийти извне: распаковка не должна расти без предела.
    #[test]
    fn gzip_reader_is_capped() {
        let dir = std::env::temp_dir().join(format!("chunkwake-nbt-cap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("level.dat");
        write_gzip_file(&path, &Compound::new()).unwrap();
        assert!(read_gzip_file_capped(&path, 1).is_err());
        assert!(read_gzip_file_capped(&path, 4096).unwrap().is_some());
        let _ = std::fs::remove_dir_all(dir);
    }
}
