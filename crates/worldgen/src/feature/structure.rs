//! Деревни и пиллагерские аванпосты: `random_spread`, пулы и `JigsawPlacement`.
//! Сундуки остаются состояниями блоков, лут и мобы не пишутся.

use super::end_city;
use super::fortress;
use super::mansion;
use super::mineshaft::{self, Cfg as ShaftCfg};
use super::monument;
use super::ocean_ruin;
use super::portal;
use super::pyramid::{self, Which};
use super::shore::{self, Body};
use super::stronghold;
use super::template::{self, JigInfo, Processor, ROTS, Rot, Template};
use super::treasure;
use super::{Bits, Features, Legacy, Level, Palette, Worldgen};
use crate::random::pos_seed;
use serde_json::Value;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::rc::Rc;
use std::sync::{Arc, Mutex, OnceLock};

/// Кэш раскладки структуры по (набор, x, z). Ячейка заводится до сборки:
/// старт, нужный сразу многим потокам, собирает один, остальные ждут его.
type LayoutCache = Mutex<HashMap<(usize, i32, i32), Arc<OnceLock<Option<Arc<Layout>>>>>>;

/// Столбцы мира для высоты старта и проверки биома. Высота — без бороды
/// и без карверов (`getBaseHeight`).
pub(crate) trait Columns {
    fn seed(&self) -> i64;
    fn base_height(&self, x: i32, z: i32) -> i32;
    /// Первый свободный `OCEAN_FLOOR_WG` того же шума, без карверов.
    fn ocean_floor(&self, x: i32, z: i32) -> i32;
    fn biome_at(&self, x: i32, y: i32, z: i32) -> Option<u32>;
    fn column_has(&self, x: i32, z: i32, biomes: &HashSet<u32>) -> bool;
    /// `getBaseColumn`: блок шума без поверхности и карверов.
    fn base_block(&self, x: i32, y: i32, z: i32) -> Option<Base>;
    fn min_y(&self) -> i32;
    /// `getMaxY()`: верх невключительно.
    fn max_y(&self) -> i32;
    /// `getSeaLevel`.
    fn sea_level(&self) -> i32;
}

/// Блок столбца `getBaseColumn`: воздух, блок по умолчанию или жидкость.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Base {
    Air,
    Solid,
    Fluid,
}

/// Жёсткая коробка куска и её линия земли.
#[derive(Clone, Debug)]
struct BeardBox {
    min: [i32; 3],
    max: [i32; 3],
    ground: i32,
    adapt: Adapt,
}

/// `TerrainAdjustment` структуры, кроме `NONE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Adapt {
    Thin,
    Bury,
    Boxed,
    /// Свой `dy` от границ коробки и половинный масштаб по всем осям.
    Encapsulate,
}

/// Добавка `beard_thin` к финальной плотности. Пустая ничего не меняет.
#[derive(Clone, Debug, Default)]
pub(crate) struct Beard {
    boxes: Vec<BeardBox>,
    junctions: Vec<[i32; 3]>,
}

impl Beard {
    pub(crate) fn is_empty(&self) -> bool {
        self.boxes.is_empty() && self.junctions.is_empty()
    }

    /// `Beardifier.sampleValueUnchecked` для `BEARD_THIN` и стыков.
    pub(crate) fn delta(&self, x: i32, y: i32, z: i32) -> f32 {
        if self.is_empty() {
            return 0.0;
        }
        let mut sum = 0.0f32;
        for b in &self.boxes {
            let dx = 0.max((b.min[0] - x).max(x - b.max[0]));
            let dz = 0.max((b.min[2] - z).max(z - b.max[2]));
            let to_ground = y - b.ground;
            sum += match b.adapt {
                Adapt::Encapsulate => {
                    let dy = 0.max((b.min[1] - y).max(y - b.max[1]));
                    bury_contrib(dx as f32 / 2.0, dy as f32 / 2.0, dz as f32 / 2.0) * 0.8
                }
                Adapt::Bury => bury_contrib(dx as f32, to_ground as f32 / 2.0, dz as f32),
                Adapt::Thin => beard_contrib(dx, to_ground, dz, to_ground) * 0.8,
                Adapt::Boxed => {
                    let dy = 0.max((b.ground - y).max(y - b.max[1]));
                    beard_contrib(dx, dy, dz, to_ground) * 0.8
                }
            };
        }
        for j in &self.junctions {
            let dx = x - j[0];
            let dy = y - j[1];
            let dz = z - j[2];
            sum += beard_contrib(dx, dy, dz, dy) * 0.4;
        }
        sum
    }
}

/// `Beardifier.getBuryContribution`: за 6 блоков вклад нулевой.
fn bury_contrib(dx: f32, dy: f32, dz: f32) -> f32 {
    let distance_sq = dx * dx + dy * dy + dz * dz;
    if distance_sq >= 36.0 {
        0.0
    } else {
        1.0 - distance_sq.sqrt() / 6.0
    }
}

fn beard_contrib(dx: i32, dy: i32, dz: i32, factor: i32) -> f32 {
    let (ix, iy, iz) = (dx + 12, dy + 12, dz + 12);
    if !(0..24).contains(&ix) || !(0..24).contains(&iy) || !(0..24).contains(&iz) {
        return 0.0;
    }
    let t = factor as f32 + 0.5;
    let len2 = dx as f32 * dx as f32 + t * t + dz as f32 * dz as f32;
    let inv = fast_inv_sqrt(f64::from(len2) / 2.0);
    let scale = -t * inv / 2.0;
    scale * KERNEL[(iz * 576 + ix * 24 + iy) as usize]
}

/// `Mth.fastInvSqrt`: один шаг Ньютона.
fn fast_inv_sqrt(x: f64) -> f32 {
    let half = 0.5 * x;
    let bits = 6_910_469_410_427_058_090u64.wrapping_sub(x.to_bits() >> 1);
    let mut y = f64::from_bits(bits);
    y *= 1.5 - half * y * y;
    y as f32
}

fn beard_compute(x: i32, y: i32, z: i32) -> f32 {
    let len = (x * x) as f64 + (y as f64 + 0.5).powi(2) + (z * z) as f64;
    (-len / 16.0).exp() as f32
}

/// `KERNEL[i * 576 + j * 24 + k] = compute(j - 12, k - 12, i - 12)`.
static KERNEL: std::sync::LazyLock<[f32; 13824]> = std::sync::LazyLock::new(|| {
    let mut k = [0f32; 13824];
    for i in 0..24 {
        for j in 0..24 {
            for n in 0..24 {
                k[i * 576 + j * 24 + n] =
                    beard_compute(j as i32 - 12, n as i32 - 12, i as i32 - 12);
            }
        }
    }
    k
});

#[derive(Clone)]
enum Elem {
    Empty,
    Single {
        template: Arc<Template>,
        processors: Vec<Processor>,
        ignore_air: bool,
        rigid: bool,
    },
    List {
        parts: Vec<Arc<Elem>>,
        rigid: bool,
    },
    Feature {
        placed: Arc<super::Placed>,
        rigid: bool,
    },
    /// Пустынная пирамида, храм в джунглях или хижина.
    Scattered(Arc<pyramid::Piece>),
    /// Иглу или кораблекрушение.
    Shore(Arc<shore::Piece>),
    /// Кусок заброшенной шахты.
    Shaft(Arc<mineshaft::Piece>),
    /// Кусок океанского монумента.
    Monument(Arc<monument::Piece>),
    /// Кусок крепости.
    Stronghold(Arc<stronghold::Piece>),
    /// Кусок крепости Незера.
    Fortress(Arc<fortress::Piece>),
    /// Окаменелость Незера.
    Fossil(Arc<FossilPiece>),
    /// Зарытый клад.
    Treasure(Arc<treasure::Piece>),
    /// Кусок лесного особняка.
    Mansion(Arc<mansion::Piece>),
    /// Кусок города Края.
    EndCity(Arc<end_city::Piece>),
    /// Руины портала.
    Portal(Arc<portal::Piece>),
    /// Куски океанской руины.
    Ruin(Arc<ocean_ruin::Piece>),
}

impl Elem {
    fn rigid(&self) -> bool {
        match self {
            Self::Empty => true,
            Self::Single { rigid, .. } | Self::List { rigid, .. } | Self::Feature { rigid, .. } => {
                *rigid
            }
            Self::Scattered(_)
            | Self::Shore(_)
            | Self::Shaft(_)
            | Self::Monument(_)
            | Self::Stronghold(_)
            | Self::Fortress(_)
            | Self::Fossil(_)
            | Self::Treasure(_)
            | Self::Mansion(_)
            | Self::EndCity(_)
            | Self::Portal(_)
            | Self::Ruin(_) => true,
        }
    }

    fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    fn place(
        &self,
        features: &Features,
        level: &mut dyn Level,
        piece: &Piece,
        r: &mut Worldgen,
        window: template::Bounds,
        reference: [i32; 3],
    ) -> bool {
        match self {
            Self::Empty => true,
            Self::Single {
                template,
                processors,
                ignore_air,
                rigid,
            } => template::place_pool(
                &features.palette,
                level,
                template,
                piece.rot,
                piece.pos,
                processors,
                *ignore_air,
                !rigid,
                r,
                window,
                features.seed,
                reference,
            ),
            Self::List { parts, .. } => {
                for part in parts {
                    if !part.place(features, level, piece, r, window, reference) {
                        return false;
                    }
                }
                true
            }
            Self::Feature { placed, .. } => {
                if window.contains(piece.pos) {
                    features.place(level, placed, None, r, piece.pos);
                }
                true
            }
            Self::Scattered(pyramid) => {
                pyramid.place(&features.palette, level, r, window);
                true
            }
            Self::Shore(piece) => {
                piece.place(&features.palette, level, r, window);
                true
            }
            Self::Shaft(piece) => {
                piece.place(&features.palette, level, r, window);
                true
            }
            Self::Monument(piece) => {
                piece.place(&features.palette, level, r, window);
                true
            }
            Self::Stronghold(piece) => {
                piece.place(&features.palette, level, r, window);
                true
            }
            Self::Fortress(piece) => {
                piece.place(&features.palette, level, r, window);
                true
            }
            Self::Fossil(piece) => {
                piece.place(&features.palette, level, r, window, features.seed);
                true
            }
            Self::Treasure(piece) => {
                piece.place(&features.palette, level, r, window);
                true
            }
            Self::Mansion(piece) => {
                piece.place(&features.palette, level, r, window);
                true
            }
            Self::EndCity(piece) => {
                piece.place(&features.palette, level, r, window);
                true
            }
            Self::Portal(piece) => {
                piece.place(&features.palette, level, r, window);
                true
            }
            Self::Ruin(piece) => {
                piece.place(&features.palette, level, r, window, features.seed);
                true
            }
        }
    }
}

struct Pool {
    templates: Vec<Arc<Elem>>,
    fallback: String,
    max_size: i32,
}

#[derive(Clone)]
struct Kind {
    #[cfg_attr(not(test), allow(dead_code))]
    name: String,
    start_pool: String,
    size: i32,
    horizontal: i32,
    vertical: i32,
    expansion: bool,
    project: bool,
    biomes: HashSet<u32>,
    beard: bool,
    /// Порядковый номер шага декорации и индекс среди структур этого шага.
    step: i32,
    seed_index: i32,
    /// `SinglePieceStructure`, не jigsaw.
    scattered: Option<Which>,
    /// Иглу или корабль. Пусто у jigsaw и пирамид.
    shore: Option<Body>,
    /// Заброшенная шахта. Пусто у остальных видов.
    shaft: Option<ShaftCfg>,
    /// Биомы вокруг монумента. Пусто у остальных видов.
    monument: Option<HashSet<u32>>,
    /// Крепость: старт — членство в кольцах, куски собираются отдельно.
    stronghold: bool,
    /// Крепость Незера: дерево `NetherFortressPieces`.
    fortress: bool,
    /// Окаменелость Незера: шаблоны и диапазон высоты.
    fossil: Option<Arc<FossilCfg>>,
    /// Зарытый клад: старт на дне в середине чанка.
    treasure: bool,
    /// Особняк: сетка-лабиринт из шаблонов, шаблоны грузятся при загрузке.
    mansion: bool,
    /// Город Края: дерево `EndCityPieces`, старт как у особняка.
    end_city: bool,
    /// Руины портала: наборы размещения.
    portal: Option<Vec<portal::Setup>>,
    /// Океанская руина: тёплая или холодная.
    ruin: Option<ocean_ruin::Cfg>,
    /// `start_height` jigsaw: своя высота вместо `WORLD_SURFACE_WG`.
    start_height: Option<StartHeight>,
    /// `ENCAPSULATE` вместо `beard_thin`.
    encapsulate: bool,
    /// `BEARD_BOX`: высота от земли до верха коробки.
    boxed: bool,
    /// `BURY` у jigsaw.
    bury: bool,
    /// `pool_aliases`: связки, раскрываются один раз в точку старта.
    aliases: Vec<PoolAlias>,
    /// `dimension_padding`: отступ от низа и верха мира.
    padding: (i32, i32),
    /// `start_jigsaw_name`: стартовый кусок встаёт так, чтобы этот
    /// шарнир пришёлся на точку старта.
    start_jigsaw: Option<String>,
}

struct Set {
    spacing: i32,
    separation: i32,
    salt: i32,
    /// `spread_type` triangular: два `nextInt`, целое среднее.
    triangular: bool,
    /// Частота старта. `Always` — без ограничения.
    frequency: Freq,
    /// `locate_offset`: точка `/locate` от угла чанка старта.
    locate_offset: [i32; 3],
    exclusion: Option<(usize, i32)>,
    kinds: Vec<Kind>,
    weights: Vec<i32>,
    /// `concentric_rings`. Пусто у `random_spread`.
    rings: Option<RingPlan>,
}

struct RingPlan {
    distance: i32,
    spread: i32,
    count: i32,
    preferred: HashSet<u32>,
}

struct RingSlot {
    initial: (i32, i32),
    fork: i64,
    at: Option<(i32, i32)>,
}

pub(crate) struct Piece {
    element: Arc<Elem>,
    pub(crate) pos: [i32; 3],
    rot: Rot,
    ground: i32,
    pub(crate) min: [i32; 3],
    pub(crate) max: [i32; 3],
    rigid: bool,
    junctions: Vec<[i32; 3]>,
}

pub(crate) struct Layout {
    pub(crate) pieces: Vec<Piece>,
    beard: bool,
    /// `TerrainAdjustment.BURY`, отдельно от `beard_thin`.
    bury: bool,
    /// `TerrainAdjustment.BEARD_BOX`.
    boxed: bool,
    /// После установки кусков нужны постаменты (`WoodlandMansionStructure.afterPlace`).
    mansion: bool,
    /// Руины портала: кусок ставится своим кодом (`RuinedPortalPiece.postProcess`).
    portal: bool,
    /// Океанская руина: высота считается при постановке.
    ruin: bool,
    /// `ENCAPSULATE`: коробка входит в бороду с половинным масштабом.
    encapsulate: bool,
    step: i32,
    seed_index: i32,
}

/// Два набора: деревни и аванпосты. Раскладка кэшируется на сид и чанк старта.
pub(crate) struct Structures {
    sets: Vec<Set>,
    pools: HashMap<String, Pool>,
    /// Шаблоны `woodland_mansion/*` по имени без префикса и `end_city/*`.
    templates: HashMap<String, Arc<template::Template>>,
    /// Шаблоны руин портала.
    portals: portal::Loaded,
    /// Климат биомов для `Properties.cold` руин портала.
    climate: Arc<Vec<super::Climate>>,
    /// Шаблоны океанских руин.
    ruins: ocean_ruin::Loaded,
    empty: Arc<Elem>,
    cache: LayoutCache,
    /// Кольца крепости на сид мира. Финальный чанк заполняется по мере надобности.
    ring_cache: Mutex<HashMap<i64, Vec<RingSlot>>>,
    /// Тег `minecraft:village` → id структур из `tags/worldgen/structure`.
    tags: HashMap<String, HashSet<String>>,
    /// Наборы измерения, которые не разобрались.
    failures: Vec<(String, String)>,
}

