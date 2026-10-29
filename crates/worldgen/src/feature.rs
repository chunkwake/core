//! Фичи декорации (`ChunkGenerator.applyBiomeDecoration`): размещённые фичи
//! `worldgen/placed_feature` с модификаторами размещения, порядок шагов
//! `FeatureSorter` и сиды `WorldgenRandom` бит в бит. Перенесены руды
//! (`ore`, `scattered_ore`); фичи других типов пока пропускаются целиком —
//! каждая фича пересевает генератор, так что пропуск не сдвигает остальные.

use crate::carver::{HeightP, IntP};
use crate::random::{Bits, Worldgen};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

/// По ID состояния блока его имя и свойства, как в реестре сервера.
pub type States = [Option<(String, Vec<(String, String)>)>];

/// Мир глазами фичи (`WorldGenLevel` региона 3×3 чанка). Координаты мировые.
pub trait Level {
    fn min_y(&self) -> i32;
    fn height(&self) -> i32;
    /// Блок; вне региона и высоты мира — воздух.
    fn get(&self, x: i32, y: i32, z: i32) -> u32;
    fn set(&mut self, x: i32, y: i32, z: i32, state: u32);
    /// `ensureCanWrite`: чанк в радиусе записи от центра.
    fn can_write(&self, x: i32, z: i32) -> bool;
    /// `getHeight(type, x, z)`: первый свободный y над столбцом.
    fn surface(&self, kind: Heightmap, x: i32, z: i32) -> i32;
    /// `getBiome(pos)` с «дрожанием» границ.
    fn biome(&self, x: i32, y: i32, z: i32) -> u32;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heightmap {
    /// `WORLD_SURFACE_WG`: всё, кроме воздуха.
    WorldSurface,
    /// `OCEAN_FLOOR_WG`: блоки, мешающие движению.
    OceanFloor,
}

impl Heightmap {
    fn parse(name: &str) -> Result<Self, String> {
        match name {
            "WORLD_SURFACE_WG" | "WORLD_SURFACE" => Ok(Self::WorldSurface),
            "OCEAN_FLOOR_WG" | "OCEAN_FLOOR" | "MOTION_BLOCKING" | "MOTION_BLOCKING_NO_LEAVES" => {
                Ok(Self::OceanFloor)
            }
            other => Err(format!("heightmap {other} is not supported")),
        }
    }
}

/// Состояния блоков сервера: имя блока по состоянию, состояния по свойствам
/// и теги блоков из `data/minecraft/tags/block`.
pub struct Palette {
    /// Состояние → номер блока.
    block_of: Vec<u32>,
    /// Состояние → свойства.
    props: Vec<Vec<(String, String)>>,
    /// Имя блока → номер и его состояния по порядку.
    blocks: HashMap<String, (u32, Vec<u32>)>,
    defaults: HashMap<String, u32>,
    air: Vec<bool>,
    fluid: Vec<bool>,
    tags: HashMap<String, Arc<Vec<bool>>>,
    tag_dir: std::path::PathBuf,
}

impl Palette {
    /// `states` — по ID состояния имя блока и свойства (как в реестре сервера);
    /// `default` — состояние блока по умолчанию.
    pub fn new(
        data_dir: &Path,
        states: &States,
        default: &dyn Fn(&str) -> Option<u32>,
    ) -> Self {
        let mut blocks: HashMap<String, (u32, Vec<u32>)> = HashMap::new();
        let mut block_of = Vec::with_capacity(states.len());
        let mut props = Vec::with_capacity(states.len());
        for (id, s) in states.iter().enumerate() {
            match s {
                Some((name, p)) => {
                    let next = blocks.len() as u32;
                    let e = blocks.entry(name.clone()).or_insert((next, Vec::new()));
                    e.1.push(id as u32);
                    block_of.push(e.0);
                    props.push(p.clone());
                }
                None => {
                    block_of.push(u32::MAX);
                    props.push(Vec::new());
                }
            }
        }
        let defaults = blocks
            .keys()
            .filter_map(|n| default(n).map(|s| (n.clone(), s)))
            .collect();
        let mut p = Self {
            air: vec![false; states.len()],
            fluid: vec![false; states.len()],
            block_of,
            props,
            blocks,
            defaults,
            tags: HashMap::new(),
            tag_dir: data_dir.join("data/minecraft/tags/block"),
        };
        for name in ["minecraft:air", "minecraft:cave_air", "minecraft:void_air"] {
            for s in p.states_of(name) {
                p.air[s as usize] = true;
            }
        }
        for name in ["minecraft:water", "minecraft:lava"] {
            for s in p.states_of(name) {
                p.fluid[s as usize] = true;
            }
        }
        p
    }

