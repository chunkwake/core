//! Сколько льда в чанках: наш мир против ванильного региона.
//!
//! `ice_count <данные> <seed> <region.mca-каталог> <cx> <cz>` — печатает
//! число блоков `packed_ice`, `blue_ice`, `snow_block` у нас и в ванили.

use chunkwake_world::ChunkColumn;
use chunkwake_world::anvil::{Format, Names, RegionStore};
use chunkwake_worldgen::Generator;
use std::collections::HashMap;

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

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::path::PathBuf::from(&a[1]);
    let seed: i64 = a[2].parse().unwrap();
    let regions = std::path::PathBuf::from(&a[3]);
    let cx: i32 = a[4].parse().unwrap();
    let cz: i32 = a[5].parse().unwrap();
    let radius: i32 = a.get(6).map_or(3, |v| v.parse().unwrap());

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

    let names = N {
        blocks: &blocks,
        biomes: &biomes,
    };
    let store = RegionStore::new(&regions);
    let mut chunks: HashMap<(i32, i32), chunkwake_world::nbt::Compound> = HashMap::new();
    let mut seen = std::collections::HashSet::new();
    for x in cx - radius..cx + radius {
        for z in cz - radius..cz + radius {
            let key = (x >> 5, z >> 5);
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

    let mut ours: HashMap<String, usize> = HashMap::new();
    let mut theirs: HashMap<String, usize> = HashMap::new();
    let mut counted = 0;
    for x in cx - radius..cx + radius {
        for z in cz - radius..cz + radius {
            let Some(col) = world.chunk(x, z) else {
                continue;
            };
            counted += 1;
            for &s in &col.states {
                if let Some((n, _)) = blocks.state(s)
                    && (n.contains("ice") || n.contains("snow"))
                {
                    *ours.entry(n.to_owned()).or_insert(0) += 1;
                }
            }
            if let Some(nbt) = chunks.get(&(x, z))
                && let Ok((want, _)) =
                    chunkwake_world::anvil::chunk_from_nbt(nbt, &template, format, &names)
            {
                for y in 0..384usize {
                    for zz in 0..16 {
                        for xx in 0..16 {
                            let id = want.block(xx, y, zz);
                            if let Some((n, _)) = blocks.state(id)
                                && (n.contains("ice") || n.contains("snow"))
                            {
                                *theirs.entry(n.to_owned()).or_insert(0) += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    println!("чанков {counted}");
    let mut keys: Vec<_> = ours.keys().chain(theirs.keys()).cloned().collect();
    keys.sort();
    keys.dedup();
    for k in keys {
        println!(
            "  {:<28} наше {:>6}   ваниль {:>6}",
            k.trim_start_matches("minecraft:"),
            ours.get(&k).copied().unwrap_or(0),
            theirs.get(&k).copied().unwrap_or(0)
        );
    }
}
