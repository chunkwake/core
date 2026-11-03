//! Фичи декорации (`ChunkGenerator.applyBiomeDecoration`): размещённые фичи
//! `worldgen/placed_feature` с модификаторами размещения, порядок шагов
//! `FeatureSorter` и сиды `WorldgenRandom` бит в бит. Перенесены руды
//! (`ore`, `scattered_ore`); фичи других типов пока пропускаются целиком —
//! каждая фича пересевает генератор, так что пропуск не сдвигает остальные.

use crate::carver::{HeightP, IntP};
use crate::noise::{NormalNoise, Simplex};
use crate::random::{Bits, Legacy, Worldgen};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

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
    /// `getSeaLevel`.
    fn sea_level(&self) -> i32;
    /// `getBiome(pos)` с «дрожанием» границ.
    fn biome(&self, x: i32, y: i32, z: i32) -> u32;
    /// Клетка, чью форму надо пересчитать после декорации соседей
    /// (`markPosForPostProcessing`). По умолчанию некуда записать.
    fn mark_shape(&mut self, _x: i32, _y: i32, _z: i32) {}
    /// `Level.setBlockEntity` для сундуков структур: ванила пишет в них
    /// `LootTable` и `LootTableSeed`, а не готовые предметы.
    fn set_block_entity(
        &mut self,
        _x: i32,
        _y: i32,
        _z: i32,
        _id: &str,
        _nbt: chunkwake_world::nbt::Compound,
    ) {
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heightmap {
    /// `WORLD_SURFACE(_WG)`: всё, кроме воздуха.
    WorldSurface,
    /// `OCEAN_FLOOR(_WG)`: тег `blocks_motion_in_heightmap`.
    OceanFloor,
    /// `MOTION_BLOCKING`: тот же тег или жидкость.
    MotionBlocking,
    /// `MOTION_BLOCKING_NO_LEAVES`: тег без листвы или жидкость.
    MotionBlockingNoLeaves,
    /// `WORLD_SURFACE_WG` и `OCEAN_FLOOR_WG`: те же правила, но карта
    /// застывает после рельефа — прото-чанк в статусе `TERRAIN` при записи
    /// фич обновляет лишь итоговые карты.
    WorldSurfaceWg,
    OceanFloorWg,
}

impl Heightmap {
    pub const ALL: [Heightmap; 4] = [
        Self::WorldSurface,
        Self::OceanFloor,
        Self::MotionBlocking,
        Self::MotionBlockingNoLeaves,
    ];

    fn parse(name: &str) -> Result<Self, String> {
        match name {
            "WORLD_SURFACE" => Ok(Self::WorldSurface),
            "OCEAN_FLOOR" => Ok(Self::OceanFloor),
            "WORLD_SURFACE_WG" => Ok(Self::WorldSurfaceWg),
            "OCEAN_FLOOR_WG" => Ok(Self::OceanFloorWg),
            "MOTION_BLOCKING" => Ok(Self::MotionBlocking),
            "MOTION_BLOCKING_NO_LEAVES" => Ok(Self::MotionBlockingNoLeaves),
            other => Err(format!("heightmap {other} is not supported")),
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

/// Жидкость состояния (`getFluidState`): источник и течение различаются,
/// как `minecraft:water` и `minecraft:flowing_water`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Fluid {
    #[default]
    None,
    Water,
    FlowingWater,
    Lava,
    FlowingLava,
}

impl Fluid {
    pub fn is_water(self) -> bool {
        matches!(self, Self::Water | Self::FlowingWater)
    }
    pub fn is_lava(self) -> bool {
        matches!(self, Self::Lava | Self::FlowingLava)
    }
}

/// Чем блок проверяет `canSurvive` — по классу блока ванили.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Survive {
    #[default]
    Always,
    /// `VegetationBlock`: снизу `supports_vegetation`.
    Vegetation,
    /// `DryVegetationBlock`: снизу `supports_dry_vegetation`.
    Dry,
    /// `DoublePlantBlock`: нижняя половина как растение, верхняя — над нижней.
    Double,
    Seagrass,
    TallSeagrass,
    SugarCane,
    Cactus,
    /// Нужна прочная грань снизу (`LeafLitterBlock`).
    SturdyBelow,
    MossyCarpet,
    Mushroom,
    Snow,
    /// Корни, грибы и ростки Ада: снизу свой тег `supports_<блок>`.
    OwnTag,
}

impl Survive {
    fn of_class(class: &str) -> Self {
        match class {
            "FlowerBlock"
            | "TallGrassBlock"
            | "BushBlock"
            | "SaplingBlock"
            | "FireflyBushBlock"
            | "FlowerBedBlock"
            | "SweetBerryBushBlock"
            | "EyeblossomBlock"
            | "AzaleaBlock"
            | "WitherRoseBlock" => Self::Vegetation,
            "DryVegetationBlock" | "ShortDryGrassBlock" | "TallDryGrassBlock" => Self::Dry,
            "DoublePlantBlock" | "TallFlowerBlock" => Self::Double,
            "SeagrassBlock" => Self::Seagrass,
            "TallSeagrassBlock" => Self::TallSeagrass,
            "SugarCaneBlock" => Self::SugarCane,
            "CactusBlock" => Self::Cactus,
            "LeafLitterBlock" => Self::SturdyBelow,
            "MossyCarpetBlock" => Self::MossyCarpet,
            "MushroomBlock" => Self::Mushroom,
            "SnowLayerBlock" => Self::Snow,
            "NetherRootsBlock" | "NetherFungusBlock" | "NetherSproutsBlock" => Self::OwnTag,
            _ => Self::Always,
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
    /// Номер блока → его имя.
    block_names: Vec<String>,
    /// Номер блока → его состояния.
    by_block: Vec<Vec<u32>>,
    /// Номер блока → смешанное основание его состояний, если они идут
    /// подряд произведением значений: `with_prop` сдвигом номера вместо
    /// перебора всех состояний блока со сравнением строк (у `sculk_vein` их
    /// 128, и на этом стояло распространение скалка).
    layouts: Vec<Option<StateLayout>>,
    defaults: HashMap<String, u32>,
    air: Vec<bool>,
    fluid: Vec<Fluid>,
    /// `isSolid`, `canBeReplaced`, `LiquidBlock` по состоянию.
    solid: Vec<bool>,
    replaceable: Vec<bool>,
    liquid: Vec<bool>,
    /// Коллизия — один полный куб. Для блоков этих шаблонов это совпадает
    /// с `isCollisionShapeFullBlock && !hasDynamicShape`.
    full_cube: Vec<bool>,
    /// Биты `isFaceSturdy` по `Direction`: низ, верх, север, юг, запад, восток.
    sturdy: Vec<u8>,
    /// `Block.canSupportCenter(..., DOWN)`: низ формы опоры закрывает
    /// квадрат 7..9 в центре (столб забора, цепь, стена).
    center_down: Vec<bool>,
    survive: Vec<Survive>,
    /// Теги карт высот: `blocks_motion_in_heightmap` и без листвы.
    motion: Arc<Vec<bool>>,
    motion_no_leaves: Arc<Vec<bool>>,
    tags: HashMap<String, Arc<Vec<bool>>>,
    tag_dir: std::path::PathBuf,
    /// Теги, которые читает `canSurvive`.
    support: Support,
    /// Сид мира: шумы фич вроде жеод строятся от него.
    world_seed: i64,
    /// Климат биомов для `Biome.getTemperature`. Индекс — сетевой ID биома;
    /// до заполнения в `Features::load` — пусто.
    climate: Arc<Vec<Climate>>,
}

/// Свойства блока со всеми значениями и шагом номера состояния на каждое.
type StateLayout = Vec<(String, Vec<String>, u32)>;

/// Раскладка состояний блока, если номер состояния — `первый + Σ индекс
/// значения × шаг`; проверяется на всех состояниях, иначе `None`.
fn state_layout(list: &[u32], props: &[Vec<(String, String)>]) -> Option<StateLayout> {
    let &first = list.first()?;
    if list.iter().enumerate().any(|(i, &s)| s != first + i as u32) {
        return None;
    }
    let base = props.get(first as usize)?;
    let mut layout: StateLayout = Vec::with_capacity(base.len());
    for (k, (key, value)) in base.iter().enumerate() {
        let mut values: Vec<String> = Vec::new();
        let mut stride = 0;
        for (i, &s) in list.iter().enumerate() {
            let (k2, v) = props[s as usize].get(k)?;
            if k2 != key {
                return None;
            }
            if !values.contains(v) {
                values.push(v.clone());
            }
            if stride == 0 && v != value {
                stride = i as u32;
            }
        }
        layout.push((key.clone(), values, stride));
    }
    for (i, &s) in list.iter().enumerate() {
        let mut at = 0;
        for (k, (_, values, stride)) in layout.iter().enumerate() {
            let v = &props[s as usize][k].1;
            at += values.iter().position(|x| x == v)? as u32 * stride;
        }
        if at != i as u32 {
            return None;
        }
    }
    Some(layout)
}

/// `Biome.ClimateSettings`: то, от чего зависят снег, лёд и холодные руины.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Climate {
    temperature: f32,
    /// `temperature_modifier: frozen`.
    frozen: bool,
    /// `has_precipitation`.
    precipitation: bool,
}

/// `Biome.TEMPERATURE_NOISE`.
fn temperature_noise() -> &'static Simplex {
    static NOISE: std::sync::OnceLock<Simplex> = std::sync::OnceLock::new();
    NOISE.get_or_init(|| Simplex::new(&mut Legacy::new(1234), true))
}

/// `Biome.FROZEN_TEMPERATURE_NOISE`: три слоя (частота, амплитуда).
fn frozen_noise(x: f64, z: f64) -> f32 {
    static NOISE: std::sync::OnceLock<[Simplex; 3]> = std::sync::OnceLock::new();
    let layers = NOISE.get_or_init(|| {
        let mut r = Legacy::new(3456);
        [
            Simplex::new(&mut r, true),
            Simplex::new(&mut r, true),
            Simplex::new(&mut r, true),
        ]
    });
    let mut value = 0.0f32;
    for (noise, (freq, amp)) in layers
        .iter()
        .zip([(1.0, 0.14285715f32), (0.5, 0.2857143), (0.25, 0.5714286)])
    {
        value += amp * noise.get(x * freq, z * freq);
    }
    value
}

/// `Biome.getHeightAdjustedTemperature`. Неизвестный биом тёплый — лучше
/// без льда, чем лёд в пустыне.
pub(crate) fn temperature(climate: &[Climate], biome: u32, pos: [i32; 3], sea_level: i32) -> f32 {
    let Some(c) = climate.get(biome as usize) else {
        return 2.0;
    };
    let mut t = c.temperature;
    if c.frozen {
        // `TemperatureModifier.FROZEN`.
        let large = (frozen_noise(pos[0] as f64 * 0.05, pos[2] as f64 * 0.05) * 7.0) as f64;
        let edge = biome_info_noise().get(pos[0] as f64 * 0.2, pos[2] as f64 * 0.2) as f64;
        if large + edge < 0.3
            && (biome_info_noise().get(pos[0] as f64 * 0.09, pos[2] as f64 * 0.09) as f64) < 0.8
        {
            t = 0.2;
        }
    }
    let snow_level = sea_level + 17;
    if pos[1] > snow_level {
        let v = temperature_noise().get(
            (pos[0] as f32 / 8.0) as f64,
            (pos[2] as f32 / 8.0) as f64,
        ) * 8.0;
        t - (v + pos[1] as f32 - snow_level as f32) * 0.05 / 40.0
    } else {
        t
    }
}

/// `Biome.coldEnoughToSnow`.
pub(crate) fn cold_enough_to_snow(climate: &[Climate], biome: u32, pos: [i32; 3], sea_level: i32) -> bool {
    !(temperature(climate, biome, pos, sea_level) >= 0.15)
}

#[derive(Default)]
struct Support {
    vegetation: Arc<Vec<bool>>,
    dry: Arc<Vec<bool>>,
    cane: Arc<Vec<bool>>,
    cane_adjacent: Arc<Vec<bool>>,
    cactus: Arc<Vec<bool>>,
    no_seagrass: Arc<Vec<bool>>,
    mushroom: Arc<Vec<bool>>,
    no_snow: Arc<Vec<bool>>,
    snow: Arc<Vec<bool>>,
    /// Номер блока → его тег опоры (`Survive::OwnTag`).
    own: HashMap<u32, Arc<Vec<bool>>>,
}

impl Palette {
    /// `states` — по ID состояния имя блока и свойства (как в реестре сервера);
    /// `default` — состояние блока по умолчанию.
    pub fn new(data_dir: &Path, states: &States, default: &dyn Fn(&str) -> Option<u32>) -> Self {
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
        let n = states.len();
        let mut p = Self {
            air: vec![false; n],
            fluid: vec![Fluid::None; n],
            solid: vec![false; n],
            replaceable: vec![false; n],
            liquid: vec![false; n],
            full_cube: vec![false; n],
            sturdy: vec![0; n],
            center_down: vec![false; n],
            survive: vec![Survive::Always; n],
            motion: Arc::new(Vec::new()),
            motion_no_leaves: Arc::new(Vec::new()),
            by_block: {
                let mut v = vec![Vec::new(); blocks.len()];
                for (id, list) in blocks.values() {
                    v[*id as usize] = list.clone();
                }
                v
            },
            layouts: Vec::new(),
            block_of,
            props,
            block_names: {
                let mut v = vec![String::new(); blocks.len()];
                for (name, (id, _)) in &blocks {
                    v[*id as usize] = name.clone();
                }
                v
            },
            blocks,
            defaults,
            tags: HashMap::new(),
            tag_dir: data_dir.join("data/minecraft/tags/block"),
            support: Support::default(),
            world_seed: 0,
            climate: Arc::new(Vec::new()),
        };
        p.layouts = p.by_block.iter().map(|list| state_layout(list, &p.props)).collect();
        for name in ["minecraft:air", "minecraft:cave_air", "minecraft:void_air"] {
            for s in p.states_of(name) {
                p.air[s as usize] = true;
            }
        }
        // `reports/worldgen_blocks.csv`: жидкость, isSolid, canBeReplaced, класс.
        let rows = std::fs::read_to_string(data_dir.join("reports/worldgen_blocks.csv"))
            .unwrap_or_default();
        for (id, line) in rows.lines().enumerate().take(n) {
            let mut f = line.split(',');
            let fluid = f.next().unwrap_or("0");
            p.solid[id] = f.next() == Some("1");
            p.replaceable[id] = f.next() == Some("1");
            let class = f.next().unwrap_or("");
            p.liquid[id] = class == "LiquidBlock";
            p.survive[id] = Survive::of_class(class);
            p.fluid[id] = match fluid {
                "1" => Fluid::Water,
                "2" => Fluid::Lava,
                _ => Fluid::None,
            };
        }
        // Течение — у состояний с уровнем больше нуля; без выгрузки жидкость
        // узнаём по имени блока.
        for (name, (src, flow)) in [
            ("minecraft:water", (Fluid::Water, Fluid::FlowingWater)),
            ("minecraft:lava", (Fluid::Lava, Fluid::FlowingLava)),
        ] {
            for s in p.states_of(name) {
                let level = p.prop(s, "level").unwrap_or("0");
                p.fluid[s as usize] = if level == "0" { src } else { flow };
                p.liquid[s as usize] = true;
            }
        }
        // `reports/light.csv`: шестой столбец — маска прочных граней.
        let light = std::fs::read_to_string(data_dir.join("reports/light.csv")).unwrap_or_default();
        for (id, line) in light.lines().enumerate().take(n) {
            p.sturdy[id] = line
                .rsplit(',')
                .next()
                .and_then(|m| m.trim().parse().ok())
                .unwrap_or(0);
        }
        // `reports/shapes.csv`: коробки коллизии через `|` после трёх чисел.
        // `getBlockSupportShape` совпадает с коллизией, кроме листвы (пусто).
        let shapes = std::fs::read_to_string(data_dir.join("reports/shapes.csv")).unwrap_or_default();
        let leaves = p.tag("minecraft:leaves").unwrap_or_default();
        for (id, line) in shapes.lines().enumerate().take(n) {
            if leaves.get(id).copied().unwrap_or(false) {
                continue;
            }
            let boxes = line.splitn(4, ',').nth(3).unwrap_or("");
            p.center_down[id] = boxes.split('|').any(|b| {
                let v: Vec<f64> = b.split(' ').filter_map(|x| x.parse().ok()).collect();
                v.len() == 6
                    && v[1] <= 0.0
                    && v[0] <= 7.0 / 16.0
                    && v[2] <= 7.0 / 16.0
                    && v[3] >= 9.0 / 16.0
                    && v[5] >= 9.0 / 16.0
            });
        }
        p.motion = p
            .tag("minecraft:blocks_motion_in_heightmap")
            .unwrap_or_default();
        p.motion_no_leaves = p
            .tag("minecraft:blocks_motion_in_heightmap_no_leaves")
            .unwrap_or_default();
        let mut tag = |name: &str| p.tag(name).unwrap_or_default();
        p.support = Support {
            vegetation: tag("minecraft:supports_vegetation"),
            dry: tag("minecraft:supports_dry_vegetation"),
            cane: tag("minecraft:supports_sugar_cane"),
            cane_adjacent: tag("minecraft:supports_sugar_cane_adjacently"),
            cactus: tag("minecraft:supports_cactus"),
            no_seagrass: tag("minecraft:cannot_support_seagrass"),
            mushroom: tag("minecraft:overrides_mushroom_light_requirement"),
            no_snow: tag("minecraft:cannot_support_snow_layer"),
            snow: tag("minecraft:support_override_snow_layer"),
            own: HashMap::new(),
        };
        let own: Vec<(u32, String)> = p
            .block_names
            .iter()
            .enumerate()
            .filter(|(i, _)| {
                p.by_block[*i].first().is_some_and(|&s| p.survive[s as usize] == Survive::OwnTag)
            })
            .map(|(i, name)| (i as u32, name.replacen("minecraft:", "minecraft:supports_", 1)))
            .collect();
        for (block, tag_name) in own {
            let t = p.tag(&tag_name).unwrap_or_default();
            p.support.own.insert(block, t);
        }
        let shapes =
            std::fs::read_to_string(data_dir.join("reports/shapes.csv")).unwrap_or_default();
        for (id, line) in shapes.lines().enumerate().take(n) {
            let shape = line.splitn(4, ',').nth(3).unwrap_or("").trim();
            p.full_cube[id] = full_cube_shape(shape);
        }
        p
    }

    fn is_full_cube(&self, state: u32) -> bool {
        self.full_cube.get(state as usize).copied().unwrap_or(false)
    }

    fn block_index(&self, state: u32) -> Option<u32> {
        self.block_of
            .get(state as usize)
            .copied()
            .filter(|id| *id != u32::MAX)
    }

    /// Имя блока по состоянию (`BlockState.getBlock` и его реестровый ключ).
    pub fn block_name(&self, state: u32) -> Option<&str> {
        let id = self.block_index(state)?;
        self.block_names.get(id as usize).map(String::as_str)
    }

    /// Блок-жидкость (`water` / `lava`), не затопленная ступень.
    fn is_liquid(&self, state: u32) -> bool {
        self.liquid.get(state as usize).copied().unwrap_or(false)
    }

    /// Значение свойства состояния.
    pub fn prop(&self, state: u32, key: &str) -> Option<&str> {
        self.props
            .get(state as usize)?
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// То же состояние с другим значением свойства (`trySetValue`): без
    /// такого свойства — `None`.
    pub fn with_prop(&self, state: u32, key: &str, value: &str) -> Option<u32> {
        let props = self.props.get(state as usize)?;
        let k = props.iter().position(|(k, _)| k == key)?;
        let block = *self.block_of.get(state as usize)?;
        if let Some(Some(layout)) = self.layouts.get(block as usize) {
            let (_, values, stride) = &layout[k];
            let new = values.iter().position(|v| v == value)? as u32;
            let old = values.iter().position(|v| *v == props[k].1)? as u32;
            return Some(state - old * stride + new * stride);
        }
        self.with_prop_scan(state, key, value)
    }

    /// `with_prop` перебором состояний блока: для раскладок не подряд.
    fn with_prop_scan(&self, state: u32, key: &str, value: &str) -> Option<u32> {
        let props = self.props.get(state as usize)?;
        props.iter().find(|(k, _)| k == key)?;
        let list = self
            .by_block
            .get(*self.block_of.get(state as usize)? as usize)?;
        list.iter().copied().find(|&s| {
            self.props[s as usize]
                .iter()
                .zip(props)
                .all(|((k, v), (k0, v0))| k == k0 && if k == key { v == value } else { v == v0 })
        })
    }

    pub fn fluid(&self, state: u32) -> Fluid {
        self.fluid.get(state as usize).copied().unwrap_or_default()
    }

    fn flag(list: &[bool], state: u32) -> bool {
        list.get(state as usize).copied().unwrap_or(false)
    }

    /// `isFaceSturdy(dir)`, `dir` в порядке `Direction`.
    /// `Block.canSupportCenter(level, pos, DOWN)`.
    pub fn supports_center_below(&self, state: u32) -> bool {
        self.center_down.get(state as usize).copied().unwrap_or(false)
    }

    pub fn sturdy(&self, state: u32, dir: usize) -> bool {
        self.sturdy
            .get(state as usize)
            .is_some_and(|m| m >> dir & 1 != 0)
    }

    fn same_block(&self, a: u32, b: u32) -> bool {
        self.block_of
            .get(a as usize)
            .is_some_and(|x| Some(x) == self.block_of.get(b as usize))
    }

    fn states_of(&self, block: &str) -> Vec<u32> {
        self.blocks
            .get(block)
            .map(|b| b.1.clone())
            .unwrap_or_default()
    }

    pub fn is_air(&self, state: u32) -> bool {
        self.air.get(state as usize).copied().unwrap_or(false)
    }

    /// `Biome.coldEnoughToSnow` (он же `!warmEnoughToRain`).
    pub fn cold_enough_to_snow(&self, biome: u32, pos: [i32; 3], sea_level: i32) -> bool {
        cold_enough_to_snow(&self.climate, biome, pos, sea_level)
    }

    /// `Biome.getPrecipitationAt(...) == SNOW`.
    pub fn snows(&self, biome: u32, pos: [i32; 3], sea_level: i32) -> bool {
        self.climate
            .get(biome as usize)
            .is_some_and(|c| c.precipitation)
            && self.cold_enough_to_snow(biome, pos, sea_level)
    }

    pub(crate) fn climate(&self) -> Arc<Vec<Climate>> {
        Arc::clone(&self.climate)
    }

    /// Учитывает ли карта высот `kind` это состояние.
    pub fn hits(&self, kind: Heightmap, state: u32) -> bool {
        match kind {
            Heightmap::WorldSurface | Heightmap::WorldSurfaceWg => !self.is_air(state),
            Heightmap::OceanFloor | Heightmap::OceanFloorWg => Self::flag(&self.motion, state),
            Heightmap::MotionBlocking => {
                Self::flag(&self.motion, state) || self.fluid(state) != Fluid::None
            }
            Heightmap::MotionBlockingNoLeaves => {
                Self::flag(&self.motion_no_leaves, state) || self.fluid(state) != Fluid::None
            }
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
                v["Name"]
                    .as_str()
                    .or_else(|| v["id"].as_str())
                    .ok_or("block state: no Name")?,
                v["Properties"]
                    .as_object()
                    .or_else(|| v["properties"].as_object()),
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
        let mask: Vec<bool> = self.block_of.iter().map(|b| blocks.contains(b)).collect();
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
        let text = std::fs::read_to_string(&file).map_err(|e| format!("tag {name}: {e}"))?;
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
            "minecraft:block_match" => Ok(Self::Block(
                palette.block(v["block"].as_str().unwrap_or(""))?,
            )),
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
    /// `simple_block`: провайдер и `schedule_tick`.
    SimpleBlock(Provider),
    /// `random_selector`: по очереди `nextFloat() < chance`, иначе по умолчанию.
    RandomSelector(Vec<(f32, Arc<Placed>)>, Arc<Placed>),
    /// `simple_random_selector`: одна из списка равновероятно.
    SimpleRandomSelector(Vec<Arc<Placed>>),
    /// `weighted_random_selector`: общий вес и (вес, фича).
    WeightedSelector(i32, Vec<(i32, Arc<Placed>)>),
    /// `block_column`: слои (высота, провайдер), направление, допустимые
    /// клетки и `prioritize_tip`.
    BlockColumn(Vec<(IntP, Provider)>, [i32; 3], Predicate, bool),
    /// `block_pile`: стог сена, тыквы, снега и льда в деревнях.
    BlockPile(Provider),
    VegetationPatch(Box<Patch>),
    /// `random_boolean_selector`: `nextBoolean()` — первая, иначе вторая.
    BooleanSelector(Arc<Placed>, Arc<Placed>),
    Tree(Arc<tree::Tree>),
    FallenTree(Arc<tree::Fallen>),
    HugeMushroom(Box<Mushroom>),
    Disk(Arc<misc::Disk>),
    Spring(Arc<misc::Spring>),
    Vines,
    Blob(Arc<misc::Blob>),
    BlueIce,
    Lake(Arc<misc::Lake>),
    MonsterRoom(Arc<misc::MonsterRoom>),
    Geode(Arc<misc::Geode>),
    RootSystem(Arc<misc::RootSystem>),
    Multiface(Arc<misc::Multiface>),
    Speleothem(Arc<misc::Speleothem>),
    LargeDripstone(Arc<misc::LargeDripstone>),
    SpeleothemCluster(Arc<misc::SpeleothemCluster>),
    /// `sequence`: фичи по порядку в одной точке.
    Sequence(Vec<Arc<Placed>>),
    /// `freeze_top_layer`: лёд и снег на поверхности холодных биомов.
    FreezeTopLayer,
    /// `underwater_magma`: магма под морским дном.
    UnderwaterMagma(Arc<misc::UnderwaterMagma>),
    /// `bamboo`: стебли бамбука; доля `probability` — саженцы на подзоле.
    Bamboo(f32),
    /// `spike`: ледяные шипы (`ice_spike`).
    IceSpike(Arc<misc::IceSpike>),
    /// `iceberg`: айсберг с синим или пакованным льдом.
    Iceberg(misc::Iceberg),
    /// `coral_tree`, `coral_claw`, `coral_mushroom`: коралл из одной фичи.
    CoralTree(String, Arc<Placed>),
    /// `overlay`: встроенные фичи поверх друг друга, каждая в своей точке.
    Overlay(Vec<Arc<Placed>>),
    /// `sculk_patch`: скалк в тёмных пещерах.
    SculkPatch(Arc<misc::SculkPatch>),
    /// `no_op`: ничего не ставит (`NoOpFeature`).
    NoOp,
    /// `void_start_platform`: квадрат 33×33 на `minY + 3` у нуля мира.
    VoidPlatform(u32, u32),
    /// `template`: взвешенный NBT-шаблон и случайный поворот.
    Template(Arc<template::TemplateFeature>),
    /// `fossil`: пара шаблонов, пустые углы и два списка процессоров.
    Fossil(Arc<template::Fossil>),
    /// `single_block_pillar`: базальтовый столб с шапкой.
    Pillar(Arc<nether::Pillar>),
    /// `delta_feature`: лужи базальтовых дельт.
    Delta(Arc<nether::Delta>),
    /// `stepped_column_cluster`: гроздь столбов базальта.
    ColumnCluster(Arc<nether::ColumnCluster>),
    /// `netherrack_replace_blobs`: пятна в незераке.
    ReplaceBlobs(Arc<nether::ReplaceBlobs>),
    /// `random_neighbor_spread`: светокамень, плакучие лианы.
    NeighborSpread(Arc<nether::NeighborSpread>),
    /// `huge_fungus`: огромные грибы Ада.
    HugeFungus(Arc<nether::HugeFungus>),
    /// `projected_random_patchy_square`: шапка базальтового столба.
    PatchySquare(Arc<nether::PatchySquare>),
    /// `end_island`: перевёрнутый конус камня Края.
    EndIsland(Arc<end::Blocks>),
    /// `end_platform`: обсидиановая площадка точки входа.
    EndPlatform(Arc<end::Blocks>),
    /// `end_gateway`: шлюз в бедроковой оправе.
    EndGateway(Arc<end::Blocks>, end::Gateway),
    /// `end_spike`: обсидиановые шипы вокруг главного острова.
    EndSpike(Arc<end::Blocks>, end::Spikes),
    /// `chorus_plant`: хорус на камне Края.
    Chorus(Arc<end::Blocks>),
}

pub(crate) mod mansion;
pub(crate) mod mineshaft;
pub(crate) mod end;
mod fortress;
mod nether;
pub(crate) mod monument;
pub(crate) mod ocean_ruin;
pub(crate) mod portal;
mod pyramid;
mod shore;
pub(crate) mod stronghold;
pub(crate) mod structure;
mod template;
mod treasure;
pub(crate) mod end_city;
pub(crate) use template::{load_public, place_public, size_public};

fn full_cube_shape(shape: &str) -> bool {
    if shape.is_empty() || shape.contains('|') {
        return false;
    }
    let v: Vec<f64> = shape.split(' ').filter_map(|x| x.parse().ok()).collect();
    v.len() == 6
        && v[0].abs() < 1e-4
        && v[1].abs() < 1e-4
        && v[2].abs() < 1e-4
        && (v[3] - 1.0).abs() < 1e-4
        && (v[4] - 1.0).abs() < 1e-4
        && (v[5] - 1.0).abs() < 1e-4
}

mod misc;

/// `huge_red_mushroom` и `huge_brown_mushroom`.
#[derive(Clone, Debug)]
struct Mushroom {
    red: bool,
    cap: Provider,
    stem: Provider,
    radius: i32,
    on: Predicate,
    replaceable: Arc<Vec<bool>>,
    leaves: Arc<Vec<bool>>,
}

mod tree;

/// `VegetationPatchFeature` и его залитый водой вариант.
#[derive(Clone, Debug)]
struct Patch {
    replaceable: Arc<Vec<bool>>,
    ground: Provider,
    vegetation: Arc<Placed>,
    /// Направление внутрь поверхности: вниз у пола, вверх у потолка.
    inwards: usize,
    depth: IntP,
    extra_bottom: f32,
    vertical_range: i32,
    vegetation_chance: f32,
    radius: IntP,
    extra_edge: f32,
    waterlogged: bool,
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
    /// `random_chance`.
    Chance(f32),
    /// `environment_scan`: шаг, цель, где можно идти, число шагов.
    Scan([i32; 3], Predicate, Predicate, i32),
    /// `noise_threshold_count`: уровень, ниже, выше.
    NoiseThreshold(f64, i32, i32),
    /// `noise_based_count`: отношение, масштаб, сдвиг.
    NoiseBased(i32, f64, f64),
    Offset(IntP, IntP, IntP),
    Filter(Predicate),
    /// `surface_water_depth_filter`.
    WaterDepth(i32),
    /// `surface_relative_threshold_filter`.
    Relative(Heightmap, i32, i32),
    /// `count_on_every_layer`: по `count` точек на каждый слой пола, пока
    /// хоть одна точка нашла свой слой.
    OnEveryLayer(IntP),
    /// `cuboid`: случайная точка в полом коробе (`CuboidPlacement`).
    Cuboid {
        /// `include_edges`, `include_interior` — какие клетки участвуют.
        edges: bool,
        interior: bool,
        xz: IntP,
        y: IntP,
    },
    /// `randomly_selected`: один модификатор из списка, `nextInt(len)`.
    RandomlySelected(Vec<Modifier>),
}

impl Modifier {
    fn parse(v: &Value, palette: &mut Palette, min_y: i32, height: i32) -> Result<Self, String> {
        let int = |k: &str, d: i64| v[k].as_i64().unwrap_or(d) as i32;
        match v["type"].as_str().unwrap_or("") {
            "minecraft:noise_threshold_count" => Ok(Self::NoiseThreshold(
                v["noise_level"].as_f64().unwrap_or(0.0),
                int("below_noise", 0),
                int("above_noise", 0),
            )),
            "minecraft:noise_based_count" => Ok(Self::NoiseBased(
                int("noise_to_count_ratio", 0),
                v["noise_factor"].as_f64().unwrap_or(1.0),
                v["noise_offset"].as_f64().unwrap_or(0.0),
            )),
            "minecraft:offset" => Ok(Self::Offset(
                IntP::parse(&v["x"])?,
                IntP::parse(&v["y"])?,
                IntP::parse(&v["z"])?,
            )),
            "minecraft:block_predicate_filter" => {
                Ok(Self::Filter(Predicate::parse(&v["predicate"], palette)?))
            }
            "minecraft:surface_water_depth_filter" => {
                Ok(Self::WaterDepth(int("max_water_depth", 0)))
            }
            "minecraft:surface_relative_threshold_filter" => Ok(Self::Relative(
                Heightmap::parse(v["heightmap"].as_str().unwrap_or(""))?,
                int("min_inclusive", i32::MIN as i64),
                int("max_inclusive", i32::MAX as i64),
            )),
            "minecraft:environment_scan" => {
                let d = v["direction_of_search"].as_str().unwrap_or("");
                let d = DIRECTION_NAMES
                    .iter()
                    .position(|n| *n == d)
                    .ok_or("environment_scan: direction")?;
                let allowed = match &v["allowed_search_condition"] {
                    Value::Null => Predicate::True,
                    a => Predicate::parse(a, palette)?,
                };
                Ok(Self::Scan(
                    DIRS[d],
                    Predicate::parse(&v["target_condition"], palette)?,
                    allowed,
                    int("max_steps", 0),
                ))
            }
            "minecraft:random_chance" => {
                Ok(Self::Chance(v["chance"].as_f64().unwrap_or(0.0) as f32))
            }
            "minecraft:count" => Ok(Self::Count(IntP::parse(&v["count"])?)),
            "minecraft:in_square" => Ok(Self::InSquare),
            "minecraft:height_range" => {
                Ok(Self::Height(HeightP::parse(&v["height"], min_y, height)?))
            }
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
                        Some([
                            p.first()?.as_i64()? as i32,
                            p.get(1)?.as_i64()? as i32,
                            p.get(2)?.as_i64()? as i32,
                        ])
                    })
                    .collect(),
            )),
            "minecraft:count_on_every_layer" => Ok(Self::OnEveryLayer(IntP::parse(&v["count"])?)),
            "minecraft:cuboid" => Ok(Self::Cuboid {
                edges: v["include_edges"].as_bool().unwrap_or(true),
                interior: v["include_interior"].as_bool().unwrap_or(true),
                xz: IntP::parse(&v["xz_size"])?,
                y: IntP::parse(&v["y_size"])?,
            }),
            "minecraft:randomly_selected" => {
                let list = v["placements"]
                    .as_array()
                    .ok_or("randomly_selected: no placements")?;
                if list.is_empty() {
                    return Err("randomly_selected: empty".into());
                }
                Ok(Self::RandomlySelected(
                    list.iter()
                        .map(|m| Self::parse(m, palette, min_y, height))
                        .collect::<Result<_, _>>()?,
                ))
            }
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
    /// Фичи, которые не разобрались: имя и причина. Пустой список у полного
    /// набора; нужен, чтобы отчёт при старте называл дыры, а не только счёт.
    /// Имена фич по `steps[step][index]` — для профиля.
    names: Vec<Vec<String>>,
    failures: Vec<(String, String)>,
    pub palette: Palette,
    seed: i64,
    min_y: i32,
    pub(crate) structures: Option<Arc<structure::Structures>>,
}

/// Configured tree features reused by sapling growth in an already loaded world.
/// World decoration and live growth must place the same trunk, foliage and roots.
pub struct RuntimeTrees {
    features: Features,
    trees: HashMap<String, Feature>,
}

impl RuntimeTrees {
    pub fn load(
        data_dir: &Path,
        states: &States,
        default: &dyn Fn(&str) -> Option<u32>,
        min_y: i32,
        height: i32,
    ) -> Result<Self, String> {
        let mut palette = Palette::new(data_dir, states, default);
        let wg = data_dir.join("data/minecraft/worldgen");
        let read = |dir: &str, name: &str| -> Result<Value, String> {
            let path = name.strip_prefix("minecraft:").unwrap_or(name);
            let bytes = std::fs::read(wg.join(dir).join(format!("{path}.json")))
                .map_err(|e| format!("{dir} {name}: {e}"))?;
            serde_json::from_slice(&bytes).map_err(|e| format!("{dir} {name}: {e}"))
        };
        const NAMES: &[&str] = &[
            "oak", "fancy_oak", "spruce", "pine", "mega_spruce", "mega_pine",
            "birch", "jungle_tree", "mega_jungle_tree", "acacia", "dark_oak",
            "cherry", "pale_oak_bonemeal", "mangrove", "tall_mangrove",
            "red_poplar", "orange_poplar", "yellow_poplar",
        ];
        let mut trees = HashMap::new();
        for &name in NAMES {
            let value = read("feature", name)?;
            let feature = parse_feature(&value, &read, &mut palette, min_y, height, data_dir)?;
            if !matches!(feature, Feature::Tree(_)) {
                return Err(format!("feature {name} is not a tree"));
            }
            trees.insert(name.to_owned(), feature);
        }
        Ok(Self {
            features: Features {
                steps: Vec::new(), by_biome: HashMap::new(), has: HashMap::new(),
                failures: Vec::new(), names: Vec::new(), palette, seed: 0, min_y, structures: None,
            },
            trees,
        })
    }

    pub fn place(&self, name: &str, level: &mut dyn Level, origin: [i32; 3], seed: i64) -> bool {
        let Some(Feature::Tree(tree)) = self.trees.get(name) else { return false };
        tree.place(&self.features, level, &mut Worldgen::new(seed), origin)
    }
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
        palette.world_seed = seed;
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
            let i = id as usize;
            let climate = Arc::make_mut(&mut palette.climate);
            if climate.len() <= i {
                climate.resize(
                    i + 1,
                    Climate {
                        temperature: 2.0,
                        frozen: false,
                        precipitation: false,
                    },
                );
            }
            climate[i] = Climate {
                temperature: biome["temperature"].as_f64().unwrap_or(0.5) as f32,
                frozen: biome["temperature_modifier"].as_str() == Some("frozen"),
                precipitation: biome["has_precipitation"].as_bool().unwrap_or(false),
            };
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
        let mut failures: Vec<(String, String)> = Vec::new();
        for (step, names) in order.iter().enumerate() {
            let mut list = Vec::with_capacity(names.len());
            for (i, name) in names.iter().enumerate() {
                global.insert((step, name.as_str()), i as u32);
                let placed = read("placed_feature", name)
                    .and_then(|v| parse_placed(&v, &read, &mut palette, min_y, height, data_dir));
                if let Err(e) = &placed {
                    // `CHUNKWAKE_FEATURE_ERRORS=1` — те же причины в stderr.
                    if std::env::var_os("CHUNKWAKE_FEATURE_ERRORS").is_some() {
                        eprintln!("{name}: {e}");
                    }
                    failures.push((name.clone(), e.clone()));
                }
                // `CHUNKWAKE_ONLY_FEATURES=a,b` — только эти фичи (для сверки).
                let placed = match std::env::var("CHUNKWAKE_ONLY_FEATURES") {
                    Ok(only) if !only.split(',').any(|n| name.ends_with(n)) => Err(String::new()),
                    _ => placed,
                };
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
        let structures =
            structure::Structures::load(
                data_dir, &mut palette, biome_order, biome_ids, &read, min_y, height,
            )?;
        failures.extend(structures.failures().iter().cloned());
        Ok(Self {
            steps,
            by_biome,
            has,
            failures,
            names: order.clone(),
            palette,
            seed,
            min_y,
            structures: Some(Arc::new(structures)),
        })
    }

    /// Фичи, которые не разобрались: имя и причина.
    pub fn failures(&self) -> &[(String, String)] {
        &self.failures
    }

    /// Сколько фич каждого шага перенесено — для отчёта при старте.
    pub fn coverage(&self) -> (usize, usize) {
        let all = self.steps.iter().map(Vec::len).sum();
        let done = self.steps.iter().flatten().filter(|p| p.is_some()).count();
        (done, all)
    }

    /// `applyBiomeDecoration` чанка `(cx, cz)`; `biomes` — биомы, что есть
    /// в секциях региона 3×3.
    pub(crate) fn decorate(
        &self,
        level: &mut dyn Level,
        cx: i32,
        cz: i32,
        biomes: &HashSet<u32>,
        columns: Option<&dyn structure::Columns>,
    ) {
        let (ox, oz) = (cx * 16, cz * 16);
        let mut r = Worldgen::new(0);
        let seed = r.set_decoration_seed(self.seed, ox, oz);
        // `CHUNKWAKE_PROF=1` — время по шагам декорации в stderr.
        let prof = std::env::var_os("CHUNKWAKE_PROF").is_some();
        // `CHUNKWAKE_FEATURE_TRACE=1` — какой индекс и сид у фичи.
        let trace = std::env::var_os("CHUNKWAKE_FEATURE_TRACE").is_some();
        for step in 0..self.steps.len().min(DECORATION_STEPS) {
            // Куски структур шага пишутся до его фич (крепость Ада — шаг 7,
            // `underground_decoration`). У каждой структуры свой `setFeatureSeed`.
            if let (Some(structures), Some(columns)) = (&self.structures, columns)
            {
                structures.place(self, level, cx, cz, columns, &mut r, seed, step as i32);
            }
            let t_step = prof.then(Instant::now);
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
                if trace {
                    eprintln!("step {step} index {index}");
                }
                r.set_feature_seed(seed, index as i32, step as i32);
                let key = Some((step as u32, index));
                let t_feature = prof.then(Instant::now);
                self.place(level, placed, key, &mut r, [ox, self.min_y, oz]);
                if let Some(t) = t_feature {
                    let us = t.elapsed().as_micros();
                    if us > 300 {
                        eprintln!("    feature {}: {us} мкс", self.names[step][index as usize]);
                    }
                }
            }
            if let Some(t) = t_step {
                let us = t.elapsed().as_micros();
                if us > 500 {
                    eprintln!("  step {step}: {us} мкс");
                }
            }
        }
    }

    /// `PlacedFeature.place` с ответом «поставлено ли»: успех хоть одной
    /// точки. Честный ответ дают дерево, `sculk_patch`, `simple_block`,
    /// `sequence`, `overlay` и выборщики; остальные фичи считаются успехом.
    fn place_ok(
        &self,
        level: &mut dyn Level,
        placed: &Placed,
        r: &mut Worldgen,
        origin: [i32; 3],
    ) -> bool {
        self.place(level, placed, None, r, origin)
    }

    /// Уже размещённая фича в готовой точке — без повторных модификаторов.
    /// Нужна вложенным фичам (`coral_tree`), где точка вычислена внешней фичей.
    fn place_feature_at(
        &self,
        level: &mut dyn Level,
        placed: &Placed,
        r: &mut Worldgen,
        pos: [i32; 3],
    ) {
        self.place(level, placed, None, r, pos);
    }

    /// `FeaturePlacer.placeWithBiomeCheck`: модификаторы в глубину, по порядку.
    fn place(
        &self,
        level: &mut dyn Level,
        placed: &Placed,
        key: Option<(u32, u32)>,
        r: &mut Worldgen,
        origin: [i32; 3],
    ) -> bool {
        self.modify(level, placed, key, r, origin, 0)
    }

    /// `CountOnEveryLayerPlacement.findOnGroundYPosition`: `layer`-й сверху
    /// пол (непустой блок под пустым, не бедрок); пустые — воздух, вода, лава.
    fn layer_floor(&self, level: &dyn Level, x: i32, top: i32, z: i32, layer: i32) -> Option<i32> {
        let pal = &self.palette;
        let empty = |s: u32| pal.is_air(s) || pal.is_liquid(s);
        let mut current = level.get(x, top, z);
        let mut seen = 0;
        for y in (level.min_y() + 1..=top).rev() {
            let below = level.get(x, y - 1, z);
            if !empty(below) && empty(current) && pal.block_name(below) != Some("minecraft:bedrock") {
                if seen == layer {
                    return Some(y);
                }
                seen += 1;
            }
            current = below;
        }
        None
    }

    fn modify(
        &self,
        level: &mut dyn Level,
        placed: &Placed,
        key: Option<(u32, u32)>,
        r: &mut Worldgen,
        pos: [i32; 3],
        at: usize,
    ) -> bool {
        let Some(m) = placed.placement.get(at) else {
            return self.feature(level, &placed.feature, r, pos);
        };
        let next = at + 1;
        let [x, y, z] = pos;
        let mut ok = false;
        match m {
            Modifier::Count(n) => {
                let n = n.sample(r);
                // Все копии в стеке до развёртывания: счёт бросается один раз.
                for _ in 0..n {
                    ok |= self.modify(level, placed, key, r, pos, next);
                }
            }
            Modifier::InSquare => {
                let nx = r.next_int_bound(16) + x;
                let nz = r.next_int_bound(16) + z;
                ok |= self.modify(level, placed, key, r, [nx, y, nz], next);
            }
            Modifier::Height(h) => {
                let ny = h.sample(r);
                ok |= self.modify(level, placed, key, r, [x, ny, z], next);
            }
            Modifier::Biome => {
                let b = level.biome(x, y, z);
                if key.is_some_and(|key| self.has.get(&b).is_some_and(|s| s.contains(&key))) {
                    ok |= self.modify(level, placed, key, r, pos, next);
                }
            }
            Modifier::Rarity(chance) => {
                if r.next_float() < 1.0 / *chance as f32 {
                    ok |= self.modify(level, placed, key, r, pos, next);
                }
            }
            Modifier::Heightmap(kind) => {
                let ny = level.surface(*kind, x, z);
                if ny > level.min_y() {
                    ok |= self.modify(level, placed, key, r, [x, ny, z], next);
                }
            }
            Modifier::Scan(dir, target, allowed, steps) => {
                let pal = &self.palette;
                if !allowed.test(&*level, pal, pos) {
                    return ok;
                }
                let (lo, hi) = (level.min_y(), level.min_y() + level.height());
                let mut at = pos;
                for _ in 0..*steps {
                    if target.test(&*level, pal, at) {
                        ok |= self.modify(level, placed, key, r, at, next);
                        return ok;
                    }
                    at = add(at, *dir);
                    if at[1] < lo || at[1] >= hi {
                        return ok;
                    }
                    if !allowed.test(&*level, pal, at) {
                        break;
                    }
                }
                if target.test(&*level, pal, at) {
                    ok |= self.modify(level, placed, key, r, at, next);
                }
            }
            Modifier::Chance(chance) => {
                if r.next_float() < *chance {
                    ok |= self.modify(level, placed, key, r, pos, next);
                }
            }
            Modifier::NoiseThreshold(level_at, below, above) => {
                let noise = biome_info_noise().get(x as f64 / 200.0, z as f64 / 200.0);
                let n = if (noise as f64) < *level_at {
                    *below
                } else {
                    *above
                };
                for _ in 0..n {
                    ok |= self.modify(level, placed, key, r, pos, next);
                }
            }
            Modifier::NoiseBased(ratio, factor, offset) => {
                let noise = biome_info_noise().get(x as f64 / factor, z as f64 / factor);
                let n = ((noise as f64 + offset) * *ratio as f64).ceil() as i32;
                for _ in 0..n {
                    ok |= self.modify(level, placed, key, r, pos, next);
                }
            }
            Modifier::OnEveryLayer(count) => {
                // `CountOnEveryLayerPlacement`: счёт бросается заново на каждой
                // итерации (`i < count.sample(random)`). `FeaturePlacer` сначала
                // собирает все точки модификатора, потом обходит их по порядку.
                let mut out = Vec::new();
                let mut layer = 0;
                loop {
                    let mut found = false;
                    let mut i = 0;
                    while i < count.sample(r) {
                        let nx = r.next_int_bound(16) + x;
                        let nz = r.next_int_bound(16) + z;
                        let top = level.surface(Heightmap::MotionBlocking, nx, nz);
                        if let Some(ny) = self.layer_floor(&*level, nx, top, nz, layer) {
                            out.push([nx, ny, nz]);
                            found = true;
                        }
                        i += 1;
                    }
                    if !found {
                        break;
                    }
                    layer += 1;
                }
                for p in out {
                    ok |= self.modify(level, placed, key, r, p, next);
                }
            }
            Modifier::Cuboid {
                edges,
                interior,
                xz,
                y: ysize,
            } => {
                // `CuboidPlacement`: точка на полом коробе вокруг центра.
                let sx = xz.sample(r).max(1);
                let sy = ysize.sample(r).max(1);
                let (hx, hy) = (sx / 2, sy / 2);
                let mut chosen = [x, y, z];
                // Грани короба и его нутро; флаги говорят, что включать.
                for _ in 0..16 {
                    let dx = r.next_int_bound(sx + 1) - hx;
                    let dz = r.next_int_bound(sx + 1) - hx;
                    let dy = r.next_int_bound(sy + 1) - hy;
                    let on_edge = dx.abs() == hx || dz.abs() == hx || dy.abs() == hy;
                    if (*edges && on_edge) || (*interior && !on_edge) {
                        chosen = [x + dx, y + dy, z + dz];
                        break;
                    }
                }
                ok |= self.modify(level, placed, key, r, chosen, next);
            }
            Modifier::Offset(dx, dy, dz) => {
                let (a, b) = (dx.sample(r), dy.sample(r));
                let c = dz.sample(r);
                ok |= self.modify(level, placed, key, r, [x + a, y + b, z + c], next);
            }
            Modifier::Filter(p) => {
                if p.test(&*level, &self.palette, pos) {
                    ok |= self.modify(level, placed, key, r, pos, next);
                }
            }
            Modifier::WaterDepth(max) => {
                let floor = level.surface(Heightmap::OceanFloor, x, z);
                let top = level.surface(Heightmap::WorldSurface, x, z);
                if top - floor <= *max {
                    ok |= self.modify(level, placed, key, r, pos, next);
                }
            }
            Modifier::Relative(kind, min, max) => {
                let h = level.surface(*kind, x, z) as i64;
                if h + *min as i64 <= y as i64 && y as i64 <= h + *max as i64 {
                    ok |= self.modify(level, placed, key, r, pos, next);
                }
            }
            Modifier::Fixed(list) => {
                for p in list {
                    // Только точки внутри этого чанка (`FixedPlacement`).
                    if p[0] >> 4 == x >> 4 && p[2] >> 4 == z >> 4 {
                        ok |= self.modify(level, placed, key, r, *p, next);
                    }
                }
            }
            Modifier::RandomlySelected(list) => {
                let i = r.next_int_bound(list.len() as i32) as usize;
                let mut placement = Vec::with_capacity(1 + placed.placement.len() - next);
                placement.push(list[i].clone());
                placement.extend_from_slice(&placed.placement[next..]);
                let tmp = Placed {
                    feature: placed.feature.clone(),
                    placement,
                };
                ok |= self.modify(level, &tmp, key, r, pos, 0);
            }
        }
        ok
    }

    /// `ConfiguredFeature.place`: ответ фичи там, где он влияет на ход
    /// генерации (`SequenceFeature` останавливается на первом отказе).
    fn feature(&self, level: &mut dyn Level, f: &Feature, r: &mut Worldgen, pos: [i32; 3]) -> bool {
        match f {
            Feature::Tree(t) => t.place(self, level, r, pos),
            Feature::SculkPatch(s) => s.place(self, level, r, pos),
            Feature::SimpleBlock(p) => self.simple_block(level, p, r, pos),
            Feature::Sequence(list) => list.iter().all(|p| self.place(level, p, None, r, pos)),
            Feature::Overlay(list) => {
                let mut any = false;
                for p in list {
                    any |= self.place(level, p, None, r, pos);
                }
                any
            }
            Feature::RandomSelector(list, default) => {
                for (chance, placed) in list {
                    if r.next_float() < *chance {
                        return self.place(level, placed, None, r, pos);
                    }
                }
                self.place(level, default, None, r, pos)
            }
            Feature::WeightedSelector(total, list) => {
                if *total <= 0 {
                    return false;
                }
                let mut i = r.next_int_bound(*total);
                for (w, placed) in list {
                    if i < *w {
                        return self.place(level, placed, None, r, pos);
                    }
                    i -= w;
                }
                false
            }
            Feature::BooleanSelector(yes, no) => {
                let f = if r.next_bool() { yes } else { no };
                self.place(level, f, None, r, pos)
            }
            _ => {
                self.feature_rest(level, f, r, pos);
                true
            }
        }
    }

    fn feature_rest(&self, level: &mut dyn Level, f: &Feature, r: &mut Worldgen, pos: [i32; 3]) {
        match f {
            Feature::Ore(ore) => self.ore(level, ore, r, pos),
            Feature::ScatteredOre(ore) => self.scattered_ore(level, ore, r, pos),
            // Ответ этих фич нужен ходу генерации — их ставит `feature`.
            Feature::SimpleBlock(_)
            | Feature::RandomSelector(..)
            | Feature::WeightedSelector(..)
            | Feature::BooleanSelector(..)
            | Feature::Tree(_)
            | Feature::Sequence(_)
            | Feature::Overlay(_)
            | Feature::SculkPatch(_) => unreachable!("фича с ответом в feature_rest"),
            Feature::BlockPile(p) => self.block_pile(level, p, r, pos),
            Feature::VegetationPatch(patch) => self.vegetation_patch(level, patch, r, pos),
            Feature::FallenTree(t) => t.place(self, level, r, pos),
            Feature::HugeMushroom(m) => self.huge_mushroom(level, m, r, pos),
            Feature::Disk(d) => d.place(self, level, r, pos),
            Feature::Spring(s) => s.place(self, level, pos),
            Feature::Vines => misc::vines(self, level, pos),
            Feature::Blob(b) => b.place(self, level, r, pos),
            Feature::BlueIce => misc::blue_ice(self, level, r, pos),
            Feature::Lake(l) => l.place(self, level, r, pos),
            Feature::MonsterRoom(m) => m.place(self, level, r, pos),
            Feature::Geode(g) => g.place(self, level, r, pos),
            Feature::RootSystem(s) => s.place(self, level, r, pos),
            Feature::FreezeTopLayer => misc::freeze_top_layer(self, level, pos),
            Feature::UnderwaterMagma(m) => m.place(self, level, r, pos),
            Feature::Bamboo(chance) => misc::bamboo(self, level, r, pos, *chance),
            Feature::IceSpike(s) => s.place(self, level, r, pos),
            Feature::Iceberg(i) => {
                // `CHUNKWAKE_ICE_TRACE=1` — какой айсберг и где: по имени
                // состояния видно, `packed` это или `blue`.
                if std::env::var_os("CHUNKWAKE_ICE_TRACE").is_some() {
                    eprintln!("iceberg {} at {pos:?}", i.state_name());
                }
                i.place(self, level, r, pos)
            }
            Feature::CoralTree(kind, inner) => misc::coral(self, level, r, pos, kind, inner),
            Feature::NoOp => {}
            Feature::VoidPlatform(stone, cobble) => {
                place_void_platform(level, pos, *stone, *cobble);
            }
            Feature::Multiface(m) => m.place(self, level, r, pos),
            Feature::Speleothem(s) => s.place(self, level, r, pos),
            Feature::LargeDripstone(d) => d.place(self, level, r, pos),
            Feature::SpeleothemCluster(s) => s.place(self, level, r, pos),
            Feature::BlockColumn(layers, dir, allowed, tip) => {
                self.block_column(level, layers, *dir, allowed, *tip, r, pos)
            }
            Feature::SimpleRandomSelector(list) => {
                let i = r.next_int_bound(list.len() as i32) as usize;
                self.place(level, &list[i], None, r, pos);
            }
            Feature::Template(t) => t.place(&self.palette, level, r, pos),
            Feature::Fossil(f) => f.place(&self.palette, level, r, pos),
            Feature::Pillar(p) => p.place(self, level, r, pos),
            Feature::Delta(d) => d.place(self, level, r, pos),
            Feature::ColumnCluster(c) => c.place(self, level, r, pos),
            Feature::ReplaceBlobs(b) => b.place(self, level, r, pos),
            Feature::NeighborSpread(n) => n.place(self, level, r, pos),
            Feature::HugeFungus(g) => g.place(self, level, r, pos),
            Feature::PatchySquare(p) => p.place(self, level, r, pos),
            Feature::EndIsland(b) => end::island(b, level, r, pos),
            Feature::EndPlatform(b) => end::platform(b, &self.palette, level, pos),
            Feature::EndGateway(b, g) => g.place(b, level, pos),
            Feature::EndSpike(b, s) => s.place(self, b, level, r, pos),
            Feature::Chorus(b) => end::chorus(&self.palette, b, level, r, pos),
        }
    }

    /// `VegetationPatchFeature.place`; множества точек обходятся в порядке
    /// `HashSet` Java.
    fn vegetation_patch(
        &self,
        level: &mut dyn Level,
        p: &Patch,
        r: &mut Worldgen,
        origin: [i32; 3],
    ) {
        let pal = &self.palette;
        let xr = p.radius.sample(r) + 1;
        let zr = p.radius.sample(r) + 1;
        let inwards = DIRS[p.inwards];
        let outwards = DIRS[p.inwards ^ 1];
        let mut surface = Vec::new();
        for dx in -xr..=xr {
            let x_edge = dx == -xr || dx == xr;
            for dz in -zr..=zr {
                let z_edge = dz == -zr || dz == zr;
                let corner = x_edge && z_edge;
                let edge = (x_edge || z_edge) && !corner;
                if corner || (edge && (p.extra_edge == 0.0 || r.next_float() > p.extra_edge)) {
                    continue;
                }
                let mut pos = add(origin, [dx, 0, dz]);
                let mut i = 0;
                while i < p.vertical_range && pal.is_air(level.get(pos[0], pos[1], pos[2])) {
                    pos = add(pos, inwards);
                    i += 1;
                }
                let mut i = 0;
                while i < p.vertical_range && !pal.is_air(level.get(pos[0], pos[1], pos[2])) {
                    pos = add(pos, outwards);
                    i += 1;
                }
                let mut below = add(pos, inwards);
                let below_state = level.get(below[0], below[1], below[2]);
                if !pal.is_air(level.get(pos[0], pos[1], pos[2]))
                    || !pal.sturdy(below_state, p.inwards ^ 1)
                {
                    continue;
                }
                let depth = p.depth.sample(r)
                    + i32::from(p.extra_bottom > 0.0 && r.next_float() < p.extra_bottom);
                let ground = below;
                // `placeGround`.
                let mut placed = true;
                for i in 0..depth {
                    let Some(state) = p.ground.get(&*level, pal, r, below) else {
                        continue;
                    };
                    let current = level.get(below[0], below[1], below[2]);
                    if pal.same_block(state, current) {
                        continue;
                    }
                    if !Palette::flag(&p.replaceable, current) {
                        placed = i != 0;
                        break;
                    }
                    if level.can_write(below[0], below[2]) {
                        level.set(below[0], below[1], below[2], state);
                    }
                    below = add(below, inwards);
                }
                if placed {
                    surface.push(ground);
                }
            }
        }
        let mut surface = java_set_order(surface);
        if p.waterlogged {
            let exposed = |level: &dyn Level, at: [i32; 3]| {
                [2usize, 5, 3, 4, 0].iter().any(|&d| {
                    let [x, y, z] = add(at, DIRS[d]);
                    !pal.sturdy(level.get(x, y, z), d ^ 1)
                })
            };
            surface = java_set_order(
                surface
                    .into_iter()
                    .filter(|&at| !exposed(&*level, at))
                    .collect(),
            );
            let water = pal.defaults.get("minecraft:water").copied().unwrap_or(0);
            for &[x, y, z] in &surface {
                if level.can_write(x, z) {
                    level.set(x, y, z, water);
                }
            }
        }
        for at in surface {
            if p.vegetation_chance > 0.0 && r.next_float() < p.vegetation_chance {
                if p.waterlogged {
                    self.place(level, &p.vegetation, None, r, at);
                    let s = level.get(at[0], at[1], at[2]);
                    if pal.prop(s, "waterlogged") == Some("false")
                        && let Some(wet) = pal.with_prop(s, "waterlogged", "true")
                        && level.can_write(at[0], at[2])
                    {
                        level.set(at[0], at[1], at[2], wet);
                    }
                } else {
                    self.place(level, &p.vegetation, None, r, add(at, outwards));
                }
            }
        }
    }

    /// `AbstractHugeMushroomFeature.place`.
    fn huge_mushroom(&self, level: &mut dyn Level, m: &Mushroom, r: &mut Worldgen, o: [i32; 3]) {
        let pal = &self.palette;
        let mut h = r.next_int_bound(3) + 4;
        if r.next_int_bound(12) == 0 {
            h *= 2;
        }
        let top = level.min_y() + level.height() - 1;
        if o[1] < level.min_y() + 1
            || o[1] + h + 1 > top
            || !m.on.test(&*level, pal, add(o, [0, -1, 0]))
        {
            return;
        }
        for dy in 0..=h {
            // У красного радиус берётся с высотой -1 — проверяется только ствол.
            let radius = if m.red || dy <= 3 { 0 } else { m.radius };
            for dx in -radius..=radius {
                for dz in -radius..=radius {
                    let s = level.get(o[0] + dx, o[1] + dy, o[2] + dz);
                    if !pal.is_air(s) && !Palette::flag(&m.leaves, s) {
                        return;
                    }
                }
            }
        }
        let put = |level: &mut dyn Level, [x, y, z]: [i32; 3], state: u32| {
            let cur = level.get(x, y, z);
            if (pal.is_air(cur) || Palette::flag(&m.replaceable, cur)) && level.can_write(x, z) {
                level.set(x, y, z, state);
            }
        };
        let faces = |mut s: u32, list: &[(&str, bool)]| {
            if list.iter().all(|(k, _)| pal.prop(s, k).is_some()) {
                for (k, v) in list {
                    s = pal
                        .with_prop(s, k, if *v { "true" } else { "false" })
                        .unwrap_or(s);
                }
            }
            s
        };
        let rad = m.radius;
        if m.red {
            for dy in h - 3..=h {
                let radius = if dy < h { rad } else { rad - 1 };
                let center = rad - 2;
                for dx in -radius..=radius {
                    for dz in -radius..=radius {
                        let x_edge = dx == -radius || dx == radius;
                        let z_edge = dz == -radius || dz == radius;
                        if dy >= h || x_edge != z_edge {
                            let Some(s) = m.cap.get(&*level, pal, r, o) else {
                                continue;
                            };
                            let s = faces(
                                s,
                                &[
                                    ("up", dy >= h - 1),
                                    ("west", dx < -center),
                                    ("east", dx > center),
                                    ("north", dz < -center),
                                    ("south", dz > center),
                                ],
                            );
                            put(level, add(o, [dx, dy, dz]), s);
                        }
                    }
                }
            }
        } else {
            for dx in -rad..=rad {
                for dz in -rad..=rad {
                    let (min_x, max_x, min_z, max_z) =
                        (dx == -rad, dx == rad, dz == -rad, dz == rad);
                    let (x_edge, z_edge) = (min_x || max_x, min_z || max_z);
                    if x_edge && z_edge {
                        continue;
                    }
                    let Some(s) = m.cap.get(&*level, pal, r, o) else {
                        continue;
                    };
                    let s = faces(
                        s,
                        &[
                            ("west", min_x || z_edge && dx == 1 - rad),
                            ("east", max_x || z_edge && dx == rad - 1),
                            ("north", min_z || x_edge && dz == 1 - rad),
                            ("south", max_z || x_edge && dz == rad - 1),
                        ],
                    );
                    put(level, add(o, [dx, h, dz]), s);
                }
            }
        }
        for dy in 0..h {
            if let Some(s) = m.stem.get(&*level, pal, r, o) {
                put(level, add(o, [0, dy, 0]), s);
            }
        }
    }

    /// `BlockColumnFeature.place`: проверка идёт со сдвигом на клетку вперёд.
    #[allow(clippy::too_many_arguments)]
    fn block_column(
        &self,
        level: &mut dyn Level,
        layers: &[(IntP, Provider)],
        dir: [i32; 3],
        allowed: &Predicate,
        tip: bool,
        r: &mut Worldgen,
        pos: [i32; 3],
    ) {
        let mut heights: Vec<i32> = layers.iter().map(|l| l.0.sample(r)).collect();
        let total: i32 = heights.iter().sum();
        if total == 0 {
            return;
        }
        let mut next = add(pos, dir);
        for y in 0..total {
            if !allowed.test(&*level, &self.palette, next) {
                let mut remove = total - y;
                let order: Vec<usize> = if tip {
                    (0..heights.len()).collect()
                } else {
                    (0..heights.len()).rev().collect()
                };
                for i in order {
                    if remove <= 0 {
                        break;
                    }
                    let take = heights[i].min(remove);
                    remove -= take;
                    heights[i] -= take;
                }
                break;
            }
            next = add(next, dir);
        }
        let mut at = pos;
        for (i, (_, provider)) in layers.iter().enumerate() {
            for _ in 0..heights[i] {
                if let Some(s) = provider.get(&*level, &self.palette, r, at)
                    && level.can_write(at[0], at[2])
                {
                    level.set(at[0], at[1], at[2], s);
                }
                at = add(at, dir);
            }
        }
    }

    /// `BlockPileFeature.place`: круг по `x` быстрее `y`, затем `z`.
    fn block_pile(
        &self,
        level: &mut dyn Level,
        provider: &Provider,
        r: &mut Worldgen,
        pos: [i32; 3],
    ) {
        if pos[1] < level.min_y() + 5 {
            return;
        }
        let wx = 2 + r.next_int_bound(2);
        let wz = 2 + r.next_int_bound(2);
        let (x0, x1) = (pos[0] - wx, pos[0] + wx);
        let (y0, y1) = (pos[1], pos[1] + 1);
        let (z0, z1) = (pos[2] - wz, pos[2] + wz);
        let (sx, sy, sz) = (x1 - x0 + 1, y1 - y0 + 1, z1 - z0 + 1);
        let n = sx * sy * sz;
        let path = self.palette.blocks.get("minecraft:dirt_path").map(|b| b.0);
        for index in 0..n {
            let dx = index % sx;
            let div = index / sx;
            let dy = div % sy;
            let dz = div / sy;
            let at = [x0 + dx, y0 + dy, z0 + dz];
            let ox = pos[0] - at[0];
            let oz = pos[2] - at[2];
            let dist = (ox * ox + oz * oz) as f32;
            let threshold = r.next_float() * 10.0 - r.next_float() * 6.0;
            let place = if dist <= threshold {
                true
            } else {
                (r.next_float() as f64) < 0.031
            };
            if place {
                self.try_pile(level, provider, r, at, path);
            }
        }
    }

    fn try_pile(
        &self,
        level: &mut dyn Level,
        provider: &Provider,
        r: &mut Worldgen,
        at: [i32; 3],
        path: Option<u32>,
    ) {
        if !self.palette.is_air(level.get(at[0], at[1], at[2])) {
            return;
        }
        let below = [at[0], at[1] - 1, at[2]];
        let ground = level.get(below[0], below[1], below[2]);
        let ok = if path.is_some_and(|id| self.palette.block_index(ground) == Some(id)) {
            r.next_bool()
        } else {
            self.palette.sturdy(ground, 1)
        };
        if !ok {
            return;
        }
        let Some(state) = provider.get(&*level, &self.palette, r, at) else {
            return;
        };
        if level.can_write(at[0], at[2]) {
            level.set(at[0], at[1], at[2], state);
        }
    }

    /// `SimpleBlockFeature.place`.
    fn simple_block(&self, level: &mut dyn Level, p: &Provider, r: &mut Worldgen, pos: [i32; 3]) -> bool {
        let pal = &self.palette;
        let Some(state) = p.get(&*level, pal, r, pos) else {
            return false;
        };
        if !can_survive(&*level, pal, state, pos) {
            return false;
        }
        let [x, y, z] = pos;
        if !level.can_write(x, z) {
            return false;
        }
        if matches!(
            pal.survive.get(state as usize),
            Some(Survive::Double | Survive::TallSeagrass)
        ) {
            let above = level.get(x, y + 1, z);
            if !pal.is_air(above)
                && (pal.fluid(state) != pal.fluid(above) || !Palette::flag(&pal.replaceable, above))
            {
                return false;
            }
            // `DoublePlantBlock.placeAt`: обе половины, вода из мира.
            let half = |half: &str, at: [i32; 3]| {
                let s = pal.with_prop(state, "half", half).unwrap_or(state);
                let wet = pal.fluid(level.get(at[0], at[1], at[2])) == Fluid::Water;
                pal.with_prop(s, "waterlogged", if wet { "true" } else { "false" })
                    .unwrap_or(s)
            };
            let (lower, upper) = (half("lower", pos), half("upper", [x, y + 1, z]));
            level.set(x, y, z, lower);
            level.set(x, y + 1, z, upper);
        } else {
            level.set(x, y, z, state);
        }
        true
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
            (zs..=zs + size_xz).any(|z| ys <= level.surface(Heightmap::OceanFloorWg, x, z))
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
            let rad = (((crate::carver::sin((std::f32::consts::PI * step) as f64) + 1.0) as f64)
                * ss
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
                        let bit =
                            (x - xs + (y - ys) * size_xz + (z - zs) * size_xz * size_y) as usize;
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
    fn scattered_ore(
        &self,
        level: &mut dyn Level,
        ore: &Ore,
        r: &mut Worldgen,
        [ox, oy, oz]: [i32; 3],
    ) {
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

/// `LevelChunk.postProcessGeneration` для пузырьков: `magma_block` и
/// `soul_sand` (`postProcessAbove`) помечают клетку над собой, и вода-источник
/// там по `LiquidBlock.tick` становится `bubble_column`
/// (`BubbleColumnBlock.updateColumn`) до первого не-источника вверх.
/// Состояния чанка — `(y * 16 + z) * 16 + x`.
pub(crate) fn bubble_columns(pal: &Palette, states: &mut [u32]) {
    let get = |n: &str| pal.defaults.get(n).copied();
    let (Some(water), Some(column), Some(magma), Some(soul)) = (
        get("minecraft:water"),
        get("minecraft:bubble_column"),
        get("minecraft:magma_block"),
        get("minecraft:soul_sand"),
    ) else {
        return;
    };
    let (Some(down), Some(up)) = (
        pal.with_prop(column, "drag", "true"),
        pal.with_prop(column, "drag", "false"),
    ) else {
        return;
    };
    let height = states.len() / 256;
    for i in 0..256 {
        for y in 0..height.saturating_sub(1) {
            let below = states[y * 256 + i];
            let fill = if below == magma {
                down
            } else if below == soul {
                up
            } else {
                continue;
            };
            let mut at = y + 1;
            while at < height && (states[at * 256 + i] == water || pal.block_name(states[at * 256 + i]) == Some("minecraft:bubble_column")) {
                states[at * 256 + i] = fill;
                at += 1;
            }
        }
    }
}

type Read<'a> = dyn Fn(&str, &str) -> Result<Value, String> + 'a;

/// Порядок обхода `HashSet<BlockPos>` Java после вставки точек по порядку:
/// корзины по `hashCode` при итоговой ёмкости, внутри корзины — порядок
/// вставки (деревья корзин не моделируются).
fn java_set_order(points: Vec<[i32; 3]>) -> Vec<[i32; 3]> {
    let mut seen = HashSet::new();
    let points: Vec<[i32; 3]> = points.into_iter().filter(|p| seen.insert(*p)).collect();
    let mut cap = 16usize;
    while points.len() > cap * 3 / 4 {
        cap *= 2;
    }
    let bucket = |&[x, y, z]: &[i32; 3]| {
        let h = (y.wrapping_add(z.wrapping_mul(31)))
            .wrapping_mul(31)
            .wrapping_add(x);
        let h = h ^ ((h as u32) >> 16) as i32;
        h as u32 as usize & (cap - 1)
    };
    let mut order: Vec<(usize, usize)> = points
        .iter()
        .enumerate()
        .map(|(i, p)| (bucket(p), i))
        .collect();
    order.sort_unstable();
    order.into_iter().map(|(_, i)| points[i]).collect()
}

/// `PlacementUtils.BIOME_INFO_NOISE`: шум счётчиков по шуму биома.
fn biome_info_noise() -> &'static Simplex {
    static NOISE: std::sync::OnceLock<Simplex> = std::sync::OnceLock::new();
    NOISE.get_or_init(|| Simplex::new(&mut Legacy::new(2345), true))
}

/// Направления в порядке `Direction`: низ, верх, север, юг, запад, восток.
const DIRS: [[i32; 3]; 6] = [
    [0, -1, 0],
    [0, 1, 0],
    [0, 0, -1],
    [0, 0, 1],
    [-1, 0, 0],
    [1, 0, 0],
];
const DIRECTION_NAMES: [&str; 6] = ["down", "up", "north", "south", "west", "east"];

fn offset(v: &Value) -> [i32; 3] {
    let mut o = [0; 3];
    for (i, c) in v["offset"]
        .as_array()
        .into_iter()
        .flatten()
        .take(3)
        .enumerate()
    {
        o[i] = c.as_i64().unwrap_or(0) as i32;
    }
    o
}

fn add(a: [i32; 3], b: [i32; 3]) -> [i32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// Список блоков или тег (`HolderSet<Block>`) как маска по состояниям.
fn block_set(v: &Value, palette: &mut Palette) -> Result<Arc<Vec<bool>>, String> {
    if let Some(tag) = v.as_str().and_then(|s| s.strip_prefix('#')) {
        return palette.tag(tag);
    }
    let names: Vec<&str> = match v {
        Value::String(s) => vec![s.as_str()],
        _ => v
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect(),
    };
    let mut ids = HashSet::new();
    for n in names {
        ids.insert(palette.block(n)?);
    }
    Ok(Arc::new(
        palette.block_of.iter().map(|b| ids.contains(b)).collect(),
    ))
}

/// `BlockPredicate` со сдвигом от проверяемой точки.
#[derive(Clone, Debug)]
enum Predicate {
    True,
    Blocks([i32; 3], Arc<Vec<bool>>),
    Fluids([i32; 3], Vec<Fluid>),
    Survive([i32; 3], u32),
    Solid([i32; 3]),
    Replaceable([i32; 3]),
    Sturdy([i32; 3], usize),
    InsideWorld([i32; 3]),
    AllOf(Vec<Predicate>),
    AnyOf(Vec<Predicate>),
    Not(Box<Predicate>),
    /// `volume_match`: каждая клетка `min`…`max` проходит вложенный предикат.
    Volume([i32; 3], [i32; 3], Box<Predicate>),
    /// `height_range`: y между якорями включительно.
    HeightRange(Anchor, Anchor),
}

/// `VerticalAnchor`, который разрешается по уровню (`WorldGenerationContext`).
#[derive(Clone, Copy, Debug)]
enum Anchor {
    Absolute(i32),
    AboveBottom(i32),
    BelowTop(i32),
    RelativeToSea(i32),
}

impl Anchor {
    fn parse(v: &Value) -> Result<Self, String> {
        let i = |k: &str| v[k].as_i64().map(|n| n as i32);
        if let Some(y) = i("absolute") {
            Ok(Self::Absolute(y))
        } else if let Some(o) = i("above_bottom") {
            Ok(Self::AboveBottom(o))
        } else if let Some(o) = i("below_top") {
            Ok(Self::BelowTop(o))
        } else if let Some(o) = i("relative_to_sea_level") {
            Ok(Self::RelativeToSea(o))
        } else {
            Err(format!("vertical anchor {v} is not supported"))
        }
    }

    fn resolve(self, level: &dyn Level) -> i32 {
        match self {
            Self::Absolute(y) => y,
            Self::AboveBottom(o) => level.min_y() + o,
            Self::BelowTop(o) => level.min_y() + level.height() - 1 - o,
            Self::RelativeToSea(o) => level.sea_level() + o,
        }
    }
}

impl Predicate {
    fn parse(v: &Value, palette: &mut Palette) -> Result<Self, String> {
        let o = offset(v);
        let list = |palette: &mut Palette| -> Result<Vec<Predicate>, String> {
            v["predicates"]
                .as_array()
                .ok_or("block predicate: no predicates")?
                .iter()
                .map(|p| Predicate::parse(p, palette))
                .collect()
        };
        match predicate_kind(v) {
            "minecraft:true" | "minecraft:always_true" => Ok(Self::True),
            "minecraft:volume_match" => {
                let inner = v
                    .get("match")
                    .or_else(|| v.get("predicate"))
                    .ok_or("volume_match: no match")?;
                Ok(Self::Volume(
                    ivec(&v["min"]),
                    ivec(&v["max"]),
                    Box::new(Self::parse(inner, palette)?),
                ))
            }
            "minecraft:matching_blocks" => Ok(Self::Blocks(o, block_set(&v["blocks"], palette)?)),
            "minecraft:matching_block_tag" => Ok(Self::Blocks(
                o,
                palette.tag(v["tag"].as_str().unwrap_or(""))?,
            )),
            "minecraft:matching_fluids" => {
                let names: Vec<&str> = match &v["fluids"] {
                    Value::String(s) => vec![s.as_str()],
                    f => f
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .collect(),
                };
                let mut fluids = Vec::new();
                for n in names {
                    fluids.extend_from_slice(match n {
                        "minecraft:empty" => &[Fluid::None][..],
                        "minecraft:water" => &[Fluid::Water],
                        "minecraft:flowing_water" => &[Fluid::FlowingWater],
                        "minecraft:lava" => &[Fluid::Lava],
                        "minecraft:flowing_lava" => &[Fluid::FlowingLava],
                        "#minecraft:water" => &[Fluid::Water, Fluid::FlowingWater],
                        "#minecraft:lava" => &[Fluid::Lava, Fluid::FlowingLava],
                        other => return Err(format!("fluid {other} is not supported")),
                    });
                }
                Ok(Self::Fluids(o, fluids))
            }
            "minecraft:would_survive" => Ok(Self::Survive(o, palette.state(&v["state"])?)),
            "minecraft:solid" => Ok(Self::Solid(o)),
            "minecraft:replaceable" => Ok(Self::Replaceable(o)),
            "minecraft:has_sturdy_face" => {
                let d = v["direction"].as_str().unwrap_or("");
                let d = DIRECTION_NAMES
                    .iter()
                    .position(|n| *n == d)
                    .ok_or_else(|| format!("direction {d}"))?;
                Ok(Self::Sturdy(o, d))
            }
            "minecraft:inside_world_bounds" => Ok(Self::InsideWorld(o)),
            "minecraft:height_range" => Ok(Self::HeightRange(
                Anchor::parse(&v["min_inclusive"])?,
                Anchor::parse(&v["max_inclusive"])?,
            )),
            "minecraft:all_of" => Ok(Self::AllOf(list(palette)?)),
            "minecraft:any_of" => Ok(Self::AnyOf(list(palette)?)),
            "minecraft:not" => Ok(Self::Not(Box::new(Self::parse(&v["predicate"], palette)?))),
            other => Err(format!("block predicate {other} is not supported")),
        }
    }

    fn test(&self, level: &dyn Level, pal: &Palette, pos: [i32; 3]) -> bool {
        let at = |o: &[i32; 3]| {
            let [x, y, z] = add(pos, *o);
            level.get(x, y, z)
        };
        match self {
            Self::True => true,
            Self::Blocks(o, mask) => Palette::flag(mask, at(o)),
            Self::Fluids(o, list) => list.contains(&pal.fluid(at(o))),
            Self::Survive(o, state) => can_survive(level, pal, *state, add(pos, *o)),
            Self::Solid(o) => Palette::flag(&pal.solid, at(o)),
            Self::Replaceable(o) => Palette::flag(&pal.replaceable, at(o)),
            Self::Sturdy(o, d) => pal.sturdy(at(o), *d),
            Self::InsideWorld(o) => {
                let y = pos[1] + o[1];
                level.min_y() <= y && y < level.min_y() + level.height()
            }
            Self::AllOf(list) => list.iter().all(|p| p.test(level, pal, pos)),
            Self::AnyOf(list) => list.iter().any(|p| p.test(level, pal, pos)),
            Self::Not(p) => !p.test(level, pal, pos),
            Self::HeightRange(min, max) => {
                min.resolve(level) <= pos[1] && pos[1] <= max.resolve(level)
            }
            Self::Volume(min, max, inner) => {
                if (0..3).any(|i| min[i] > max[i]) {
                    return true;
                }
                for x in min[0]..=max[0] {
                    for z in min[2]..=max[2] {
                        for y in min[1]..=max[1] {
                            if !inner.test(level, pal, [pos[0] + x, pos[1] + y, pos[2] + z]) {
                                return false;
                            }
                        }
                    }
                }
                true
            }
        }
    }
}

fn predicate_kind(v: &Value) -> &str {
    v["type"]
        .as_str()
        .or_else(|| v["predicate_type"].as_str())
        .unwrap_or("")
}

fn ivec(v: &Value) -> [i32; 3] {
    let mut o = [0; 3];
    for (i, c) in v.as_array().into_iter().flatten().take(3).enumerate() {
        o[i] = c.as_i64().unwrap_or(0) as i32;
    }
    o
}

/// Шум провайдера: `NormalNoise` с сидом `LegacyRandomSource`.
#[derive(Clone)]
struct ProviderNoise(Arc<crate::noise::Stack>);

impl std::fmt::Debug for ProviderNoise {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProviderNoise")
    }
}

impl ProviderNoise {
    fn parse(v: &Value, seed: i64) -> Result<Self, String> {
        Ok(Self(Arc::new(
            NormalNoise::from_json(v)?.create_legacy(seed),
        )))
    }

    fn get(&self, x: f64, y: f64, z: f64) -> f32 {
        self.0.get(x, y, z)
    }
}

/// `BlockStateProvider`.
#[derive(Clone, Debug)]
enum Provider {
    Simple(u32),
    Weighted(i32, Vec<(i32, u32)>),
    Noise {
        noise: ProviderNoise,
        scale: f32,
        states: Vec<u32>,
    },
    Threshold {
        noise: ProviderNoise,
        scale: f32,
        threshold: f32,
        high_chance: f32,
        default: u32,
        low: Vec<u32>,
        high: Vec<u32>,
    },
    Dual {
        noise: ProviderNoise,
        scale: f32,
        states: Vec<u32>,
        slow: ProviderNoise,
        slow_scale: f32,
        variety: (i32, i32),
    },
    RandomizedInt(Box<Provider>, String, IntP),
    /// `rotated`: источник и направление (`None` — случайное).
    Rotated(Box<Provider>, Option<usize>),
    /// `random_block`: состояния по умолчанию блоков набора.
    RandomBlock(Vec<u32>),
    RuleBased(Vec<(Predicate, Provider)>, Option<Box<Provider>>),
}

impl Provider {
    fn parse(v: &Value, read: &Read, palette: &mut Palette) -> Result<Self, String> {
        if let Value::String(name) = v {
            return Self::parse(&read("block_state_provider", name)?, read, palette);
        }
        let Some(kind) = v["type"].as_str() else {
            return Ok(Self::Simple(palette.state(v)?));
        };
        let states = |k: &str, palette: &Palette| -> Result<Vec<u32>, String> {
            v[k].as_array()
                .ok_or_else(|| format!("provider: no {k}"))?
                .iter()
                .map(|s| palette.state(s))
                .collect()
        };
        let seed = v["seed"].as_i64().unwrap_or(0);
        let f32_of = |k: &str| v[k].as_f64().unwrap_or(0.0) as f32;
        match kind {
            "minecraft:simple_state_provider" => Ok(Self::Simple(palette.state(&v["state"])?)),
            "minecraft:weighted" => {
                let mut list = Vec::new();
                for e in v["entries"].as_array().ok_or("weighted: no entries")? {
                    list.push((
                        e["weight"].as_i64().unwrap_or(1) as i32,
                        palette.state(&e["data"])?,
                    ));
                }
                let total = list.iter().map(|e| e.0).sum();
                Ok(Self::Weighted(total, list))
            }
            "minecraft:noise" => Ok(Self::Noise {
                noise: ProviderNoise::parse(&v["noise"], seed)?,
                scale: f32_of("scale"),
                states: states("states", palette)?,
            }),
            "minecraft:noise_threshold" => Ok(Self::Threshold {
                noise: ProviderNoise::parse(&v["noise"], seed)?,
                scale: f32_of("scale"),
                threshold: f32_of("threshold"),
                high_chance: f32_of("high_chance"),
                default: palette.state(&v["default_state"])?,
                low: states("low_states", palette)?,
                high: states("high_states", palette)?,
            }),
            "minecraft:dual_noise" => {
                let range = &v["variety"];
                let (min, max) = match range.as_array() {
                    Some(a) => (
                        a.first().and_then(Value::as_i64).unwrap_or(1),
                        a.get(1).and_then(Value::as_i64).unwrap_or(1),
                    ),
                    None => {
                        let n = range.as_i64().unwrap_or(1);
                        (n, n)
                    }
                };
                Ok(Self::Dual {
                    noise: ProviderNoise::parse(&v["noise"], seed)?,
                    scale: f32_of("scale"),
                    states: states("states", palette)?,
                    slow: ProviderNoise::parse(&v["slow_noise"], seed)?,
                    slow_scale: f32_of("slow_scale"),
                    variety: (min as i32, max as i32),
                })
            }
            "minecraft:randomized_int" => Ok(Self::RandomizedInt(
                Box::new(Self::parse(&v["source"], read, palette)?),
                v["property"].as_str().unwrap_or("").to_owned(),
                IntP::parse(&v["values"])?,
            )),
            "minecraft:rotated" => {
                let dir = match v["direction"].as_str() {
                    Some(d) => Some(
                        DIRECTION_NAMES
                            .iter()
                            .position(|n| *n == d)
                            .ok_or_else(|| format!("direction {d}"))?,
                    ),
                    None => None,
                };
                Ok(Self::Rotated(
                    Box::new(Self::parse(&v["state"], read, palette)?),
                    dir,
                ))
            }
            "minecraft:random_block" => {
                let mask = block_set(&v["blocks"], palette)?;
                // Порядок элементов набора — порядок блоков в реестре.
                let mut list: Vec<(u32, u32)> = palette
                    .blocks
                    .iter()
                    .filter(|(_, (_, states))| states.first().is_some_and(|&st| mask[st as usize]))
                    .filter_map(|(name, (id, _))| Some((*id, *palette.defaults.get(name)?)))
                    .collect();
                list.sort_unstable();
                let defaults = list.into_iter().map(|e| e.1).collect();
                Ok(Self::RandomBlock(defaults))
            }
            "minecraft:rule_based" => {
                let mut rules = Vec::new();
                for e in v["rules"].as_array().into_iter().flatten() {
                    rules.push((
                        Predicate::parse(&e["if_true"], palette)?,
                        Self::parse(&e["then"], read, palette)?,
                    ));
                }
                let fallback = match &v["fallback"] {
                    Value::Null => None,
                    f => Some(Box::new(Self::parse(f, read, palette)?)),
                };
                Ok(Self::RuleBased(rules, fallback))
            }
            other => Err(format!("state provider {other} is not supported")),
        }
    }

    /// `getOptionalState`: `None`, если провайдеру нечего поставить.
    fn get(
        &self,
        level: &dyn Level,
        pal: &Palette,
        r: &mut Worldgen,
        pos: [i32; 3],
    ) -> Option<u32> {
        let [x, y, z] = pos;
        // `NoiseProvider.getRandomState`.
        let pick = |states: &[u32], v: f32| {
            let t = ((1.0 + v) / 2.0).clamp(0.0, 0.9999);
            states[(t * states.len() as f32) as usize]
        };
        let sample = |noise: &ProviderNoise, scale: f32| {
            let s = scale as f64;
            noise.get(x as f64 * s, y as f64 * s, z as f64 * s)
        };
        Some(match self {
            Self::Simple(s) => *s,
            Self::Weighted(total, list) => {
                let mut i = r.next_int_bound(*total);
                let mut out = list[0].1;
                for (w, s) in list {
                    if i < *w {
                        out = *s;
                        break;
                    }
                    i -= w;
                }
                out
            }
            Self::Noise {
                noise,
                scale,
                states,
            } => pick(states, sample(noise, *scale)),
            Self::Threshold {
                noise,
                scale,
                threshold,
                high_chance,
                default,
                low,
                high,
            } => {
                if (sample(noise, *scale) as f64) < *threshold as f64 {
                    low[r.next_int_bound(low.len() as i32) as usize]
                } else if r.next_float() < *high_chance {
                    high[r.next_int_bound(high.len() as i32) as usize]
                } else {
                    *default
                }
            }
            Self::Dual {
                noise,
                scale,
                states,
                slow,
                slow_scale,
                variety,
            } => {
                // Медленный шум: `pos * slowScale` во float.
                let slow_at = |dx: i32, dz: i32| {
                    let s = *slow_scale;
                    slow.get(
                        ((x + dx) as f32 * s) as f64,
                        (y as f32 * s) as f64,
                        ((z + dz) as f32 * s) as f64,
                    )
                };
                let v = slow_at(0, 0) as f64;
                let (lo, hi) = (variety.0 as f64, variety.1 as f64 + 1.0);
                let t = ((v + 1.0) / 2.0).clamp(0.0, 1.0);
                let n = (lo + t * (hi - lo)) as i32;
                let local: Vec<u32> = (0..n)
                    .map(|i| pick(states, slow_at(i * 54545, i * 34234)))
                    .collect();
                pick(&local, sample(noise, *scale))
            }
            Self::RandomizedInt(source, prop, values) => {
                let s = source.get(level, pal, r, pos)?;
                if pal.prop(s, prop).is_none() {
                    return Some(s);
                }
                let v = values.sample(r);
                pal.with_prop(s, prop, &v.to_string()).unwrap_or(s)
            }
            Self::Rotated(source, dir) => {
                let d = match dir {
                    Some(d) => *d,
                    None => r.next_int_bound(6) as usize,
                };
                let s = source.get(level, pal, r, pos)?;
                let axis = ["y", "y", "z", "z", "x", "x"][d];
                let s = pal.with_prop(s, "axis", axis).unwrap_or(s);
                let s = pal.with_prop(s, "facing", DIRECTION_NAMES[d]).unwrap_or(s);
                if d >= 2 {
                    pal.with_prop(s, "horizontal_facing", DIRECTION_NAMES[d])
                        .unwrap_or(s)
                } else {
                    s
                }
            }
            Self::RandomBlock(list) => {
                if list.is_empty() {
                    return None;
                }
                list[r.next_int_bound(list.len() as i32) as usize]
            }
            Self::RuleBased(rules, fallback) => {
                for (p, then) in rules {
                    if p.test(level, pal, pos)
                        && let Some(s) = then.get(level, pal, r, pos)
                    {
                        return Some(s);
                    }
                }
                return fallback.as_ref()?.get(level, pal, r, pos);
            }
        })
    }
}

/// `BlockState.canSurvive` для классов растений, которые ставят фичи.
fn can_survive(level: &dyn Level, pal: &Palette, state: u32, [x, y, z]: [i32; 3]) -> bool {
    let below = level.get(x, y - 1, z);
    let sup = &pal.support;
    let flag = Palette::flag;
    let upper = || pal.prop(state, "half") == Some("upper");
    let on_lower = || pal.same_block(below, state) && pal.prop(below, "half") == Some("lower");
    let seagrass = || pal.sturdy(below, 1) && !flag(&sup.no_seagrass, below);
    const SIDES: [[i32; 2]; 4] = [[0, -1], [0, 1], [-1, 0], [1, 0]];
    match pal.survive.get(state as usize).copied().unwrap_or_default() {
        Survive::Always => true,
        Survive::Vegetation => flag(&sup.vegetation, below),
        Survive::Dry => flag(&sup.dry, below),
        Survive::Double => {
            if upper() {
                on_lower()
            } else {
                flag(&sup.vegetation, below)
            }
        }
        Survive::Seagrass => seagrass(),
        Survive::TallSeagrass => {
            if upper() {
                on_lower()
            } else {
                seagrass() && pal.fluid(level.get(x, y, z)) == Fluid::Water
            }
        }
        Survive::SugarCane => {
            if pal.same_block(below, state) {
                return true;
            }
            flag(&sup.cane, below)
                && SIDES.iter().any(|[dx, dz]| {
                    let s = level.get(x + dx, y - 1, z + dz);
                    pal.fluid(s).is_water() || flag(&sup.cane_adjacent, s)
                })
        }
        Survive::Cactus => {
            for [dx, dz] in SIDES {
                let s = level.get(x + dx, y, z + dz);
                if flag(&pal.solid, s) || pal.fluid(s).is_lava() {
                    return false;
                }
            }
            (pal.same_block(below, state) || flag(&sup.cactus, below))
                && !flag(&pal.liquid, level.get(x, y + 1, z))
        }
        Survive::SturdyBelow => pal.sturdy(below, 1),
        Survive::MossyCarpet => {
            if pal.prop(state, "bottom") == Some("true") {
                !pal.is_air(below)
            } else {
                pal.same_block(below, state) && pal.prop(below, "bottom") == Some("true")
            }
        }
        // Свет на генерации не считается: остаётся проверка опоры.
        Survive::Mushroom => {
            flag(&sup.mushroom, below) || (flag(&pal.solid, below) && pal.sturdy(below, 1))
        }
        Survive::OwnTag => pal
            .block_index(state)
            .and_then(|b| sup.own.get(&b))
            .is_some_and(|t| flag(t, below)),
        Survive::Snow => {
            if flag(&sup.no_snow, below) {
                return false;
            }
            flag(&sup.snow, below)
                || pal.sturdy(below, 1)
                || (pal.same_block(below, state) && pal.prop(below, "layers") == Some("8"))
        }
    }
}

/// Свой кусок платформы: чанк не дальше 1 от `(0, 0)`, блоки внутри квадрата.
fn place_void_platform(level: &mut dyn Level, pos: [i32; 3], stone: u32, cobble: u32) {
    let cx = pos[0].div_euclid(16);
    let cz = pos[2].div_euclid(16);
    if !crate::void_platform_chunk(cx, cz) {
        return;
    }
    let y = crate::void_platform_y(level.min_y());
    let x0 = cx * 16;
    let z0 = cz * 16;
    for dz in 0..16 {
        for dx in 0..16 {
            let x = x0 + dx;
            let z = z0 + dz;
            let Some(kind) = crate::void_platform_block(x, z) else {
                continue;
            };
            if !level.can_write(x, z) {
                continue;
            }
            let state = match kind {
                crate::VoidPlatformBlock::Cobblestone => cobble,
                crate::VoidPlatformBlock::Stone => stone,
            };
            level.set(x, y, z, state);
        }
    }
}

fn parse_placed(
    v: &Value,
    read: &Read,
    palette: &mut Palette,
    min_y: i32,
    height: i32,
    data_dir: &Path,
) -> Result<Placed, String> {
    // Ссылка на `placed_feature` строкой или встроенная фича с размещением.
    let v = match v {
        Value::String(name) => &read("placed_feature", name)?,
        v => v,
    };
    let feature = match &v["feature"] {
        Value::String(name) => read("feature", name)?,
        inline => inline.clone(),
    };
    let feature = parse_feature(&feature, read, palette, min_y, height, data_dir)?;
    let placement = v["placement"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|m| Modifier::parse(m, palette, min_y, height))
        .collect::<Result<_, _>>()?;
    Ok(Placed { feature, placement })
}

fn parse_feature(
    v: &Value,
    read: &Read,
    palette: &mut Palette,
    min_y: i32,
    height: i32,
    data_dir: &Path,
) -> Result<Feature, String> {
    let placed = |v: &Value, palette: &mut Palette| -> Result<Arc<Placed>, String> {
        Ok(Arc::new(parse_placed(
            v, read, palette, min_y, height, data_dir,
        )?))
    };
    let ore = |palette: &mut Palette| -> Result<Ore, String> {
        let mut targets = Vec::new();
        for t in v["targets"].as_array().ok_or("ore: no targets")? {
            targets.push((
                Rule::parse(&t["target"], palette)?,
                palette.state(&t["state"])?,
            ));
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
        "minecraft:block_pile" => Ok(Feature::BlockPile(Provider::parse(
            &v["state_provider"],
            read,
            palette,
        )?)),
        "minecraft:simple_block" => Ok(Feature::SimpleBlock(Provider::parse(
            &v["to_place"],
            read,
            palette,
        )?)),
        "minecraft:random_selector" => {
            let mut list = Vec::new();
            for e in v["features"]
                .as_array()
                .ok_or("random_selector: no features")?
            {
                list.push((
                    e["chance"].as_f64().unwrap_or(0.0) as f32,
                    placed(&e["feature"], palette)?,
                ));
            }
            Ok(Feature::RandomSelector(
                list,
                placed(&v["default"], palette)?,
            ))
        }
        "minecraft:random_boolean_selector" => Ok(Feature::BooleanSelector(
            placed(&v["feature_true"], palette)?,
            placed(&v["feature_false"], palette)?,
        )),
        "minecraft:weighted_random_selector" => {
            let mut list = Vec::new();
            for e in v["features"]
                .as_array()
                .ok_or("weighted_random_selector: no features")?
            {
                list.push((
                    e["weight"].as_i64().unwrap_or(1) as i32,
                    placed(&e["data"], palette)?,
                ));
            }
            Ok(Feature::WeightedSelector(
                list.iter().map(|e| e.0).sum(),
                list,
            ))
        }
        kind @ ("minecraft:vegetation_patch" | "minecraft:waterlogged_vegetation_patch") => {
            let f = |k: &str| v[k].as_f64().unwrap_or(0.0) as f32;
            Ok(Feature::VegetationPatch(Box::new(Patch {
                replaceable: block_set(&v["replaceable"], palette)?,
                ground: Provider::parse(&v["ground_state"], read, palette)?,
                vegetation: placed(&v["vegetation_feature"], palette)?,
                inwards: if v["surface"].as_str() == Some("ceiling") {
                    1
                } else {
                    0
                },
                depth: IntP::parse(&v["depth"])?,
                extra_bottom: f("extra_bottom_block_chance"),
                vertical_range: v["vertical_range"].as_i64().unwrap_or(0) as i32,
                vegetation_chance: f("vegetation_chance"),
                radius: IntP::parse(&v["xz_radius"])?,
                extra_edge: f("extra_edge_column_chance"),
                waterlogged: kind == "minecraft:waterlogged_vegetation_patch",
            })))
        }
        "minecraft:tree" => Ok(Feature::Tree(Arc::new(tree::Tree::parse(
            v,
            palette,
            &mut |v, palette| Provider::parse(v, read, palette),
            &mut |v, palette| placed(v, palette),
        )?))),
        kind @ ("minecraft:huge_red_mushroom" | "minecraft:huge_brown_mushroom") => {
            Ok(Feature::HugeMushroom(Box::new(Mushroom {
                red: kind == "minecraft:huge_red_mushroom",
                cap: Provider::parse(&v["cap_provider"], read, palette)?,
                stem: Provider::parse(&v["stem_provider"], read, palette)?,
                radius: v["foliage_radius"].as_i64().unwrap_or(2) as i32,
                on: Predicate::parse(&v["can_place_on"], palette)?,
                replaceable: palette.tag("minecraft:replaceable_by_mushrooms")?,
                leaves: palette.tag("minecraft:leaves")?,
            })))
        }
        "minecraft:disk" => {
            let provider = Provider::parse(&v["state_provider"], read, palette)?;
            Ok(Feature::Disk(Arc::new(misc::Disk::parse(
                v, palette, provider,
            )?)))
        }
        "minecraft:spring_feature" => {
            Ok(Feature::Spring(Arc::new(misc::Spring::parse(v, palette)?)))
        }
        "minecraft:vines" => Ok(Feature::Vines),
        "minecraft:block_blob" => Ok(Feature::Blob(Arc::new(misc::Blob::parse(v, palette)?))),
        "minecraft:blue_ice" => Ok(Feature::BlueIce),
        "minecraft:lake" => {
            let fluid = Provider::parse(&v["fluid"], read, palette)?;
            let barrier = Provider::parse(&v["barrier"], read, palette)?;
            Ok(Feature::Lake(Arc::new(misc::Lake::parse(
                v, palette, fluid, barrier,
            )?)))
        }
        "minecraft:geode" => {
            let seed = palette.world_seed;
            let geode = misc::Geode::parse(
                v,
                palette,
                &mut |v, palette| Provider::parse(v, read, palette),
                seed,
            )?;
            Ok(Feature::Geode(Arc::new(geode)))
        }
        "minecraft:root_system" => {
            let tree = placed(&v["feature"], palette)?;
            let system = misc::RootSystem::parse(
                v,
                palette,
                &mut |v, palette| Provider::parse(v, read, palette),
                tree,
            )?;
            Ok(Feature::RootSystem(Arc::new(system)))
        }
        "minecraft:multiface_growth" => Ok(Feature::Multiface(Arc::new(misc::Multiface::parse(
            v, palette,
        )?))),
        "minecraft:large_dripstone" => Ok(Feature::LargeDripstone(Arc::new(
            misc::LargeDripstone::parse(v, palette)?,
        ))),
        "minecraft:speleothem" => Ok(Feature::Speleothem(Arc::new(misc::Speleothem::parse(
            v, palette,
        )?))),
        "minecraft:speleothem_cluster" => Ok(Feature::SpeleothemCluster(Arc::new(
            misc::SpeleothemCluster::parse(v, palette)?,
        ))),
        "minecraft:monster_room" => Ok(Feature::MonsterRoom(Arc::new(misc::MonsterRoom::parse(
            palette,
        )?))),
        "minecraft:sequence" => Ok(Feature::Sequence(
            v["features"]
                .as_array()
                .ok_or("sequence: no features")?
                .iter()
                .map(|e| placed(e, palette))
                .collect::<Result<_, _>>()?,
        )),
        "minecraft:freeze_top_layer" => Ok(Feature::FreezeTopLayer),
        "minecraft:underwater_magma" => Ok(Feature::UnderwaterMagma(Arc::new(
            misc::UnderwaterMagma::parse(v)?,
        ))),
        "minecraft:bamboo" => Ok(Feature::Bamboo(
            v["probability"].as_f64().unwrap_or(0.0) as f32
        )),
        "minecraft:spike" => Ok(Feature::IceSpike(Arc::new(misc::IceSpike::parse(
            v, palette,
        )?))),
        "minecraft:iceberg" => Ok(Feature::Iceberg(misc::Iceberg::parse(v, palette)?)),
        "minecraft:coral_tree" | "minecraft:coral_claw" | "minecraft:coral_mushroom" => {
            Ok(Feature::CoralTree(
                v["type"].as_str().unwrap_or("").to_owned(),
                placed(&v["feature"], palette)?,
            ))
        }
        "minecraft:overlay" => Ok(Feature::Overlay(
            v["features"]
                .as_array()
                .ok_or("overlay: no features")?
                .iter()
                .map(|e| placed(e, palette))
                .collect::<Result<_, _>>()?,
        )),
        "minecraft:sculk_patch" => Ok(Feature::SculkPatch(Arc::new(misc::SculkPatch::parse(
            v, palette,
        )?))),
        "minecraft:no_op" => Ok(Feature::NoOp),
        "minecraft:void_start_platform" => {
            let stone = palette.state(&Value::String("minecraft:stone".into()))?;
            let cobble = palette.state(&Value::String("minecraft:cobblestone".into()))?;
            Ok(Feature::VoidPlatform(stone, cobble))
        }
        "minecraft:fallen_tree" => Ok(Feature::FallenTree(Arc::new(tree::Fallen::parse(
            v,
            palette,
            &mut |v, palette| Provider::parse(v, read, palette),
            &mut |v, palette| placed(v, palette),
        )?))),
        "minecraft:block_column" => {
            let mut layers = Vec::new();
            for l in v["layers"].as_array().ok_or("block_column: no layers")? {
                layers.push((
                    IntP::parse(&l["height"])?,
                    Provider::parse(&l["provider"], read, palette)?,
                ));
            }
            let d = v["direction"].as_str().unwrap_or("up");
            let d = DIRECTION_NAMES
                .iter()
                .position(|n| *n == d)
                .ok_or("block_column: direction")?;
            Ok(Feature::BlockColumn(
                layers,
                DIRS[d],
                Predicate::parse(&v["allowed_placement"], palette)?,
                v["prioritize_tip"].as_bool().unwrap_or(false),
            ))
        }
        "minecraft:template" => Ok(Feature::Template(Arc::new(
            template::TemplateFeature::parse(v, read, palette, data_dir)?,
        ))),
        "minecraft:fossil" => Ok(Feature::Fossil(Arc::new(template::Fossil::parse(
            v, read, palette, data_dir,
        )?))),
        "minecraft:simple_random_selector" => Ok(Feature::SimpleRandomSelector(
            v["features"]
                .as_array()
                .ok_or("simple_random_selector: no features")?
                .iter()
                .map(|e| placed(e, palette))
                .collect::<Result<_, _>>()?,
        )),
        "minecraft:single_block_pillar" => {
            let block = Provider::parse(&v["block"], read, palette)?;
            let cap = match v.get("cap_feature") {
                Some(c) if !c.is_null() => Some(placed(c, palette)?),
                _ => None,
            };
            Ok(Feature::Pillar(Arc::new(nether::Pillar::parse(
                v, palette, block, cap,
            )?)))
        }
        "minecraft:delta_feature" => Ok(Feature::Delta(Arc::new(nether::Delta::parse(
            v, palette,
        )?))),
        "minecraft:stepped_column_cluster" => {
            let block = Provider::parse(&v["block"], read, palette)?;
            Ok(Feature::ColumnCluster(Arc::new(
                nether::ColumnCluster::parse(v, palette, block)?,
            )))
        }
        "minecraft:netherrack_replace_blobs" => Ok(Feature::ReplaceBlobs(Arc::new(
            nether::ReplaceBlobs::parse(v, palette)?,
        ))),
        "minecraft:random_neighbor_spread" => {
            let block = Provider::parse(&v["block"], read, palette)?;
            Ok(Feature::NeighborSpread(Arc::new(
                nether::NeighborSpread::parse(v, palette, block)?,
            )))
        }
        "minecraft:projected_random_patchy_square" => {
            let block = Provider::parse(&v["block"], read, palette)?;
            Ok(Feature::PatchySquare(Arc::new(
                nether::PatchySquare::parse(v, palette, block)?,
            )))
        }
        "minecraft:huge_fungus" => Ok(Feature::HugeFungus(Arc::new(
            nether::HugeFungus::parse(v, palette)?,
        ))),
        "minecraft:end_island" => Ok(Feature::EndIsland(end::Blocks::load(palette)?)),
        "minecraft:end_platform" => Ok(Feature::EndPlatform(end::Blocks::load(palette)?)),
        "minecraft:end_gateway" => Ok(Feature::EndGateway(
            end::Blocks::load(palette)?,
            end::Gateway::parse(v)?,
        )),
        "minecraft:end_spike" => Ok(Feature::EndSpike(
            end::Blocks::load(palette)?,
            end::Spikes::parse(v)?,
        )),
        "minecraft:chorus_plant" => Ok(Feature::Chorus(end::Blocks::load(palette)?)),
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
        dfs(
            &edges,
            &mut discovered,
            &mut BTreeSet::new(),
            &mut sorted,
            node,
        );
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
            v.iter()
                .map(|x| x.iter().map(|s| s.to_string()).collect())
                .collect()
        };
        let a = s(&[&["a", "c"], &["x"]]);
        let b = s(&[&["a", "b", "c"]]);
        let out = sort_features([a.as_slice(), b.as_slice()].into_iter());
        assert_eq!(out[0], vec!["a", "b", "c"]);
        assert_eq!(out[1], vec!["x"]);
    }

    /// Декорация не должна молча откатываться: каждая фича биомов
    /// оверворлда разбирается. `CHUNKWAKE_DATA` — каталог `generated/26.3`;
    /// без него тест пропускается.
    /// Фичи и структуры Ада и Края разбираются без отказов.
    #[test]
    fn other_dimension_features_parse() {
        let Some(data) = std::env::var_os("CHUNKWAKE_DATA").map(std::path::PathBuf::from) else {
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
        let state = |n: &str| blocks.default_state(n).ok().map(|s| s as u32);
        for settings in ["minecraft:nether", "minecraft:end"] {
            let mut noise = crate::Generator::load_settings(&data, 1, None, settings).unwrap();
            noise.bind_biomes(biome_ids.clone());
            let features = Features::load(
                &data,
                Palette::new(&data, &blocks.states(), &state),
                &noise.feature_biomes(),
                &biome_ids,
                1,
                noise.min_y,
                noise.height,
            )
            .unwrap();
            assert!(
                features.failures().is_empty(),
                "{settings}: отказы {:?}",
                features.failures()
            );
            let (done, all) = features.coverage();
            assert_eq!(done, all, "{settings}: {done} из {all}");
        }
    }

    /// Сдвиг по раскладке даёт то же состояние, что и перебор, на всех
    /// состояниях, свойствах и значениях.
    #[test]
    fn with_prop_layout_matches_scan() {
        let Some(data) = std::env::var_os("CHUNKWAKE_DATA").map(std::path::PathBuf::from) else {
            return;
        };
        let blocks = chunkwake_data::world::Blocks::load(&data).unwrap();
        let state = |n: &str| blocks.default_state(n).ok().map(|s| s as u32);
        let pal = Palette::new(&data, &blocks.states(), &state);
        let laid = pal.layouts.iter().filter(|l| l.is_some()).count();
        assert!(laid * 10 > pal.layouts.len() * 9, "раскладок {laid} из {}", pal.layouts.len());
        for (b, list) in pal.by_block.iter().enumerate() {
            let Some(layout) = &pal.layouts[b] else {
                continue;
            };
            for &s in list {
                for (key, values, _) in layout {
                    for v in values.iter().map(String::as_str).chain(["nope"]) {
                        assert_eq!(pal.with_prop(s, key, v), pal.with_prop_scan(s, key, v), "{s} {key}={v}");
                    }
                }
            }
        }
    }

    #[test]
    fn overworld_features_parse() {
        let Some(data) = std::env::var_os("CHUNKWAKE_DATA").map(std::path::PathBuf::from) else {
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
        let mut noise = crate::Generator::load_with_version(&data, 1, None).unwrap();
        noise.bind_biomes(biome_ids.clone());
        let state = |n: &str| blocks.default_state(n).ok().map(|s| s as u32);
        let features = Features::load(
            &data,
            Palette::new(&data, &blocks.states(), &state),
            &noise.biome_order(),
            &biome_ids,
            1,
            noise.min_y,
            noise.height,
        )
        .unwrap();
        assert!(
            features.failures().is_empty(),
            "фичи без причины: {:?}",
            features.failures()
        );
        let (done, all) = features.coverage();
        assert_eq!(done, all, "{done} из {all}");
        assert_eq!(all, 171);
        let structures = features.structures.as_ref().expect("structures");
        assert_eq!(structures.structure_count(), 48);
        assert_eq!(
            structures.feature_seed("minecraft:village_plains"),
            Some((4, 40))
        );
        assert_eq!(
            structures.feature_seed("minecraft:pillager_outpost"),
            Some((4, 27))
        );
        assert_eq!(
            structures.feature_seed("minecraft:abandoned_camp_forest"),
            Some((4, 5))
        );
        assert_eq!(
            structures.feature_seed("minecraft:desert_pyramid"),
            Some((4, 19))
        );
        assert_eq!(
            structures.feature_seed("minecraft:jungle_pyramid"),
            Some((4, 22))
        );
        assert_eq!(structures.feature_seed("minecraft:igloo"), Some((4, 21)));
        assert_eq!(
            structures.feature_seed("minecraft:shipwreck"),
            Some((4, 35))
        );
        assert_eq!(
            structures.feature_seed("minecraft:shipwreck_beached"),
            Some((4, 36))
        );
        assert_eq!(
            structures.feature_seed("minecraft:swamp_hut"),
            Some((4, 38))
        );
        assert_eq!(structures.feature_seed("minecraft:mineshaft"), Some((3, 1)));
        assert_eq!(
            structures.feature_seed("minecraft:mineshaft_mesa"),
            Some((3, 2))
        );
        assert_eq!(structures.feature_seed("minecraft:monument"), Some((4, 24)));
        assert_eq!(
            structures.feature_seed("minecraft:stronghold"),
            Some((4, 37))
        );
        assert_eq!(
            structures.feature_seed("minecraft:trial_chambers"),
            Some((3, 4))
        );
        assert_eq!(
            structures.feature_seed("minecraft:ocean_ruin_cold"),
            Some((4, 25))
        );
        assert_eq!(
            structures.feature_seed("minecraft:ocean_ruin_warm"),
            Some((4, 26))
        );
        assert_eq!(
            structures.feature_seed("minecraft:ruined_portal"),
            Some((4, 28))
        );
        assert_eq!(
            structures.feature_seed("minecraft:ruined_portal_swamp"),
            Some((4, 34))
        );
    }

    #[test]
    fn runtime_trees_parse_26_3() {
        let Some(data) = std::env::var_os("CHUNKWAKE_DATA").map(std::path::PathBuf::from) else {
            return;
        };
        let blocks = chunkwake_data::world::Blocks::load(&data).unwrap();
        let state = |n: &str| blocks.default_state(n).ok().map(|s| s as u32);
        let trees = RuntimeTrees::load(&data, &blocks.states(), &state, -64, 384).unwrap();
        assert_eq!(trees.trees.len(), 18);
        struct Flat {
            air: u32,
            dirt: u32,
            placed: HashMap<[i32; 3], u32>,
        }
        impl Level for Flat {
            fn min_y(&self) -> i32 { -64 }
            fn height(&self) -> i32 { 384 }
            fn get(&self, x: i32, y: i32, z: i32) -> u32 {
                self.placed.get(&[x, y, z]).copied()
                    .unwrap_or(if y == 63 { self.dirt } else { self.air })
            }
            fn set(&mut self, x: i32, y: i32, z: i32, value: u32) {
                self.placed.insert([x, y, z], value);
            }
            fn can_write(&self, _x: i32, _z: i32) -> bool { true }
            fn surface(&self, _kind: Heightmap, _x: i32, _z: i32) -> i32 { 64 }
            fn sea_level(&self) -> i32 { 63 }
            fn biome(&self, _x: i32, _y: i32, _z: i32) -> u32 { 0 }
        }
        let mut level = Flat {
            air: state("minecraft:air").unwrap(),
            dirt: state("minecraft:dirt").unwrap(),
            placed: HashMap::new(),
        };
        assert!(trees.place("oak", &mut level, [0, 64, 0], 12345));
        assert!(level.placed.values().any(|&s| trees.features.palette.block_name(s) == Some("minecraft:oak_log")));
        assert!(level.placed.values().any(|&s| trees.features.palette.block_name(s) == Some("minecraft:oak_leaves")));
    }
}
