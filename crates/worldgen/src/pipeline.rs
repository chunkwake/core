//! Конвейер чанка оверворлда (`NoiseBasedChunkGenerator.buildTerrain`):
//! шум и акифер, стадия поверхности, карверы, затем декорация фичами.
//! Один код для сервера, замеров и стенда сверки с ванилью.
//!
//! Фичи чанка пишут в соседей, поэтому, как `ChunkStep` ванили, чанк отдаётся
//! только когда задекорированы все девять чанков вокруг него. Для этого
//! держится область прото-чанков (после карверов): декорация чанка берёт
//! его 3×3, готовый чанк требует 5×5.

use crate::feature::{self, Features, Heightmap};
use crate::surface::{self, Chunk as _, Kind, Rule};
use crate::{BiomeMemo, CarveBlocks, Fill, Generator};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Плотности жил в порядке [`surface::Vein`].
const VEIN_DENSITIES: [&str; 4] = [
    "minecraft:overworld/ore_vein/copper_density",
    "minecraft:overworld/ore_vein/iron_density",
    "minecraft:overworld/ore_vein/richness",
    "minecraft:overworld/ore_vein/gap",
];

/// Блоки чанка по индексу `((y - min_y) * 16 + z) * 16 + x` и его биомы
/// по клеткам 4×4×4 (`None` — климата нет, мир из одного биома).
#[derive(Clone)]
pub struct Blocks {
    pub states: Vec<u32>,
    pub biomes: Option<Vec<u32>>,
    /// Карты `WORLD_SURFACE_WG` и `OCEAN_FLOOR_WG` после рельефа: считаются
    /// при первой декорации, до записей фич.
    pub worldgen_heights: Option<Arc<[[i32; 256]; 2]>>,
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
    features: Option<Features>,
    area: Mutex<Area>,
}

/// Прото-чанки, ещё нужные декорации, и что уже сделано.
#[derive(Default)]
struct Area {
    protos: HashMap<(i32, i32), Blocks>,
    /// Чанки, чья декорация уже прошла (её записи лежат в прото-чанках).
    decorated: HashSet<(i32, i32)>,
    /// Отданные чанки: их прото-чанк больше никто не меняет.
    done: HashSet<(i32, i32)>,
}

