//! Замер хранения: сколько весят чанки каталога `region` при разных уровнях
//! deflate и сколько стоит сжатие и распаковка.
//!
//! `region_stats <каталог region>`
use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use chunkwake_world::nbt::{self, Compound, Tag};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::time::Instant;

fn main() {
    let dir = std::env::args().nth(1).expect("каталог region");
    let mut raw = Vec::new();
    let (mut files, mut file_bytes, mut stored) = (0usize, 0u64, 0usize);
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "mca") {
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        files += 1;
        file_bytes += bytes.len() as u64;
        for i in 0..1024 {
            let loc = u32::from_be_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
            if loc == 0 {
                continue;
            }
            let off = (loc >> 8) as usize * 4096;
            let len = u32::from_be_bytes(bytes[off..off + 4].try_into().unwrap()) as usize;
            let kind = bytes[off + 4];
            assert_eq!(kind, 2, "только zlib");
            stored += len - 1;
            let mut out = Vec::new();
            ZlibDecoder::new(&bytes[off + 5..off + 4 + len]).read_to_end(&mut out).unwrap();
            raw.push(out);
        }
    }
    let nbt: usize = raw.iter().map(Vec::len).sum();
    breakdown(&raw);
    println!(
        "{files} файлов, {} МБ на диске, {} чанков, сжато {} МБ, NBT {} МБ",
        file_bytes >> 20,
        raw.len(),
        stored >> 20,
        nbt >> 20
    );
    for level in [1, 3, 6, 9] {
        let t = Instant::now();
        let mut packed = Vec::with_capacity(raw.len());
        for chunk in &raw {
            let mut enc = ZlibEncoder::new(Vec::new(), Compression::new(level));
            enc.write_all(chunk).unwrap();
            packed.push(enc.finish().unwrap());
        }
        let ct = t.elapsed();
        let t = Instant::now();
        for p in &packed {
            let mut out = Vec::with_capacity(nbt / raw.len() * 2);
            ZlibDecoder::new(&p[..]).read_to_end(&mut out).unwrap();
        }
        let dt = t.elapsed();
        let size: usize = packed.iter().map(Vec::len).sum();
        // Сектор 4 КиБ: место в файле с округлением, как в `.mca`.
        let sectors: usize = packed.iter().map(|p| (p.len() + 5).div_ceil(4096)).sum();
        println!(
            "уровень {level}: {} МБ, в секторах {} МБ, сжатие {:.0} мкс/чанк, распаковка {:.0} мкс/чанк",
            size >> 20,
            (sectors * 4096) >> 20,
            ct.as_secs_f64() * 1e6 / raw.len() as f64,
            dt.as_secs_f64() * 1e6 / raw.len() as f64
        );
    }
}

/// Размер записи NBT `key: tag` в байтах.
fn size(key: &str, tag: &Tag) -> usize {
    let mut c = Compound::new();
    c.insert(key.to_owned(), tag.clone());
    nbt::write_network(&c).len() - 2
}

/// Из чего состоят полные чанки: ключи корня и секций, среднее на чанк.
fn breakdown(raw: &[Vec<u8>]) {
    let mut by: BTreeMap<String, usize> = BTreeMap::new();
    let mut statuses: BTreeMap<String, usize> = BTreeMap::new();
    let mut full = 0usize;
    for bytes in raw {
        let root = nbt::read_root(bytes).unwrap();
        let status = root.get("Status").and_then(Tag::as_str).unwrap_or("?").to_owned();
        *statuses.entry(status.clone()).or_default() += 1;
        if status != "minecraft:full" {
            continue;
        }
        full += 1;
        *by.entry("(всё)".into()).or_default() += bytes.len();
        for (k, v) in &root {
            *by.entry(k.clone()).or_default() += size(k, v);
            if k == "sections" {
                for sec in v.as_list().unwrap_or(&[]) {
                    for (sk, sv) in sec.as_compound().unwrap() {
                        *by.entry(format!("sections/{sk}")).or_default() += size(sk, sv);
                        if let Some(inner) = sv.as_compound() {
                            for (ik, iv) in inner {
                                *by.entry(format!("sections/{sk}/{ik}")).or_default() += size(ik, iv);
                            }
                        }
                    }
                }
            }
        }
    }
    println!("статусы: {statuses:?}");
    // Свет неба без однородных секций (как у ванили): сколько это после сжатия.
    let (mut with, mut without, mut n) = (0usize, 0usize, 0usize);
    for bytes in raw.iter().step_by(7) {
        let mut root = nbt::read_root(bytes).unwrap();
        if root.get("Status").and_then(Tag::as_str) != Some("minecraft:full") {
            continue;
        }
        n += 1;
        with += deflate(&nbt::write_root(&root), 6);
        if let Some(Tag::List(secs)) = root.get_mut("sections") {
            for sec in secs {
                if let Tag::Compound(c) = sec
                    && let Some(Tag::ByteArray(b)) = c.get("SkyLight")
                    && b.iter().all(|&v| v == b[0])
                {
                    c.remove("SkyLight");
                }
            }
        }
        without += deflate(&nbt::write_root(&root), 6);
    }
    if let (Some(with), Some(without)) = (with.checked_div(n), without.checked_div(n)) {
        println!("уровень 6, полные чанки: со светом неба {with} Б, без однородного {without} Б на чанк");
    }
    if full == 0 {
        return;
    }
    println!("полных чанков {full}, средний NBT по ключам:");
    for (k, v) in by {
        println!("  {k:<32} {:>8} Б", v / full);
    }
}

fn deflate(b: &[u8], level: u32) -> usize {
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::new(level));
    enc.write_all(b).unwrap();
    enc.finish().unwrap().len()
}
