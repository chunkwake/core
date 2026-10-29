//! Конвейер чанка оверворлда (`NoiseBasedChunkGenerator.buildTerrain`):
//! шум и акифер, стадия поверхности, карверы. Один код для сервера,
//! замеров и стенда сверки с ванилью.

use crate::surface::{self, Chunk as _, Kind, Rule};
use crate::{BiomeMemo, CarveBlocks, Fill, Generator};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

/// Плотности жил в порядке [`surface::Vein`].
const VEIN_DENSITIES: [&str; 4] = [
    "minecraft:overworld/ore_vein/copper_density",
    "minecraft:overworld/ore_vein/iron_density",
    "minecraft:overworld/ore_vein/richness",
    "minecraft:overworld/ore_vein/gap",
];

/// Блоки чанка по индексу `((y - min_y) * 16 + z) * 16 + x` и его биомы
/// по клеткам 4×4×4 (`None` — климата нет, мир из одного биома).
pub struct Blocks {
    pub states: Vec<u32>,
    pub biomes: Option<Vec<u32>>,
}

/// Скомпилированная стадия поверхности.
struct Surface {
    rule: Rule,
    noises: Vec<(String, Arc<crate::noise::Stack>)>,
    bands: Vec<Vec<u32>>,
    randoms: Vec<crate::random::Positional>,
}

pub struct Overworld {
    noise: Arc<Generator>,
    surface: Option<Surface>,
    carve: CarveBlocks,
    stone: u32,
}

impl Overworld {
    /// `state` — состояние блока по имени (`minecraft:stone`); `biome_ids` —
    /// сетевые ID биомов для условий `minecraft:biome`.
    pub fn new(
        noise: Arc<Generator>,
        state: &dyn Fn(&str) -> Option<u32>,
        biome_ids: &HashMap<String, u32>,
    ) -> Result<Self, String> {
        let need = |name: &str| state(name).ok_or_else(|| format!("no block state {name}"));
        let carve = CarveBlocks {
            air: need("minecraft:air")?,
            water: need("minecraft:water")?,
            lava: need("minecraft:lava")?,
            bedrock: need("minecraft:bedrock")?,
            grass: need("minecraft:grass_block")?,
            mycelium: need("minecraft:mycelium")?,
            dirt: need("minecraft:dirt")?,
        };
        let stone = need("minecraft:stone")?;
        let surface = Self::compile_surface(&noise, state, biome_ids)?;
        Ok(Self {
            noise,
            surface,
            carve,
            stone,
        })
    }

    /// Есть ли правила поверхности: без них мир остаётся голым камнем.
    pub fn has_surface(&self) -> bool {
        self.surface.is_some()
    }

    pub fn generator(&self) -> &Generator {
        &self.noise
    }

    fn compile_surface(
        noise: &Generator,
        state: &dyn Fn(&str) -> Option<u32>,
        biome_ids: &HashMap<String, u32>,
    ) -> Result<Option<Surface>, String> {
        let (rules, conditions) = noise.surface_files();
        if rules.is_empty() {
            return Ok(None);
        }
        let mut names = HashMap::new();
        for rule in rules.values() {
            collect_states(rule, state, &mut names);
        }
        let noises = noise.surface_noises();
        let noise_names: Vec<String> = noises.iter().map(|(n, _)| n.clone()).collect();
        let densities: Vec<String> = VEIN_DENSITIES.iter().map(|s| (*s).to_owned()).collect();
        let mut compiled = surface::Rules::new(
            rules.clone(),
            conditions.clone(),
            names.clone(),
            biome_ids.clone(),
            &noise_names,
            &densities,
            noise.surface_random(),
            noise.min_y,
            noise.height,
        );
        let rule = compiled
            .top("minecraft:overworld")
            .map_err(|e| format!("material_rule overworld: {e}"))?;
        let bands = compiled.bands(&names);
        let randoms = compiled.into_randoms();
        Ok(Some(Surface {
            rule,
            noises,
            bands,
            randoms,
        }))
    }