/// Прото-чанков сверх этого — повод выбросить те, что только читаются.
const AREA_SOFT_LIMIT: usize = 1024;

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
            features: None,
            area: Mutex::new(Area::default()),
        })
    }

    /// Включает фичи декорации. `states` — по ID состояния имя блока и
    /// свойства; `default` — состояние блока по умолчанию; `biome_ids` —
    /// сетевые ID биомов.
    pub fn with_features(
        mut self,
        data_dir: &Path,
        states: &feature::States,
        default: &dyn Fn(&str) -> Option<u32>,
        biome_ids: &HashMap<String, u32>,
    ) -> Result<Self, String> {
        if !self.noise.has_biomes() {
            return Ok(self);
        }
        let palette = feature::Palette::new(data_dir, states, default);
        let n = &self.noise;
        self.features = Some(Features::load(
            data_dir,
            palette,
            &n.biome_order(),
            biome_ids,
            n.seed(),
            n.min_y,
            n.height,
        )?);
        Ok(self)
    }

    /// Перенесённые и все фичи оверворлда.
    pub fn feature_coverage(&self) -> Option<(usize, usize)> {
        self.features.as_ref().map(Features::coverage)
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

    /// Готовый чанк: прото-чанк и фичи всех соседей. Свет — забота
    /// вызывающего.
    pub fn chunk(&self, cx: i32, cz: i32) -> Blocks {
        let Some(features) = &self.features else {
            return self.proto(cx, cz);
        };
        let around = |r: i32| (-r..=r).flat_map(move |dz| (-r..=r).map(move |dx| (cx + dx, cz + dz)));
        // Прото-чанки строятся без замка: это самая тяжёлая часть.
        let missing: Vec<(i32, i32)> = {
            let area = self.area.lock().unwrap();
            around(2).filter(|p| !area.protos.contains_key(p)).collect()
        };
        let fresh: Vec<_> = missing.into_iter().map(|p| (p, self.proto(p.0, p.1))).collect();
        let mut area = self.area.lock().unwrap();
        for (p, b) in fresh {
            area.protos.entry(p).or_insert(b);
        }
        if area.done.contains(&(cx, cz)) {
            // Чанк уже отдавали: соседи задекорированы, и их записи в него не
            // повторятся. Собираем заново на копии области.
            let mut copy: HashMap<(i32, i32), Blocks> = around(2)
                .filter_map(|p| Some((p, area.protos.get(&p)?.clone())))
                .collect();
            drop(area);
            copy.insert((cx, cz), self.proto(cx, cz));
            for n in around(1) {
                self.decorate(features, &mut copy, n);
            }
            return copy.remove(&(cx, cz)).expect("own proto chunk");
        }
        for n in around(1) {
            if area.decorated.insert(n) {
                self.decorate(features, &mut area.protos, n);
            }
        }
        area.done.insert((cx, cz));
        let out = area.protos.remove(&(cx, cz)).expect("own proto chunk");
        if area.protos.len() > AREA_SOFT_LIMIT {
            // Отданные чанки, построенные заново лишь для чтения соседями.
            let done = std::mem::take(&mut area.done);
            area.protos.retain(|p, _| !done.contains(p));
            area.done = done;
        }
        out
    }

    /// `applyBiomeDecoration` чанка `(cx, cz)` над его областью 3×3.
    fn decorate(&self, features: &Features, protos: &mut HashMap<(i32, i32), Blocks>, (cx, cz): (i32, i32)) {
        let at = |i: usize| (cx + i as i32 % 3 - 1, cz + i as i32 / 3 - 1);
        let mut chunks: [Option<Blocks>; 9] = Default::default();
        for (i, slot) in chunks.iter_mut().enumerate() {
            *slot = protos.remove(&at(i));
            if let Some(c) = slot
                && c.worldgen_heights.is_none()
            {
                let mut maps = [[0; 256]; 2];
                for (map, kind) in maps.iter_mut().zip([Heightmap::WorldSurfaceWg, Heightmap::OceanFloorWg]) {
                    for (column, h) in map.iter_mut().enumerate() {
                        *h = column_height(&c.states, column, self.noise.height, kind, &features.palette)
                            + self.noise.min_y;
                    }
                }
                c.worldgen_heights = Some(Arc::new(maps));
            }
        }
        let biomes: HashSet<u32> = chunks
            .iter()
            .flatten()
            .filter_map(|c| c.biomes.as_ref())
            .flatten()
            .copied()
            .collect();
        let mut region = Region {
            chunks,
            cx,
            cz,
            noise: &self.noise,
            palette: &features.palette,
            air: self.carve.air,
            memo: RefCell::new(Default::default()),
            heights: RefCell::new(Default::default()),
        };
        features.decorate(&mut region, cx, cz, &biomes);
        for (i, slot) in region.chunks.into_iter().enumerate() {
            if let Some(c) = slot {
                protos.insert(at(i), c);
            }
        }
    }

    /// Прото-чанк: шум, поверхность, карверы.
    fn proto(&self, cx: i32, cz: i32) -> Blocks {
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
        Blocks { states, biomes, worldgen_heights: None }
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

/// Две карты высот чанка по столбцам `z * 16 + x`.
type Heights = [[i32; 256]; 4];

/// Регион 3×3 прото-чанков вокруг декорируемого (`WorldGenRegion`).
struct Region<'a> {
    /// По индексу `(dz + 1) * 3 + dx + 1`.
    chunks: [Option<Blocks>; 9],
    cx: i32,
    cz: i32,
    noise: &'a Generator,
    palette: &'a feature::Palette,
    air: u32,
    /// Кэш «дрожания» по чанку региона: он отсчитывается от своего чанка.
    memo: RefCell<[BiomeMemo; 9]>,
    /// Карты высот чанков региона (`WORLD_SURFACE_WG`, `OCEAN_FLOOR_WG`):
    /// первый свободный y над столбцом, считаются при первом запросе и
    /// дальше идут за записями, как `Heightmap.update` у прото-чанка.
    heights: RefCell<[Option<Box<Heights>>; 9]>,
}

impl Region<'_> {
    fn slot(&self, x: i32, z: i32) -> Option<usize> {
        let dx = (x >> 4) - self.cx + 1;
        let dz = (z >> 4) - self.cz + 1;
        ((0..3).contains(&dx) && (0..3).contains(&dz)).then_some((dz * 3 + dx) as usize)
    }

    fn index(&self, x: i32, y: i32, z: i32) -> Option<(usize, usize)> {
        let ry = y - self.noise.min_y;
        if ry < 0 || ry >= self.noise.height {
            return None;
        }
        let slot = self.slot(x, z)?;
        Some((slot, (ry as usize * 16 + (z & 15) as usize) * 16 + (x & 15) as usize))
    }
}