impl Structures {
    pub(crate) fn load(
        data_dir: &Path,
        palette: &mut Palette,
        biome_order: &[String],
        biome_ids: &HashMap<String, u32>,
        read: &super::Read<'_>,
        min_y: i32,
        height: i32,
    ) -> Result<Self, String> {
        let mut loader = Loader {
            data_dir,
            palette,
            biome_ids,
            read,
            min_y,
            height,
            pools: HashMap::new(),
            templates: HashMap::new(),
            features: HashMap::new(),
            seeds: feature_seeds(data_dir)?,
            alias_names: HashSet::new(),
        };
        // Все наборы `structure_set/*`, как `ChunkGeneratorStructureState`:
        // набор, ни одна структура которого не встаёт в биомы измерения,
        // в этом измерении не участвует.
        let dimension: HashSet<u32> = biome_order
            .iter()
            .filter_map(|b| biome_ids.get(b).copied())
            .collect();
        let mut names = Vec::new();
        let dir = data_dir.join("data/minecraft/worldgen/structure_set");
        for entry in std::fs::read_dir(&dir).map_err(|e| format!("structure sets: {e}"))? {
            let path = entry.map_err(|e| format!("structure sets: {e}"))?.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json")
                && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
            {
                names.push(format!("minecraft:{stem}"));
            }
        }
        names.sort();
        let mut sets = Vec::new();
        let mut failures = Vec::new();
        for name in names {
            let v = read("structure_set", &name)?;
            let mut fits = false;
            for entry in v["structures"].as_array().into_iter().flatten() {
                let id = entry["structure"].as_str().unwrap_or("");
                let s = read("structure", id)?;
                let biomes = loader.biomes(s["biomes"].as_str().unwrap_or(""))?;
                fits |= biomes.iter().any(|b| dimension.contains(b));
            }
            if !fits {
                continue;
            }
            match loader.set(&name) {
                Ok(set) => sets.push(set),
                Err(e) => {
                    if std::env::var_os("CHUNKWAKE_FEATURE_ERRORS").is_some() {
                        eprintln!("{name}: {e}");
                    }
                    failures.push((name, e));
                }
            }
        }
        loader.close_pools()?;
        let names: Vec<String> = sets.iter().map(|s| s.name.clone()).collect();
        let sets = sets
            .into_iter()
            .map(|s| s.finish(&names))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            sets,
            pools: loader.pools,
            templates: load_mansion_templates(data_dir, palette)?,
            portals: portal::load(data_dir, palette)?,
            climate: palette.climate(),
            ruins: ocean_ruin::load(data_dir, palette)?,
            empty: Arc::new(Elem::Empty),
            cache: Mutex::new(HashMap::new()),
            ring_cache: Mutex::new(HashMap::new()),
            tags: load_structure_tags(data_dir)?,
            failures,
        })
    }

    /// Наборы, которые не разобрались: имя и причина.
    pub(crate) fn failures(&self) -> &[(String, String)] {
        &self.failures
    }

    #[cfg(test)]
    pub(crate) fn structure_count(&self) -> usize {
        self.sets.iter().map(|s| s.kinds.len()).sum()
    }

    /// Борода чанка: жёсткие коробки и стыки в радиусе ядра.
    pub(crate) fn beard_at(&self, columns: &dyn Columns, cx: i32, cz: i32) -> Beard {
        let mut beard = Beard::default();
        // `Beardifier.forStructuresInChunk` берёт старты по ссылкам чанка.
        for (_, sx, sz, layout) in self.nearby(cx, cz, REACH, columns) {
            if !referenced(&layout, sx, sz, cx, cz) {
                continue;
            }
            let adapt = if layout.encapsulate {
                Adapt::Encapsulate
            } else if layout.bury {
                Adapt::Bury
            } else if layout.boxed {
                Adapt::Boxed
            } else if layout.beard {
                Adapt::Thin
            } else {
                continue;
            };
            for piece in &layout.pieces {
                // `isCloseToChunk(chunk, 12)` и для коробки, и для стыков.
                if !near_xz(piece.min, piece.max, cx, cz, 12) {
                    continue;
                }
                if piece.rigid {
                    beard.boxes.push(BeardBox {
                        min: piece.min,
                        max: piece.max,
                        ground: piece.min[1] + piece.ground,
                        adapt,
                    });
                }
                // Стыки берутся строго внутри окна ±12, при любой адаптации.
                for j in &piece.junctions {
                    let (x0, z0) = (cx * 16, cz * 16);
                    if j[0] > x0 - 12 && j[2] > z0 - 12 && j[0] < x0 + 27 && j[2] < z0 + 27 {
                        beard.junctions.push(*j);
                    }
                }
            }
        }
        beard
    }

    /// Ближайший старт структуры или тега `#…` в 100 кольцах клетки `spacing`.
    ///
    /// Выбор вида и проверка биома — как у сборки, но дочерние куски jigsaw
    /// не строятся: ответ — середина чанка старта на высоте поверхности.
    pub(crate) fn nearest(
        &self,
        columns: &dyn Columns,
        origin: [i32; 3],
        name: &str,
    ) -> Option<[i32; 3]> {
        // `ChunkGenerator.findNearestMapStructure`: кольца целиком, затем
        // `random_spread` по радиусам; на первом радиусе с находкой — ближайшая
        // из находок всех наборов по `distSqr` с высотой.
        let wanted = self.wanted(name)?;
        let offer = |p: [i32; 3], best: &mut Option<([i32; 3], i64)>| {
            let d = (0..3)
                .map(|i| (p[i] as i64 - origin[i] as i64).pow(2))
                .sum();
            if best.is_none_or(|(_, b)| d < b) {
                *best = Some((p, d));
            }
        };
        let mut best: Option<([i32; 3], i64)> = None;
        let mut spread = Vec::new();
        for (si, set) in self.sets.iter().enumerate() {
            if !set.kinds.iter().any(|k| wanted.contains(&k.name)) {
                continue;
            }
            if set.rings.is_some() {
                if let Some(p) = self.nearest_rings(si, origin, columns, &wanted) {
                    offer(p, &mut best);
                }
            } else {
                spread.push(si);
            }
        }
        let cx = origin[0] >> 4;
        let cz = origin[2] >> 4;
        for radius in 0i32..=100 {
            if spread.is_empty() {
                break;
            }
            let mut found = false;
            for &si in &spread {
                if let Some(p) = self.spread_ring(si, cx, cz, radius, columns, &wanted) {
                    found = true;
                    offer(p, &mut best);
                }
            }
            if found {
                break;
            }
        }
        best.map(|(p, _)| p)
    }

    /// Первая находка набора на кольце `radius` в порядке x, затем z.
    fn spread_ring(
        &self,
        si: usize,
        cx: i32,
        cz: i32,
        radius: i32,
        columns: &dyn Columns,
        wanted: &HashSet<String>,
    ) -> Option<[i32; 3]> {
        let set = &self.sets[si];
        for x in -radius..=radius {
            let x_edge = x == -radius || x == radius;
            for z in -radius..=radius {
                if !x_edge && z != -radius && z != radius {
                    continue;
                }
                let (sx, sz) = spread_chunk(
                    set.spacing,
                    set.separation,
                    set.salt,
                    columns.seed(),
                    cx.wrapping_add(set.spacing.wrapping_mul(x)),
                    cz.wrapping_add(set.spacing.wrapping_mul(z)),
                    set.triangular,
                );
                if let Some(p) = self.probe(si, sx, sz, columns, wanted) {
                    return Some(p);
                }
            }
        }
        None
    }

    /// Id, которые команда просит: одно имя или развёрнутый тег. Пусто, если
    /// среди поставленных структур такого нет.
    fn wanted(&self, name: &str) -> Option<HashSet<String>> {
        let name = normalize_id(name);
        let ids = if let Some(tag) = name.strip_prefix('#') {
            self.tags.get(tag)?.clone()
        } else {
            HashSet::from([name])
        };
        let placed: HashSet<String> = ids
            .into_iter()
            .filter(|id| {
                self.sets
                    .iter()
                    .any(|s| s.kinds.iter().any(|k| k.name == *id))
            })
            .collect();
        (!placed.is_empty()).then_some(placed)
    }

    /// Старт в этом чанке, если выбранный вид входит в запрос и биом его пускает.
    fn probe(
        &self,
        si: usize,
        sx: i32,
        sz: i32,
        columns: &dyn Columns,
        wanted: &HashSet<String>,
    ) -> Option<[i32; 3]> {
        let seed = columns.seed();
        if !self.is_structure_chunk(si, sx, sz, seed) {
            return None;
        }
        let n = self.sets[si].kinds.len();
        let accept = |ki: usize| -> Option<Option<[i32; 3]>> {
            self.kind_starts(si, ki, sx, sz, columns)?;
            if !wanted.contains(&self.sets[si].kinds[ki].name) {
                return Some(None);
            }
            // `getLocatePos`: угол чанка старта плюс `locate_offset`.
            let off = self.sets[si].locate_offset;
            Some(Some([sx * 16 + off[0], off[1], sz * 16 + off[2]]))
        };
        if n == 1 {
            return accept(0).flatten();
        }
        let mut select = Legacy::new(0);
        select.set_large_feature_seed(seed, sx, sz);
        let mut left: Vec<usize> = (0..n).collect();
        let mut total: i32 = left.iter().map(|&i| self.sets[si].weights[i]).sum();
        while !left.is_empty() && total > 0 {
            let mut roll = select.next_int_bound(total);
            let mut chosen = 0;
            for (n, &i) in left.iter().enumerate() {
                roll -= self.sets[si].weights[i];
                if roll < 0 {
                    chosen = n;
                    break;
                }
            }
            let i = left[chosen];
            if let Some(hit) = accept(i) {
                return hit;
            }
            total -= self.sets[si].weights[i];
            left.remove(chosen);
        }
        None
    }

    /// Биом пускает стартовый кусок. Дочерние соединения jigsaw не собираются.
    fn kind_starts(
        &self,
        si: usize,
        ki: usize,
        sx: i32,
        sz: i32,
        columns: &dyn Columns,
    ) -> Option<()> {
        let kind = &self.sets[si].kinds[ki];
        if let Some(which) = kind.scattered {
            return pyramid::can_start(which, sx, sz, columns, &kind.biomes);
        }
        if let Some(body) = &kind.shore {
            let ocean = matches!(body, Body::Ship { beached: false, .. });
            return shore::surface_ok(columns, sx, sz, &kind.biomes, ocean);
        }
        if kind.shaft.is_some() {
            return self.assemble(si, ki, sx, sz, columns).map(|_| ());
        }
        if let Some(around) = &kind.monument {
            return monument::can_start(sx, sz, columns, &kind.biomes, around);
        }
        if kind.stronghold {
            let biome = columns.biome_at(sx * 16, 0, sz * 16)?;
            return kind.biomes.contains(&biome).then_some(());
        }
        if kind.fortress {
            let biome = columns.biome_at(sx * 16, 64, sz * 16)?;
            return kind.biomes.contains(&biome).then_some(());
        }
        if kind.fossil.is_some() {
            return fossil_layout(kind, sx, sz, columns).map(|_| ());
        }
        if kind.treasure {
            return shore::surface_ok(columns, sx, sz, &kind.biomes, true);
        }
        if kind.mansion || kind.end_city {
            return mansion_start(kind, sx, sz, columns.seed(), columns).map(|_| ());
        }
        if kind.ruin.is_some() {
            return ocean_ruin::can_start(columns, sx, sz, &kind.biomes);
        }
        if kind.portal.is_some() {
            let piece = portal::build(
                &self.portals,
                kind.portal.as_ref()?,
                sx,
                sz,
                columns.seed(),
                columns,
            )?;
            // `isValidBiome` смотрит биом в точке старта — в ней же стоит шаблон.
            let biome = columns.biome_at(piece.origin[0], piece.origin[1], piece.origin[2])?;
            return kind.biomes.contains(&biome).then_some(());
        }
        let mut rng = Legacy::new(0);
        rng.set_large_feature_seed(columns.seed(), sx, sz);
        let planned_y = kind.start_height.map(|h| h.sample(&mut rng));
        let rot = ROTS[rng.next_int_bound(4) as usize];
        let alias_map = resolve_aliases(
            &kind.aliases,
            [sx * 16, planned_y.unwrap_or(0), sz * 16],
            columns.seed(),
        );
        let start_name = aliased(&alias_map, &kind.start_pool);
        let pool = self.pools.get(&start_name)?;
        let element = pick(&self.empty, pool, &mut rng).clone();
        if element.is_empty() {
            return None;
        }
        let origin = [sx * 16, 0, sz * 16];
        let (min, max) = bounds(&element, origin, rot)?;
        let center_x = (max[0] + min[0]) / 2;
        let center_z = (max[2] + min[2]) / 2;
        // `isValidBiome` у jigsaw смотрит точку старта: `start_height` плюс
        // проекция на карту высот, а без проекции — просто `start_height`.
        let stub_y = if kind.project {
            if !columns.column_has(center_x, center_z, &kind.biomes) {
                return None;
            }
            planned_y.unwrap_or(0) + columns.base_height(center_x, center_z)
        } else {
            planned_y.unwrap_or(0)
        };
        let biome = columns.biome_at(center_x, stub_y, center_z)?;
        kind.biomes.contains(&biome).then_some(())
    }

    /// Индекс `setFeatureSeed` структуры: шаг декорации и место в реестре.
    #[cfg(test)]
    pub(crate) fn feature_seed(&self, name: &str) -> Option<(i32, i32)> {
        self.sets
            .iter()
            .flat_map(|set| set.kinds.iter())
            .find_map(|kind| (kind.name == name).then_some((kind.step, kind.seed_index)))
    }

    pub(crate) fn layout_count(&self) -> usize {
        self.cache.lock().unwrap().len()
    }

    pub(crate) fn ring_count(&self) -> usize {
        self.ring_cache.lock().unwrap().len()
    }

    /// Один биом вида: тест поиска подставляет его во все столбцы.
    #[cfg(test)]
    pub(crate) fn one_biome(&self, structure: &str) -> Option<u32> {
        self.sets
            .iter()
            .flat_map(|set| set.kinds.iter())
            .find(|kind| kind.name == structure)?
            .biomes
            .iter()
            .next()
            .copied()
    }

    /// Чанк — старт какого-нибудь поставленного набора (`random_spread`).
    #[cfg(test)]
    pub(crate) fn is_structure_start(&self, cx: i32, cz: i32, seed: i64) -> bool {
        self.sets
            .iter()
            .enumerate()
            .any(|(si, _)| self.is_structure_chunk(si, cx, cz, seed))
    }

    /// Куски, чья коробка задевает декорируемый чанк. Перед кусками одной
    /// структуры генератор сбрасывается `setFeatureSeed` её индекса.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn place(
        &self,
        features: &Features,
        level: &mut dyn Level,
        cx: i32,
        cz: i32,
        columns: &dyn Columns,
        r: &mut Worldgen,
        decoration_seed: i64,
        step: i32,
    ) {
        if !self
            .sets
            .iter()
            .flat_map(|s| &s.kinds)
            .any(|k| k.step == step)
        {
            return;
        }
        let min_y = level.min_y();
        let window = template::Bounds {
            min: [cx * 16, min_y, cz * 16],
            max: [cx * 16 + 15, min_y + level.height() - 1, cz * 16 + 15],
        };
        let mut found = self.nearby(cx, cz, REACH, columns);
        // Только старты со ссылкой из этого чанка; вставка — x снаружи, z внутри.
        found.retain(|(_, sx, sz, layout)| referenced(layout, *sx, *sz, cx, cz));
        found.sort_by(|a, b| {
            a.3.seed_index
                .cmp(&b.3.seed_index)
                .then(a.1.cmp(&b.1))
                .then(a.2.cmp(&b.2))
        });
        // `startsForStructure` обходит `LongOpenHashSet` ссылок, а не порядок
        // вставки: несколько стартов одной структуры в чанке делят генератор
        // в этом порядке.
        let mut start = 0;
        while start < found.len() {
            let mut end = start + 1;
            while end < found.len() && found[end].3.seed_index == found[start].3.seed_index {
                end += 1;
            }
            if end - start > 1 {
                let keys: Vec<i64> = found[start..end]
                    .iter()
                    .map(|(_, sx, sz, _)| (*sx as u32 as i64) | ((*sz as u32 as i64) << 32))
                    .collect();
                let group: Vec<_> = found[start..end].to_vec();
                for (slot, i) in long_set_order(&keys).into_iter().enumerate() {
                    found[start + slot] = group[i].clone();
                }
            }
            start = end;
        }
        let mut seeded: Option<i32> = None;
        for (_, _, _, layout) in &found {
            if layout.step != step {
                continue;
            }
            if seeded != Some(layout.seed_index) {
                r.set_feature_seed(decoration_seed, layout.seed_index, layout.step);
                seeded = Some(layout.seed_index);
            }
            // `StructureStart.placeInChunk`: опорная точка — центр коробки
            // первого куска на её нижнем уровне.
            let reference = layout.pieces.first().map_or([0; 3], |p| {
                let c = |i: usize| p.min[i] + (p.max[i] - p.min[i] + 1) / 2;
                [c(0), p.min[1], c(2)]
            });
            for piece in &layout.pieces {
                if !overlaps(piece.min, piece.max, window.min, window.max) {
                    continue;
                }
                piece
                    .element
                    .place(features, level, piece, r, window, reference);
            }
            // Руины портала и океанские руины ставятся своими Elem; общих
            // действий после кусков у них нет.
            let _ = (layout.portal, layout.ruin);
            if layout.mansion {
                let boxes: Vec<([i32; 3], [i32; 3])> =
                    layout.pieces.iter().map(|p| (p.min, p.max)).collect();
                mansion::after_place(
                    &features.palette,
                    level,
                    &boxes,
                    window,
                    features.palette.defaults["minecraft:cobblestone"],
                );
            }
        }
    }

    fn nearby(
        &self,
        cx: i32,
        cz: i32,
        radius: i32,
        columns: &dyn Columns,
    ) -> Vec<(usize, i32, i32, Arc<Layout>)> {
        let seed = columns.seed();
        let mut out = Vec::new();
        for (si, set) in self.sets.iter().enumerate() {
            if set.rings.is_some() {
                self.nearby_rings(si, cx, cz, radius, columns, &mut out);
                continue;
            }
            let x0 = (cx - radius).div_euclid(set.spacing);
            let x1 = (cx + radius).div_euclid(set.spacing);
            let z0 = (cz - radius).div_euclid(set.spacing);
            let z1 = (cz + radius).div_euclid(set.spacing);
            for cell_z in z0..=z1 {
                for cell_x in x0..=x1 {
                    let probe_x = cell_x.wrapping_mul(set.spacing);
                    let probe_z = cell_z.wrapping_mul(set.spacing);
                    let (sx, sz) = spread_chunk(
                        set.spacing,
                        set.separation,
                        set.salt,
                        seed,
                        probe_x,
                        probe_z,
                        set.triangular,
                    );
                    if (sx - cx).abs() > radius || (sz - cz).abs() > radius {
                        continue;
                    }
                    if !self.is_structure_chunk(si, sx, sz, seed) {
                        continue;
                    }
                    if let Some(layout) = self.layout(si, sx, sz, columns) {
                        out.push((si, sx, sz, layout));
                    }
                }
            }
        }
        out
    }

    fn layout(&self, si: usize, sx: i32, sz: i32, columns: &dyn Columns) -> Option<Arc<Layout>> {
        let key = (si, sx, sz);
        let cell = {
            let mut cache = self.cache.lock().unwrap();
            if cache.len() >= 4096 && !cache.contains_key(&key) {
                cache.clear();
            }
            cache.entry(key).or_default().clone()
        };
        // Сборка вне замка: раньше каждый поток, которому старт понадобился
        // до конца чужой сборки, собирал его сам.
        cell.get_or_init(|| self.build(si, sx, sz, columns)).clone()
    }

    fn build(&self, si: usize, sx: i32, sz: i32, columns: &dyn Columns) -> Option<Arc<Layout>> {
        let t = std::time::Instant::now();
        let built = self.build_layout(si, sx, sz, columns);
        if std::env::var_os("CHUNKWAKE_PROF").is_some() {
            let name = self.sets[si].kinds.first().map_or("?", |k| k.name.as_str());
            let jig = template::JIG.with(|c| c.replace(0.0));
            eprintln!(
                "build {name} {sx} {sz}: {:.2} мс {} (разъёмы {jig:.2})",
                t.elapsed().as_secs_f64() * 1000.0,
                built.is_some()
            );
        }
        // Отладка: `CHUNKWAKE_PIECES=sx,sz` печатает коробки кусков старта.
        if let (Some(layout), Ok(want)) = (&built, std::env::var("CHUNKWAKE_PIECES"))
            && want == format!("{sx},{sz}")
        {
            for (i, p) in layout.pieces.iter().enumerate() {
                eprintln!("наш кусок {i}: {:?} {:?} {:?}", p.min, p.max, p.rot);
            }
        }
        built
    }

    fn build_layout(
        &self,
        si: usize,
        sx: i32,
        sz: i32,
        columns: &dyn Columns,
    ) -> Option<Arc<Layout>> {
        let seed = columns.seed();
        if !self.is_structure_chunk(si, sx, sz, seed) {
            return None;
        }
        let n = self.sets[si].kinds.len();
        if n == 1 {
            return self.assemble(si, 0, sx, sz, columns).map(Arc::new);
        }
        let mut select = Legacy::new(0);
        select.set_large_feature_seed(seed, sx, sz);
        let mut left: Vec<usize> = (0..n).collect();
        let mut total: i32 = left.iter().map(|&i| self.sets[si].weights[i]).sum();
        while !left.is_empty() {
            let mut roll = select.next_int_bound(total);
            let mut chosen = 0;
            for (n, &i) in left.iter().enumerate() {
                roll -= self.sets[si].weights[i];
                if roll < 0 {
                    chosen = n;
                    break;
                }
            }
            let i = left[chosen];
            if let Some(layout) = self.assemble(si, i, sx, sz, columns) {
                return Some(Arc::new(layout));
            }
            total -= self.sets[si].weights[i];
            left.remove(chosen);
        }
        None
    }

    fn is_structure_chunk(&self, si: usize, cx: i32, cz: i32, seed: i64) -> bool {
        let set = &self.sets[si];
        if set.rings.is_some() {
            let cache = self.ring_cache.lock().unwrap();
            return cache
                .get(&seed)
                .is_some_and(|slots| slots.iter().any(|slot| slot.at == Some((cx, cz))));
        }
        let (sx, sz) = spread_chunk(
            set.spacing,
            set.separation,
            set.salt,
            seed,
            cx,
            cz,
            set.triangular,
        );
        if sx != cx || sz != cz {
            return false;
        }
        if !frequency_pass(set.frequency, seed, cx, cz) {
            return false;
        }
        let Some((other, count)) = set.exclusion else {
            return true;
        };
        for x in cx - count..=cx + count {
            for z in cz - count..=cz + count {
                if self.is_structure_chunk(other, x, z, seed) {
                    return false;
                }
            }
        }
        true
    }

    fn assemble(
        &self,
        si: usize,
        ki: usize,
        sx: i32,
        sz: i32,
        columns: &dyn Columns,
    ) -> Option<Layout> {
        let kind = self.sets[si].kinds[ki].clone();
        let seed = columns.seed();
        if let Some(which) = kind.scattered {
            return self.assemble_scattered(which, &kind, sx, sz, seed, columns);
        }
        if kind.shore.is_some() {
            return self.assemble_shore(&kind, sx, sz, seed, columns);
        }
        if kind.shaft.is_some() {
            return self.assemble_shaft(&kind, sx, sz, seed, columns);
        }
        if kind.monument.is_some() {
            return self.assemble_monument(&kind, sx, sz, seed, columns);
        }
        if kind.stronghold {
            return self.assemble_stronghold(&kind, sx, sz, seed, columns);
        }
        if kind.fortress {
            return self.assemble_fortress(&kind, sx, sz, seed, columns);
        }
        if kind.fossil.is_some() {
            return fossil_layout(&kind, sx, sz, columns);
        }
        if kind.treasure {
            shore::surface_ok(columns, sx, sz, &kind.biomes, true)?;
            let piece = treasure::Piece {
                pos: [sx * 16 + 9, 90, sz * 16 + 9],
            };
            let (min, max) = piece.bounds();
            return Some(Layout {
                pieces: vec![Piece {
                    element: Arc::new(Elem::Treasure(Arc::new(piece))),
                    pos: min,
                    rot: Rot::None,
                    ground: 0,
                    min,
                    max,
                    rigid: true,
                    junctions: Vec::new(),
                }],
                beard: false,
                bury: false,
                boxed: false,
                mansion: false,
                portal: false,
                ruin: false,
                encapsulate: false,
                step: kind.step,
                seed_index: kind.seed_index,
            });
        }
        if kind.mansion {
            return self.assemble_mansion(&kind, sx, sz, seed, columns);
        }
        if kind.end_city {
            return self.assemble_end_city(&kind, sx, sz, seed, columns);
        }
        if kind.portal.is_some() {
            return self.assemble_portal(&kind, sx, sz, seed, columns);
        }
        if kind.ruin.is_some() {
            return self.assemble_ruin(&kind, sx, sz, seed, columns);
        }
        let mut rng = Legacy::new(0);
        rng.set_large_feature_seed(seed, sx, sz);
        // `JigsawStructure.findGenerationPoint`: `start_height.sample` идёт
        // ПЕРВЫМ броском, а `Rotation.getRandom` — уже внутри `addPieces`.
        let planned_y = kind.start_height.map(|h| h.sample(&mut rng));
        let rot = ROTS[rng.next_int_bound(4) as usize];
        let origin = [sx * 16, 0, sz * 16];
        // `PoolAliasLookup.create(poolAliases, startPos, seed)`: `startPos` — угол
        // чанка с уже выбранной высотой.
        let alias_map = resolve_aliases(
            &kind.aliases,
            [origin[0], planned_y.unwrap_or(0), origin[2]],
            seed,
        );
        let start_name = aliased(&alias_map, &kind.start_pool);
        let pool = self.pools.get(&start_name)?;
        let element = pick(&self.empty, pool, &mut rng).clone();
        if element.is_empty() {
            return None;
        }
        // `getRandomNamedJigsaw`: шарниры старта перемешиваются от точки
        // старта, и кусок сдвигается так, чтобы названный встал на неё.
        let position = [origin[0], planned_y.unwrap_or(0), origin[2]];
        let local = match &kind.start_jigsaw {
            Some(name) => {
                let found = jigsaws(&element, position, rot, &mut rng)
                    .into_iter()
                    .find(|j| j.name.as_deref().map(mc).as_deref() == Some(name.as_str()))?;
                [
                    found.pos[0] - position[0],
                    found.pos[1] - position[1],
                    found.pos[2] - position[2],
                ]
            }
            None => [0; 3],
        };
        let adjusted = [
            position[0] - local[0],
            position[1] - local[1],
            position[2] - local[2],
        ];
        let (mut min, mut max) = bounds(&element, adjusted, rot)?;
        let center_x = (max[0] + min[0]) / 2;
        let center_z = (max[2] + min[2]) / 2;
        let mut pos = adjusted;
        let stub_y = {
            // `JigsawPlacement.addPieces`: `bottomY` — либо `start_height`,
            // либо `startPos.y + firstFreeHeight` по проекции. Проекция
            // прибавляется к высоте старта, а не заменяет её.
            let ground_y = min[1] + 1;
            let bottom_y = if kind.project {
                if !columns.column_has(center_x, center_z, &kind.biomes) {
                    return None;
                }
                planned_y.unwrap_or(0) + columns.base_height(center_x, center_z)
            } else {
                planned_y?;
                adjusted[1]
            };
            let dy = bottom_y - ground_y;
            min[1] += dy;
            max[1] += dy;
            pos[1] += dy;
            // `isStartTooCloseToWorldHeightLimits`.
            if kind.padding != (0, 0)
                && (min[1] < columns.min_y() + kind.padding.0
                    || max[1] > columns.max_y() - 1 - kind.padding.1)
            {
                return None;
            }
            bottom_y + local[1]
        };
        let biome = columns.biome_at(center_x, stub_y, center_z)?;
        if !kind.biomes.contains(&biome) {
            return None;
        }
        let rigid = element.rigid();
        let mut pieces = vec![Piece {
            element,
            pos,
            rot,
            ground: 1,
            min,
            max,
            rigid,
            junctions: Vec::new(),
        }];
        if kind.size > 0 {
            let allowed_min = [
                center_x - kind.horizontal,
                (stub_y - kind.vertical).max(columns.min_y() + kind.padding.0),
                center_z - kind.horizontal,
            ];
            let allowed_max = [
                center_x + kind.horizontal,
                (stub_y + kind.vertical).min(columns.max_y() - kind.padding.1),
                center_z + kind.horizontal,
            ];
            let space = Rc::new(RefCell::new(Space {
                min: allowed_min,
                max: allowed_max,
                occupied: vec![(min, max)],
            }));
            let mut queue = Queue::default();
            self.try_children(
                &mut pieces,
                &mut rng,
                columns,
                &kind,
                0,
                0,
                &space,
                &mut queue,
                &alias_map,
            );
            while let Some(job) = queue.pop() {
                self.try_children(
                    &mut pieces,
                    &mut rng,
                    columns,
                    &kind,
                    job.index,
                    job.depth,
                    &job.space,
                    &mut queue,
                    &alias_map,
                );
            }
        }
        Some(Layout {
            pieces,
            beard: kind.beard,
            bury: kind.bury,
            boxed: kind.boxed,
            mansion: false,
            portal: false,
            ruin: false,
            encapsulate: kind.encapsulate,
            step: kind.step,
            seed_index: kind.seed_index,
        })
    }

    /// Шахта. Генератор куска свежий: провал биома его не крутит.
    fn assemble_shaft(
        &self,
        kind: &Kind,
        sx: i32,
        sz: i32,
        seed: i64,
        columns: &dyn Columns,
    ) -> Option<Layout> {
        let cfg = kind.shaft.as_ref()?;
        let mut rng = Legacy::new(0);
        rng.set_large_feature_seed(seed, sx, sz);
        let built = mineshaft::build(cfg, sx, sz, &mut rng, columns, &kind.biomes)?;
        let pieces = built
            .into_iter()
            .map(|piece| {
                let (min, max) = piece.bounds();
                Piece {
                    element: Arc::new(Elem::Shaft(Arc::new(piece))),
                    pos: [sx * 16, 0, sz * 16],
                    rot: Rot::None,
                    ground: 0,
                    min,
                    max,
                    rigid: true,
                    junctions: Vec::new(),
                }
            })
            .collect();
        Some(Layout {
            pieces,
            beard: false,
            bury: false,
            boxed: false,
            mansion: false,
            portal: false,
            ruin: false,
            encapsulate: kind.encapsulate,
            step: kind.step,
            seed_index: kind.seed_index,
        })
    }

    /// Крепость Незера: биом в `(minX, 64, minZ)`, дерево без адаптации рельефа.
    fn assemble_fortress(
        &self,
        kind: &Kind,
        sx: i32,
        sz: i32,
        seed: i64,
        columns: &dyn Columns,
    ) -> Option<Layout> {
        let biome = columns.biome_at(sx * 16, 64, sz * 16)?;
        if !kind.biomes.contains(&biome) {
            return None;
        }
        let pieces = fortress::build(seed, sx, sz)
            .into_iter()
            .map(|piece| {
                let (min, max) = piece.bounds();
                Piece {
                    element: Arc::new(Elem::Fortress(Arc::new(piece))),
                    pos: [sx * 16, 64, sz * 16],
                    rot: Rot::None,
                    ground: 0,
                    min,
                    max,
                    rigid: true,
                    junctions: Vec::new(),
                }
            })
            .collect();
        Some(Layout {
            pieces,
            beard: false,
            bury: false,
            boxed: false,
            mansion: false,
            portal: false,
            ruin: false,
            encapsulate: false,
            step: kind.step,
            seed_index: kind.seed_index,
        })
    }

    /// Крепость. Биом старта — `#is_overworld` в начале чанка, куски — `Legacy`.
    fn assemble_stronghold(
        &self,
        kind: &Kind,
        sx: i32,
        sz: i32,
        seed: i64,
        columns: &dyn Columns,
    ) -> Option<Layout> {
        let biome = columns.biome_at(sx * 16, 0, sz * 16)?;
        if !kind.biomes.contains(&biome) {
            return None;
        }
        let built = stronghold::build(seed, sx, sz, columns.sea_level(), columns.min_y());
        if built.is_empty() {
            return None;
        }
        let pieces = built
            .into_iter()
            .map(|piece| {
                let (min, max) = piece.bounds();
                Piece {
                    element: Arc::new(Elem::Stronghold(Arc::new(piece))),
                    pos: [sx * 16, 0, sz * 16],
                    rot: Rot::None,
                    ground: 0,
                    min,
                    max,
                    rigid: true,
                    junctions: Vec::new(),
                }
            })
            .collect();
        Some(Layout {
            pieces,
            beard: false,
            bury: true,
            boxed: false,
            mansion: false,
            portal: false,
            ruin: false,
            encapsulate: kind.encapsulate,
            step: kind.step,
            seed_index: kind.seed_index,
        })
    }

    /// Город Края: старт как у особняка, затем `startHouseTower` тем же
    /// генератором.
    fn assemble_end_city(
        &self,
        kind: &Kind,
        sx: i32,
        sz: i32,
        seed: i64,
        columns: &dyn Columns,
    ) -> Option<Layout> {
        let (start, rot) = mansion_start(kind, sx, sz, seed, columns)?;
        let mut rng = Legacy::new(0);
        rng.set_large_feature_seed(seed, sx, sz);
        let _ = rng.next_int_bound(4);
        let built = end_city::build(&self.templates, start, rot, &mut rng);
        if built.is_empty() {
            return None;
        }
        let pieces = built
            .into_iter()
            .map(|piece| {
                let (min, max) = piece.bounds();
                Piece {
                    element: Arc::new(Elem::EndCity(Arc::new(piece))),
                    pos: [sx * 16, 0, sz * 16],
                    rot: Rot::None,
                    ground: 0,
                    min,
                    max,
                    rigid: true,
                    junctions: Vec::new(),
                }
            })
            .collect();
        Some(Layout {
            pieces,
            beard: false,
            bury: false,
            boxed: false,
            mansion: false,
            portal: false,
            ruin: false,
            encapsulate: false,
            step: kind.step,
            seed_index: kind.seed_index,
        })
    }

    /// Лесной особняк: поворот, старт по четырём углам следа и куски.
    fn assemble_mansion(
        &self,
        kind: &Kind,
        sx: i32,
        sz: i32,
        seed: i64,
        columns: &dyn Columns,
    ) -> Option<Layout> {
        let (start, rot) = mansion_start(kind, sx, sz, seed, columns)?;
        let mut rng = Legacy::new(0);
        rng.set_large_feature_seed(seed, sx, sz);
        // `findGenerationPoint` уже снял `Rotation.getRandom`; куски продолжают
        // тот же генератор.
        let _ = rng.next_int_bound(4);
        let built = mansion::build(&self.templates, sx, sz, start[1], rot, &mut rng);
        if built.is_empty() {
            return None;
        }
        let pieces: Vec<Piece> = built
            .into_iter()
            .map(|piece| {
                let (min, max) = piece.bounds();
                Piece {
                    element: Arc::new(Elem::Mansion(Arc::new(piece))),
                    pos: [sx * 16, 0, sz * 16],
                    rot: Rot::None,
                    ground: 0,
                    min,
                    max,
                    rigid: true,
                    junctions: Vec::new(),
                }
            })
            .collect();
        let _ = rot;
        Some(Layout {
            pieces,
            beard: false,
            bury: false,
            boxed: false,
            mansion: true,
            portal: false,
            ruin: false,
            encapsulate: kind.encapsulate,
            step: kind.step,
            seed_index: kind.seed_index,
        })
    }

    /// Океанские руины: набор шаблонов с высотой, считаемой при постановке.
    fn assemble_ruin(
        &self,
        kind: &Kind,
        sx: i32,
        sz: i32,
        seed: i64,
        columns: &dyn Columns,
    ) -> Option<Layout> {
        let cfg = kind.ruin?;
        ocean_ruin::can_start(columns, sx, sz, &kind.biomes)?;
        let built = ocean_ruin::build(&self.ruins, cfg, sx, sz, seed);
        if built.is_empty() {
            return None;
        }
        let pieces: Vec<Piece> = built
            .into_iter()
            .map(|piece| {
                let (min, max) = piece.bounds();
                Piece {
                    element: Arc::new(Elem::Ruin(Arc::new(piece))),
                    pos: [sx * 16, 0, sz * 16],
                    rot: Rot::None,
                    ground: 0,
                    min,
                    max,
                    rigid: true,
                    junctions: Vec::new(),
                }
            })
            .collect();
        Some(Layout {
            pieces,
            beard: false,
            bury: false,
            boxed: false,
            mansion: false,
            portal: false,
            ruin: true,
            encapsulate: false,
            step: kind.step,
            seed_index: kind.seed_index,
        })
    }

    /// Руины портала: один кусок, высота по `findSuitableY`.
    fn assemble_portal(
        &self,
        kind: &Kind,
        sx: i32,
        sz: i32,
        seed: i64,
        columns: &dyn Columns,
    ) -> Option<Layout> {
        let setups = kind.portal.as_ref()?;
        let mut piece = portal::build(&self.portals, setups, sx, sz, seed, columns)?;
        // `isValidBiome`: биом в точке старта.
        let biome = columns.biome_at(piece.origin[0], piece.origin[1], piece.origin[2])?;
        if !kind.biomes.contains(&biome) {
            return None;
        }
        // `Properties.cold`: `coldEnoughToSnow` шумового биома в `origin`.
        piece.setup.cold = piece.setup.can_be_cold
            && super::cold_enough_to_snow(&self.climate, biome, piece.origin, columns.sea_level());
        Some(Layout {
            pieces: vec![Piece {
                element: Arc::new(Elem::Portal(Arc::new(piece))),
                pos: [sx * 16, 0, sz * 16],
                rot: Rot::None,
                ground: 0,
                min: [0; 3],
                max: [0; 3],
                rigid: true,
                junctions: Vec::new(),
            }]
            .into_iter()
            .map(|mut p| {
                let (min, max) = match &*p.element {
                    Elem::Portal(piece) => piece.bounds(),
                    _ => unreachable!(),
                };
                p.min = min;
                p.max = max;
                p
            })
            .collect(),
            beard: false,
            bury: false,
            boxed: false,
            mansion: false,
            portal: true,
            ruin: false,
            encapsulate: kind.encapsulate,
            step: kind.step,
            seed_index: kind.seed_index,
        })
    }

    fn ensure_rings(&self, seed: i64) {
        if self.ring_cache.lock().unwrap().contains_key(&seed) {
            return;
        }
        let Some(plan) = self.sets.iter().find_map(|set| set.rings.as_ref()) else {
            return;
        };
        let slots = stronghold::plan_rings(seed, plan.distance, plan.spread, plan.count)
            .into_iter()
            .map(|spot| RingSlot {
                initial: spot.initial,
                fork: spot.fork,
                at: None,
            })
            .collect();
        let mut cache = self.ring_cache.lock().unwrap();
        if cache.len() >= 64 && !cache.contains_key(&seed) {
            let old: Vec<_> = cache.keys().take(32).copied().collect();
            for key in old {
                cache.remove(&key);
            }
        }
        cache.entry(seed).or_insert(slots);
    }

    fn resolve_ring(&self, seed: i64, index: usize, columns: &dyn Columns) -> (i32, i32) {
        self.ensure_rings(seed);
        if let Some(pos) = self
            .ring_cache
            .lock()
            .unwrap()
            .get(&seed)
            .and_then(|slots| slots.get(index))
            .and_then(|slot| slot.at)
        {
            return pos;
        }
        let (initial, fork, preferred) = {
            let cache = self.ring_cache.lock().unwrap();
            let slot = &cache[&seed][index];
            let preferred = self
                .sets
                .iter()
                .find_map(|set| set.rings.as_ref())
                .map(|plan| plan.preferred.clone())
                .unwrap_or_default();
            (slot.initial, slot.fork, preferred)
        };
        let pos = stronghold::adjust_chunk(initial, fork, &preferred, columns);
        if let Some(slot) = self
            .ring_cache
            .lock()
            .unwrap()
            .get_mut(&seed)
            .and_then(|slots| slots.get_mut(index))
        {
            slot.at = Some(pos);
        }
        pos
    }

    /// Финальный чанк сдвигается от начального не дальше квадрата в 112 блоков.
    fn nearby_rings(
        &self,
        si: usize,
        cx: i32,
        cz: i32,
        radius: i32,
        columns: &dyn Columns,
        out: &mut Vec<(usize, i32, i32, Arc<Layout>)>,
    ) {
        let seed = columns.seed();
        self.ensure_rings(seed);
        let initials: Vec<(i32, i32)> = self.ring_cache.lock().unwrap()[&seed]
            .iter()
            .map(|slot| slot.initial)
            .collect();
        let margin = radius + 8;
        for (index, (ix, iz)) in initials.into_iter().enumerate() {
            if (ix - cx).abs() > margin || (iz - cz).abs() > margin {
                continue;
            }
            let (sx, sz) = self.resolve_ring(seed, index, columns);
            if (sx - cx).abs() > radius || (sz - cz).abs() > radius {
                continue;
            }
            if out
                .iter()
                .any(|(i, x, z, _)| *i == si && *x == sx && *z == sz)
            {
                continue;
            }
            if let Some(layout) = self.layout(si, sx, sz, columns) {
                out.push((si, sx, sz, layout));
            }
        }
    }

    fn nearest_rings(
        &self,
        si: usize,
        origin: [i32; 3],
        columns: &dyn Columns,
        wanted: &HashSet<String>,
    ) -> Option<[i32; 3]> {
        // Ваниль перебирает готовые точки колец, меряет середину чанка на
        // высоте 32 и проверяет только точки ближе текущей. Здесь порядок —
        // по начальным точкам: сдвиг к биому короче запаса `reach`.
        let seed = columns.seed();
        self.ensure_rings(seed);
        let initials: Vec<(i32, i32)> = self.ring_cache.lock().unwrap()[&seed]
            .iter()
            .map(|slot| slot.initial)
            .collect();
        let mut order: Vec<usize> = (0..initials.len()).collect();
        order.sort_by_key(|&index| ring_dist(initials[index], origin));
        let mut best: Option<([i32; 3], i64, usize)> = None;
        for index in order {
            if let Some((_, best_d, _)) = best {
                let reach = (best_d as f64).sqrt() + 200.0;
                if (ring_dist(initials[index], origin) as f64).sqrt() > reach {
                    break;
                }
            }
            let (sx, sz) = self.resolve_ring(seed, index, columns);
            let d = ring_dist((sx, sz), origin);
            let closer = best.is_none_or(|(_, b, bi)| d < b || (d == b && index < bi));
            if !closer {
                continue;
            }
            if let Some(pos) = self.probe(si, sx, sz, columns, wanted) {
                best = Some((pos, d, index));
            }
        }
        best.map(|(p, _, _)| p)
    }

    /// Монумент. Куб биомов вокруг моря и дно океана в середине чанка.
    fn assemble_monument(
        &self,
        kind: &Kind,
        sx: i32,
        sz: i32,
        seed: i64,
        columns: &dyn Columns,
    ) -> Option<Layout> {
        let around = kind.monument.as_ref()?;
        monument::can_start(sx, sz, columns, &kind.biomes, around)?;
        let mut rng = Legacy::new(0);
        rng.set_large_feature_seed(seed, sx, sz);
        let pieces = monument::build(&mut rng, sx, sz)
            .into_iter()
            .map(|piece| {
                let (min, max) = piece.bounds();
                Piece {
                    element: Arc::new(Elem::Monument(Arc::new(piece))),
                    pos: [sx * 16, 39, sz * 16],
                    rot: Rot::None,
                    ground: 0,
                    min,
                    max,
                    rigid: true,
                    junctions: Vec::new(),
                }
            })
            .collect();
        Some(Layout {
            pieces,
            beard: false,
            bury: false,
            boxed: false,
            mansion: false,
            portal: false,
            ruin: false,
            encapsulate: kind.encapsulate,
            step: kind.step,
            seed_index: kind.seed_index,
        })
    }

    /// Один кусок на y = 64. Поворот — `setLargeFeatureSeed` и
    /// `Plane.HORIZONTAL` (север, восток, юг, запад).
    fn assemble_scattered(
        &self,
        which: Which,
        kind: &Kind,
        sx: i32,
        sz: i32,
        seed: i64,
        columns: &dyn Columns,
    ) -> Option<Layout> {
        pyramid::can_start(which, sx, sz, columns, &kind.biomes)?;
        let mut rng = Legacy::new(0);
        rng.set_large_feature_seed(seed, sx, sz);
        let facing = rng.next_int_bound(4) as u8;
        let built = pyramid::Piece::new(which, facing, sx, sz);
        let (min, max) = built.bounds();
        Some(Layout {
            pieces: vec![Piece {
                element: Arc::new(Elem::Scattered(Arc::new(built))),
                pos: [sx * 16, 64, sz * 16],
                rot: Rot::None,
                ground: 0,
                min,
                max,
                rigid: true,
                junctions: Vec::new(),
            }],
            beard: false,
            bury: false,
            boxed: false,
            mansion: false,
            portal: false,
            ruin: false,
            encapsulate: kind.encapsulate,
            step: kind.step,
            seed_index: kind.seed_index,
        })
    }

    /// Иглу или корабль. Генератор куска свежий: провал биома его не крутит.
    fn assemble_shore(
        &self,
        kind: &Kind,
        sx: i32,
        sz: i32,
        seed: i64,
        columns: &dyn Columns,
    ) -> Option<Layout> {
        let body = kind.shore.as_ref()?;
        let ocean = matches!(body, Body::Ship { beached: false, .. });
        shore::surface_ok(columns, sx, sz, &kind.biomes, ocean)?;
        let mut rng = Legacy::new(0);
        rng.set_large_feature_seed(seed, sx, sz);
        let built = shore::build(body, sx, sz, &mut rng, columns);
        let pieces = built
            .into_iter()
            .map(|piece| {
                let (min, max) = piece.bounds();
                Piece {
                    element: Arc::new(Elem::Shore(Arc::new(piece))),
                    pos: [sx * 16, 0, sz * 16],
                    rot: Rot::None,
                    ground: 0,
                    min,
                    max,
                    rigid: true,
                    junctions: Vec::new(),
                }
            })
            .collect();
        Some(Layout {
            pieces,
            beard: false,
            bury: false,
            boxed: false,
            mansion: false,
            portal: false,
            ruin: false,
            encapsulate: kind.encapsulate,
            step: kind.step,
            seed_index: kind.seed_index,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn try_children(
        &self,
        pieces: &mut Vec<Piece>,
        rng: &mut Legacy,
        columns: &dyn Columns,
        kind: &Kind,
        index: usize,
        depth: i32,
        space: &Rc<RefCell<Space>>,
        queue: &mut Queue,
        alias_map: &HashMap<String, String>,
    ) {
        let parent = &pieces[index];
        let parent_min = parent.min;
        let parent_max = parent.max;
        let parent_ground = parent.ground;
        let parent_rigid = parent.rigid;
        let parent_pos = parent.pos;
        let parent_rot = parent.rot;
        let element = Arc::clone(&parent.element);
        let jigs = jigsaws(&element, parent_pos, parent_rot, rng);
        let mut inside: Option<Rc<RefCell<Space>>> = None;
        for parent_jig in &jigs {
            let front = step(&parent_jig.front);
            let target = [
                parent_jig.pos[0] + front[0],
                parent_jig.pos[1] + front[1],
                parent_jig.pos[2] + front[2],
            ];
            let from_bottom = parent_jig.pos[1] - parent_min[1];
            // `poolAliasLookup.lookup(sourceJigsaw.pool())`.
            let source_pool = aliased(alias_map, &parent_jig.pool);
            let (primary_templates, fallback_name) = {
                let Some(primary) = self.pools.get(&source_pool) else {
                    continue;
                };
                if primary.templates.is_empty() && source_pool != "minecraft:empty" {
                    continue;
                }
                (primary.templates.clone(), primary.fallback.clone())
            };
            let fallback_templates = {
                let Some(fallback) = self.pools.get(&fallback_name) else {
                    continue;
                };
                if fallback.templates.is_empty() && fallback_name != "minecraft:empty" {
                    continue;
                }
                fallback.templates.clone()
            };
            let mut candidates = Vec::new();
            if depth != kind.size {
                let mut list = primary_templates;
                template::shuffle(&mut list, rng);
                candidates.extend(list);
            }
            {
                let mut list = fallback_templates;
                template::shuffle(&mut list, rng);
                candidates.extend(list);
            }
            let shape = if inside_box(parent_min, parent_max, target) {
                inside.get_or_insert_with(|| {
                    Rc::new(RefCell::new(Space {
                        min: parent_min,
                        max: parent_max,
                        occupied: Vec::new(),
                    }))
                });
                inside.as_ref().unwrap().clone()
            } else {
                Rc::clone(space)
            };
            let mut surface: Option<i32> = None;
            let mut attached = false;
            for candidate in &candidates {
                if candidate.is_empty() {
                    break;
                }
                let mut rots = ROTS.to_vec();
                template::shuffle(&mut rots, rng);
                for rot in rots {
                    let child_jigs = jigsaws(candidate, [0, 0, 0], rot, rng);
                    let Some((zmin, zmax)) = bounds(candidate, [0, 0, 0], rot) else {
                        continue;
                    };
                    let yspan = zmax[1] - zmin[1] + 1;
                    let expand = if kind.expansion && yspan <= 16 {
                        child_jigs
                            .iter()
                            .map(|j| self.expand_amount(j, zmin, zmax, alias_map))
                            .max()
                            .unwrap_or(0)
                    } else {
                        0
                    };
                    for child_jig in &child_jigs {
                        if !can_attach(parent_jig, child_jig) {
                            continue;
                        }
                        let place = [
                            target[0] - child_jig.pos[0],
                            target[1] - child_jig.pos[1],
                            target[2] - child_jig.pos[2],
                        ];
                        let Some((bmin, bmax)) = bounds(candidate, place, rot) else {
                            continue;
                        };
                        // `JigsawBlock.getFrontFacing(source).getStepY()`: у
                        // ванили это шаг фронта РОДИТЕЛЯ, не ребёнка.
                        let step_y = front[1];
                        let vertical = from_bottom - child_jig.pos[1] + step_y;
                        let child_rigid = candidate.rigid();
                        let child_min_y = if parent_rigid && child_rigid {
                            parent_min[1] + vertical
                        } else {
                            let surface = *surface.get_or_insert_with(|| {
                                columns.base_height(parent_jig.pos[0], parent_jig.pos[2])
                            });
                            surface - child_jig.pos[1]
                        };
                        let delta = child_min_y - bmin[1];
                        let bmin = [bmin[0], bmin[1] + delta, bmin[2]];
                        let mut bmax = [bmax[0], bmax[1] + delta, bmax[2]];
                        let place = [place[0], place[1] + delta, place[2]];
                        if expand > 0 {
                            let span = (expand + 1).max(bmax[1] - bmin[1]);
                            let y = bmin[1] + span;
                            if y > bmax[1] {
                                bmax[1] = y;
                            }
                        }
                        if !shape.borrow().fits(bmin, bmax) {
                            continue;
                        }
                        shape.borrow_mut().occupied.push((bmin, bmax));
                        let new_ground = if child_rigid {
                            parent_ground - vertical
                        } else {
                            1
                        };
                        let source_y = if parent_rigid {
                            parent_min[1] + from_bottom
                        } else if child_rigid {
                            child_min_y + child_jig.pos[1]
                        } else {
                            let surface = *surface.get_or_insert_with(|| {
                                columns.base_height(parent_jig.pos[0], parent_jig.pos[2])
                            });
                            surface + vertical / 2
                        };
                        let child_index = pieces.len();
                        pieces.push(Piece {
                            element: Arc::clone(candidate),
                            pos: place,
                            rot,
                            ground: new_ground,
                            min: bmin,
                            max: bmax,
                            rigid: child_rigid,
                            junctions: vec![
                                [target[0], source_y - from_bottom + parent_ground, target[2]],
                                [
                                    parent_jig.pos[0],
                                    source_y - child_jig.pos[1] + new_ground,
                                    parent_jig.pos[2],
                                ],
                            ],
                        });
                        if depth < kind.size {
                            queue.push(
                                parent_jig.placement_priority,
                                Job {
                                    index: child_index,
                                    depth: depth + 1,
                                    space: Rc::clone(&shape),
                                },
                            );
                        }
                        attached = true;
                        break;
                    }
                    if attached {
                        break;
                    }
                }
                if attached {
                    break;
                }
            }
        }
    }

    fn expand_amount(
        &self,
        jig: &JigInfo,
        min: [i32; 3],
        max: [i32; 3],
        aliases: &HashMap<String, String>,
    ) -> i32 {
        let s = step(&jig.front);
        let front = [jig.pos[0] + s[0], jig.pos[1] + s[1], jig.pos[2] + s[2]];
        if !inside_box(min, max, front) {
            return 0;
        }
        let name = aliased(aliases, &jig.pool);
        let Some(pool) = self.pools.get(&name) else {
            return 0;
        };
        let fallback = self
            .pools
            .get(&pool.fallback)
            .map(|p| p.max_size)
            .unwrap_or(0);
        pool.max_size.max(fallback)
    }
}

struct Job {
    index: usize,
    depth: i32,
    space: Rc<RefCell<Space>>,
}

/// `SequencedPriorityIterator`: очередь на каждый приоритет, всегда берётся
/// самый высокий. Кэш «текущего» приоритета повторяет ванильный: при
/// добавлении с приоритетом не ниже текущего кэш переключается на новую
/// очередь, и старая продолжает ждать, пока не опустеет новая.
#[derive(Default)]
struct Queue {
    map: HashMap<i32, VecDeque<Job>>,
    current: Option<i32>,
}

impl Queue {
    fn push(&mut self, priority: i32, job: Job) {
        self.map.entry(priority).or_default().push_back(job);
        if self.current.is_none_or(|c| priority >= c) {
            self.current = Some(priority);
        }
    }

    fn pop(&mut self) -> Option<Job> {
        let key = self.current?;
        let q = self.map.get_mut(&key)?;
        let job = q.pop_front()?;
        if q.is_empty() {
            self.map.remove(&key);
            // `switchCacheToNextHighestPrioQueue`: следующий по убыванию.
            self.current = self.map.keys().copied().max();
        }
        Some(job)
    }
}

struct Space {
    min: [i32; 3],
    max: [i32; 3],
    occupied: Vec<([i32; 3], [i32; 3])>,
}

impl Space {
    fn fits(&self, min: [i32; 3], max: [i32; 3]) -> bool {
        (0..3).all(|i| min[i] >= self.min[i] && max[i] <= self.max[i])
            && !self.occupied.iter().any(|&(a, b)| overlaps(min, max, a, b))
    }
}

/// Старты в радиусе 12 чанков: 80 блоков от центра, размер куска и ядро бороды.
const REACH: i32 = 12;

/// `GenerationStep.Decoration.values()` — порядковый номер шага.
const DECORATION: &[&str] = &[
    "raw_generation",
    "lakes",
    "local_modifications",
    "underground_structures",
    "surface_structures",
    "strongholds",
    "underground_ores",
    "underground_decoration",
    "fluid_springs",
    "vegetal_decoration",
    "top_layer_modification",
];

/// Индекс `setFeatureSeed`: реестр сортируется по идентификатору, внутри шага
/// номер — порядок встречи в этом списке.
/// Шаблоны особняка: все файлы `structure/woodland_mansion/*.nbt`.
fn load_mansion_templates(
    data_dir: &Path,
    palette: &Palette,
) -> Result<HashMap<String, Arc<template::Template>>, String> {
    let mut out = HashMap::new();
    load_template_dir(data_dir, palette, "woodland_mansion", "", &mut out)?;
    // Город Края — с префиксом, чтобы имена не смешались с особняком.
    load_template_dir(data_dir, palette, "end_city", "end_city/", &mut out)?;
    Ok(out)
}

fn load_template_dir(
    data_dir: &Path,
    palette: &Palette,
    folder: &str,
    prefix: &str,
    out: &mut HashMap<String, Arc<template::Template>>,
) -> Result<(), String> {
    let dir = data_dir.join("data/minecraft/structure").join(folder);
    for entry in std::fs::read_dir(&dir).map_err(|e| format!("mansion templates: {e}"))? {
        let entry = entry.map_err(|e| format!("mansion templates: {e}"))?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("nbt") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let id = format!("minecraft:{folder}/{stem}");
        let loaded = template::load_template(data_dir, palette, &id)?;
        out.insert(format!("{prefix}{stem}"), Arc::new(loaded));
    }
    Ok(())
}

/// Старт особняка: `findGenerationPoint` без проверки биома.
///
/// Биом проверяется по всему столбцу центра чанка (`couldValidBiomeExistIn
/// TerrainColumn`), затем поворот, высота по четырём углам 5×5 и порог 60.
fn mansion_start(
    kind: &Kind,
    sx: i32,
    sz: i32,
    seed: i64,
    columns: &dyn Columns,
) -> Option<([i32; 3], Rot)> {
    let x = sx * 16 + 7;
    let z = sz * 16 + 7;
    if !columns.column_has(x, z, &kind.biomes) {
        return None;
    }
    let mut rng = Legacy::new(0);
    rng.set_large_feature_seed(seed, sx, sz);
    let rot = ROTS[rng.next_int_bound(4) as usize];
    let (ox, oz) = match rot {
        Rot::None => (5, 5),
        Rot::Cw90 => (-5, 5),
        Rot::Cw180 => (-5, -5),
        Rot::Ccw90 => (5, -5),
    };
    // `Structure.getLowestY` по четырём углам `WORLD_SURFACE_WG`.
    let y = [
        columns.base_height(x, z) - 1,
        columns.base_height(x, z + oz) - 1,
        columns.base_height(x + ox, z) - 1,
        columns.base_height(x + ox, z + oz) - 1,
    ]
    .into_iter()
    .min()?;
    if y < 60 {
        return None;
    }
    Some(([x, y, z], rot))
}

/// `HeightProvider` высоты старта jigsaw.
#[derive(Clone, Copy, Debug)]
enum StartHeight {
    /// `ConstantHeight`: бросков не тратит.
    Constant(i32),
    /// `UniformHeight`: `randomBetweenInclusive`, даже при равных краях.
    Uniform(i32, i32),
}

impl StartHeight {
    fn sample(self, rng: &mut Legacy) -> i32 {
        match self {
            Self::Constant(y) => y,
            Self::Uniform(lo, hi) if lo > hi => lo,
            Self::Uniform(lo, hi) => rng.between_inclusive(lo, hi),
        }
    }
}

/// `VerticalAnchor`: `absolute`, `above_bottom`, `below_top`.
fn anchor(v: &Value) -> i32 {
    if let Some(abs) = v["absolute"].as_i64() {
        return abs as i32;
    }
    // `WorldGenerationContext` в этих настройках не нужен: оба края
    // триальных камер заданы абсолютно.
    if let Some(above) = v["above_bottom"].as_i64() {
        return ABOVE_BOTTOM.with(|m| *m.borrow()) + above as i32;
    }
    if let Some(below) = v["below_top"].as_i64() {
        return BELOW_TOP.with(|m| *m.borrow()) - below as i32;
    }
    0
}

thread_local! {
    /// Низ и верх мира для `above_bottom`/`below_top` в `start_height`.
    pub(crate) static ABOVE_BOTTOM: std::cell::RefCell<i32> = const { std::cell::RefCell::new(-64) };
    pub(crate) static BELOW_TOP: std::cell::RefCell<i32> = const { std::cell::RefCell::new(320) };
}

/// `PoolAliasBinding`: `direct` фиксирован, `random` — один взвешенный выбор,
/// `random_group` — выбор группы, внутри которой пары раскрываются по порядку.
#[derive(Clone, Debug)]
pub(crate) enum PoolAlias {
    Direct {
        alias: String,
        target: String,
    },
    Random {
        alias: String,
        targets: Vec<(String, i32)>,
    },
    Group {
        groups: Vec<(Vec<PoolAlias>, i32)>,
    },
}

fn load_pool_aliases(v: &Value) -> Result<Vec<PoolAlias>, String> {
    let mut out = Vec::new();
    let Some(list) = v.as_array() else {
        return Ok(out);
    };
    for alias in list {
        out.push(parse_alias(alias)?);
    }
    Ok(out)
}

fn parse_alias(v: &Value) -> Result<PoolAlias, String> {
    match v["type"].as_str().unwrap_or("") {
        "minecraft:direct" => Ok(PoolAlias::Direct {
            alias: mc(v["alias"].as_str().unwrap_or("")),
            target: mc(v["target"].as_str().unwrap_or("")),
        }),
        "minecraft:random" => {
            let mut targets = Vec::new();
            for t in v["targets"].as_array().into_iter().flatten() {
                targets.push((
                    mc(t["data"].as_str().unwrap_or("")),
                    t["weight"].as_i64().unwrap_or(1) as i32,
                ));
            }
            Ok(PoolAlias::Random {
                alias: mc(v["alias"].as_str().unwrap_or("")),
                targets,
            })
        }
        "minecraft:random_group" => {
            let mut groups = Vec::new();
            for group in v["groups"].as_array().into_iter().flatten() {
                let mut inner = Vec::new();
                for entry in group["data"].as_array().into_iter().flatten() {
                    inner.push(parse_alias(entry)?);
                }
                groups.push((inner, group["weight"].as_i64().unwrap_or(1) as i32));
            }
            Ok(PoolAlias::Group { groups })
        }
        other => Err(format!("pool alias {other}")),
    }
}

/// `PoolAliasLookup.create`: один `RandomSource` от `forkPositional().at(pos)`,
/// затем пары раскрываются по порядку. Повторный псевдоним перезаписывает.
fn resolve_aliases(bindings: &[PoolAlias], pos: [i32; 3], seed: i64) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if bindings.is_empty() {
        return out;
    }
    let mut base = crate::random::Xoroshiro::from_seed(seed);
    let factory = base.fork_positional();
    let mut rng = Worldgen::from_rng(factory.at(pos[0], pos[1], pos[2]));
    for binding in bindings {
        binding.resolve(&mut rng, &mut out);
    }
    out
}

impl PoolAlias {
    /// Имена псевдонимов: файлов для них нет, грузить нечего.
    fn collect_names(&self, out: &mut HashSet<String>) {
        match self {
            Self::Direct { alias, .. } => {
                out.insert(alias.clone());
            }
            Self::Random { alias, .. } => {
                out.insert(alias.clone());
            }
            Self::Group { groups } => {
                for (inner, _) in groups {
                    for binding in inner {
                        binding.collect_names(out);
                    }
                }
            }
        }
    }

    /// Цели псевдонимов: на них могут ссылаться только отсюда, поэтому их
    /// нужно загрузить явно.
    fn collect_targets(&self, out: &mut HashSet<String>) {
        match self {
            Self::Direct { target, .. } => {
                out.insert(target.clone());
            }
            Self::Random { targets, .. } => out.extend(targets.iter().map(|(t, _)| t.clone())),
            Self::Group { groups } => {
                for (inner, _) in groups {
                    for binding in inner {
                        binding.collect_targets(out);
                    }
                }
            }
        }
    }

    fn resolve(&self, rng: &mut Worldgen, out: &mut HashMap<String, String>) {
        match self {
            Self::Direct { alias, target } => {
                out.insert(alias.clone(), target.clone());
            }
            Self::Random { alias, targets } => {
                let total: i32 = targets.iter().map(|(_, w)| *w).sum();
                if total <= 0 {
                    return;
                }
                let mut roll = rng.next_int_bound(total);
                for (target, weight) in targets {
                    roll -= *weight;
                    if roll < 0 {
                        out.insert(alias.clone(), target.clone());
                        return;
                    }
                }
            }
            Self::Group { groups } => {
                let total: i32 = groups.iter().map(|(_, w)| *w).sum();
                if total <= 0 {
                    return;
                }
                let mut roll = rng.next_int_bound(total);
                for (inner, weight) in groups {
                    roll -= *weight;
                    if roll < 0 {
                        for binding in inner {
                            binding.resolve(rng, out);
                        }
                        return;
                    }
                }
            }
        }
    }
}

/// `PoolAliasLookup`: имя пула заменяется на цель псевдонима.
/// `PoolAliasLookup.lookup` по уже раскрытой карте.
fn aliased(aliases: &HashMap<String, String>, name: &str) -> String {
    aliases
        .get(name)
        .cloned()
        .unwrap_or_else(|| name.to_owned())
}

fn feature_seeds(data_dir: &Path) -> Result<HashMap<String, (i32, i32)>, String> {
    let dir = data_dir.join("data/minecraft/worldgen/structure");
    let mut rows = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| format!("structures: {e}"))? {
        let entry = entry.map_err(|e| format!("structures: {e}"))?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| format!("structure name {}", path.display()))?;
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{stem}: {e}"))?;
        let v: Value = serde_json::from_str(&text).map_err(|e| format!("{stem}: {e}"))?;
        let step_name = v["step"].as_str().unwrap_or("surface_structures");
        let step = DECORATION
            .iter()
            .position(|name| *name == step_name)
            .ok_or_else(|| format!("{stem}: step {step_name}"))? as i32;
        rows.push((stem.to_owned(), step));
    }
    rows.sort();
    let mut count = [0i32; DECORATION.len()];
    let mut out = HashMap::new();
    for (name, step) in rows {
        let index = count[step as usize];
        count[step as usize] += 1;
        out.insert(format!("minecraft:{name}"), (step, index));
    }
    Ok(out)
}

