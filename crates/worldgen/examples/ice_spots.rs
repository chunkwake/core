//! Айсберги: пятна льда на уровне моря у нас и в ванильном регионе.
//!
//! `ice_spots <данные> <seed> <region-каталог> <cx> <cz> <radius>`
//!
//! Печатает число пятен, их площади и суммарные клетки льда с обеих сторон:
//! так видно, расходится размещение (число пятен) или размер (площадь).

use chunkwake_world::ChunkColumn;
use chunkwake_world::anvil::{Format, Names, RegionStore};
use chunkwake_worldgen::Generator;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

struct N<'a> {
    blocks: &'a chunkwake_data::world::Blocks,
    biomes: &'a HashMap<String, u32>,
}

impl Names for N<'_> {
    fn block_name(&self, id: u32) -> Option<(&str, &[(String, String)])> {
        self.blocks.state(id)
    }
    fn block_id(&self, name: &str, props: &[(String, String)]) -> Option<u32> {
        self.blocks.state_id(name, props)
    }
    fn block_default(&self, name: &str) -> Option<u32> {
        self.blocks.default_state(name).ok().map(|s| s as u32)
    }
    fn biome_name(&self, id: u32) -> Option<&str> {
        self.biomes
            .iter()
            .find(|(_, v)| **v == id)
            .map(|(k, _)| k.as_str())
    }
    fn biome_id(&self, name: &str) -> Option<u32> {
        self.biomes.get(name).copied()
    }
}

/// Связные пятна в множестве клеток (4-соседство по XZ).
fn spots(cells: &HashSet<(i32, i32)>) -> Vec<usize> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for &start in cells {
        if !seen.insert(start) {
            continue;
        }
        let mut q = VecDeque::from([start]);
        let mut n = 0;
        while let Some(c) = q.pop_front() {
            n += 1;
            for d in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let next = (c.0 + d.0, c.1 + d.1);
                if cells.contains(&next) && seen.insert(next) {
                    q.push_back(next);
                }
            }
        }
        out.push(n);
    }
    out.sort_unstable_by(|a, b| b.cmp(a));
    out
}

fn report(label: &str, cells: &HashSet<(i32, i32)>) {
    let list = spots(cells);
    let total: usize = list.iter().sum();
    println!(
        "{label}: клеток {total} в {} пятнах, крупнейшие {:?}",
        list.len(),
        &list[..list.len().min(10)]
    );
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::path::PathBuf::from(&a[1]);
    let seed: i64 = a[2].parse().unwrap();
    let regions = std::path::PathBuf::from(&a[3]);
    let cx0: i32 = a[4].parse().unwrap();
    let cz0: i32 = a[5].parse().unwrap();
    let rad: i32 = a[6].parse().unwrap();
    let level: i32 = a.get(7).map_or(63, |v| v.parse().unwrap());

    let regs = chunkwake_data::registries::RegistryData::load(&data).unwrap();
    let biomes: HashMap<String, u32> = regs
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
    let state = |n: &str| blocks.default_state(n).ok().map(|s| s as u32);
    let mut noise = Generator::load_with_version(&data, seed, None).unwrap();
    noise.bind_biomes(biomes.clone());
    let world =
        chunkwake_worldgen::pipeline::Overworld::new(std::sync::Arc::new(noise), &state, &biomes)
            .unwrap()
            .with_features(&data, &blocks.states(), &state, &biomes)
            .unwrap();

    let ice: Vec<u32> = ["minecraft:packed_ice", "minecraft:blue_ice"]
        .iter()
        .filter_map(|n| state(n))
        .collect();

    // Наши пятна: чанк за чанком, только клетки на уровне моря.
    //
    // Считаем с запасом в один чанк: ванила декорирует и кольцо вокруг
    // `forceload`-квадрата, поэтому айсберг с центром за границей всё равно
    // кладёт лёд внутрь. Без запаса такие пятна выглядят «потерянными».
    let mut ours: HashSet<(i32, i32)> = HashSet::new();
    let mut our_total = 0usize;
    for cx in cx0 - rad - 2..cx0 + rad + 2 {
        for cz in cz0 - rad - 2..cz0 + rad + 2 {
            let Some(col) = world.chunk(cx, cz) else {
                continue;
            };
            let inside =
                (cx0 - rad..cx0 + rad).contains(&cx) && (cz0 - rad..cz0 + rad).contains(&cz);
            let ry = (level - (-64)) as usize;
            for z in 0..16usize {
                for x in 0..16usize {
                    let s = col.states[(ry * 16 + z) * 16 + x];
                    if ice.contains(&s) && inside {
                        ours.insert((cx * 16 + x as i32, cz * 16 + z as i32));
                    }
                }
            }
            if inside {
                our_total += col.states.iter().filter(|s| ice.contains(s)).count();
            }
        }
    }
    report("наше", &ours);
    println!("  всего клеток льда у нас: {our_total}");

    // Ванильные: читаем регион и собираем то же самое.
    let names = N {
        blocks: &blocks,
        biomes: &biomes,
    };
    let store = RegionStore::new(&regions);
    let mut chunks: HashMap<(i32, i32), chunkwake_world::nbt::Compound> = HashMap::new();
    let mut seen = HashSet::new();
    for cx in cx0 - rad - 2..cx0 + rad + 2 {
        for cz in cz0 - rad - 2..cz0 + rad + 2 {
            let key = (cx >> 5, cz >> 5);
            if seen.insert(key) {
                for (pos, root) in store.load_region(key.0, key.1).unwrap() {
                    chunks.insert(pos, root);
                }
            }
        }
    }
    let format = Format {
        data_version: 5023,
        min_section: -4,
    };
    let empty = vec![0u32; 384 * 256];
    let template = ChunkColumn::from_blocks(&empty, 0, 15, 7, true);
    let mut theirs: HashSet<(i32, i32)> = HashSet::new();
    let mut their_total = 0usize;
    for cx in cx0 - rad..cx0 + rad {
        for cz in cz0 - rad..cz0 + rad {
            let Some(nbt) = chunks.get(&(cx, cz)) else {
                continue;
            };
            let Ok((want, _)) =
                chunkwake_world::anvil::chunk_from_nbt(nbt, &template, format, &names)
            else {
                continue;
            };
            let inside =
                (cx0 - rad..cx0 + rad).contains(&cx) && (cz0 - rad..cz0 + rad).contains(&cz);
            let ry = (level - (-64)) as usize;
            for z in 0..16usize {
                for x in 0..16usize {
                    if ice.contains(&want.block(x, ry, z)) && inside {
                        theirs.insert((cx * 16 + x as i32, cz * 16 + z as i32));
                    }
                }
            }
            if inside {
                for y in 0..384usize {
                    for z in 0..16usize {
                        for x in 0..16usize {
                            if ice.contains(&want.block(x, y, z)) {
                                their_total += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    report("ваниль", &theirs);
    println!("  всего клеток льда в ванили: {their_total}");
    let _ = Path::new("");
}