    fn states_of(&self, block: &str) -> Vec<u32> {
        self.blocks.get(block).map(|b| b.1.clone()).unwrap_or_default()
    }

    pub fn is_air(&self, state: u32) -> bool {
        self.air.get(state as usize).copied().unwrap_or(false)
    }

    /// Мешает ли движению (`blocksMotion`): пока всё твёрдое, кроме жидкостей.
    pub fn blocks_motion(&self, state: u32) -> bool {
        !self.is_air(state) && !self.fluid.get(state as usize).copied().unwrap_or(false)
    }

    /// Учитывает ли карта высот `kind` это состояние.
    pub fn hits(&self, kind: Heightmap, state: u32) -> bool {
        match kind {
            Heightmap::WorldSurface => !self.is_air(state),
            Heightmap::OceanFloor => self.blocks_motion(state),
        }
    }

    fn block(&self, name: &str) -> Result<u32, String> {
        self.blocks
            .get(name)
            .map(|b| b.0)
            .ok_or_else(|| format!("no block {name}"))
    }

    /// `BlockState` из JSON: строка-имя или `{Name, Properties}` поверх
    /// состояния по умолчанию.
    pub fn state(&self, v: &Value) -> Result<u32, String> {
        let (name, overrides) = match v {
            Value::String(s) => (s.as_str(), None),
            _ => (
                v["Name"].as_str().ok_or("block state: no Name")?,
                v["Properties"].as_object(),
            ),
        };
        let base = *self
            .defaults
            .get(name)
            .ok_or_else(|| format!("no block {name}"))?;
        let Some(overrides) = overrides.filter(|o| !o.is_empty()) else {
            return Ok(base);
        };
        let want: Vec<(String, String)> = self.props[base as usize]
            .iter()
            .map(|(k, v)| {
                let v = overrides
                    .get(k)
                    .and_then(|x| x.as_str())
                    .map_or_else(|| v.clone(), str::to_owned);
                (k.clone(), v)
            })
            .collect();
        self.blocks[name]
            .1
            .iter()
            .copied()
            .find(|&s| self.props[s as usize] == want)
            .ok_or_else(|| format!("no state {v}"))
    }

    /// Тег блоков как маска по состояниям; вложенные теги разворачиваются.
    pub fn tag(&mut self, name: &str) -> Result<Arc<Vec<bool>>, String> {
        if let Some(t) = self.tags.get(name) {
            return Ok(t.clone());
        }
        let mut blocks = HashSet::new();
        self.collect_tag(name, &mut blocks, &mut HashSet::new())?;
        let mask: Vec<bool> = self
            .block_of
            .iter()
            .map(|b| blocks.contains(b))
            .collect();
        let mask = Arc::new(mask);
        self.tags.insert(name.to_owned(), mask.clone());
        Ok(mask)
    }

