//! Что стадия поверхности делает с чанком: `surface_probe <данные> <seed> [cx cz]`.
//!
//! Печатает срез столбца 8,8: сверху вниз состояния и первые 24 блока.
//! Нужен, чтобы глазами увидеть траву, землю и песок над камнем.

use chunkwake_worldgen::Generator;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::path::Path::new(&a[1]);
    let seed: i64 = a[2].parse().unwrap();
    let (cx, cz) = (
        a.get(3).map_or(0, |v| v.parse().unwrap()),
        a.get(4).map_or(0, |v| v.parse().unwrap()),
    );
    let g = Generator::load(data, seed).unwrap();
    println!(
        "biomes={} rules={} noises={}",
        g.has_biomes(),
        g.has_surface(),
        g.surface_noises().len()
    );
    // Стадия поверхности: имена состояний и биомов в ID, как это делает сервер.
    let blocks = chunkwake_data::world::Blocks::load(data).unwrap();
    let mut names = std::collections::HashMap::new();
    let (rules, conditions) = g.surface_files();
    for rule in rules.values() {
        collect(rule, &blocks, &mut names);
    }
    let noises = g.surface_noises();
    let noise_names: Vec<String> = noises.iter().map(|(n, _)| n.clone()).collect();
    // Все биомы реестра: правила ссылаются и на те, что не попали в климат.
    let registries = chunkwake_data::registries::RegistryData::load(data).unwrap();
    let all_biomes: std::collections::HashMap<String, u32> = registries
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
    let density_names: Vec<String> = [
        "minecraft:overworld/ore_vein/copper_density",
        "minecraft:overworld/ore_vein/iron_density",
        "minecraft:overworld/ore_vein/richness",
        "minecraft:overworld/ore_vein/gap",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    let mut compiled = chunkwake_worldgen::surface::Rules::new(
        rules.clone(),
        conditions.clone(),
        names.clone(),
        all_biomes.clone(),
        &noise_names,
        &density_names,
        g.surface_random(),
        g.min_y,
        g.height,
    );
    let rule = match compiled.top("minecraft:overworld") {
        Ok(r) => r,
        Err(e) => {
            eprintln!("rule: {e}");
            return;
        }
    };
    let densities = g.vein_densities().to_vec();
    let bands = compiled.bands(&names);
    let randoms = compiled.into_randoms();
    let noise_list = noises.clone();
    let air = blocks.default_state("minecraft:air").unwrap() as u32;
    let fluids: Vec<u32> = ["minecraft:water", "minecraft:lava"]
        .iter()
        .filter_map(|n| blocks.default_state(n).ok().map(|s| s as u32))
        .collect();
    let biomes = g.biomes(cx, cz).unwrap_or_default();
    let preliminary = g.preliminary(cx, cz);
    let mut states: Vec<u32> = g
        .fill(cx, cz)
        .iter()
        .map(|f| match f {
            chunkwake_worldgen::Fill::Air => air,
            chunkwake_worldgen::Fill::Solid => {
                blocks.default_state("minecraft:stone").unwrap() as u32
            }
            chunkwake_worldgen::Fill::Water => {
                blocks.default_state("minecraft:water").unwrap() as u32
            }
            chunkwake_worldgen::Fill::Lava => {
                blocks.default_state("minecraft:lava").unwrap() as u32
            }
        })
        .collect();
    let mut chunk = Probe {
        states: &mut states,
        biomes: &biomes,
        air,
        fluids: &fluids,
        height: g.height as usize,
    };
    let stage = chunkwake_worldgen::surface::Stage {
        rule: &rule,
        min_y: g.min_y,
        height: g.height,
        noises: &noise_list,
        bands: &bands,
        densities: &densities,
        randoms: &randoms,
        preliminary: &preliminary,
        random: &g.surface_random(),
        clay_offset: noise_list
            .iter()
            .position(|(n, _)| n == "minecraft:clay_bands_offset"),
    };
    stage.build(&mut chunk, cx, cz);
    drop(chunk);

    let mut top = 0;
    for y in (0..g.height).rev() {
        let i = ((y * 16 + 8) * 16 + 8) as usize;
        if states[i] != air {
            top = y;
            break;
        }
    }
    let name = |s: u32| {
        blocks
            .state(s)
            .map_or("?", |(n, _)| n)
            .trim_start_matches("minecraft:")
            .to_owned()
    };
    print!("столбец 8,8 сверху (y={}):", g.min_y + top);
    for y in (0..=top).rev().take(14) {
        let i = ((y * 16 + 8) * 16 + 8) as usize;
        print!(" {}", name(states[i]));
    }
    println!();
}

/// Имена всех состояний, которые упоминают правила.
fn collect(
    v: &serde_json::Value,
    blocks: &chunkwake_data::world::Blocks,
    out: &mut std::collections::HashMap<String, u32>,
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
                collect(child, blocks, out);
            }
        }
        serde_json::Value::Array(list) => {
            for child in list {
                collect(child, blocks, out);
            }
        }
        _ => {}
    }
}

struct Probe<'a> {
    states: &'a mut [u32],
    biomes: &'a [u32],
    air: u32,
    fluids: &'a [u32],
    height: usize,
}

impl chunkwake_worldgen::surface::Chunk for Probe<'_> {
    fn get(&self, x: usize, y: usize, z: usize) -> u32 {
        self.states[(y * 16 + z) * 16 + x]
    }
    fn set(&mut self, x: usize, y: usize, z: usize, state: u32) -> u32 {
        let i = (y * 16 + z) * 16 + x;
        std::mem::replace(&mut self.states[i], state)
    }
    fn biome(&self, x: usize, y: usize, z: usize) -> u32 {
        self.biomes
            .get((y / 4) * 16 + (z / 4) * 4 + x / 4)
            .copied()
            .unwrap_or(0)
    }
    fn surface(&self, x: usize, z: usize) -> usize {
        (0..self.height)
            .rev()
            .find(|&y| self.get(x, y, z) != self.air)
            .unwrap_or(0)
    }
    fn kind(&self, state: u32) -> chunkwake_worldgen::surface::Kind {
        use chunkwake_worldgen::surface::Kind;
        if state == self.air {
            Kind::Air
        } else if self.fluids.contains(&state) {
            Kind::Fluid
        } else {
            Kind::Solid
        }
    }
}
