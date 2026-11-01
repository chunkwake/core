//! Один айсберг: наш столбец против ванильного.
//!
//! `ice_one <данные> <seed> <region-каталог> <x0> <z0> <размер> <y>`

use chunkwake_world::ChunkColumn;
use chunkwake_world::anvil::{self, Format, Names, RegionStore};
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
    let x0: i32 = a[4].parse().unwrap();
    let z0: i32 = a[5].parse().unwrap();
    let n: i32 = a[6].parse().unwrap();
    let y: i32 = a[7].parse().unwrap();

    let regs = chunkwake_data::registries::RegistryData::load(&data).unwrap();
    let biomes: HashMap<String, u32> = regs
        .synchronized
        .iter()
        .find(|r| r.name == "worldgen/biome")
        .map(|r| {
            r.entries
                .iter()
                .enumerate()
                .map(|(i, name)| (name.clone(), i as u32))
                .collect()
        })
        .unwrap_or_default();
    let blocks = chunkwake_data::world::Blocks::load(&data).unwrap();
    let state = |name: &str| blocks.default_state(name).ok().map(|s| s as u32);
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
    for cx in (x0 >> 4) - 2..((x0 + n) >> 4) + 2 {
        for cz in (z0 >> 4) - 2..((z0 + n) >> 4) + 2 {
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

    let mark = |id: u32| {
        let (name, _) = blocks.state(id).unwrap_or(("?", &[]));
        match name {
            "minecraft:packed_ice" => 'P',
            "minecraft:blue_ice" => 'B',
            "minecraft:snow_block" => 'S',
            "minecraft:water" => '~',
            "minecraft:ice" => 'i',
            "minecraft:air" => '.',
            _ => '#',
        }
    };

    println!("срез y={y}, x {x0}..{} z {z0}..{}", x0 + n, z0 + n);
    // `Overworld::chunk` отдаёт чанк один раз: второй запрос — `None`.
    let mut cache: HashMap<(i32, i32), Option<chunkwake_worldgen::pipeline::Blocks>> =
        HashMap::new();
    for z in z0..z0 + n {
        let mut ours = String::new();
        let mut theirs = String::new();
        for x in x0..x0 + n {
            let (cx, cz) = (x >> 4, z >> 4);
            let (lx, lz) = (x & 15, z & 15);
            let ry = (y + 64) as usize;
            let col = cache.entry((cx, cz)).or_insert_with(|| world.chunk(cx, cz));
            ours.push(col.as_ref().map_or('?', |c| {
                mark(c.states[(ry * 16 + lz as usize) * 16 + lx as usize])
            }));
            let ch = chunks
                .get(&(cx, cz))
                .and_then(|nbt| anvil::chunk_from_nbt(nbt, &template, format, &names).ok())
                .map_or('?', |(want, _)| {
                    mark(want.block(lx as usize, ry, lz as usize))
                });
            theirs.push(ch);
        }
        println!("z {z:>6}  наше {ours}   ваниль {theirs}");
    }
}