    fn collect_tag(
        &self,
        name: &str,
        out: &mut HashSet<u32>,
        seen: &mut HashSet<String>,
    ) -> Result<(), String> {
        if !seen.insert(name.to_owned()) {
            return Ok(());
        }
        let path = name.strip_prefix("minecraft:").unwrap_or(name);
        let file = self.tag_dir.join(format!("{path}.json"));
        let text =
            std::fs::read_to_string(&file).map_err(|e| format!("tag {name}: {e}"))?;
        let v: Value = serde_json::from_str(&text).map_err(|e| format!("tag {name}: {e}"))?;
        for e in v["values"].as_array().into_iter().flatten() {
            let (id, required) = match e {
                Value::String(s) => (s.as_str(), true),
                _ => (
                    e["id"].as_str().unwrap_or(""),
                    e["required"].as_bool().unwrap_or(true),
                ),
            };
            let res = if let Some(t) = id.strip_prefix('#') {
                self.collect_tag(t, out, seen)
            } else {
                self.block(id).map(|b| {
                    out.insert(b);
                })
            };
            if let Err(e) = res
                && required
            {
                return Err(e);
            }
        }
        Ok(())
    }
}

/// `RuleTest` замены руды.
#[derive(Clone, Debug)]
pub enum Rule {
    True,
    Tag(Arc<Vec<bool>>),
    Block(u32),
    State(u32),
    RandomBlock(u32, f32),
    Height(i32, i32),
    AnyOf(Vec<Rule>),
    AllOf(Vec<Rule>),
    Not(Box<Rule>),
}

impl Rule {
    fn parse(v: &Value, palette: &mut Palette) -> Result<Self, String> {
        let list = |palette: &mut Palette| -> Result<Vec<Rule>, String> {
            v["rules"]
                .as_array()
                .ok_or("rule test: no rules")?
                .iter()
                .map(|r| Rule::parse(r, palette))
                .collect()
        };
        match v["predicate_type"].as_str().unwrap_or("") {
            "minecraft:always_true" => Ok(Self::True),
            "minecraft:tag_match" => Ok(Self::Tag(palette.tag(v["tag"].as_str().unwrap_or(""))?)),
            "minecraft:block_match" => {
                Ok(Self::Block(palette.block(v["block"].as_str().unwrap_or(""))?))
            }
            "minecraft:blockstate_match" => Ok(Self::State(palette.state(&v["block_state"])?)),
            "minecraft:random_block_match" => Ok(Self::RandomBlock(
                palette.block(v["block"].as_str().unwrap_or(""))?,
                v["probability"].as_f64().unwrap_or(0.0) as f32,
            )),
            "minecraft:height_match" => Ok(Self::Height(
                v["min_inclusive"].as_i64().unwrap_or(i64::from(i32::MIN)) as i32,
                v["max_inclusive"].as_i64().unwrap_or(i64::from(i32::MAX)) as i32,
            )),
            "minecraft:any_of" => Ok(Self::AnyOf(list(palette)?)),
            "minecraft:all_of" => Ok(Self::AllOf(list(palette)?)),
            "minecraft:not" => Ok(Self::Not(Box::new(Self::parse(&v["rule"], palette)?))),
            other => Err(format!("rule test {other} is not supported")),
        }
    }

