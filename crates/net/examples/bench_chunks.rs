//! Замер пути чанка по этапам: `bench_chunks <данные> <seed> [радиус]`.
//!
//! Печатает мс на генерацию блоков, биомы, предварительную поверхность,
//! правила поверхности, сборку столбца, свет, кодирование и сжатие.
//! Запуск в release — иначе цифры ничего не говорят о сервере.

use chunkwake_protocol::frame::FrameCodec;
use chunkwake_world::light::{self, LightTable};
use chunkwake_world::{ChunkColumn, bits_for};
use std::collections::HashMap;
use std::time::Instant;

/// Чанк для стадии поверхности: состояния по индексу `((y * 16 + z) * 16 + x)`.
struct Bench<'a> {
    states: Vec<u32>,
    biomes: Vec<u32>,
    height: usize,
    fluid: [u32; 2],
    /// Биом с «дрожанием» границ, как в сервере.
    noise: &'a chunkwake_worldgen::Generator,
    at: (i32, i32),
    memo: std::cell::RefCell<chunkwake_worldgen::BiomeMemo>,
}

impl chunkwake_worldgen::surface::Chunk for Bench<'_> {
    fn get(&self, x: usize, y: usize, z: usize) -> u32 {
        self.states[(y * 16 + z) * 16 + x]
    }
    fn set(&mut self, x: usize, y: usize, z: usize, state: u32) -> u32 {
        let i = (y * 16 + z) * 16 + x;
        std::mem::replace(&mut self.states[i], state)
    }
    fn biome(&self, x: usize, y: usize, z: usize) -> u32 {
        let (cx, cz) = self.at;
        self.noise.zoomed_biome(
            cx,
            cz,
            &self.biomes,
            cx * 16 + x as i32,
            self.noise.min_y + y as i32,
            cz * 16 + z as i32,
            &mut self.memo.borrow_mut(),
        )
    }
    fn surface(&self, x: usize, z: usize) -> usize {
        (0..self.height)
            .rev()
            .find(|&y| self.get(x, y, z) != 0)
            .unwrap_or(0)
    }
    fn kind(&self, state: u32) -> chunkwake_worldgen::surface::Kind {
        use chunkwake_worldgen::surface::Kind;
        if state == 0 {
            Kind::Air
        } else if self.fluid.contains(&state) {
            Kind::Fluid
        } else {
            Kind::Solid
        }
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::path::Path::new(&a[1]);
    let seed: i64 = a[2].parse().unwrap();
    let radius: i32 = a.get(3).map_or(8, |v| v.parse().unwrap());

    let rows = chunkwake_data::world::load_light_rows(data).unwrap();
    let table = LightTable::new(rows.light.iter().copied());
    let blocks = chunkwake_data::world::Blocks::load(data).unwrap();
    let registries = chunkwake_data::registries::RegistryData::load(data).unwrap();
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
    let mut noise = chunkwake_worldgen::Generator::load_with_version(data, seed, None).unwrap();
    noise.bind_biomes(biome_ids.clone());

    let block_bits = bits_for(blocks.state_count());
    let biome_bits = bits_for(biome_ids.len().max(1));
    let stone = blocks.default_state("minecraft:stone").unwrap() as u32;
    let water = blocks.default_state("minecraft:water").unwrap() as u32;
    let lava = blocks.default_state("minecraft:lava").unwrap() as u32;

    let coords: Vec<(i32, i32)> = (-radius..=radius)
        .flat_map(|x| (-radius..=radius).map(move |z| (x, z)))
        .collect();
    let n = coords.len() as f64;
    println!("{} чанков (радиус {radius})", coords.len());

    let t = Instant::now();
    let fills: Vec<_> = coords.iter().map(|&(x, z)| noise.fill(x, z)).collect();
    let fill_ms = t.elapsed().as_secs_f64() * 1000.0;

    // Маска карверов: ход тоннелей из 289 соседних чанков.
    let t = Instant::now();
    let carved = coords
        .iter()
        .filter(|&&(x, z)| noise.carving_mask(x, z).is_some())
        .count();
    let carve_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!("карверы задели {carved} чанков");

    let t = Instant::now();
    let biomes: Vec<_> = coords.iter().map(|&(x, z)| noise.biomes(x, z)).collect();
    let biome_ms = t.elapsed().as_secs_f64() * 1000.0;

    let t = Instant::now();
    let prelim: Vec<Vec<f32>> = coords.iter().map(|&(x, z)| noise.preliminary(x, z)).collect();
    let prelim_ms = t.elapsed().as_secs_f64() * 1000.0;

    // Правила компилируются один раз, как на сервере при старте мира.
    let t = Instant::now();
    let mut names = HashMap::new();
    for rule in noise.surface_files().0.values() {
        collect_states(rule, &blocks, &mut names);
    }
    let noise_list = noise.surface_noises();
    let noise_names: Vec<String> = noise_list.iter().map(|(n, _)| n.clone()).collect();
    let density_names: Vec<String> = [
        "minecraft:overworld/ore_vein/copper_density",
        "minecraft:overworld/ore_vein/iron_density",
        "minecraft:overworld/ore_vein/richness",
        "minecraft:overworld/ore_vein/gap",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    let (rules, conditions) = noise.surface_files();
    let mut compiled = chunkwake_worldgen::surface::Rules::new(
        rules.clone(),
        conditions.clone(),
        names.clone(),
        biome_ids.clone(),
        &noise_names,
        &density_names,
        noise.surface_random(),
        noise.min_y,
        noise.height,
    );
    let rule = compiled.top("minecraft:overworld").ok();
    let bands = compiled.bands(&names);
    let randoms = compiled.into_randoms();
    let setup_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!(
        "правил поверхности: {}, компиляция {setup_ms:.1} ms",
        usize::from(rule.is_some())
    );

    // Стадия поверхности по одному чанку, как в замыкании сервера.
    let t = Instant::now();
    let mut chunks: Vec<Bench> = fills
        .iter()
        .zip(&biomes)
        .zip(&coords)
        .map(|((fill, biomes), &at)| Bench {
            states: fill
                .iter()
                .map(|f| match f {
                    chunkwake_worldgen::Fill::Air => 0,
                    chunkwake_worldgen::Fill::Solid => stone,
                    chunkwake_worldgen::Fill::Water => water,
                    chunkwake_worldgen::Fill::Lava => lava,
                })
                .collect(),
            biomes: biomes.clone().unwrap_or_default(),
            height: noise.height as usize,
            fluid: [water, lava],
            noise: &noise,
            at,
            memo: Default::default(),
        })
        .collect();
    for (i, &(cx, cz)) in coords.iter().enumerate() {
        if let Some(rule) = &rule {
            let stage = chunkwake_worldgen::surface::Stage {
                rule,
                min_y: noise.min_y,
                height: noise.height,
                noises: &noise_list,
                bands: &bands,
                densities: noise.vein_densities(),
                randoms: &randoms,
                preliminary: &prelim[i],
                random: &noise.surface_random(),
            };
            stage.build(&mut chunks[i], cx, cz);
        }
    }
    let surface_ms = t.elapsed().as_secs_f64() * 1000.0;

    let t = Instant::now();
    let mut columns: Vec<ChunkColumn> = chunks
        .drain(..)
        .map(|c| {
            if c.biomes.is_empty() {
                ChunkColumn::from_blocks(&c.states, 0, block_bits, biome_bits, true)
            } else {
                ChunkColumn::from_blocks_with_biomes(&c.states, &c.biomes, block_bits, biome_bits, true)
            }
        })
        .collect();
    let column_ms = t.elapsed().as_secs_f64() * 1000.0;

    let t = Instant::now();
    let mut map: HashMap<(i32, i32), ChunkColumn> =
        coords.iter().copied().zip(columns.drain(..)).collect();
    let mut light_ms = 0.0;
    for &pos in &coords {
        let t = Instant::now();
        light::spread_chunk(&mut map, pos, &table);
        light_ms += t.elapsed().as_secs_f64() * 1000.0;
    }

    let t = Instant::now();
    let bodies: Vec<Vec<u8>> = map.values().map(|c| c.encode_body()).collect();
    let encode_ms = t.elapsed().as_secs_f64() * 1000.0;

    let t = Instant::now();
    let codec = FrameCodec {
        threshold: Some(256),
    };
    let mut out = Vec::new();
    let mut total = 0usize;
    for body in &bodies {
        out.clear();
        codec.encode(body, &mut out);
        total += out.len();
    }
    let zlib_ms = t.elapsed().as_secs_f64() * 1000.0;

    let row = |name: &str, ms: f64| {
        println!("{name:<12} {:>9.1} ms всего {:>7.2} ms/чанк", ms, ms / n);
    };
    println!("---");
    row("fill", fill_ms);
    row("carvers", carve_ms);
    row("biomes", biome_ms);
    row("preliminary", prelim_ms);
    row("surface", surface_ms);
    row("column", column_ms);
    row("light", light_ms);
    row("encode", encode_ms);
    row("zlib", zlib_ms);
    row(
        "итого",
        fill_ms + carve_ms + biome_ms + prelim_ms + surface_ms + column_ms + light_ms + encode_ms + zlib_ms,
    );
    println!(
        "сжатый поток {:.1} МиБ, {:.1} КиБ/чанк",
        total as f64 / (1024.0 * 1024.0),
        total as f64 / n / 1024.0
    );

    // Конвейер сервера целиком: прото-чанки и декорация фичами.
    let state = |name: &str| blocks.default_state(name).ok().map(|s| s as u32);
    let noise = std::sync::Arc::new(noise);
    for features in [false, true] {
        let overworld =
            chunkwake_worldgen::pipeline::Overworld::new(noise.clone(), &state, &biome_ids).unwrap();
        let overworld = if features {
            overworld
                .with_features(data, &blocks.states(), &state, &biome_ids)
                .unwrap()
        } else {
            overworld
        };
        let t = Instant::now();
        for &(x, z) in &coords {
            std::hint::black_box(overworld.chunk(x, z));
        }
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        row(if features { "pipeline+ft" } else { "pipeline" }, ms);
    }
}

/// Имена состояний, которые упоминают правила.
fn collect_states(
    v: &serde_json::Value,
    blocks: &chunkwake_data::world::Blocks,
    out: &mut HashMap<String, u32>,
) {
    if let Some(name) = v.get("result_state").and_then(|s| s.as_str()) {
        if let Ok(state) = blocks.default_state(name) {
            out.insert(name.to_owned(), state as u32);
        }
    }
    for key in ["ore_block", "raw_ore_block", "filler_block"] {
        if let Some(name) = v.get(key).and_then(|s| s.as_str()) {
            if let Ok(state) = blocks.default_state(name) {
                out.insert(name.to_owned(), state as u32);
            }
        }
    }
    match v {
        serde_json::Value::Object(map) => {
            for child in map.values() {
                collect_states(child, blocks, out);
            }
        }
        serde_json::Value::Array(list) => {
            for child in list {
                collect_states(child, blocks, out);
            }
        }
        _ => {}
    }
}