pub(crate) struct FossilCfg {
    templates: Vec<Arc<Template>>,
    low: i32,
    high: i32,
}

/// `NetherFossilPiece`: шаблон с `STRUCTURE_AND_AIR`, окно чанка
/// расширено на всю окаменелость.
pub(crate) struct FossilPiece {
    template: Arc<Template>,
    rot: Rot,
    pos: [i32; 3],
    bounds: template::Bounds,
}

impl FossilPiece {
    fn place(
        &self,
        pal: &Palette,
        level: &mut dyn Level,
        r: &mut Worldgen,
        window: template::Bounds,
        seed: i64,
    ) {
        let b = self.bounds;
        let wide = template::Bounds {
            min: [0, 1, 2].map(|i| window.min[i].min(b.min[i])),
            max: [0, 1, 2].map(|i| window.max[i].max(b.max[i])),
        };
        template::place_pool(
            pal,
            level,
            &self.template,
            self.rot,
            self.pos,
            &[],
            true,
            false,
            r,
            wide,
            seed,
            self.pos,
        );
        // `placeDriedGhast`: `forkPositional().at(center)` от сида мира.
        let factory = Legacy::new(seed).next_long();
        let center = [0, 1, 2].map(|i| b.min[i] + (b.max[i] - b.min[i] + 1) / 2);
        let mut pr = Legacy::new(pos_seed(center[0], center[1], center[2]) ^ factory);
        if pr.next_float() < 0.5 {
            let x = b.min[0] + pr.next_int_bound(b.max[0] - b.min[0] + 1);
            let y = b.min[1];
            let z = b.min[2] + pr.next_int_bound(b.max[2] - b.min[2] + 1);
            if pal.is_air(level.get(x, y, z)) && wide.contains([x, y, z]) {
                let rot = ROTS[pr.next_int_bound(4) as usize];
                let ghast = pal.defaults["minecraft:dried_ghast"];
                if level.can_write(x, z) {
                    level.set(x, y, z, template::rotate_state(pal, ghast, rot));
                }
            }
        }
    }
}