    fn test(&self, state: u32, y: i32, palette: &Palette, r: &mut Worldgen) -> bool {
        match self {
            Self::True => true,
            Self::Tag(mask) => mask.get(state as usize).copied().unwrap_or(false),
            Self::Block(b) => palette.block_of.get(state as usize) == Some(b),
            Self::State(s) => state == *s,
            Self::RandomBlock(b, p) => {
                palette.block_of.get(state as usize) == Some(b) && r.next_float() < *p
            }
            Self::Height(min, max) => *min <= y && y <= *max,
            Self::AnyOf(list) => list.iter().any(|t| t.test(state, y, palette, r)),
            Self::AllOf(list) => list.iter().all(|t| t.test(state, y, palette, r)),
            Self::Not(t) => !t.test(state, y, palette, r),
        }
    }
}

/// `AbstractOreFeature`: цели замены, размер и шанс отказа у воздуха.
#[derive(Clone, Debug)]
pub struct Ore {
    targets: Vec<(Rule, u32)>,
    size: i32,
    discard: f32,
}

#[derive(Clone, Debug)]
enum Feature {
    Ore(Ore),
    ScatteredOre(Ore),
}

/// `PlacementModifier`.
#[derive(Clone, Debug)]
enum Modifier {
    Count(IntP),
    InSquare,
    Height(HeightP),
    Biome,
    Rarity(i32),
    Heightmap(Heightmap),
    Fixed(Vec<[i32; 3]>),
}

impl Modifier {
    fn parse(v: &Value, min_y: i32, height: i32) -> Result<Self, String> {
        match v["type"].as_str().unwrap_or("") {
            "minecraft:count" => Ok(Self::Count(IntP::parse(&v["count"])?)),
            "minecraft:in_square" => Ok(Self::InSquare),
            "minecraft:height_range" => Ok(Self::Height(HeightP::parse(&v["height"], min_y, height)?)),
            "minecraft:biome" => Ok(Self::Biome),
            "minecraft:rarity_filter" => Ok(Self::Rarity(v["chance"].as_i64().unwrap_or(1) as i32)),
            "minecraft:heightmap" => Ok(Self::Heightmap(Heightmap::parse(
                v["heightmap"].as_str().unwrap_or(""),
            )?)),
            "minecraft:fixed_placement" => Ok(Self::Fixed(
                v["positions"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|p| {
                        let p = p.as_array()?;
                        Some([p.first()?.as_i64()? as i32, p.get(1)?.as_i64()? as i32, p.get(2)?.as_i64()? as i32])
                    })
                    .collect(),
            )),
            other => Err(format!("placement {other} is not supported")),
        }
    }
}

#[derive(Clone, Debug)]
struct Placed {
    feature: Feature,
    placement: Vec<Modifier>,
}

/// Все фичи оверворлда, разложенные по шагам декорации.
pub struct Features {
    /// Размещённые фичи по глобальному номеру: `steps[step][index]`;
    /// `None` — тип ещё не перенесён.
    steps: Vec<Vec<Option<Arc<Placed>>>>,
    /// Биом → по шагу номера его фич в [`Self::steps`].
    by_biome: HashMap<u32, Vec<Vec<u32>>>,
    /// Биом → все его фичи (`(шаг, номер)`) для фильтра `minecraft:biome`.
    has: HashMap<u32, HashSet<(u32, u32)>>,
    pub palette: Palette,
    seed: i64,
    min_y: i32,
}

/// Число шагов `GenerationStep.Decoration`.
const DECORATION_STEPS: usize = 11;

impl Features {
    /// `biome_order` — биомы источника в порядке `possibleBiomes()`;
    /// `biome_ids` — их сетевые ID.
    pub fn load(
        data_dir: &Path,
        mut palette: Palette,
        biome_order: &[String],
        biome_ids: &HashMap<String, u32>,
        seed: i64,
        min_y: i32,
        height: i32,
    ) -> Result<Self, String> {
        let wg = data_dir.join("data/minecraft/worldgen");
        let read = |dir: &str, name: &str| -> Result<Value, String> {
            let path = name.strip_prefix("minecraft:").unwrap_or(name);
            let text = std::fs::read_to_string(wg.join(dir).join(format!("{path}.json")))
                .map_err(|e| format!("{dir} {name}: {e}"))?;
            serde_json::from_str(&text).map_err(|e| format!("{dir} {name}: {e}"))
        };
        // Списки фич биомов в порядке источника.
        let mut sources: Vec<(u32, Vec<Vec<String>>)> = Vec::new();
        for name in biome_order {
            let Some(&id) = biome_ids.get(name) else {
                continue;
            };
            let biome = read("biome", name)?;
            let steps = biome["features"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|step| match step {
                    Value::String(one) => vec![one.clone()],
                    _ => step
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|x| x.as_str().map(str::to_owned))
                        .collect(),
                })
                .collect();
            sources.push((id, steps));
        }
        let order = sort_features(sources.iter().map(|(_, s)| s.as_slice()));
        let mut steps: Vec<Vec<Option<Arc<Placed>>>> = Vec::new();
        let mut global: HashMap<(usize, &str), u32> = HashMap::new();
        for (step, names) in order.iter().enumerate() {
            let mut list = Vec::with_capacity(names.len());
            for (i, name) in names.iter().enumerate() {
                global.insert((step, name.as_str()), i as u32);
                let placed = read("placed_feature", name)
                    .and_then(|v| parse_placed(&v, &read, &mut palette, min_y, height));
                list.push(placed.ok().map(Arc::new));
            }
            steps.push(list);
        }
        let mut by_biome = HashMap::new();
        let mut has: HashMap<u32, HashSet<(u32, u32)>> = HashMap::new();
        for (id, biome_steps) in &sources {
            let mut per = Vec::new();
            for (step, names) in biome_steps.iter().enumerate() {
                let idx: Vec<u32> = names
                    .iter()
                    .filter_map(|n| global.get(&(step, n.as_str())).copied())
                    .collect();
                has.entry(*id)
                    .or_default()
                    .extend(idx.iter().map(|&i| (step as u32, i)));
                per.push(idx);
            }
            by_biome.insert(*id, per);
        }
        Ok(Self {
            steps,
            by_biome,
            has,
            palette,
            seed,
            min_y,
        })
    }

