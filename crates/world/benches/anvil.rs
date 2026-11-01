//! Сохранение региона: NBT + zlib на чанк и запись в `.mca`.
//!
//! Автосейв идёт в блокирующем пуле, но держит диск и ядра: его цена видна
//! как рывок тика, если сохранение не успевает. Здесь меряются обе половины —
//! кодирование одного чанка и сохранение региона целиком.

use chunkwake_world::ChunkColumn;
use chunkwake_world::anvil::{self, Format, Names, RegionStore};
use criterion::{Criterion, criterion_group, criterion_main};
use std::collections::BTreeMap;
use std::hint::black_box;

/// Имена состояний: 0 — воздух, 1 — камень, 2/3 — трава со свойством.
struct Test;

const STONE: u32 = 1;
const GRASS: u32 = 2;
const DIRT: u32 = 3;

impl Names for Test {
    fn block_name(&self, id: u32) -> Option<(&str, &[(String, String)])> {
        Some(match id {
            0 => ("minecraft:air", &[][..]),
            STONE => ("minecraft:stone", &[][..]),
            GRASS => ("minecraft:grass_block", &[][..]),
            DIRT => ("minecraft:dirt", &[][..]),
            _ => return None,
        })
    }

    fn block_id(&self, name: &str, props: &[(String, String)]) -> Option<u32> {
        props.is_empty().then_some(0)?;
        Some(match name {
            "minecraft:air" => 0,
            "minecraft:stone" => STONE,
            "minecraft:grass_block" => GRASS,
            "minecraft:dirt" => DIRT,
            _ => return None,
        })
    }

    fn block_default(&self, name: &str) -> Option<u32> {
        self.block_id(name, &[])
    }

    fn biome_name(&self, id: u32) -> Option<&str> {
        (id == 0).then_some("minecraft:plains")
    }

    fn biome_id(&self, name: &str) -> Option<u32> {
        (name == "minecraft:plains").then_some(0)
    }
}

const FORMAT: Format = Format {
    data_version: 5023,
    min_section: -4,
};

/// Столбец с рельефом: секции камня, полоса земли и травы сверху, воздух выше.
/// Одинаковые секции сжимаются в одно состояние, поэтому столбец дешёвый,
/// а `chunk_to_nbt` меряет именно кодирование настоящей формы.
fn column() -> ChunkColumn {
    let mut c = ChunkColumn::flat(&[(STONE, 64), (DIRT, 1), (GRASS, 1)], 24, 0, 2, 0, true);
    // Немного неровностей: иначе все секции однородные и палитра не работает.
    for x in 0..16 {
        for z in 0..16 {
            if (x + z) % 5 == 0 {
                c.set_block(x, 60, z, STONE);
            }
        }
    }
    c
}

fn save(c: &mut Criterion) {
    let names = Test;
    let column = column();
    let mut group = c.benchmark_group("anvil");
    group.bench_function("chunk_to_nbt", |b| {
        b.iter(|| black_box(anvil::chunk_to_nbt(&column, (3, -2), FORMAT, &names).unwrap()));
    });
    let chunks: Vec<_> = (0..64)
        .map(|i| {
            let pos = (i % 8, i / 8);
            (
                pos,
                anvil::chunk_to_nbt(&column, pos, FORMAT, &names).unwrap(),
            )
        })
        .collect();
    let chunk_nbt: Vec<_> = chunks.iter().map(|(_, nbt)| nbt.clone()).collect();
    // Чистое сжатие без диска: видно, сколько из сохранения — zlib.
    group.bench_function("compress_64", |b| {
        b.iter(|| black_box(anvil::compress_for_bench(&chunk_nbt)));
    });
    group.bench_function("compress_64_sequential", |b| {
        b.iter(|| black_box(anvil::compress_for_bench_sequential(&chunk_nbt)));
    });
    let dir = std::env::temp_dir().join(format!("chunkwake-anvil-bench-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = RegionStore::new(&dir);
    // Первая запись создаёт файл; меряется повторная, как у автосейва.
    store.save(&chunks).expect("первый сейв");
    group.bench_function("save_region_64", |b| {
        b.iter(|| store.save(black_box(&chunks)).expect("сейв"));
    });
    let _ = std::fs::remove_dir_all(&dir);
    group.finish();

    // Загрузка региона: чтение 64 чанков, zlib и разбор NBT.
    let dir = std::env::temp_dir().join(format!("chunkwake-anvil-load-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = RegionStore::new(&dir);
    store.save(&chunks).expect("сейв");
    let template = column.clone();
    let mut load = c.benchmark_group("anvil_load");
    load.bench_function("decode_64_chunks", |b| {
        b.iter(|| {
            let stored = store.load_region(0, 0).expect("регион");
            let mut out: BTreeMap<_, _> = BTreeMap::new();
            for (pos, nbt) in stored {
                let (c, _) = anvil::chunk_from_nbt(&nbt, &template, FORMAT, &names).expect("чанк");
                out.insert(pos, c);
            }
            black_box(out.len());
        });
    });
    load.finish();
    let _ = std::fs::remove_dir_all(&dir);
}

criterion_group!(benches, save);
criterion_main!(benches);