    /// Чанк целиком: шум, поверхность, карверы. Свет — забота вызывающего.
    pub fn chunk(&self, cx: i32, cz: i32) -> Blocks {
        let noise = &*self.noise;
        let mut terrain = noise.terrain(cx, cz);
        let c = self.carve;
        let mut states: Vec<u32> = terrain
            .fills
            .iter()
            .map(|f| match f {
                Fill::Air => c.air,
                Fill::Solid => self.stone,
                Fill::Water => c.water,
                Fill::Lava => c.lava,
            })
            .collect();
        // Климат считается один раз на чанк: биомы нужны и стадии
        // поверхности, и палитре секций.
        let biomes = noise.biomes(cx, cz);
        let own = biomes.as_deref().unwrap_or(&[]);
        let memo = RefCell::new(BiomeMemo::default());
        let preliminary = noise.preliminary(cx, cz);
        let random = noise.surface_random();
        if let Some(s) = &self.surface {
            let mut chunk = self.view(&mut states, cx, cz, own, &memo);
            s.stage(noise, &preliminary, &random).build(&mut chunk, cx, cz);
        }
        // Карверы после поверхности (`generateCarvers`): тоннели и каньоны,
        // залитые акифером того же чанка.
        if let Some(mask) = noise.carving_mask(cx, cz) {
            let mut heights: Option<Vec<usize>> = None;
            noise.carve(&mut terrain, &mask, &c, &mut states, |st, x, y, z, fluid| {
                let s = self.surface.as_ref()?;
                let chunk = self.view(st, cx, cz, own, &memo);
                let heights = heights.get_or_insert_with(|| {
                    (0..256).map(|i| chunk.surface(i % 16, i / 16)).collect()
                });
                let y = (y - noise.min_y) as usize;
                s.stage(noise, &preliminary, &random)
                    .top_material(&chunk, x, y, z, cx, cz, heights, fluid)
            });
        }
        Blocks { states, biomes }
    }

    fn view<'a>(
        &'a self,
        states: &'a mut [u32],
        cx: i32,
        cz: i32,
        biomes: &'a [u32],
        memo: &'a RefCell<BiomeMemo>,
    ) -> View<'a> {
        View {
            states,
            biomes,
            air: self.carve.air,
            water: self.carve.water,
            lava: self.carve.lava,
            height: self.noise.height as usize,
            noise: &self.noise,
            cx,
            cz,
            memo,
        }
    }
}

impl Surface {
    fn stage<'a>(
        &'a self,
        noise: &'a Generator,
        preliminary: &'a [f32],
        random: &'a crate::random::Positional,
    ) -> surface::Stage<'a> {
        surface::Stage {
            rule: &self.rule,
            min_y: noise.min_y,
            height: noise.height,
            noises: &self.noises,
            bands: &self.bands,
            densities: noise.vein_densities(),
            randoms: &self.randoms,
            preliminary,
            random,
        }
    }
}

/// Чанк глазами стадии поверхности: состояния по `((y * 16 + z) * 16 + x)`.
struct View<'a> {
    states: &'a mut [u32],
    /// Биомы по клеткам 4×4×4, как в секции.
    biomes: &'a [u32],
    air: u32,
    water: u32,
    lava: u32,
    height: usize,
    /// Для биомов соседних чанков: «дрожание» границы выводит за чанк.
    noise: &'a Generator,
    cx: i32,
    cz: i32,
    memo: &'a RefCell<BiomeMemo>,
}

impl surface::Chunk for View<'_> {
    fn get(&self, x: usize, y: usize, z: usize) -> u32 {
        self.states
            .get((y * 16 + z) * 16 + x)
            .copied()
            .unwrap_or(self.air)
    }
    fn set(&mut self, x: usize, y: usize, z: usize, state: u32) -> u32 {
        std::mem::replace(&mut self.states[(y * 16 + z) * 16 + x], state)
    }
    fn biome(&self, x: usize, y: usize, z: usize) -> u32 {
        // `BiomeManager.getBiome`, как у стадии поверхности ванили.
        self.noise.zoomed_biome(
            self.cx,
            self.cz,
            self.biomes,
            self.cx * 16 + x as i32,
            self.noise.min_y + y as i32,
            self.cz * 16 + z as i32,
            &mut self.memo.borrow_mut(),
        )
    }
    fn surface(&self, x: usize, z: usize) -> usize {
        (0..self.height)
            .rev()
            .find(|&y| self.get(x, y, z) != self.air)
            .unwrap_or(0)
    }
    fn kind(&self, state: u32) -> Kind {
        // Сравнение по двум полям дешевле `contains` по срезу на каждый блок.
        if state == self.air {
            Kind::Air
        } else if state == self.water || state == self.lava {
            Kind::Fluid
        } else {
            Kind::Solid
        }
    }
}

/// Имена состояний из правил поверхности → ID.
fn collect_states(v: &Value, state: &dyn Fn(&str) -> Option<u32>, out: &mut HashMap<String, u32>) {
    if let Some(name) = v.get("result_state").and_then(|s| s.as_str())
        && let Some(id) = state(name)
    {
        out.insert(name.to_owned(), id);
    }
    for key in ["ore_block", "raw_ore_block", "filler_block"] {
        if let Some(name) = v.get(key).and_then(|s| s.as_str())
            && let Some(id) = state(name)
        {
            out.insert(name.to_owned(), id);
        }
    }
    match v {
        Value::Object(map) => map.values().for_each(|c| collect_states(c, state, out)),
        Value::Array(list) => list.iter().for_each(|c| collect_states(c, state, out)),
        _ => {}
    }
}