impl feature::Level for Region<'_> {
    fn min_y(&self) -> i32 {
        self.noise.min_y
    }
    fn height(&self) -> i32 {
        self.noise.height
    }
    fn get(&self, x: i32, y: i32, z: i32) -> u32 {
        self.index(x, y, z)
            .and_then(|(s, i)| self.chunks[s].as_ref()?.states.get(i).copied())
            .unwrap_or(self.air)
    }
    fn set(&mut self, x: i32, y: i32, z: i32, state: u32) {
        let Some((s, i)) = self.index(x, y, z) else {
            return;
        };
        let Some(c) = self.chunks[s].as_mut() else {
            return;
        };
        c.states[i] = state;
        let column = ((z & 15) * 16 + (x & 15)) as usize;
        let mut heights = self.heights.borrow_mut();
        let Some(maps) = heights[s].as_mut() else {
            return;
        };
        for (k, kind) in Heightmap::ALL.into_iter().enumerate() {
            let h = &mut maps[k][column];
            if self.palette.hits(kind, state) {
                *h = (*h).max(y + 1);
            } else if *h == y + 1 {
                // Сняли верхний блок: ищем следующий ниже.
                *h = column_height(&c.states, column, y - self.noise.min_y, kind, self.palette)
                    + self.noise.min_y;
            }
        }
    }
    fn can_write(&self, x: i32, z: i32) -> bool {
        self.slot(x, z).is_some_and(|s| self.chunks[s].is_some())
    }
    fn surface(&self, kind: Heightmap, x: i32, z: i32) -> i32 {
        let min_y = self.noise.min_y;
        let Some(s) = self.slot(x, z) else {
            return min_y;
        };
        let Some(c) = self.chunks[s].as_ref() else {
            return min_y;
        };
        let column = ((z & 15) * 16 + (x & 15)) as usize;
        match kind {
            Heightmap::WorldSurfaceWg | Heightmap::OceanFloorWg => {
                let wg = c.worldgen_heights.as_ref().expect("worldgen heightmaps");
                return wg[usize::from(kind == Heightmap::OceanFloorWg)][column];
            }
            _ => {}
        }
        let mut heights = self.heights.borrow_mut();
        let maps = heights[s].get_or_insert_with(|| {
            let mut maps = Box::new([[0; 256]; 4]);
            for (k, kind) in Heightmap::ALL.into_iter().enumerate() {
                for (column, h) in maps[k].iter_mut().enumerate() {
                    *h = column_height(&c.states, column, self.noise.height, kind, self.palette) + min_y;
                }
            }
            maps
        });
        maps[kind.index()][column]
    }
    fn biome(&self, x: i32, y: i32, z: i32) -> u32 {
        let Some(s) = self.slot(x, z) else {
            return 0;
        };
        let own = self.chunks[s]
            .as_ref()
            .and_then(|c| c.biomes.as_deref())
            .unwrap_or(&[]);
        self.noise
            .zoomed_biome(x >> 4, z >> 4, own, x, y, z, &mut self.memo.borrow_mut()[s])
    }
}

/// Первый свободный y (от низа мира) над столбцом, ниже `below`.
fn column_height(states: &[u32], column: usize, below: i32, kind: Heightmap, palette: &feature::Palette) -> i32 {
    (0..below.max(0) as usize)
        .rev()
        .find(|&ry| palette.hits(kind, states[ry * 256 + column]))
        .map_or(0, |ry| ry as i32 + 1)
}