    /// Сколько фич каждого шага перенесено — для отчёта при старте.
    pub fn coverage(&self) -> (usize, usize) {
        let all = self.steps.iter().map(Vec::len).sum();
        let done = self.steps.iter().flatten().filter(|p| p.is_some()).count();
        (done, all)
    }

    /// `applyBiomeDecoration` чанка `(cx, cz)`; `biomes` — биомы, что есть
    /// в секциях региона 3×3.
    pub fn decorate(&self, level: &mut dyn Level, cx: i32, cz: i32, biomes: &HashSet<u32>) {
        let (ox, oz) = (cx * 16, cz * 16);
        let mut r = Worldgen::new(0);
        let seed = r.set_decoration_seed(self.seed, ox, oz);
        // Шаги без фич (`DECORATION_STEPS` и больше) пока пусты: там только
        // структуры, которых ещё нет.
        for step in 0..self.steps.len().min(DECORATION_STEPS) {
            let mut wanted: Vec<u32> = biomes
                .iter()
                .filter_map(|b| self.by_biome.get(b)?.get(step))
                .flatten()
                .copied()
                .collect();
            wanted.sort_unstable();
            wanted.dedup();
            for index in wanted {
                let Some(placed) = &self.steps[step][index as usize] else {
                    continue;
                };
                r.set_feature_seed(seed, index as i32, step as i32);
                let key = (step as u32, index);
                self.place(level, placed, key, &mut r, [ox, self.min_y, oz]);
            }
        }
    }

    /// `FeaturePlacer.placeWithBiomeCheck`: модификаторы в глубину, по порядку.
    fn place(
        &self,
        level: &mut dyn Level,
        placed: &Placed,
        key: (u32, u32),
        r: &mut Worldgen,
        origin: [i32; 3],
    ) {
        self.modify(level, placed, key, r, origin, 0);
    }

    fn modify(
        &self,
        level: &mut dyn Level,
        placed: &Placed,
        key: (u32, u32),
        r: &mut Worldgen,
        pos: [i32; 3],
        at: usize,
    ) {
        let Some(m) = placed.placement.get(at) else {
            self.feature(level, &placed.feature, r, pos);
            return;
        };
        let next = at + 1;
        let [x, y, z] = pos;
        match m {
            Modifier::Count(n) => {
                let n = n.sample(r);
                // Все копии в стеке до развёртывания: счёт бросается один раз.
                for _ in 0..n {
                    self.modify(level, placed, key, r, pos, next);
                }
            }
            Modifier::InSquare => {
                let nx = r.next_int_bound(16) + x;
                let nz = r.next_int_bound(16) + z;
                self.modify(level, placed, key, r, [nx, y, nz], next);
            }
            Modifier::Height(h) => {
                let ny = h.sample(r);
                self.modify(level, placed, key, r, [x, ny, z], next);
            }
            Modifier::Biome => {
                let b = level.biome(x, y, z);
                if self.has.get(&b).is_some_and(|s| s.contains(&key)) {
                    self.modify(level, placed, key, r, pos, next);
                }
            }
            Modifier::Rarity(chance) => {
                if r.next_float() < 1.0 / *chance as f32 {
                    self.modify(level, placed, key, r, pos, next);
                }
            }
            Modifier::Heightmap(kind) => {
                let ny = level.surface(*kind, x, z);
                if ny > level.min_y() {
                    self.modify(level, placed, key, r, [x, ny, z], next);
                }
            }
            Modifier::Fixed(list) => {
                for p in list {
                    // Только точки внутри этого чанка (`FixedPlacement`).
                    if p[0] >> 4 == x >> 4 && p[2] >> 4 == z >> 4 {
                        self.modify(level, placed, key, r, *p, next);
                    }
                }
            }
        }
    }

