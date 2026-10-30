//! Золотой тест генерации: блоки чанков не меняются ни на бит.
//!
//! Оптимизации генерации (аллокации, объёмы, поверхность) не имеют права
//! менять мир: тест считает хеш блоков по квадрату чанков и сверяет с
//! эталоном. Без данных игры (`CHUNKWAKE_DATA` или `target/release/generated`)
//! тест пропускается — как бенчмарки, которым нужен настоящий мир.

use chunkwake_worldgen::pipeline::Overworld;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Сид и квадрат, для которого зафиксирован эталон.
const SEED: i64 = 12345;
const RADIUS: i32 = 3;

/// Эталонные хеши по чанкам `(cx, cz)`, сид 12345, данные 26.3.
///
/// Если тест упал, значит генерация изменилась. Так можно только если мир
/// намеренно исправлен: тогда хеши пересчитываются вместе с обоснованием
/// в сообщении коммита.
const GOLDEN: &[((i32, i32), u64)] = &[
    ((0, 0), 0xe0143658ff38ef0f),
    ((-1, -1), 0x677abb97f79a4445),
    ((-1, 0), 0x148824b6a3633552),
    ((-1, 1), 0x5896992d53968f2c),
    ((0, -1), 0xa24c2a9a59fa2f79),
    ((0, 1), 0x095f401d39164f3b),
    ((1, -1), 0xdd25336713b701c2),
    ((1, 0), 0x8c485683639b2198),
    ((1, 1), 0xbbeb2261d9443ea3),
    ((-2, -2), 0xbeb049e091d1cab1),
    ((-2, -1), 0x6b555bfef5dff6be),
    ((-2, 0), 0xea9daf587f67d11e),
    ((-2, 1), 0x9edc4d641e2a776d),
    ((-2, 2), 0xb731ec827a70644d),
    ((-1, -2), 0x8131996430d233d7),
    ((-1, 2), 0x5ed95c4713ceebb3),
    ((0, -2), 0xafe392a43b7e4de6),
    ((0, 2), 0x184e20be6f947dcb),
    ((1, -2), 0x98960fa54a1d3fd6),
    ((1, 2), 0x620ef00a7c4a901c),
    ((2, -2), 0x1caddc7a827ebbc5),
    ((2, -1), 0x47ec0790c7eab83d),
    ((2, 0), 0x3488907148677c42),
    ((2, 1), 0x164b9df8ac49f43d),
    ((2, 2), 0xf0f3e5dde14b51d2),
    ((-3, -3), 0x27984c79e5934e1c),
    ((-3, -2), 0xab055f0cae47afa2),
    ((-3, -1), 0xedcadcede7fba82e),
    ((-3, 0), 0x434926196b6dd9c4),
    ((-3, 1), 0x5a2a57485a3414d0),
    ((-3, 2), 0x90d25b3c46fe3ea6),
    ((-3, 3), 0x5af26960a470320c),
    ((-2, -3), 0xf0c93800b86620ff),
    ((-2, 3), 0x6e157c21540ee0b1),
    ((-1, -3), 0xbe9c17f06ba15792),
    ((-1, 3), 0xca95afe1286e785c),
    ((0, -3), 0x9e07352db256a821),
    ((0, 3), 0x982452649609148f),
    ((1, -3), 0xee786b1b1544b945),
    ((1, 3), 0xbb01b12902564085),
    ((2, -3), 0xa091de293e04a0bf),
    ((2, 3), 0x43d35b91d4d6fa12),
    ((3, -3), 0x79f1ced3d773c84f),
    ((3, -2), 0xa145a91566b41651),
    ((3, -1), 0xe6400bc3b8f5b260),
    ((3, 0), 0xe9cb466db04dd46c),
    ((3, 1), 0x37c4a5c0f45f4ebe),
    ((3, 2), 0xea0d140ee0a906db),
    ((3, 3), 0x46bf7878807ad490),
];

fn data_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("CHUNKWAKE_DATA") {
        let dir = PathBuf::from(dir);
        if dir.join("reports/light.csv").is_file() {
            return Some(dir);
        }
    }
    let cache =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/release/generated/26.3");
    cache.join("reports/light.csv").is_file().then_some(cache)
}

/// FNV-1a по состояниям: дешёвый и достаточно чувствительный к одной клетке.
fn hash(states: &[u32]) -> u64 {
    let mut h: u64 = 0xcbf29ce4_84222325;
    for &s in states {
        for b in s.to_le_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x1000_0000_01b3);
        }
    }
    h
}

#[test]
fn chunk_blocks_match_golden() {
    let Some(data) = data_dir() else {
        eprintln!("золотой тест генерации: нет данных, пропуск");
        return;
    };
    let registries = chunkwake_data::registries::RegistryData::load(&data).unwrap();
    let biome_ids: HashMap<String, u32> = registries
        .synchronized
        .iter()
        .find(|r| r.name == "worldgen/biome")
        .map(|r| {
            r.entries
                .iter()
                .enumerate()
                .map(|(i, n)| (n.clone(), i as u32))
                .collect()
        })
        .unwrap_or_default();
    let blocks = chunkwake_data::world::Blocks::load(&data).unwrap();
    let mut noise = chunkwake_worldgen::Generator::load_with_version(&data, SEED, None).unwrap();
    noise.bind_biomes(biome_ids.clone());
    let state = |n: &str| blocks.default_state(n).ok().map(|s| s as u32);
    let ow = Overworld::new(Arc::new(noise), &state, &biome_ids).unwrap();

    // Порядок важен: декорация зависит от порядка запросов, поэтому квадрат
    // обходится так же, как его генерирует сервер (кольцами от центра).
    let mut got = Vec::new();
    for ring in 0..=RADIUS {
        for cx in -ring..=ring {
            for cz in -ring..=ring {
                if cx.abs().max(cz.abs()) != ring {
                    continue;
                }
                // Прото-чанк без декорации: фичи зависят от порядка и от
                // соседей, а эталон должен ловить рельеф и поверхность.
                let chunk = ow.proto_for_test(cx, cz);
                got.push(((cx, cz), hash(&chunk.states)));
            }
        }
    }

    let want: HashMap<(i32, i32), u64> = GOLDEN.iter().copied().collect();
    assert_eq!(got.len(), want.len(), "число чанков");
    let mut bad = Vec::new();
    for (pos, h) in &got {
        match want.get(pos) {
            Some(w) if w == h => {}
            Some(w) => bad.push(format!("{pos:?}: {h:#018x} != {w:#018x}")),
            None => bad.push(format!("{pos:?}: нет в эталоне")),
        }
    }
    assert!(bad.is_empty(), "генерация изменилась:\n{}", bad.join("\n"));
}