/// `NetherFossilStructure.findGenerationPoint` и `isValidBiome` по точке старта.
fn fossil_layout(kind: &Kind, sx: i32, sz: i32, columns: &dyn Columns) -> Option<Layout> {
    let cfg = kind.fossil.as_ref()?;
    let mut rng = Legacy::new(0);
    rng.set_large_feature_seed(columns.seed(), sx, sz);
    let x = sx * 16 + rng.next_int_bound(16);
    let z = sz * 16 + rng.next_int_bound(16);
    let sea = columns.sea_level();
    let mut y = if cfg.low > cfg.high {
        cfg.low
    } else {
        cfg.low + rng.next_int_bound(cfg.high - cfg.low + 1)
    };
    let block = |y: i32| columns.base_block(x, y, z).unwrap_or(Base::Air);
    while y > sea {
        let current = block(y);
        y -= 1;
        if current == Base::Air && block(y) == Base::Solid {
            break;
        }
    }
    if y <= sea {
        return None;
    }
    let biome = columns.biome_at(x, y, z)?;
    if !kind.biomes.contains(&biome) {
        return None;
    }
    let rot = ROTS[rng.next_int_bound(4) as usize];
    let template = Arc::clone(&cfg.templates[rng.next_int_bound(14) as usize]);
    let pos = [x, y, z];
    let bounds = template::template_bounds(pos, rot, template.size);
    Some(Layout {
        pieces: vec![Piece {
            element: Arc::new(Elem::Fossil(Arc::new(FossilPiece {
                template,
                rot,
                pos,
                bounds,
            }))),
            pos,
            rot,
            ground: 0,
            min: bounds.min,
            max: bounds.max,
            rigid: true,
            junctions: Vec::new(),
        }],
        beard: true,
        bury: false,
        boxed: false,
        mansion: false,
        portal: false,
        ruin: false,
        encapsulate: false,
        step: kind.step,
        seed_index: kind.seed_index,
    })
}

