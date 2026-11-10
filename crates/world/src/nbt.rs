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

/// Читает gzip-NBT файл (`playerdata`, `level.dat`); нет файла — `None`.
pub fn read_gzip_file(path: &std::path::Path) -> Result<Option<Compound>, String> {
    use std::io::Read;
    let raw = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("read {}: {e}", path.display())),
    };
    let mut data = Vec::new();
    flate2::read::GzDecoder::new(&raw[..])
        .read_to_end(&mut data)
        .map_err(|e| format!("gunzip {}: {e}", path.display()))?;
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
    // Имена и значения у нас ASCII; modified UTF-8 совпадает с UTF-8 без NUL и вне BMP.
    out.extend_from_slice(&(s.len() as u16).to_be_bytes());
    out.extend_from_slice(s.as_bytes());
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
    Ok(String::from_utf8_lossy(take(r, n)?).into_owned())
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
                    Tag::Compound(mut c) if is_wrapper(&c) => c.remove("").unwrap_or(Tag::Compound(c)),
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
}