    fn feature(&self, level: &mut dyn Level, f: &Feature, r: &mut Worldgen, pos: [i32; 3]) {
        match f {
            Feature::Ore(ore) => self.ore(level, ore, r, pos),
            Feature::ScatteredOre(ore) => self.scattered_ore(level, ore, r, pos),
        }
    }

    /// `AbstractOreFeature.canPlaceOre`.
    fn can_place_ore(
        &self,
        level: &dyn Level,
        state: u32,
        rule: &Rule,
        discard: f32,
        r: &mut Worldgen,
        [x, y, z]: [i32; 3],
    ) -> bool {
        if !rule.test(state, y, &self.palette, r) {
            return false;
        }
        // `shouldSkipAirCheck`.
        let skip = if discard <= 0.0 {
            true
        } else if discard >= 1.0 {
            false
        } else {
            r.next_float() >= discard
        };
        if skip {
            return true;
        }
        // `Direction.values()`: вниз, вверх, север, юг, запад, восток.
        let near = [
            [x, y - 1, z],
            [x, y + 1, z],
            [x, y, z - 1],
            [x, y, z + 1],
            [x - 1, y, z],
            [x + 1, y, z],
        ];
        !near
            .iter()
            .any(|&[a, b, c]| self.palette.is_air(level.get(a, b, c)))
    }