struct Loader<'a> {
    data_dir: &'a Path,
    palette: &'a mut Palette,
    biome_ids: &'a HashMap<String, u32>,
    read: &'a super::Read<'a>,
    min_y: i32,
    height: i32,
    pools: HashMap<String, Pool>,
    templates: HashMap<String, Arc<Template>>,
    features: HashMap<String, Arc<super::Placed>>,
    /// Имя структуры → (шаг декорации, индекс `setFeatureSeed`).
    seeds: HashMap<String, (i32, i32)>,
    /// Имена псевдонимов пулов: файлов для них нет, подставляются в рантайме.
    alias_names: HashSet<String>,
}

struct RawSet {
    name: String,
    spacing: i32,
    separation: i32,
    salt: i32,
    triangular: bool,
    frequency: Freq,
    locate_offset: [i32; 3],
    exclusion: Option<(String, i32)>,
    kinds: Vec<Kind>,
    weights: Vec<i32>,
    rings: Option<RingPlan>,
}

impl RawSet {
    fn finish(self, names: &[String]) -> Result<Set, String> {
        let exclusion = match self.exclusion {
            Some((name, count)) => {
                let other = names
                    .iter()
                    .position(|n| n == &name)
                    .ok_or_else(|| format!("exclusion {name}"))?;
                Some((other, count))
            }
            None => None,
        };
        Ok(Set {
            spacing: self.spacing,
            separation: self.separation,
            salt: self.salt,
            triangular: self.triangular,
            frequency: self.frequency,
            locate_offset: self.locate_offset,
            exclusion,
            kinds: self.kinds,
            weights: self.weights,
            rings: self.rings,
        })
    }
}

