//! Проверка совместимости сжатия регионов с ванилью.
//!
//! `region_rewrite <region_dir>` — прочитать все чанки и посчитать виды
//! сжатия (байт перед payload).
//! `region_rewrite <region_dir> <deflate|lz4|none>` — то же и переписать
//! все чанки на месте выбранным сжатием.
use chunkwake_world::anvil::{Codec, RegionStore};
use std::collections::BTreeMap;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(&a[1]);
    let mut kinds: BTreeMap<u8, usize> = BTreeMap::new();
    for e in std::fs::read_dir(&dir).unwrap().flatten() {
        let bytes = std::fs::read(e.path()).unwrap();
        if !e.file_name().to_string_lossy().ends_with(".mca") || bytes.len() < 8192 {
            continue;
        }
        for i in 0..1024 {
            let loc = u32::from_be_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap()) as usize;
            if loc != 0 {
                *kinds.entry(bytes[(loc >> 8) * 4096 + 4]).or_default() += 1;
            }
        }
    }
    let store = RegionStore::new(&dir);
    let chunks = store.load_all().unwrap();
    println!("read {} chunks, compression kinds {kinds:?}", chunks.len());
    if let Some(codec) = a.get(2) {
        let codec = Codec::parse(codec).expect("deflate, lz4 or none");
        for (rx, rz) in store.regions().unwrap() {
            let _ = std::fs::remove_file(dir.join(format!("r.{rx}.{rz}.mca")));
        }
        RegionStore::new(&dir).with_codec(codec).save(&chunks).unwrap();
        let again = RegionStore::new(&dir).load_all().unwrap();
        assert_eq!(again.len(), chunks.len());
        println!("rewrote {} chunks as {codec:?}", again.len());
    }
}