    /// `OreFeature.place`.
    fn ore(&self, level: &mut dyn Level, ore: &Ore, r: &mut Worldgen, [ox, oy, oz]: [i32; 3]) {
        let size = ore.size;
        let dir = r.next_float() * std::f32::consts::PI;
        let spread = size as f32 / 8.0;
        let max_radius = ((size as f32 / 16.0 * 2.0 + 1.0) / 2.0).ceil() as i32;
        let (sin, cos) = ((dir as f64).sin(), (dir as f64).cos());
        let spread64 = spread as f64;
        let x0 = ox as f64 + sin * spread64;
        let x1 = ox as f64 - sin * spread64;
        let z0 = oz as f64 + cos * spread64;
        let z1 = oz as f64 - cos * spread64;
        let y0 = (oy + r.next_int_bound(3) - 2) as f64;
        let y1 = (oy + r.next_int_bound(3) - 2) as f64;
        let ceil_spread = spread.ceil() as i32;
        let xs = ox - ceil_spread - max_radius;
        let ys = oy - 2 - max_radius;
        let zs = oz - ceil_spread - max_radius;
        let size_xz = 2 * (ceil_spread + max_radius);
        let size_y = 2 * (2 + max_radius);
        let reachable = (xs..=xs + size_xz).any(|x| {
            (zs..=zs + size_xz).any(|z| ys <= level.surface(Heightmap::OceanFloor, x, z))
        });
        if !reachable {
            return;
        }
        // `doPlace`.
        let n = size as usize;
        let mut data = vec![[0f64; 4]; n];
        for (i, d) in data.iter_mut().enumerate() {
            let step = i as f32 / size as f32;
            let s = step as f64;
            let xx = x0 + s * (x1 - x0);
            let yy = y0 + s * (y1 - y0);
            let zz = z0 + s * (z1 - z0);
            let ss = r.next_double() * size as f64 / 16.0;
            let rad = (((crate::carver::sin((std::f32::consts::PI * step) as f64) + 1.0) as f64) * ss
                + 1.0)
                / 2.0;
            *d = [xx, yy, zz, rad];
        }
        for a in 0..n.saturating_sub(1) {
            if data[a][3] <= 0.0 {
                continue;
            }
            for b in a + 1..n {
                if data[b][3] <= 0.0 {
                    continue;
                }
                let dx = data[a][0] - data[b][0];
                let dy = data[a][1] - data[b][1];
                let dz = data[a][2] - data[b][2];
                let dr = data[a][3] - data[b][3];
                if dr * dr > dx * dx + dy * dy + dz * dz {
                    if dr > 0.0 {
                        data[b][3] = -1.0;
                    } else {
                        data[a][3] = -1.0;
                    }
                }
            }
        }
        let (min_y, max_y) = (level.min_y(), level.min_y() + level.height());
        let mut tested = vec![false; (size_xz * size_y * size_xz) as usize];
        for &[xx, yy, zz, rad] in &data {
            if rad < 0.0 {
                continue;
            }
            let x_min = ((xx - rad).floor() as i32).max(xs);
            let y_min = ((yy - rad).floor() as i32).max(ys);
            let z_min = ((zz - rad).floor() as i32).max(zs);
            let x_max = ((xx + rad).floor() as i32).max(x_min);
            let y_max = ((yy + rad).floor() as i32).max(y_min);
            let z_max = ((zz + rad).floor() as i32).max(z_min);
            for x in x_min..=x_max {
                let xd = (x as f64 + 0.5 - xx) / rad;
                if xd * xd >= 1.0 {
                    continue;
                }
                for y in y_min..=y_max {
                    let yd = (y as f64 + 0.5 - yy) / rad;
                    if xd * xd + yd * yd >= 1.0 {
                        continue;
                    }
                    for z in z_min..=z_max {
                        let zd = (z as f64 + 0.5 - zz) / rad;
                        if xd * xd + yd * yd + zd * zd >= 1.0 || y < min_y || y >= max_y {
                            continue;
                        }
                        let bit = (x - xs + (y - ys) * size_xz + (z - zs) * size_xz * size_y) as usize;
                        // Индекс может выйти за набор у краёв: `BitSet` растёт сам.
                        if bit >= tested.len() {
                            tested.resize(bit + 1, false);
                        }
                        if tested[bit] {
                            continue;
                        }
                        tested[bit] = true;
                        if !level.can_write(x, z) {
                            continue;
                        }
                        let state = level.get(x, y, z);
                        for (rule, to) in &ore.targets {
                            if self.can_place_ore(&*level, state, rule, ore.discard, r, [x, y, z]) {
                                level.set(x, y, z, *to);
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    /// `ScatteredOreFeature.place`.
    fn scattered_ore(&self, level: &mut dyn Level, ore: &Ore, r: &mut Worldgen, [ox, oy, oz]: [i32; 3]) {
        let tries = r.next_int_bound(ore.size + 1);
        for i in 0..tries {
            let d = i.min(7) as f32;
            let mut axis = || ((r.next_float() - r.next_float()) * d + 0.5).floor() as i32;
            let (x, y, z) = (ox + axis(), oy + axis(), oz + axis());
            let (min_y, max_y) = (level.min_y(), level.min_y() + level.height());
            let state = if (min_y..max_y).contains(&y) {
                level.get(x, y, z)
            } else {
                u32::MAX
            };
            for (rule, to) in &ore.targets {
                if self.can_place_ore(&*level, state, rule, ore.discard, r, [x, y, z]) {
                    if (min_y..max_y).contains(&y) && level.can_write(x, z) {
                        level.set(x, y, z, *to);
                    }
                    break;
                }
            }
        }
    }
}

fn parse_placed(
    v: &Value,
    read: &dyn Fn(&str, &str) -> Result<Value, String>,
    palette: &mut Palette,
    min_y: i32,
    height: i32,
) -> Result<Placed, String> {
    let feature = match &v["feature"] {
        Value::String(name) => read("feature", name)?,
        inline => inline.clone(),
    };
    let feature = parse_feature(&feature, palette)?;
    let placement = v["placement"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|m| Modifier::parse(m, min_y, height))
        .collect::<Result<_, _>>()?;
    Ok(Placed { feature, placement })
}

fn parse_feature(v: &Value, palette: &mut Palette) -> Result<Feature, String> {
    let ore = |palette: &mut Palette| -> Result<Ore, String> {
        let mut targets = Vec::new();
        for t in v["targets"].as_array().ok_or("ore: no targets")? {
            targets.push((Rule::parse(&t["target"], palette)?, palette.state(&t["state"])?));
        }
        Ok(Ore {
            targets,
            size: v["size"].as_i64().unwrap_or(0) as i32,
            discard: v["discard_chance_on_air_exposure"].as_f64().unwrap_or(0.0) as f32,
        })
    };
    match v["type"].as_str().unwrap_or("") {
        "minecraft:ore" => Ok(Feature::Ore(ore(palette)?)),
        "minecraft:scattered_ore" => Ok(Feature::ScatteredOre(ore(palette)?)),
        other => Err(format!("feature {other} is not supported")),
    }
}

/// `FeatureSorter.buildFeaturesPerStep`: общий порядок фич шага по всем
/// биомам — топологическая сортировка цепочек «фича → следующая в биоме».
/// Возвращает имена фич по шагам.
pub fn sort_features<'a>(sources: impl Iterator<Item = &'a [Vec<String>]>) -> Vec<Vec<String>> {
    let mut index: HashMap<&str, usize> = HashMap::new();
    let mut names: Vec<&str> = Vec::new();
    // Узел — `(шаг, номер фичи)`: так же их сравнивает `TreeMap` ванили.
    let mut edges: BTreeMap<(usize, usize), BTreeSet<(usize, usize)>> = BTreeMap::new();
    let mut max_step = 0;
    for steps in sources {
        max_step = max_step.max(steps.len());
        let mut list = Vec::new();
        for (step, feats) in steps.iter().enumerate() {
            for f in feats {
                let i = *index.entry(f.as_str()).or_insert_with(|| {
                    names.push(f.as_str());
                    names.len() - 1
                });
                list.push((step, i));
            }
        }
        for (k, node) in list.iter().enumerate() {
            let set = edges.entry(*node).or_default();
            if let Some(next) = list.get(k + 1) {
                set.insert(*next);
            }
        }
    }
    let mut discovered = BTreeSet::new();
    let mut sorted = Vec::new();
    fn dfs(
        edges: &BTreeMap<(usize, usize), BTreeSet<(usize, usize)>>,
        discovered: &mut BTreeSet<(usize, usize)>,
        visiting: &mut BTreeSet<(usize, usize)>,
        out: &mut Vec<(usize, usize)>,
        node: (usize, usize),
    ) {
        if discovered.contains(&node) || !visiting.insert(node) {
            return;
        }
        if let Some(next) = edges.get(&node) {
            for &n in next {
                dfs(edges, discovered, visiting, out, n);
            }
        }
        visiting.remove(&node);
        discovered.insert(node);
        out.push(node);
    }
    for &node in edges.keys() {
        dfs(&edges, &mut discovered, &mut BTreeSet::new(), &mut sorted, node);
    }
    sorted.reverse();
    (0..max_step)
        .map(|step| {
            sorted
                .iter()
                .filter(|n| n.0 == step)
                .map(|n| names[n.1].to_owned())
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorter_merges_chains() {
        let s = |v: &[&[&str]]| -> Vec<Vec<String>> {
            v.iter().map(|x| x.iter().map(|s| s.to_string()).collect()).collect()
        };
        let a = s(&[&["a", "c"], &["x"]]);
        let b = s(&[&["a", "b", "c"]]);
        let out = sort_features([a.as_slice(), b.as_slice()].into_iter());
        assert_eq!(out[0], vec!["a", "b", "c"]);
        assert_eq!(out[1], vec!["x"]);
    }
}