impl Loader<'_> {
    fn set(&mut self, name: &str) -> Result<RawSet, String> {
        let v = (self.read)("structure_set", name)?;
        let place = &v["placement"];
        if place["type"].as_str() == Some("minecraft:concentric_rings") {
            return self.ring_set(name, place, &v);
        }
        if place["type"].as_str() != Some("minecraft:random_spread") {
            return Err(format!("{name}: placement"));
        }
        let triangular = match place["spread_type"].as_str() {
            None | Some("linear") => false,
            Some("triangular") => true,
            Some(kind) => return Err(format!("{name}: spread {kind}")),
        };
        let frequency = match place.get("frequency") {
            Some(f) if !f.is_null() => {
                let method = place["frequency_reduction_method"].as_str().unwrap_or("");
                let value = f.as_f64().unwrap_or(1.0) as f32;
                let salt = place["salt"].as_i64().unwrap_or(0) as i32;
                match method {
                    _ if value >= 1.0 => Freq::Always,
                    "" | "default" => Freq::Default(value, salt),
                    "legacy_type_1" => Freq::Legacy1(value),
                    "legacy_type_2" => Freq::Legacy2(value),
                    "legacy_type_3" => Freq::Legacy3(value),
                    _ => return Err(format!("{name}: frequency {method}")),
                }
            }
            _ => Freq::Always,
        };
        let mut locate_offset = [0; 3];
        let offsets = place["locate_offset"].as_array().into_iter().flatten();
        for (i, v) in offsets.enumerate().take(3) {
            locate_offset[i] = v.as_i64().unwrap_or(0) as i32;
        }
        let exclusion = place.get("exclusion_zone").and_then(|z| {
            if z.is_null() {
                return None;
            }
            Some((
                mc(z["other_set"].as_str().unwrap_or("")),
                z["chunk_count"].as_i64().unwrap_or(0) as i32,
            ))
        });
        let _ = &self.alias_names;
        let mut kinds = Vec::new();
        let mut weights = Vec::new();
        for entry in v["structures"].as_array().into_iter().flatten() {
            let id = entry["structure"].as_str().unwrap_or("");
            kinds.push(self.kind(id)?);
            weights.push(entry["weight"].as_i64().unwrap_or(1) as i32);
        }
        Ok(RawSet {
            name: mc(name),
            spacing: place["spacing"].as_i64().unwrap_or(1) as i32,
            separation: place["separation"].as_i64().unwrap_or(0) as i32,
            salt: place["salt"].as_i64().unwrap_or(0) as i32,
            triangular,
            frequency,
            locate_offset,
            exclusion,
            kinds,
            weights,
            rings: None,
        })
    }

    fn ring_set(&mut self, name: &str, place: &Value, v: &Value) -> Result<RawSet, String> {
        let preferred = self.biomes(place["preferred_biomes"].as_str().unwrap_or(""))?;
        let _ = &self.alias_names;
        let mut kinds = Vec::new();
        let mut weights = Vec::new();
        for entry in v["structures"].as_array().into_iter().flatten() {
            let id = entry["structure"].as_str().unwrap_or("");
            kinds.push(self.kind(id)?);
            weights.push(entry["weight"].as_i64().unwrap_or(1) as i32);
        }
        Ok(RawSet {
            name: mc(name),
            spacing: 1,
            separation: 0,
            salt: 0,
            triangular: false,
            frequency: Freq::Always,
            locate_offset: [0; 3],
            exclusion: None,
            kinds,
            weights,
            rings: Some(RingPlan {
                distance: place["distance"].as_i64().unwrap_or(32) as i32,
                spread: place["spread"].as_i64().unwrap_or(3) as i32,
                count: place["count"].as_i64().unwrap_or(128) as i32,
                preferred,
            }),
        })
    }

    fn kind(&mut self, name: &str) -> Result<Kind, String> {
        let v = (self.read)("structure", name)?;
        match v["type"].as_str() {
            Some("minecraft:desert_pyramid") => return self.scattered(name, &v, Which::Desert),
            Some("minecraft:jungle_temple") => return self.scattered(name, &v, Which::Jungle),
            Some("minecraft:swamp_hut") => return self.scattered(name, &v, Which::Swamp),
            Some("minecraft:igloo") => return self.shore_kind(name, &v, true, false),
            Some("minecraft:shipwreck") => {
                let beached = v["is_beached"].as_bool().unwrap_or(false);
                return self.shore_kind(name, &v, false, beached);
            }
            Some("minecraft:mineshaft") => return self.shaft_kind(name, &v),
            Some("minecraft:ocean_monument") => return self.monument_kind(name, &v),
            Some("minecraft:stronghold") => return self.stronghold_kind(name, &v),
            Some("minecraft:fortress") => return self.fortress_kind(name, &v),
            Some("minecraft:buried_treasure") => {
                let mut kind = self.stronghold_kind_with(name, &v, "none")?;
                kind.stronghold = false;
                kind.treasure = true;
                return Ok(kind);
            }
            Some("minecraft:nether_fossil") => return self.fossil_kind(name, &v),
            Some("minecraft:woodland_mansion") => return self.mansion_kind(name, &v),
            Some("minecraft:end_city") => {
                let mut kind = self.mansion_kind(name, &v)?;
                kind.mansion = false;
                kind.end_city = true;
                return Ok(kind);
            }
            Some("minecraft:ruined_portal") => return self.portal_kind(name, &v),
            Some("minecraft:ocean_ruin") => return self.ruin_kind(name, &v),
            Some("minecraft:jigsaw") => {}
            _ => return Err(format!("{name}: not jigsaw")),
        }
        let adapt = v["terrain_adaptation"].as_str().unwrap_or("none");
        if !matches!(
            adapt,
            "beard_thin" | "beard_box" | "bury" | "none" | "encapsulate"
        ) {
            return Err(format!("{name}: terrain {adapt}"));
        }
        let project = match v.get("project_start_to_heightmap").and_then(|s| s.as_str()) {
            Some("WORLD_SURFACE_WG") => true,
            Some(other) => return Err(format!("{name}: heightmap {other}")),
            None => false,
        };
        let (horizontal, vertical) = distance(&v["max_distance_from_center"]);
        let start = mc(v["start_pool"].as_str().unwrap_or(""));
        self.ensure_pool(&start)?;
        // `HeightProvider`: `uniform` с границами, `constant` с `value` или
        // краткая форма — сам `VerticalAnchor` (`{"absolute": 33}` у бастиона,
        // древнего города, троп).
        let start_height = match v.get("start_height") {
            Some(h) if !h.is_null() => Some(match h["type"].as_str() {
                Some("minecraft:uniform") => {
                    StartHeight::Uniform(anchor(&h["min_inclusive"]), anchor(&h["max_inclusive"]))
                }
                Some("minecraft:constant") => StartHeight::Constant(anchor(&h["value"])),
                Some(other) => return Err(format!("{name}: start_height {other}")),
                None => StartHeight::Constant(anchor(h)),
            }),
            _ => None,
        };
        let aliases = load_pool_aliases(&v["pool_aliases"])?;
        let mut alias_targets = HashSet::new();
        for alias in &aliases {
            alias.collect_names(&mut self.alias_names);
            alias.collect_targets(&mut alias_targets);
        }
        // На цели псевдонимов из пулов не ссылаются — грузим явно.
        for target in alias_targets {
            self.ensure_pool(&target)?;
        }
        let biomes = self.biomes(v["biomes"].as_str().unwrap_or(""))?;
        let id = mc(name);
        let (step, seed_index) = self
            .seeds
            .get(&id)
            .copied()
            .ok_or_else(|| format!("{name}: no decoration index"))?;
        Ok(Kind {
            name: id,
            start_pool: start,
            boxed: adapt == "beard_box",
            bury: adapt == "bury",
            size: v["size"].as_i64().unwrap_or(0) as i32,
            horizontal,
            vertical,
            expansion: v["use_expansion_hack"].as_bool().unwrap_or(false),
            project,
            biomes,
            beard: adapt == "beard_thin",
            start_height,
            encapsulate: adapt == "encapsulate",
            aliases,
            // `DimensionPadding.CODEC` принимает и число, и `{bottom, top}`.
            start_jigsaw: v["start_jigsaw_name"].as_str().map(mc),
            padding: match v.get("dimension_padding") {
                Some(p) if !p.is_null() => match p.as_i64() {
                    Some(n) => (n as i32, n as i32),
                    None => (
                        p["bottom"].as_i64().unwrap_or(0) as i32,
                        p["top"].as_i64().unwrap_or(0) as i32,
                    ),
                },
                _ => (0, 0),
            },
            step,
            seed_index,
            scattered: None,
            shore: None,
            shaft: None,
            monument: None,
            stronghold: false,
            fortress: false,
            fossil: None,
            treasure: false,
            mansion: false,
            end_city: false,
            portal: None,
            ruin: None,
        })
    }

    fn scattered(&self, name: &str, v: &Value, which: Which) -> Result<Kind, String> {
        let adapt = v["terrain_adaptation"].as_str().unwrap_or("none");
        if adapt != "none" {
            return Err(format!("{name}: terrain {adapt}"));
        }
        let biomes = self.biomes(v["biomes"].as_str().unwrap_or(""))?;
        let id = mc(name);
        let (step, seed_index) = self
            .seeds
            .get(&id)
            .copied()
            .ok_or_else(|| format!("{name}: no decoration index"))?;
        Ok(Kind {
            name: id,
            start_pool: String::new(),
            boxed: false,
            bury: false,
            size: 0,
            horizontal: 0,
            vertical: 0,
            expansion: false,
            project: false,
            biomes,
            beard: false,
            step,
            seed_index,
            scattered: Some(which),
            shore: None,
            shaft: None,
            monument: None,
            stronghold: false,
            fortress: false,
            fossil: None,
            treasure: false,
            mansion: false,
            end_city: false,
            portal: None,
            ruin: None,
            start_height: None,
            encapsulate: false,
            aliases: Vec::new(),
            padding: (0, 0),
            start_jigsaw: None,
        })
    }

    fn shore_kind(
        &mut self,
        name: &str,
        v: &Value,
        igloo: bool,
        beached: bool,
    ) -> Result<Kind, String> {
        let adapt = v["terrain_adaptation"].as_str().unwrap_or("none");
        if adapt != "none" {
            return Err(format!("{name}: terrain {adapt}"));
        }
        let biomes = self.biomes(v["biomes"].as_str().unwrap_or(""))?;
        let id = mc(name);
        let (step, seed_index) = self
            .seeds
            .get(&id)
            .copied()
            .ok_or_else(|| format!("{name}: no decoration index"))?;
        let shore = if igloo {
            shore::load_igloo(self.data_dir, self.palette)?
        } else {
            shore::load_ship(self.data_dir, self.palette, beached)?
        };
        Ok(Kind {
            name: id,
            start_pool: String::new(),
            boxed: false,
            bury: false,
            size: 0,
            horizontal: 0,
            vertical: 0,
            expansion: false,
            project: false,
            biomes,
            beard: false,
            step,
            seed_index,
            scattered: None,
            shore: Some(shore),
            shaft: None,
            monument: None,
            stronghold: false,
            fortress: false,
            fossil: None,
            treasure: false,
            mansion: false,
            end_city: false,
            portal: None,
            ruin: None,
            start_height: None,
            encapsulate: false,
            aliases: Vec::new(),
            padding: (0, 0),
            start_jigsaw: None,
        })
    }

    fn shaft_kind(&self, name: &str, v: &Value) -> Result<Kind, String> {
        let adapt = v["terrain_adaptation"].as_str().unwrap_or("none");
        if adapt != "none" {
            return Err(format!("{name}: terrain {adapt}"));
        }
        let mesa = v["mineshaft_type"].as_str() == Some("mesa");
        let biomes = self.biomes(v["biomes"].as_str().unwrap_or(""))?;
        let blocking = Arc::new(self.biomes("#minecraft:mineshaft_blocking")?);
        let id = mc(name);
        let (step, seed_index) = self
            .seeds
            .get(&id)
            .copied()
            .ok_or_else(|| format!("{name}: no decoration index"))?;
        Ok(Kind {
            name: id,
            start_pool: String::new(),
            boxed: false,
            bury: false,
            size: 0,
            horizontal: 0,
            vertical: 0,
            expansion: false,
            project: false,
            biomes,
            beard: false,
            step,
            seed_index,
            scattered: None,
            shore: None,
            shaft: Some(ShaftCfg { mesa, blocking }),
            monument: None,
            stronghold: false,
            fortress: false,
            fossil: None,
            treasure: false,
            mansion: false,
            end_city: false,
            portal: None,
            ruin: None,
            start_height: None,
            encapsulate: false,
            aliases: Vec::new(),
            padding: (0, 0),
            start_jigsaw: None,
        })
    }

    fn monument_kind(&mut self, name: &str, v: &Value) -> Result<Kind, String> {
        let adapt = v["terrain_adaptation"].as_str().unwrap_or("none");
        if adapt != "none" {
            return Err(format!("{name}: terrain {adapt}"));
        }
        let biomes = self.biomes(v["biomes"].as_str().unwrap_or(""))?;
        let around = self.biomes("#minecraft:required_ocean_monument_surrounding")?;
        let id = mc(name);
        let (step, seed_index) = self
            .seeds
            .get(&id)
            .copied()
            .ok_or_else(|| format!("{name}: no decoration index"))?;
        Ok(Kind {
            name: id,
            start_pool: String::new(),
            boxed: false,
            bury: false,
            size: 0,
            horizontal: 0,
            vertical: 0,
            expansion: false,
            project: false,
            biomes,
            beard: false,
            step,
            seed_index,
            scattered: None,
            shore: None,
            shaft: None,
            monument: Some(around),
            stronghold: false,
            fortress: false,
            fossil: None,
            treasure: false,
            mansion: false,
            end_city: false,
            portal: None,
            ruin: None,
            start_height: None,
            encapsulate: false,
            aliases: Vec::new(),
            padding: (0, 0),
            start_jigsaw: None,
        })
    }

    fn stronghold_kind(&mut self, name: &str, v: &Value) -> Result<Kind, String> {
        self.stronghold_kind_with(name, v, "bury")
    }

    fn stronghold_kind_with(
        &mut self,
        name: &str,
        v: &Value,
        terrain: &str,
    ) -> Result<Kind, String> {
        let adapt = v["terrain_adaptation"].as_str().unwrap_or("none");
        if adapt != terrain {
            return Err(format!("{name}: terrain {adapt}"));
        }
        let biomes = self.biomes(v["biomes"].as_str().unwrap_or(""))?;
        let id = mc(name);
        let (step, seed_index) = self
            .seeds
            .get(&id)
            .copied()
            .ok_or_else(|| format!("{name}: no decoration index"))?;
        Ok(Kind {
            name: id,
            start_pool: String::new(),
            boxed: false,
            bury: false,
            size: 0,
            horizontal: 0,
            vertical: 0,
            expansion: false,
            project: false,
            biomes,
            beard: false,
            step,
            seed_index,
            scattered: None,
            shore: None,
            shaft: None,
            monument: None,
            stronghold: true,
            fortress: false,
            fossil: None,
            treasure: false,
            mansion: false,
            end_city: false,
            portal: None,
            ruin: None,
            start_height: None,
            encapsulate: false,
            aliases: Vec::new(),
            padding: (0, 0),
            start_jigsaw: None,
        })
    }

    /// `NetherFossilStructure`: 14 шаблонов, `height` — uniform между якорями.
    fn fossil_kind(&mut self, name: &str, v: &Value) -> Result<Kind, String> {
        let mut kind = self.stronghold_kind_with(name, v, "beard_thin")?;
        kind.stronghold = false;
        let height = &v["height"];
        if height["type"].as_str() != Some("minecraft:uniform") {
            return Err(format!("{name}: height {}", height["type"]));
        }
        let low = self.anchor(&height["min_inclusive"])?;
        let high = self.anchor(&height["max_inclusive"])?;
        let mut templates = Vec::new();
        for i in 1..=14 {
            let id = format!("minecraft:nether_fossils/fossil_{i}");
            templates.push(Arc::new(template::load_template(
                self.data_dir,
                self.palette,
                &id,
            )?));
        }
        kind.fossil = Some(Arc::new(FossilCfg {
            templates,
            low,
            high,
        }));
        Ok(kind)
    }

    /// `VerticalAnchor.resolveY` в `WorldGenerationContext` генератора.
    fn anchor(&self, v: &Value) -> Result<i32, String> {
        if let Some(y) = v["absolute"].as_i64() {
            return Ok(y as i32);
        }
        if let Some(d) = v["above_bottom"].as_i64() {
            return Ok(self.min_y + d as i32);
        }
        if let Some(d) = v["below_top"].as_i64() {
            return Ok(self.min_y + self.height - 1 - d as i32);
        }
        Err(format!("anchor {v}"))
    }

    fn fortress_kind(&mut self, name: &str, v: &Value) -> Result<Kind, String> {
        let mut kind = self.stronghold_kind_with(name, v, "none")?;
        kind.stronghold = false;
        kind.fortress = true;
        Ok(kind)
    }

    /// Особняк: сетка-лабиринт из шаблонов `woodland_mansion/*`.
    fn mansion_kind(&mut self, name: &str, v: &Value) -> Result<Kind, String> {
        let adapt = v["terrain_adaptation"].as_str().unwrap_or("none");
        if adapt != "none" {
            return Err(format!("{name}: terrain {adapt}"));
        }
        let biomes = self.biomes(v["biomes"].as_str().unwrap_or(""))?;
        let id = mc(name);
        let (step, seed_index) = self
            .seeds
            .get(&id)
            .copied()
            .ok_or_else(|| format!("{name}: no decoration index"))?;
        Ok(Kind {
            name: id,
            start_pool: String::new(),
            boxed: false,
            bury: false,
            size: 0,
            horizontal: 0,
            vertical: 0,
            expansion: false,
            project: false,
            biomes,
            beard: false,
            step,
            seed_index,
            scattered: None,
            shore: None,
            shaft: None,
            monument: None,
            stronghold: false,
            fortress: false,
            fossil: None,
            treasure: false,
            mansion: true,
            end_city: false,
            portal: None,
            ruin: None,
            start_height: None,
            encapsulate: false,
            aliases: Vec::new(),
            padding: (0, 0),
            start_jigsaw: None,
        })
    }

    /// Руины портала: наборы `setups` и шаблоны `ruined_portal/*`.
    fn portal_kind(&mut self, name: &str, v: &Value) -> Result<Kind, String> {
        let adapt = v["terrain_adaptation"].as_str().unwrap_or("none");
        if adapt != "none" {
            return Err(format!("{name}: terrain {adapt}"));
        }
        let biomes = self.biomes(v["biomes"].as_str().unwrap_or(""))?;
        let mut setups = Vec::new();
        for entry in v["setups"].as_array().ok_or("portal: no setups")? {
            let placement = portal::Placement::parse(entry["placement"].as_str().unwrap_or(""))
                .ok_or_else(|| format!("{name}: placement"))?;
            setups.push(portal::Setup {
                placement,
                air_pocket: entry["air_pocket_probability"].as_f64().unwrap_or(0.0) as f32,
                mossiness: entry["mossiness"].as_f64().unwrap_or(0.0) as f32,
                overgrown: entry["overgrown"].as_bool().unwrap_or(false),
                vines: entry["vines"].as_bool().unwrap_or(false),
                can_be_cold: entry["can_be_cold"].as_bool().unwrap_or(false),
                cold: false,
                blackstone: entry["replace_with_blackstone"].as_bool().unwrap_or(false),
                weight: entry["weight"].as_f64().unwrap_or(1.0) as f32,
            });
        }
        let id = mc(name);
        let (step, seed_index) = self
            .seeds
            .get(&id)
            .copied()
            .ok_or_else(|| format!("{name}: no decoration index"))?;
        Ok(Kind {
            name: id,
            start_pool: String::new(),
            boxed: false,
            bury: false,
            size: 0,
            horizontal: 0,
            vertical: 0,
            expansion: false,
            project: false,
            biomes,
            beard: false,
            step,
            seed_index,
            scattered: None,
            shore: None,
            shaft: None,
            monument: None,
            stronghold: false,
            fortress: false,
            fossil: None,
            treasure: false,
            mansion: false,
            end_city: false,
            portal: Some(setups),
            ruin: None,
            start_height: None,
            encapsulate: false,
            aliases: Vec::new(),
            padding: (0, 0),
            start_jigsaw: None,
        })
    }

    /// Океанская руина: тёплая или холодная, шаблоны грузятся заранее.
    fn ruin_kind(&mut self, name: &str, v: &Value) -> Result<Kind, String> {
        let adapt = v["terrain_adaptation"].as_str().unwrap_or("none");
        if adapt != "none" {
            return Err(format!("{name}: terrain {adapt}"));
        }
        let warm = match v["biome_temp"].as_str().unwrap_or("") {
            "warm" => true,
            "cold" => false,
            other => return Err(format!("{name}: biome_temp {other}")),
        };
        let biomes = self.biomes(v["biomes"].as_str().unwrap_or(""))?;
        let id = mc(name);
        let (step, seed_index) = self
            .seeds
            .get(&id)
            .copied()
            .ok_or_else(|| format!("{name}: no decoration index"))?;
        Ok(Kind {
            name: id,
            start_pool: String::new(),
            boxed: false,
            bury: false,
            size: 0,
            horizontal: 0,
            vertical: 0,
            expansion: false,
            project: false,
            biomes,
            beard: false,
            step,
            seed_index,
            scattered: None,
            shore: None,
            shaft: None,
            monument: None,
            stronghold: false,
            fortress: false,
            fossil: None,
            treasure: false,
            mansion: false,
            end_city: false,
            portal: None,
            ruin: Some(ocean_ruin::Cfg {
                warm,
                large_probability: v["large_probability"].as_f64().unwrap_or(0.0) as f32,
                cluster_probability: v["cluster_probability"].as_f64().unwrap_or(0.0) as f32,
            }),
            start_height: None,
            encapsulate: false,
            aliases: Vec::new(),
            padding: (0, 0),
            start_jigsaw: None,
        })
    }

    fn biomes(&self, spec: &str) -> Result<HashSet<u32>, String> {
        let mut out = HashSet::new();
        self.walk_biome(spec, &mut out, &mut HashSet::new())?;
        Ok(out)
    }
    fn walk_biome(
        &self,
        spec: &str,
        out: &mut HashSet<u32>,
        seen: &mut HashSet<String>,
    ) -> Result<(), String> {
        let spec = spec.trim();
        if spec.is_empty() {
            return Ok(());
        }
        if let Some(tag) = spec.strip_prefix('#') {
            let tag = mc(tag);
            if !seen.insert(tag.clone()) {
                return Ok(());
            }
            let path = tag.strip_prefix("minecraft:").unwrap_or(&tag);
            let file = self
                .data_dir
                .join("data/minecraft/tags/worldgen/biome")
                .join(path)
                .with_extension("json");
            let text =
                std::fs::read_to_string(&file).map_err(|e| format!("biome tag {tag}: {e}"))?;
            let v: Value =
                serde_json::from_str(&text).map_err(|e| format!("biome tag {tag}: {e}"))?;
            for value in v["values"].as_array().into_iter().flatten() {
                if let Some(name) = value.as_str() {
                    self.walk_biome(name, out, seen)?;
                }
            }
            return Ok(());
        }
        let name = mc(spec);
        if let Some(id) = self.biome_ids.get(&name) {
            out.insert(*id);
        }
        Ok(())
    }

    fn ensure_pool(&mut self, name: &str) -> Result<(), String> {
        let name = mc(name);
        if self.pools.contains_key(&name) {
            return Ok(());
        }
        self.pools.insert(
            name.clone(),
            Pool {
                templates: Vec::new(),
                fallback: name.clone(),
                max_size: 0,
            },
        );
        let v = (self.read)("template_pool", &name)?;
        let pool = self.parse_pool(&v)?;
        self.pools.insert(name, pool);
        Ok(())
    }

    fn parse_pool(&mut self, v: &Value) -> Result<Pool, String> {
        let mut templates = Vec::new();
        for entry in v["elements"].as_array().into_iter().flatten() {
            let el = Arc::new(self.parse_element(&entry["element"])?);
            let weight = entry["weight"].as_i64().unwrap_or(1).max(0) as usize;
            for _ in 0..weight {
                templates.push(Arc::clone(&el));
            }
        }
        // StructureTemplatePool.getMaxSize: max(primary, fallback) по Y.
        let max_size = templates
            .iter()
            .filter(|e| !e.is_empty())
            .map(|e| y_span(e))
            .max()
            .unwrap_or(0);
        Ok(Pool {
            templates,
            fallback: mc(v["fallback"].as_str().unwrap_or("minecraft:empty")),
            max_size,
        })
    }

    fn parse_element(&mut self, v: &Value) -> Result<Elem, String> {
        let kind = v["element_type"].as_str().unwrap_or("");
        let rigid = !matches!(
            v["projection"].as_str(),
            Some("terrain_matching") | Some("minecraft:terrain_matching")
        );
        match kind {
            "minecraft:empty_pool_element" => Ok(Elem::Empty),
            "minecraft:legacy_single_pool_element" | "minecraft:single_pool_element" => {
                let loc = v["location"].as_str().unwrap_or("");
                let template = self.template(loc)?;
                let processors = match v.get("processors") {
                    Some(p) if !p.is_null() => {
                        template::parse_processors(p, self.read, self.palette)?
                    }
                    _ => Vec::new(),
                };
                Ok(Elem::Single {
                    template,
                    processors,
                    ignore_air: kind.contains("legacy"),
                    rigid,
                })
            }
            "minecraft:list_pool_element" => {
                let mut parts = Vec::new();
                for child in v["elements"].as_array().into_iter().flatten() {
                    parts.push(Arc::new(self.parse_element(child)?));
                }
                Ok(Elem::List { parts, rigid })
            }
            "minecraft:feature_pool_element" => {
                let name = v["feature"].as_str().unwrap_or("");
                let placed = self.feature(name)?;
                Ok(Elem::Feature { placed, rigid })
            }
            other => Err(format!("pool element {other}")),
        }
    }

    fn template(&mut self, id: &str) -> Result<Arc<Template>, String> {
        let id = mc(id);
        if let Some(t) = self.templates.get(&id) {
            return Ok(Arc::clone(t));
        }
        let t = Arc::new(template::load_template(self.data_dir, self.palette, &id)?);
        self.templates.insert(id, Arc::clone(&t));
        Ok(t)
    }

    fn feature(&mut self, name: &str) -> Result<Arc<super::Placed>, String> {
        let name = mc(name);
        if let Some(f) = self.features.get(&name) {
            return Ok(Arc::clone(f));
        }
        let v = (self.read)("placed_feature", &name)?;
        let placed = super::parse_placed(
            &v,
            self.read,
            self.palette,
            self.min_y,
            self.height,
            self.data_dir,
        )?;
        let placed = Arc::new(placed);
        self.features.insert(name, Arc::clone(&placed));
        Ok(placed)
    }

    fn close_pools(&mut self) -> Result<(), String> {
        loop {
            let mut names = HashSet::new();
            for pool in self.pools.values() {
                names.insert(pool.fallback.clone());
                for el in &pool.templates {
                    collect_pools(el, &mut names);
                }
            }
            let missing: Vec<String> = names
                .into_iter()
                .filter(|n| !self.pools.contains_key(n))
                .filter(|n| !self.alias_names.contains(n))
                .collect();
            if missing.is_empty() {
                return Ok(());
            }
            for name in missing {
                self.ensure_pool(&name)?;
            }
        }
    }
}

fn collect_pools(el: &Elem, out: &mut HashSet<String>) {
    match el {
        Elem::Empty
        | Elem::Feature { .. }
        | Elem::Scattered(_)
        | Elem::Shore(_)
        | Elem::Shaft(_)
        | Elem::Monument(_)
        | Elem::Stronghold(_)
        | Elem::Fortress(_)
        | Elem::Fossil(_)
        | Elem::Treasure(_)
        | Elem::Mansion(_)
        | Elem::EndCity(_)
        | Elem::Portal(_)
        | Elem::Ruin(_) => {}
        Elem::List { parts, .. } => {
            for p in parts {
                collect_pools(p, out);
            }
        }
        Elem::Single { template, .. } => {
            for pal in &template.jigsaws {
                for jig in pal {
                    out.insert(jig.pool.to_string());
                }
            }
        }
    }
}

fn pick<'a>(empty: &'a Arc<Elem>, pool: &'a Pool, rng: &mut impl Bits) -> &'a Arc<Elem> {
    if pool.templates.is_empty() {
        return empty;
    }
    &pool.templates[rng.next_int_bound(pool.templates.len() as i32) as usize]
}

fn jigsaws(el: &Elem, pos: [i32; 3], rot: Rot, rng: &mut impl Bits) -> Vec<JigInfo> {
    match el {
        Elem::Empty => Vec::new(),
        Elem::Single { template, .. } => template::shuffled_jigsaws(template, rot, pos, rng),
        Elem::List { parts, .. } => parts
            .first()
            .map(|p| jigsaws(p, pos, rot, rng))
            .unwrap_or_default(),
        Elem::Scattered(_)
        | Elem::Shore(_)
        | Elem::Shaft(_)
        | Elem::Monument(_)
        | Elem::Stronghold(_)
        | Elem::Fortress(_)
        | Elem::Fossil(_)
        | Elem::Treasure(_)
        | Elem::Mansion(_)
        | Elem::EndCity(_)
        | Elem::Portal(_)
        | Elem::Ruin(_) => Vec::new(),
        Elem::Feature { .. } => vec![JigInfo {
            pos,
            front: "down".into(),
            top: "south".into(),
            rollable: true,
            name: None,
            target: "minecraft:empty".into(),
            pool: "minecraft:empty".into(),
            placement_priority: 0,
            selection_priority: 0,
        }],
    }
}

fn bounds(el: &Elem, pos: [i32; 3], rot: Rot) -> Option<([i32; 3], [i32; 3])> {
    match el {
        Elem::Empty
        | Elem::Scattered(_)
        | Elem::Shore(_)
        | Elem::Shaft(_)
        | Elem::Monument(_)
        | Elem::Stronghold(_)
        | Elem::Fortress(_)
        | Elem::Fossil(_)
        | Elem::Treasure(_)
        | Elem::Mansion(_)
        | Elem::EndCity(_)
        | Elem::Portal(_)
        | Elem::Ruin(_) => None,
        Elem::Feature { .. } => Some((pos, pos)),
        Elem::Single { template, .. } => {
            let b = template::template_bounds(pos, rot, template.size);
            Some((b.min, b.max))
        }
        Elem::List { parts, .. } => {
            let mut acc: Option<([i32; 3], [i32; 3])> = None;
            for part in parts {
                let Some((mn, mx)) = bounds(part, pos, rot) else {
                    continue;
                };
                acc = Some(match acc {
                    None => (mn, mx),
                    Some((a, b)) => (
                        [a[0].min(mn[0]), a[1].min(mn[1]), a[2].min(mn[2])],
                        [b[0].max(mx[0]), b[1].max(mx[1]), b[2].max(mx[2])],
                    ),
                });
            }
            acc
        }
    }
}

fn y_span(el: &Elem) -> i32 {
    let Some((mn, mx)) = bounds(el, [0, 0, 0], Rot::None) else {
        return 0;
    };
    mx[1] - mn[1] + 1
}

fn can_attach(parent: &JigInfo, child: &JigInfo) -> bool {
    if opposite(&parent.front) != child.front {
        return false;
    }
    if !parent.rollable && parent.top != child.top {
        return false;
    }
    match &child.name {
        None => true,
        Some(name) => name == &parent.target,
    }
}

fn opposite(dir: &str) -> &str {
    match dir {
        "down" => "up",
        "up" => "down",
        "north" => "south",
        "south" => "north",
        "west" => "east",
        "east" => "west",
        other => other,
    }
}

fn step(dir: &str) -> [i32; 3] {
    match dir {
        "down" => [0, -1, 0],
        "up" => [0, 1, 0],
        "north" => [0, 0, -1],
        "south" => [0, 0, 1],
        "west" => [-1, 0, 0],
        "east" => [1, 0, 0],
        _ => [0, 0, 0],
    }
}

fn inside_box(min: [i32; 3], max: [i32; 3], p: [i32; 3]) -> bool {
    (0..3).all(|i| min[i] <= p[i] && p[i] <= max[i])
}

/// `ChunkGenerator.createReferences`: старт в чанках ±8 от цели, и его
/// `getBoundingBox()` (у адаптации рельефа раздута на 12) задевает чанк по x/z.
fn referenced(layout: &Layout, sx: i32, sz: i32, cx: i32, cz: i32) -> bool {
    if (sx - cx).abs() > 8 || (sz - cz).abs() > 8 {
        return false;
    }
    let mut pieces = layout.pieces.iter();
    let Some(first) = pieces.next() else {
        return false;
    };
    let (mut lo, mut hi) = (first.min, first.max);
    for p in pieces {
        lo = [0, 1, 2].map(|i| lo[i].min(p.min[i]));
        hi = [0, 1, 2].map(|i| hi[i].max(p.max[i]));
    }
    let pad = if layout.beard || layout.bury || layout.boxed || layout.encapsulate {
        12
    } else {
        0
    };
    let (x0, z0) = (cx * 16, cz * 16);
    lo[0] - pad <= x0 + 15 && x0 <= hi[0] + pad && lo[2] - pad <= z0 + 15 && z0 <= hi[2] + pad
}

/// Порядок обхода fastutil `LongOpenHashSet` (ёмкость по умолчанию 16,
/// таблица 32) для ключей в порядке вставки: ноль первым, затем ячейки
/// таблицы от конца к началу. Перестройка таблицы при 24 ключах не нужна —
/// столько стартов одной структуры на чанк не бывает.
fn long_set_order(keys: &[i64]) -> Vec<usize> {
    fn mix(x: i64) -> i64 {
        let h = x.wrapping_mul(0x9E37_79B9_7F4A_7C15_u64 as i64);
        let h = h ^ ((h as u64) >> 32) as i64;
        h ^ ((h as u64) >> 16) as i64
    }
    let mut n = 32usize;
    while keys.len() >= n * 3 / 4 {
        n *= 2;
    }
    let mask = n - 1;
    let mut table: Vec<Option<usize>> = vec![None; n];
    let mut zero = None;
    for (i, &k) in keys.iter().enumerate() {
        if k == 0 {
            zero.get_or_insert(i);
            continue;
        }
        let mut pos = (mix(k) as i32 as usize) & mask;
        while let Some(j) = table[pos] {
            if keys[j] == k {
                break;
            }
            pos = (pos + 1) & mask;
        }
        table[pos].get_or_insert(i);
    }
    zero.into_iter()
        .chain(table.into_iter().rev().flatten())
        .collect()
}

fn overlaps(a0: [i32; 3], a1: [i32; 3], b0: [i32; 3], b1: [i32; 3]) -> bool {
    (0..3).all(|i| a0[i] <= b1[i] && b0[i] <= a1[i])
}

fn ring_dist(chunk: (i32, i32), origin: [i32; 3]) -> i64 {
    let dx = (chunk.0 * 16 + 8) as i64 - origin[0] as i64;
    let dz = (chunk.1 * 16 + 8) as i64 - origin[2] as i64;
    dx * dx + dz * dz
}

fn near_xz(min: [i32; 3], max: [i32; 3], cx: i32, cz: i32, pad: i32) -> bool {
    min[0] <= cx * 16 + 15 + pad
        && max[0] >= cx * 16 - pad
        && min[2] <= cz * 16 + 15 + pad
        && max[2] >= cz * 16 - pad
}

fn distance(v: &Value) -> (i32, i32) {
    if let Some(n) = v.as_i64() {
        let n = n as i32;
        return (n, n);
    }
    let h = v["horizontal"].as_i64().unwrap_or(80) as i32;
    let vertical = v["vertical"].as_i64().unwrap_or(h as i64) as i32;
    (h, vertical)
}

/// `#village` и `minecraft:#village` становятся `#minecraft:village`.
fn normalize_id(raw: &str) -> String {
    let raw = raw.trim();
    let raw = raw
        .strip_prefix("minecraft:#")
        .map(|rest| format!("#{rest}"))
        .unwrap_or_else(|| raw.to_owned());
    if let Some(tag) = raw.strip_prefix('#') {
        format!("#{}", mc(tag))
    } else {
        mc(&raw)
    }
}

/// Теги `worldgen/structure`: имя тега → id, включая вложенные `#`.
fn load_structure_tags(data_dir: &Path) -> Result<HashMap<String, HashSet<String>>, String> {
    let dir = data_dir.join("data/minecraft/tags/worldgen/structure");
    let mut raw: HashMap<String, Vec<String>> = HashMap::new();
    for ent in std::fs::read_dir(&dir).map_err(|e| format!("structure tags: {e}"))? {
        let path = ent.map_err(|e| format!("structure tags: {e}"))?.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{stem}: {e}"))?;
        let v: Value = serde_json::from_str(&text).map_err(|e| format!("{stem}: {e}"))?;
        let values = v["values"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|x| x.as_str().map(str::to_owned))
            .collect();
        raw.insert(format!("minecraft:{stem}"), values);
    }
    let mut out = HashMap::new();
    for name in raw.keys() {
        let mut set = HashSet::new();
        let mut seen = HashSet::new();
        expand_tag(&raw, name, &mut set, &mut seen);
        out.insert(name.clone(), set);
    }
    Ok(out)
}

fn expand_tag(
    raw: &HashMap<String, Vec<String>>,
    name: &str,
    set: &mut HashSet<String>,
    seen: &mut HashSet<String>,
) {
    if !seen.insert(name.to_owned()) {
        return;
    }
    let Some(values) = raw.get(name) else {
        return;
    };
    for value in values {
        if let Some(tag) = value.strip_prefix('#') {
            expand_tag(raw, &mc(tag), set, seen);
        } else {
            set.insert(mc(value));
        }
    }
}

fn mc(name: &str) -> String {
    let name = name.strip_prefix('#').unwrap_or(name);
    if name.contains(':') || name.is_empty() {
        name.to_owned()
    } else {
        format!("minecraft:{name}")
    }
}

/// Клетка `random_spread`. `triangular` — среднее двух `nextInt`.
pub(crate) fn spread_chunk(
    spacing: i32,
    separation: i32,
    salt: i32,
    seed: i64,
    cx: i32,
    cz: i32,
    triangular: bool,
) -> (i32, i32) {
    let cell_x = cx.div_euclid(spacing);
    let cell_z = cz.div_euclid(spacing);
    let mut rng = Legacy::new(0);
    rng.set_large_feature_with_salt(seed, cell_x, cell_z, salt);
    let spread = spacing - separation;
    let roll = |rng: &mut Legacy| {
        if triangular {
            (rng.next_int_bound(spread) + rng.next_int_bound(spread)) / 2
        } else {
            rng.next_int_bound(spread)
        }
    };
    let dx = roll(&mut rng);
    let dz = roll(&mut rng);
    (
        cell_x.wrapping_mul(spacing).wrapping_add(dx),
        cell_z.wrapping_mul(spacing).wrapping_add(dz),
    )
}

#[derive(Clone, Copy)]
enum Freq {
    Always,
    /// `default`: соль набора на месте координаты x, как в `probabilityReducer`.
    Default(f32, i32),
    Legacy1(f32),
    Legacy2(f32),
    Legacy3(f32),
}

/// `legacy_type_3`: `setLargeFeatureSeed`, затем `nextDouble` против `float`.
fn frequency_pass(frequency: Freq, seed: i64, cx: i32, cz: i32) -> bool {
    match frequency {
        Freq::Always => true,
        Freq::Default(value, salt) => {
            let mut rng = Legacy::new(0);
            rng.set_large_feature_with_salt(seed, salt, cx, cz);
            rng.next_float() < value
        }
        Freq::Legacy1(value) => frequency_ok(Some(value), seed, cx, cz),
        // `legacyArbitrarySaltProbabilityReducer`: соль 10387320 вместо набора.
        Freq::Legacy2(value) => {
            let mut rng = Legacy::new(0);
            rng.set_large_feature_with_salt(seed, cx, cz, 10387320);
            rng.next_float() < value
        }
        Freq::Legacy3(value) => {
            let mut rng = Legacy::new(0);
            rng.set_large_feature_seed(seed, cx, cz);
            rng.next_double() < value as f64
        }
    }
}

/// `legacy_type_1`: соль не используется, граница — `(1 / frequency)` как `float`.
pub(crate) fn frequency_ok(frequency: Option<f32>, seed: i64, cx: i32, cz: i32) -> bool {
    let Some(frequency) = frequency else {
        return true;
    };
    if frequency >= 1.0 {
        return true;
    }
    let region_x = cx >> 4;
    let region_z = cz >> 4;
    let mut rng = Legacy::new(0);
    rng.set_seed(((region_x ^ region_z.wrapping_shl(4)) as i64) ^ seed);
    rng.next_int();
    let bound = (1.0f32 / frequency) as i32;
    rng.next_int_bound(bound) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trial_prefix_detail() {
        let Some(data) = std::env::var_os("CHUNKWAKE_DATA").map(std::path::PathBuf::from) else {
            return;
        };
        let path = std::path::Path::new(
            r"E:\paper_rust\target\trial-12345\world\dimensions\minecraft\overworld\region",
        );
        if !path.is_dir() {
            return;
        }
        let registries = chunkwake_data::registries::RegistryData::load(&data).unwrap();
        let biome_ids: std::collections::HashMap<String, u32> = registries
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
        let states = blocks.states();
        let state = |n: &str| blocks.default_state(n).ok().map(|s| s as u32);
        let mut palette = crate::feature::Palette::new(&data, &states, &state);
        let read = |dir: &str, name: &str| -> Result<serde_json::Value, String> {
            let path = name.strip_prefix("minecraft:").unwrap_or(name);
            let text = std::fs::read_to_string(
                data.join("data/minecraft/worldgen")
                    .join(dir)
                    .join(format!("{path}.json")),
            )
            .map_err(|e| format!("{dir} {name}: {e}"))?;
            serde_json::from_str(&text).map_err(|e| format!("{dir} {name}: {e}"))
        };
        let order = crate::Generator::load_with_version(&data, 12345, None)
            .unwrap()
            .feature_biomes();
        let structures =
            Structures::load(&data, &mut palette, &order, &biome_ids, &read, -64, 384).unwrap();
        let mut noise = crate::Generator::load_with_version(&data, 12345, None).unwrap();
        noise.bind_biomes(biome_ids.clone());
        let cols = crate::pipeline::Cols {
            noise: &noise,
            carve: crate::CarveBlocks {
                air: 0,
                water: 0,
                lava: 0,
                liquid: 0,
                bedrock: 0,
                grass: 0,
                mycelium: 0,
                dirt: 0,
            },
            stone: 0,
            palette: &palette,
            heights: &std::sync::Mutex::new(std::collections::HashMap::new()),
            bases: &std::sync::Mutex::new(std::collections::HashMap::new()),
            base_limit: 16_384,
        };
        let (si, ki) = structures
            .sets
            .iter()
            .enumerate()
            .find_map(|(si, set)| {
                set.kinds
                    .iter()
                    .position(|k| k.name.contains("trial"))
                    .map(|ki| (si, ki))
            })
            .expect("trial kind");
        let layout = structures.assemble(si, ki, 0, 14, &cols).expect("layout");

        let store = chunkwake_world::anvil::RegionStore::new(path);
        let chunks = store.load_region(0, 0).unwrap();
        let (_, root) = chunks.iter().find(|(p, _)| *p == (0, 14)).unwrap();
        let children = root
            .get("structures")
            .and_then(|t| t.as_compound())
            .and_then(|c| c.get("starts"))
            .and_then(|t| t.as_compound())
            .and_then(|c| c.get("minecraft:trial_chambers"))
            .and_then(|t| t.as_compound())
            .and_then(|c| c.get("Children"))
            .and_then(|t| t.as_list())
            .unwrap();
        println!("van {} ours {}", children.len(), layout.pieces.len());
        let needle = [18, -29, 256];
        for (i, p) in layout.pieces.iter().enumerate() {
            if p.min == needle {
                println!("ours has 148-child at index {i}");
            }
        }
        for (i, c) in children.iter().enumerate() {
            let c = c.as_compound().unwrap();
            let bb = c.get("BB").and_then(|t| t.as_int_array()).unwrap_or(&[]);
            if bb.len() == 6 && bb[0] == needle[0] && bb[1] == needle[1] && bb[2] == needle[2] {
                println!("vanilla has it at index {i}");
            }
        }
    }

    #[test]
    fn trial_matches_vanilla_pieces() {
        let Some(data) = std::env::var_os("CHUNKWAKE_DATA").map(std::path::PathBuf::from) else {
            return;
        };
        let path = std::path::Path::new(
            r"E:\paper_rust\target\trial-12345\world\dimensions\minecraft\overworld\region",
        );
        if !path.is_dir() {
            return;
        }
        let registries = chunkwake_data::registries::RegistryData::load(&data).unwrap();
        let biome_ids: std::collections::HashMap<String, u32> = registries
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
        let states = blocks.states();
        let state = |n: &str| blocks.default_state(n).ok().map(|s| s as u32);
        let mut palette = crate::feature::Palette::new(&data, &states, &state);
        let read = |dir: &str, name: &str| -> Result<serde_json::Value, String> {
            let path = name.strip_prefix("minecraft:").unwrap_or(name);
            let text = std::fs::read_to_string(
                data.join("data/minecraft/worldgen")
                    .join(dir)
                    .join(format!("{path}.json")),
            )
            .map_err(|e| format!("{dir} {name}: {e}"))?;
            serde_json::from_str(&text).map_err(|e| format!("{dir} {name}: {e}"))
        };
        let order = crate::Generator::load_with_version(&data, 12345, None)
            .unwrap()
            .feature_biomes();
        let structures =
            Structures::load(&data, &mut palette, &order, &biome_ids, &read, -64, 384).unwrap();
        let mut noise = crate::Generator::load_with_version(&data, 12345, None).unwrap();
        noise.bind_biomes(biome_ids.clone());
        let cols = crate::pipeline::Cols {
            noise: &noise,
            carve: crate::CarveBlocks {
                air: 0,
                water: 0,
                lava: 0,
                liquid: 0,
                bedrock: 0,
                grass: 0,
                mycelium: 0,
                dirt: 0,
            },
            stone: 0,
            palette: &palette,
            heights: &std::sync::Mutex::new(std::collections::HashMap::new()),
            bases: &std::sync::Mutex::new(std::collections::HashMap::new()),
            base_limit: 16_384,
        };
        let (si, ki) = structures
            .sets
            .iter()
            .enumerate()
            .find_map(|(si, set)| {
                set.kinds
                    .iter()
                    .position(|k| k.name.contains("trial"))
                    .map(|ki| (si, ki))
            })
            .expect("trial kind");
        let layout = structures.assemble(si, ki, 0, 14, &cols).expect("layout");

        let store = chunkwake_world::anvil::RegionStore::new(path);
        let chunks = store.load_region(0, 0).unwrap();
        let (_, root) = chunks.iter().find(|(p, _)| *p == (0, 14)).unwrap();
        let children = root
            .get("structures")
            .and_then(|t| t.as_compound())
            .and_then(|c| c.get("starts"))
            .and_then(|t| t.as_compound())
            .and_then(|c| c.get("minecraft:trial_chambers"))
            .and_then(|t| t.as_compound())
            .and_then(|c| c.get("Children"))
            .and_then(|t| t.as_list())
            .unwrap();
        assert_eq!(children.len(), layout.pieces.len(), "число кусков");
        let mut bad = 0;
        for (i, (c, p)) in children.iter().zip(layout.pieces.iter()).enumerate() {
            let c = c.as_compound().unwrap();
            let bb = c.get("BB").and_then(|t| t.as_int_array()).unwrap_or(&[]);
            // `StructurePiece` пишет поворот как `O` — 2D-данные `Direction`.
            let rot_str = c.get("rotation").and_then(|t| t.as_str()).unwrap_or("");
            let rot_ok_str = match p.rot {
                Rot::None => rot_str == "NONE",
                Rot::Cw90 => rot_str == "CLOCKWISE_90",
                Rot::Cw180 => rot_str == "CLOCKWISE_180",
                Rot::Ccw90 => rot_str == "COUNTERCLOCKWISE_90",
            };
            let rot_ok = rot_ok_str;
            let same = rot_ok
                && bb.len() == 6
                && p.min[0] == bb[0]
                && p.min[1] == bb[1]
                && p.min[2] == bb[2]
                && p.max[0] == bb[3]
                && p.max[1] == bb[4]
                && p.max[2] == bb[5];
            if !same {
                if bad < 3 {
                    println!(
                        "{i}: rot van {rot_str:?} ours {:?} bb {bb:?} vs {:?}..{:?}",
                        p.rot, p.min, p.max
                    );
                }
                bad += 1;
            }
        }
        assert_eq!(bad, 0, "коробки расходятся");
    }

    #[test]
    fn legacy_type_1_bound_truncates() {
        // `1.0f / 0.2f` в Java — ровно 5, усечение даёт 5.
        assert_eq!((1.0f32 / 0.2f32) as i32, 5);
        let a = frequency_ok(Some(0.2), 12345, 0, 0);
        assert_eq!(a, frequency_ok(Some(0.2), 12345, 0, 0));
        assert!(frequency_ok(None, 1, 3, 4));
        assert!(frequency_ok(Some(1.0), 1, 3, 4));
    }

    #[test]
    fn spread_candidate_stays_in_its_cell() {
        let (x, z) = spread_chunk(34, 8, 10387312, 12345, 0, 0, false);
        assert!((0..26).contains(&x), "{x}");
        assert!((0..26).contains(&z), "{z}");
        assert_eq!(spread_chunk(34, 8, 10387312, 12345, x, z, false), (x, z));
        assert_eq!(
            spread_chunk(34, 8, 10387312, 12345, x + 1, z, false),
            (x, z)
        );
        let mut rng = Legacy::new(0);
        rng.set_large_feature_with_salt(12345, 0, 0, 10387313);
        let dx = (rng.next_int_bound(27) + rng.next_int_bound(27)) / 2;
        let dz = (rng.next_int_bound(27) + rng.next_int_bound(27)) / 2;
        assert_eq!(spread_chunk(32, 5, 10387313, 12345, 0, 0, true), (dx, dz));
    }

    /// `#minecraft:village` — чанк старта, без сборки jigsaw.
    #[test]
    fn nearest_village_is_a_start_chunk() {
        let Some(data) = locate_data() else {
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
        let states = blocks.states();
        let state = |n: &str| blocks.default_state(n).ok().map(|s| s as u32);
        let mut palette = Palette::new(&data, &states, &state);
        let wg = data.join("data/minecraft/worldgen");
        let read = |dir: &str, name: &str| -> Result<Value, String> {
            let path = name.strip_prefix("minecraft:").unwrap_or(name);
            let text = std::fs::read_to_string(wg.join(dir).join(format!("{path}.json")))
                .map_err(|e| format!("{dir} {name}: {e}"))?;
            serde_json::from_str(&text).map_err(|e| format!("{dir} {name}: {e}"))
        };
        let order = crate::Generator::load_with_version(&data, 12345, None)
            .unwrap()
            .feature_biomes();
        let structures =
            Structures::load(&data, &mut palette, &order, &biome_ids, &read, -64, 384).unwrap();
        let biome = structures
            .one_biome("minecraft:village_plains")
            .expect("у деревни есть биом");
        let cols = FlatCols { seed: 7, biome };
        // Крепость теперь ищется по кольцам: в равнине биом подходит.
        let keep = structures
            .nearest(&cols, [0, 64, 0], "minecraft:stronghold")
            .expect("крепость");
        assert_eq!(keep[0].rem_euclid(16), 0);
        assert_eq!(keep[2].rem_euclid(16), 0);
        let pos = structures
            .nearest(&cols, [0, 64, 0], "#minecraft:village")
            .expect("деревня");
        let (sx, sz) = (pos[0].div_euclid(16), pos[2].div_euclid(16));
        assert!(structures.is_structure_start(sx, sz, 7), "{pos:?}");
        assert_eq!(pos[0].rem_euclid(16), 0);
        assert_eq!(pos[2].rem_euclid(16), 0);
        assert_eq!(pos[1], 0);
        assert_eq!(
            structures.layout_count(),
            0,
            "поиск не должен собирать jigsaw"
        );
    }

    fn locate_data() -> Option<std::path::PathBuf> {
        let from_env = std::env::var_os("CHUNKWAKE_DATA").map(std::path::PathBuf::from);
        let fallback = std::path::PathBuf::from("../../target/release/generated/26.3");
        from_env
            .into_iter()
            .chain(std::iter::once(fallback))
            .find(|p| p.join("reports/light.csv").is_file())
    }

    struct FlatCols {
        seed: i64,
        biome: u32,
    }

    impl Columns for FlatCols {
        fn seed(&self) -> i64 {
            self.seed
        }
        fn base_height(&self, _: i32, _: i32) -> i32 {
            64
        }
        fn ocean_floor(&self, _: i32, _: i32) -> i32 {
            64
        }
        fn biome_at(&self, _: i32, _: i32, _: i32) -> Option<u32> {
            Some(self.biome)
        }
        fn column_has(&self, _: i32, _: i32, biomes: &HashSet<u32>) -> bool {
            biomes.contains(&self.biome)
        }
        fn base_block(&self, _: i32, _: i32, _: i32) -> Option<Base> {
            None
        }
        fn min_y(&self) -> i32 {
            -64
        }
        fn max_y(&self) -> i32 {
            320
        }
        fn sea_level(&self) -> i32 {
            63
        }
    }

    /// Сид 12345: клетка деревни, в которой реально стоят доски.
    /// Медленный — гоняет декорацию. `cargo test -- --ignored seed_12345_village`.
    #[test]
    #[ignore]
    fn seed_12345_village() {
        let data = std::path::PathBuf::from(std::env::var_os("CHUNKWAKE_DATA").unwrap());
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
            .unwrap();
        let blocks = chunkwake_data::world::Blocks::load(&data).unwrap();
        let states = blocks.states();
        let mut noise = crate::Generator::load_with_version(&data, 12345, None).unwrap();
        noise.bind_biomes(biome_ids.clone());
        let state = |n: &str| blocks.default_state(n).ok().map(|s| s as u32);
        let world = crate::pipeline::Overworld::new(std::sync::Arc::new(noise), &state, &biome_ids)
            .unwrap()
            .with_features(&data, &states, &state, &biome_ids)
            .unwrap();
        let mut hits = Vec::new();
        for cell_z in -1..=1 {
            for cell_x in -1..=1 {
                let (x, z) = spread_chunk(34, 8, 10387312, 12345, cell_x * 34, cell_z * 34, false);
                let Some(chunk) = world.chunk(x, z) else {
                    continue;
                };
                let planks = chunk
                    .states
                    .iter()
                    .filter(|&&s| {
                        states
                            .get(s as usize)
                            .and_then(|s| s.as_ref())
                            .is_some_and(|(n, _)| {
                                n.ends_with("_planks")
                                    || n.contains("sandstone")
                                    || n == "minecraft:cobblestone"
                            })
                    })
                    .count();
                if planks > 40 {
                    hits.push((x, z, planks));
                }
            }
        }
        eprintln!("villages {hits:?}");
        assert!(!hits.is_empty(), "no village planks");
    }

    #[test]
    fn beard_fills_below_and_carves_above() {
        let beard = Beard {
            boxes: vec![BeardBox {
                min: [0, 64, 0],
                max: [0, 70, 0],
                ground: 64,
                adapt: Adapt::Thin,
            }],
            junctions: Vec::new(),
        };
        assert!(beard.delta(0, 63, 0) > 0.0);
        assert!(beard.delta(0, 70, 0) < 0.0);
        assert_eq!(beard.delta(80, 64, 80), 0.0);
    }
}
