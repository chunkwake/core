//! Конвейер чанка оверворлда (`NoiseBasedChunkGenerator.buildTerrain`):
//! шум и акифер, стадия поверхности, карверы, затем декорация фичами.
//! Один код для сервера, замеров и стенда сверки с ванилью.
//!
//! Фичи чанка пишут в соседей, поэтому, как `ChunkStep` ванили, чанк отдаётся
//! только когда задекорированы все девять чанков вокруг него. Для этого
//! держится область прото-чанков (после карверов): декорация чанка берёт
//! его 3×3, готовый чанк требует 5×5.

use crate::feature::structure::{Base, Beard, Columns};
use crate::feature::{self, Features, Heightmap};
use crate::surface::{self, Chunk as _, Kind, Rule};
use crate::{BiomeMemo, CarveBlocks, Fill, Generator};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex};

/// `getBaseHeight` по столбцам: `WORLD_SURFACE_WG` и `OCEAN_FLOOR_WG`.
type BaseHeights = Mutex<HashMap<(i32, i32), (i32, i32)>>;
/// `getBaseColumn` по столбцам.
type BaseColumns = Mutex<HashMap<(i32, i32), Arc<Vec<Fill>>>>;

/// Блоки чанка по индексу `((y - min_y) * 16 + z) * 16 + x` и его биомы
/// по клеткам 4×4×4 (`None` — климата нет, мир из одного биома).
///
/// Наружу состояния идут `u32`; прото-чанки внутри генератора хранятся
/// `u16` ([`Proto`]): состояний меньше 65536, а прото-чанков у края мира
/// тысячи, и `u32` удваивал их память (~393 КБ против ~197 КБ на чанк).
#[derive(Clone, Default)]
pub struct Blocks<S = u32> {
    pub states: Vec<S>,
    pub biomes: Option<Vec<u32>>,
    /// Карты `WORLD_SURFACE_WG` и `OCEAN_FLOOR_WG` после рельефа: считаются
    /// при первой декорации, до записей фич.
    pub worldgen_heights: Option<Arc<[[i32; 256]; 2]>>,
    /// Живые карты высот прото-чанка (`Heightmap.update`): строятся при
    /// первом запросе декорации и дальше идут за её записями. Раньше их
    /// заново сканировал каждый регион 3×3 — до девяти чанков на декорацию.
    live_heights: Option<Box<Heights>>,
    /// Заборы шахты: форма сторон считается заново, когда чанк отдают и
    /// соседи в радиусе 1 уже декорированы (`postProcessGeneration`).
    shape_posts: Vec<[i32; 3]>,
    /// Блок-сущности структур (LootTable + LootTableSeed у сундуков).
    pub block_entities: Vec<([i32; 3], String, chunkwake_world::nbt::Compound)>,
    /// Жидкость, которую сервер тикнет, когда чанк станет полным
    /// (`postProcessGeneration` и `fluid_ticks` прото-чанка): стыки уровней
    /// акифера и родники. Мировые координаты.
    pub fluid_posts: Vec<[i32; 3]>,
    /// Блоки, которым фича поставила тик (`simple_block` с
    /// `schedule_tick`). Мировые координаты.
    pub block_ticks: Vec<[i32; 3]>,
}

/// Прото-чанк в области генератора.
type Proto = Blocks<u16>;

impl Blocks<u32> {
    /// В хранение области: состояния в `u16`.
    fn pack(self) -> Proto {
        let Self {
            states,
            biomes,
            worldgen_heights,
            live_heights,
            shape_posts,
            block_entities,
            fluid_posts,
            block_ticks,
        } = self;
        Blocks {
            states: states
                .into_iter()
                .map(|s| u16::try_from(s).expect("block state id fits u16"))
                .collect(),
            biomes,
            worldgen_heights,
            live_heights,
            shape_posts,
            block_entities,
            fluid_posts,
            block_ticks,
        }
    }
}

impl Proto {
    /// Отдаваемый чанк: состояния обратно в `u32`.
    fn unpack(self) -> Blocks {
        let Self {
            states,
            biomes,
            worldgen_heights,
            live_heights,
            shape_posts,
            block_entities,
            fluid_posts,
            block_ticks,
        } = self;
        Blocks {
            states: states.into_iter().map(u32::from).collect(),
            biomes,
            worldgen_heights,
            live_heights,
            shape_posts,
            block_entities,
            fluid_posts,
            block_ticks,
        }
    }
}

/// Скомпилированная стадия поверхности.
pub(crate) struct Surface {
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
    /// `getBaseHeight` по столбцам: `WORLD_SURFACE_WG` и `OCEAN_FLOOR_WG`.
    base_heights: BaseHeights,
    /// `getBaseColumn`: шум с акифером, по столбцам.
    base_columns: BaseColumns,
    area_soft_limit: usize,
    area_evict_slack: usize,
    base_column_limit: usize,
    area: Mutex<Area>,
    /// Будит ждущих, когда прото-чанк построен или декорация вернула область.
    wake: Condvar,
}

/// Прото-чанки, ещё нужные декорации, и что уже сделано.
///
/// Замок держится только на учёт: прото-чанки строятся и декорируются вне
/// него. Декорация забирает свою область 3×3 из `protos` (чанки становятся
/// `busy`), и несоседние области декорируются параллельно, как в ваниле.
/// Порядок декорации пересекающихся областей — порядок прихода запросов.
#[derive(Default)]
struct Area {
    /// Прото-чанки ещё не отданных чанков и их соседей.
    protos: Protos,
    /// Чанки, чья декорация уже прошла: повторно фичи не ставятся.
    decorated: HashSet<(i32, i32)>,
    /// Отданные чанки. Их прото-чанк больше никому не нужен: область 3×3,
    /// задевающая отданный чанк, центрирована на его соседе, а все соседи
    /// отданного уже декорированы. Поэтому он уходит из `protos` сразу.
    ///
    /// Раньше отданные держались до предела памяти и вытеснялись вместе с
    /// отметкой `decorated`: на радиусе 24 и больше предел превышался, и
    /// соседей декорировали повторно, а запись фичи в уже отданный чанк
    /// терялась — деревья обрезались по границе чанка.
    done: HashSet<(i32, i32)>,
    /// Прото-чанки, которые сейчас строит какой-то поток.
    building: HashSet<(i32, i32)>,
    /// Чанки, забранные декорацией: их нет в `protos`, пока она идёт.
    busy: HashSet<(i32, i32)>,
    /// Центры идущих декораций.
    decorating: HashSet<(i32, i32)>,
    /// Центры идущих запросов `chunk` (с кратностью): их области 5×5 не
    /// вытесняются, пока запрос ждёт соседей без замка.
    active: HashMap<(i32, i32), u32>,
}

/// Прото-чанки области: нужные сейчас — как есть, давно не нужные — со
/// сжатыми состояниями.
///
/// Тронутый декорацией прото-чанк выбросить нельзя (запись соседа в него
/// пропала бы), а за полётом их остаётся полоса по обе стороны пути: три
/// минуты на обзоре 32 — 2300 штук и 430 МБ. Состояния прото-чанка — слои
/// воздуха, камня и воды, RLE сжимает их в десятки раз.
#[derive(Default)]
struct Protos {
    hot: HashMap<(i32, i32), Proto>,
    /// `states` пуст, состояния в RLE: пары (состояние, длина − 1).
    cold: HashMap<(i32, i32), ColdProto>,
    /// Часы последнего обращения к горячему.
    used: HashMap<(i32, i32), u64>,
    /// Растёт на каждый запрос чанка.
    clock: u64,
}

impl Protos {
    fn len(&self) -> usize {
        self.hot.len() + self.cold.len()
    }

    fn contains_key(&self, p: &(i32, i32)) -> bool {
        self.hot.contains_key(p) || self.cold.contains_key(p)
    }

    fn keys(&self) -> impl Iterator<Item = &(i32, i32)> {
        self.hot.keys().chain(self.cold.keys())
    }

    fn insert(&mut self, p: (i32, i32), b: Proto) {
        self.cold.remove(&p);
        self.used.insert(p, self.clock);
        self.hot.insert(p, b);
    }

    fn insert_if_absent(&mut self, p: (i32, i32), b: Proto) {
        if !self.contains_key(&p) {
            self.insert(p, b);
        }
    }

    /// Сжатый — обратно в горячие; отметка обращения.
    fn thaw(&mut self, p: (i32, i32)) {
        if let Some((mut b, rle)) = self.cold.remove(&p) {
            b.states = rle_decode(&rle);
            self.hot.insert(p, b);
        }
        if self.hot.contains_key(&p) {
            self.used.insert(p, self.clock);
        }
    }

    fn remove(&mut self, p: &(i32, i32)) -> Option<Proto> {
        self.thaw(*p);
        self.used.remove(p);
        self.hot.remove(p)
    }

    /// Выбросить, не распаковывая.
    fn discard(&mut self, p: &(i32, i32)) {
        self.hot.remove(p);
        self.cold.remove(p);
        self.used.remove(p);
    }

    /// Только горячий: сжатые сначала [`Self::thaw`].
    fn get(&self, p: &(i32, i32)) -> Option<&Proto> {
        self.hot.get(p)
    }

    fn get_mut(&mut self, p: &(i32, i32)) -> Option<&mut Proto> {
        self.thaw(*p);
        self.hot.get_mut(p)
    }

    /// Сжимает горячие, к которым давно не обращались, кроме `pinned`.
    fn freeze_idle(&mut self, pinned: impl Fn(&(i32, i32)) -> bool) {
        let clock = self.clock;
        let idle: Vec<(i32, i32)> = self
            .used
            .iter()
            .filter(|(p, at)| clock - **at > PROTO_FREEZE_AFTER && !pinned(p))
            .map(|(p, _)| *p)
            .collect();
        for p in idle {
            self.used.remove(&p);
            if let Some(mut b) = self.hot.remove(&p) {
                let rle = rle_encode(&b.states);
                b.states = Vec::new();
                // Карты высот строятся заново при следующей декорации.
                b.live_heights = None;
                self.cold.insert(p, (b, rle));
            }
        }
    }

    /// Куча состояний: горячие и сжатые, байты.
    fn bytes(&self) -> (usize, usize) {
        (
            self.hot.values().map(|b| b.states.capacity() * 2).sum(),
            self.cold.values().map(|(_, r)| r.capacity() * 2).sum(),
        )
    }
}

/// Прото-чанк без состояний и его состояния в RLE.
type ColdProto = (Proto, Vec<u16>);

/// Запросов чанка без обращения, после которых прото-чанк сжимается
/// (на полёте ~160 запросов в секунду — около 13 с).
const PROTO_FREEZE_AFTER: u64 = 2048;

fn rle_encode(states: &[u16]) -> Vec<u16> {
    let mut out = Vec::new();
    let mut it = states.iter().copied();
    let Some(mut cur) = it.next() else {
        return out;
    };
    let mut run: u16 = 0;
    for s in it {
        if s == cur && run < u16::MAX {
            run += 1;
        } else {
            out.extend([cur, run]);
            cur = s;
            run = 0;
        }
    }
    out.extend([cur, run]);
    out.shrink_to_fit();
    out
}

fn rle_decode(rle: &[u16]) -> Vec<u16> {
    let (pairs, _) = rle.as_chunks::<2>();
    let total: usize = pairs.iter().map(|&[_, n]| n as usize + 1).sum();
    let mut out = Vec::with_capacity(total);
    for &[s, n] in pairs {
        out.extend(std::iter::repeat_n(s, n as usize + 1));
    }
    out
}

impl Area {
    /// В простое оставляем затронутые декорацией прото, чтобы не потерять
    /// запись на границе; чистые можно построить повторно без отличий.
    fn trim_idle(&mut self) {
        if !self.active.is_empty()
            || !self.building.is_empty()
            || !self.busy.is_empty()
            || !self.decorating.is_empty()
        {
            return;
        }
        let clean: Vec<_> = self
            .protos
            .keys()
            .filter(|p| {
                !(-1..=1)
                    .any(|dz| (-1..=1).any(|dx| self.decorated.contains(&(p.0 + dx, p.1 + dz))))
            })
            .copied()
            .collect();
        for pos in clean {
            self.protos.discard(&pos);
        }
        self.protos.clock += PROTO_FREEZE_AFTER + 1;
        self.protos.freeze_idle(|_| false);
    }

    /// Сжимает давно не нужные прото-чанки (раз в 64 запроса: обход словаря).
    fn freeze(&mut self) {
        self.protos.clock += 1;
        if !self.protos.clock.is_multiple_of(64) {
            return;
        }
        let active = &self.active;
        let pinned = |p: &(i32, i32)| {
            active
                .keys()
                .any(|c| (c.0 - p.0).abs() <= 2 && (c.1 - p.1).abs() <= 2)
        };
        self.protos.freeze_idle(pinned);
    }

    /// Выбрасывает прото-чанки сверх предела, но только чистые: ни одна
    /// декорация в них ещё не писала, и построенный заново будет тем же.
    /// Такие копятся за краем прорисовки (область 5×5 запроса шире радиуса)
    /// и у чанков, выпавших из очереди, когда игрок улетел.
    ///
    /// Обход всех ключей дорогой: на радиусе 24 `protos` держит больше
    /// двух тысяч столбцов, и вызов на каждый чанк превращался в проход по
    /// всему словарю под общим замком. Поэтому чистим, только когда запас
    /// вырос заметно, и сразу до половины предела.
    fn evict(&mut self, soft_limit: usize, slack: usize) {
        if self.protos.len() <= soft_limit + slack {
            return;
        }
        let touched = |p: &(i32, i32)| {
            (-1..=1).any(|dz| (-1..=1).any(|dx| self.decorated.contains(&(p.0 + dx, p.1 + dz))))
        };
        let pinned = |p: &(i32, i32)| {
            self.active
                .keys()
                .any(|c| (c.0 - p.0).abs() <= 2 && (c.1 - p.1).abs() <= 2)
        };
        let clean: Vec<(i32, i32)> = self
            .protos
            .keys()
            .filter(|p| !touched(p) && !pinned(p))
            .copied()
            .collect();
        let excess = self.protos.len() - soft_limit / 2;
        for p in clean.into_iter().take(excess) {
            self.protos.discard(&p);
        }
    }
}

/// Прото-чанков сверх этого — повод выбросить чистые (по ~393 КБ каждый).
const AREA_SOFT_LIMIT: usize = 2048;
/// Запас сверх предела, после которого чистка того стоит: обход словаря
/// на каждый чанк дороже, чем подержать лишнюю тысячу столбцов.
const AREA_EVICT_SLACK: usize = 1024;

impl Overworld {
    /// Сбрасывает только воспроизводимые кэши и холодит нужные для границ прото.
    pub fn trim_idle(&self) {
        self.area.lock().unwrap().trim_idle();
        self.base_columns.lock().unwrap().clear();
        self.base_heights.lock().unwrap().clear();
        self.noise.trim_idle();
    }

    /// `state` — состояние блока по имени (`minecraft:stone`); `biome_ids` —
    /// сетевые ID биомов для условий `minecraft:biome`.
    pub fn new(
        noise: Arc<Generator>,
        state: &dyn Fn(&str) -> Option<u32>,
        biome_ids: &HashMap<String, u32>,
    ) -> Result<Self, String> {
        Self::new_for(
            noise,
            state,
            biome_ids,
            "minecraft:stone",
            "minecraft:water",
        )
    }

    /// Как [`Self::new`], но с блоком вместо `minecraft:stone` и с настройками
    /// своего измерения: у Нижнего мира `default_block` — `netherrack`,
    /// карверы там не ставит берёзу и мицелий.
    pub fn new_for(
        noise: Arc<Generator>,
        state: &dyn Fn(&str) -> Option<u32>,
        biome_ids: &HashMap<String, u32>,
        default_block: &str,
        default_fluid: &str,
    ) -> Result<Self, String> {
        let need = |name: &str| state(name).ok_or_else(|| format!("no block state {name}"));
        let carve = CarveBlocks {
            air: need("minecraft:air")?,
            water: need("minecraft:water")?,
            lava: need("minecraft:lava")?,
            liquid: need(default_fluid)?,
            bedrock: need("minecraft:bedrock")?,
            grass: need("minecraft:grass_block")?,
            mycelium: need("minecraft:mycelium")?,
            dirt: need("minecraft:dirt")?,
        };
        let stone = need(default_block)?;
        let surface = Self::compile_surface(&noise, state, biome_ids)?;
        Ok(Self {
            noise,
            surface,
            carve,
            stone,
            features: None,
            base_heights: Mutex::new(HashMap::new()),
            base_columns: Mutex::new(HashMap::new()),
            area_soft_limit: AREA_SOFT_LIMIT,
            area_evict_slack: AREA_EVICT_SLACK,
            base_column_limit: 16_384,
            area: Mutex::new(Area::default()),
            wake: Condvar::new(),
        })
    }

    /// Лимиты горячих данных, выбранные серверным профилем памяти.
    pub fn with_memory_limits(mut self, proto_limit: usize, base_column_limit: usize) -> Self {
        self.area_soft_limit = proto_limit.max(64);
        self.area_evict_slack = (self.area_soft_limit / 2).max(64);
        self.base_column_limit = base_column_limit.max(128);
        self
    }

    /// Ближайший биом для `/locate biome`. Имя — `plains` или `minecraft:plains`.
    ///
    /// Поиск идёт на высоте игрока, а в ответ подставляется поверхность
    /// столбца: иначе `/tp` по клику оставляет ноги внутри холма.
    pub fn locate_biome(&self, name: &str, origin: [i32; 3]) -> Option<[i32; 3]> {
        let mut hit = self.noise.find_biome(name, origin, 4_096)?;
        if let Some(cols) = self.columns() {
            hit[1] = cols.base_height(hit[0], hit[2]);
        }
        Some(hit)
    }

    /// Ближайшая поставленная структура для `/locate structure`. Чанк не строится.
    pub fn locate_structure(&self, name: &str, origin: [i32; 3]) -> Option<[i32; 3]> {
        let structures = self.features.as_ref()?.structures.as_ref()?;
        structures.nearest(&self.columns()?, origin, name)
    }

    fn columns(&self) -> Option<Cols<'_>> {
        let features = self.features.as_ref()?;
        Some(Cols {
            noise: &self.noise,
            carve: self.carve,
            stone: self.stone,
            palette: &features.palette,
            heights: &self.base_heights,
            bases: &self.base_columns,
            base_limit: self.base_column_limit,
        })
    }

    /// Включает фичи декорации. `states` — по ID состояния имя блока и
    /// свойства; `default` — состояние блока по умолчанию; `biome_ids` —
    /// сетевые ID биомов.
    pub fn with_features(
        self,
        data_dir: &Path,
        states: &feature::States,
        default: &dyn Fn(&str) -> Option<u32>,
        biome_ids: &HashMap<String, u32>,
    ) -> Result<Self, String> {
        self.with_features_palette(
            data_dir,
            feature::Palette::new(data_dir, states, default),
            biome_ids,
        )
    }

    pub fn with_features_palette(
        mut self,
        data_dir: &Path,
        palette: feature::Palette,
        biome_ids: &HashMap<String, u32>,
    ) -> Result<Self, String> {
        let n = &self.noise;
        let order = n.feature_biomes();
        if order.is_empty() {
            return Ok(self);
        }
        self.features = Some(Features::load(
            data_dir,
            palette,
            &order,
            biome_ids,
            n.seed(),
            n.min_y,
            n.height,
        )?);
        Ok(self)
    }

    /// Перенесённые и все фичи оверворлда.
    /// Размеры кэшей генератора для замера памяти (`CHUNKWAKE_MEMSTATS`).
    pub fn mem_stats(&self) -> String {
        let (protos, proto_bytes, decorated, done) = {
            let a = self.area.lock().unwrap();
            let (hot, cold) = a.protos.bytes();
            (
                a.protos.len(),
                (hot, cold, a.protos.cold.len()),
                a.decorated.len(),
                a.done.len(),
            )
        };
        let heights = self.base_heights.lock().unwrap().len();
        let (bases, base_bytes) = {
            let b = self.base_columns.lock().unwrap();
            (b.len(), b.values().map(|v| v.capacity()).sum::<usize>())
        };
        let layouts = self
            .features
            .as_ref()
            .and_then(|f| f.structures.as_ref())
            .map_or(0, |s| s.layout_count());
        let rings = self
            .features
            .as_ref()
            .and_then(|f| f.structures.as_ref())
            .map_or(0, |s| s.ring_count());
        let palette_bytes = self.features.as_ref().map_or(0, |f| f.palette.heap_bytes());
        format!(
            "protos {protos} (hot {} MB, cold {} of them {} MB), decorated {decorated}, done {done}, base_heights {heights}, base_columns {bases} ({} MB), layouts {layouts}, rings {rings}, palette {} MB",
            proto_bytes.0 >> 20,
            proto_bytes.2,
            proto_bytes.1 >> 20,
            base_bytes >> 20,
            palette_bytes >> 20
        )
    }

    pub fn feature_coverage(&self) -> Option<(usize, usize)> {
        self.features.as_ref().map(Features::coverage)
    }

    /// Фичи, которые не разобрались: имя и причина.
    pub fn feature_failures(&self) -> &[(String, String)] {
        self.features.as_ref().map_or(&[], Features::failures)
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
        let (files, conditions) = noise.surface_files();
        let (id, inline) = noise.surface_rule()?;
        if files.is_empty() && inline.is_none() {
            return Ok(None);
        }
        let mut rules = files.clone();
        if let Some(rule) = inline {
            rules.insert(id.to_owned(), rule.clone());
        }
        let mut names = HashMap::new();
        for rule in rules.values() {
            collect_states(rule, state, &mut names);
        }
        let noises = noise.surface_noises();
        let noise_names: Vec<String> = noises.iter().map(|(n, _)| n.clone()).collect();
        let densities = noise.vein_names().to_vec();
        let mut compiled = surface::Rules::new(
            rules,
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
            .top(id)
            .map_err(|e| format!("material_rule {id}: {e}"))?;
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
    ///
    /// `None` — чанк уже отдан раньше (декорация ушла вместе с ним), и
    /// строить его заново незачем: вызывающий держит готовый столбец.
    ///
    /// Порядок декорации — порядок запросов, как у ванилы: чанк декорируется
    /// один раз, при первой области 3×3, которой он понадобился, и его записи
    /// остаются в прото-чанках соседей. Перестановка этого порядка меняет
    /// результат (проверено: трава 3001 → 2872 на сиде 12345); несоседние
    /// области декорируются параллельно.
    ///
    /// Отданный прото-чанк уходит из области (см. [`Area::done`]) и заново
    /// не строится: область 5×5 следующего запроса его пропускает.
    pub fn chunk(&self, cx: i32, cz: i32) -> Option<Blocks> {
        let Some(features) = &self.features else {
            return Some(self.proto(cx, cz));
        };
        let mut area = self.area.lock().unwrap();
        *area.active.entry((cx, cz)).or_default() += 1;
        let out = self.chunk_locked(features, area, cx, cz);
        let mut area = self.area.lock().unwrap();
        if let std::collections::hash_map::Entry::Occupied(mut e) = area.active.entry((cx, cz)) {
            *e.get_mut() -= 1;
            if *e.get() == 0 {
                e.remove();
            }
        }
        area.evict(self.area_soft_limit, self.area_evict_slack);
        area.freeze();
        drop(area);
        // Раньше здесь на `None` строился полный прото-чанк впустую (16 мс),
        // чтобы вызывающий его выбросил.
        out
    }

    /// Тело [`Self::chunk`] под замком области; `None` — чанк уже отдан.
    fn chunk_locked<'a>(
        &'a self,
        features: &Features,
        mut area: std::sync::MutexGuard<'a, Area>,
        cx: i32,
        cz: i32,
    ) -> Option<Blocks> {
        let around =
            |r: i32| (-r..=r).flat_map(move |dz| (-r..=r).map(move |dx| (cx + dx, cz + dz)));
        // Прото-чанки 5×5: по одному за раз, ближние к центру первыми. Каждый
        // строит один поток, остальные берут следующий свободный. Раньше
        // первый запрос в новой области забирал все 25 и строил их подряд
        // (~0.5 с), а соседние запросы ждали его — на краю мира вся
        // параллельность пропадала, и рядом с игроком было пусто.
        loop {
            let mine = around(2)
                .filter(|p| {
                    !area.protos.contains_key(p)
                        && !area.busy.contains(p)
                        && !area.building.contains(p)
                        && !area.done.contains(p)
                })
                .min_by_key(|p| (p.0 - cx).abs().max((p.1 - cz).abs()));
            if let Some(p) = mine {
                area.building.insert(p);
                drop(area);
                let b = self.proto_at(p.0, p.1, Some(features)).pack();
                area = self.area.lock().unwrap();
                area.building.remove(&p);
                area.protos.insert_if_absent(p, b);
                self.wake.notify_all();
                continue;
            }
            if around(2).any(|p| area.building.contains(&p)) {
                area = self.wake.wait(area).unwrap();
                continue;
            }
            break;
        }
        if !area.done.contains(&(cx, cz)) {
            // `ChunkGenerationTask.scheduleLayer`: соседи по x снаружи, по z
            // внутри. Пятна соседних чанков перекрываются, и кто первый
            // занял незерак — решает этот порядок.
            for n in (-1..=1).flat_map(|dx| (-1..=1).map(move |dz| (cx + dx, cz + dz))) {
                area = self.decorate_one(features, area, n);
            }
            // Декорации соседей, начатые другими потоками, должны закончиться.
            while around(1).any(|p| area.decorating.contains(&p)) {
                area = self.wake.wait(area).unwrap();
            }
            // Тот же чанк мог отдать параллельный запрос, пока этот ждал.
            if area.done.insert((cx, cz)) {
                self.refresh_shapes(features, &mut area, cx, cz);
                let mut chunk = area
                    .protos
                    .remove(&(cx, cz))
                    .expect("own proto chunk stays in the area")
                    .unpack();
                feature::bubble_columns(&features.palette, &mut chunk.states);
                return Some(chunk);
            }
        }
        // Повторный запрос отданного чанка: декорация уже ушла с ним, вызов
        // вернёт только основу (вызывающий держит готовый столбец и её отбросит).
        None
    }

    /// Декорация чанка `n` над его областью 3×3, если её ещё не было.
    /// Прото-чанки области уже построены.
    fn decorate_one<'a>(
        &'a self,
        features: &Features,
        mut area: std::sync::MutexGuard<'a, Area>,
        n: (i32, i32),
    ) -> std::sync::MutexGuard<'a, Area> {
        // Своя область 3×3 должна быть на месте: её может держать
        // чужая декорация.
        loop {
            if area.decorated.contains(&n) {
                return area;
            }
            let region = (-1..=1).flat_map(|dz| (-1..=1).map(move |dx| (n.0 + dx, n.1 + dz)));
            if region.clone().any(|p| area.busy.contains(&p)) {
                area = self.wake.wait(area).unwrap();
                continue;
            }
            area.decorated.insert(n);
            area.decorating.insert(n);
            let at = |i: usize| (n.0 + i as i32 % 3 - 1, n.1 + i as i32 / 3 - 1);
            let mut chunks: [Option<Proto>; 9] = Default::default();
            for (i, slot) in chunks.iter_mut().enumerate() {
                *slot = area.protos.remove(&at(i));
                if slot.is_some() {
                    area.busy.insert(at(i));
                }
            }
            drop(area);
            let chunks = self.decorate(features, chunks, n);
            area = self.area.lock().unwrap();
            for (i, slot) in chunks.into_iter().enumerate() {
                if let Some(c) = slot {
                    area.busy.remove(&at(i));
                    area.protos.insert(at(i), c);
                }
            }
            area.decorating.remove(&n);
            self.wake.notify_all();
            return area;
        }
    }

    /// Декорировать чанк заранее, не отдавая его. Стенды так повторяют
    /// приоритет ванили: чанки с тикетом декорируются раньше кольца вокруг.
    /// Однопоточно: только для сверки.
    pub fn predecorate(&self, cx: i32, cz: i32) {
        let Some(features) = &self.features else {
            return;
        };
        let mut area = self.area.lock().unwrap();
        if area.decorated.contains(&(cx, cz)) {
            return;
        }
        for dz in -1..=1 {
            for dx in -1..=1 {
                let p = (cx + dx, cz + dz);
                if !area.protos.contains_key(&p) && !area.done.contains(&p) {
                    drop(area);
                    let b = self.proto_at(p.0, p.1, Some(features)).pack();
                    area = self.area.lock().unwrap();
                    area.protos.insert_if_absent(p, b);
                }
            }
        }
        drop(self.decorate_one(features, area, (cx, cz)));
    }

    /// Стороны заборов шахты и решёток крепости по уже декорированным соседям.
    /// Чанк, которого нет в области, сторону не меняет: его блоки ушли вместе
    /// с отдачей.
    fn refresh_shapes(&self, features: &Features, area: &mut Area, cx: i32, cz: i32) {
        self.refresh_chunk_shapes(features, area, cx, cz, true);
        // Уходящий чанк последний раз видит готовых соседей: их клетки на
        // границе получают свою сторону от него. Их собственный список
        // пометок не чистим — они ещё не отданы.
        for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            self.refresh_chunk_shapes(features, area, cx + dx, cz + dz, false);
        }
    }

    fn refresh_chunk_shapes(
        &self,
        features: &Features,
        area: &mut Area,
        cx: i32,
        cz: i32,
        own: bool,
    ) {
        // Сжатые соседи читаются ниже без распаковки — распаковать заранее.
        area.protos.thaw((cx, cz));
        if area
            .protos
            .get(&(cx, cz))
            .is_some_and(|c| !c.shape_posts.is_empty())
        {
            for dz in -1..=1 {
                for dx in -1..=1 {
                    area.protos.thaw((cx + dx, cz + dz));
                }
            }
        }
        let Some(chunk) = area.protos.get(&(cx, cz)) else {
            return;
        };
        if chunk.shape_posts.is_empty() {
            return;
        }
        let posts = chunk.shape_posts.clone();
        let min_y = self.noise.min_y;
        let height = self.noise.height;
        let get = |x: i32, y: i32, z: i32| -> Option<u32> {
            let ry = y - min_y;
            if ry < 0 || ry >= height {
                return None;
            }
            let chunk = area.protos.get(&(x >> 4, z >> 4))?;
            let i = (ry as usize * 16 + (z & 15) as usize) * 16 + (x & 15) as usize;
            chunk.states.get(i).copied().map(u32::from)
        };
        let mut updates = crate::feature::mineshaft::fence_updates(&features.palette, &posts, get);
        // Стены `cobblestone_wall` пересобираются тем же проходом.
        updates.extend(crate::feature::mineshaft::wall_updates(
            &features.palette,
            &posts,
            get,
        ));
        for ([x, y, z], state) in updates {
            let Some(chunk) = area.protos.get_mut(&(x >> 4, z >> 4)) else {
                continue;
            };
            let ry = y - min_y;
            if ry < 0 || ry >= height {
                continue;
            }
            let i = (ry as usize * 16 + (z & 15) as usize) * 16 + (x & 15) as usize;
            if let Some(slot) = chunk.states.get_mut(i) {
                *slot = state as u16;
                chunk.live_heights = None;
            }
        }
        if own && let Some(chunk) = area.protos.get_mut(&(cx, cz)) {
            chunk.shape_posts.clear();
        }
    }

    /// `applyBiomeDecoration` чанка `(cx, cz)` над его областью 3×3,
    /// забранной из общей области.
    fn decorate(
        &self,
        features: &Features,
        mut chunks: [Option<Proto>; 9],
        (cx, cz): (i32, i32),
    ) -> [Option<Proto>; 9] {
        for c in chunks.iter_mut().flatten() {
            if c.worldgen_heights.is_none() {
                let mut maps = [[0; 256]; 2];
                for (map, kind) in maps
                    .iter_mut()
                    .zip([Heightmap::WorldSurfaceWg, Heightmap::OceanFloorWg])
                {
                    for (column, h) in map.iter_mut().enumerate() {
                        *h = column_height(
                            &c.states,
                            column,
                            self.noise.height,
                            kind,
                            &features.palette,
                        ) + self.noise.min_y;
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
        let heights =
            std::array::from_fn(|i| chunks[i].as_mut().and_then(|c| c.live_heights.take()));
        let mut region = Region {
            chunks,
            cx,
            cz,
            noise: &self.noise,
            palette: &features.palette,
            air: self.carve.air,
            memo: RefCell::new(Default::default()),
            heights: RefCell::new(heights),
        };
        let cols = Cols {
            noise: &self.noise,
            carve: self.carve,
            stone: self.stone,
            palette: &features.palette,
            heights: &self.base_heights,
            bases: &self.base_columns,
            base_limit: self.base_column_limit,
        };
        features.decorate(&mut region, cx, cz, &biomes, Some(&cols));
        let mut chunks = region.chunks;
        for (c, h) in chunks.iter_mut().zip(region.heights.into_inner()) {
            if let Some(c) = c {
                c.live_heights = h;
            }
        }
        chunks
    }

    /// Прото-чанк: шум, поверхность, карверы.
    ///
    /// `CHUNKWAKE_PROF=1` печатает время стадий в stderr: без этого видно
    /// только «20 мс на чанк» и непонятно, что оптимизировать.
    fn proto(&self, cx: i32, cz: i32) -> Blocks {
        self.proto_at(cx, cz, None)
    }

    fn proto_at(&self, cx: i32, cz: i32, features: Option<&Features>) -> Blocks {
        let prof = std::env::var_os("CHUNKWAKE_PROF").is_some();
        let t0 = std::time::Instant::now();
        let structures = features.and_then(|f| f.structures.clone());
        let beard = if let Some(structures) = &structures {
            let features = features.expect("features");
            let cols = Cols {
                noise: &self.noise,
                carve: self.carve,
                stone: self.stone,
                palette: &features.palette,
                heights: &self.base_heights,
                bases: &self.base_columns,
                base_limit: self.base_column_limit,
            };
            structures.beard_at(&cols, cx, cz)
        } else {
            Beard::default()
        };
        let t_beard = t0.elapsed().as_secs_f64() * 1000.0;
        let noise = &*self.noise;
        let mut terrain = if beard.is_empty() {
            noise.terrain(cx, cz)
        } else {
            noise.terrain_beard(cx, cz, &beard)
        };
        let t_noise = t0.elapsed().as_secs_f64() * 1000.0;
        let c = self.carve;
        let mut states: Vec<u32> = terrain
            .fills
            .iter()
            .map(|f| match f {
                Fill::Air => c.air,
                Fill::Solid => self.stone,
                // `default_fluid` настроек: в оверворлде вода, в Аду лава.
                Fill::Liquid => c.liquid,
                Fill::Water => c.water,
                Fill::Lava => c.lava,
            })
            .collect();
        // Климат считается один раз на чанк: биомы нужны и стадии
        // поверхности, и палитре секций.
        let t = std::time::Instant::now();
        let biomes = noise.biomes(cx, cz);
        let t_biomes = t.elapsed().as_secs_f64() * 1000.0;
        let own = biomes.as_deref().unwrap_or(&[]);
        let memo = RefCell::new(BiomeMemo::default());
        let t = std::time::Instant::now();
        let preliminary = noise.preliminary(cx, cz);
        let t_prelim = t.elapsed().as_secs_f64() * 1000.0;
        let random = noise.surface_random();
        let t = std::time::Instant::now();
        // `CHUNKWAKE_NO_SURFACE=1` — сверка без стадии поверхности: видно,
        // что именно она меняет, отдельно от рельефа и акуифера.
        if let Some(s) = &self.surface
            && std::env::var_os("CHUNKWAKE_NO_SURFACE").is_none()
        {
            let mut chunk = self.view(&mut states, cx, cz, own, &memo);
            s.stage(noise, &preliminary, &random)
                .build(&mut chunk, cx, cz);
        }
        let t_surface = t.elapsed().as_secs_f64() * 1000.0;
        // Карверы после поверхности (`generateCarvers`): тоннели и каньоны,
        // залитые акифером того же чанка.
        let t = std::time::Instant::now();
        let mut carved = false;
        if let Some(mask) = noise.carving_mask(cx, cz) {
            carved = true;
            let mut heights: Option<Vec<usize>> = None;
            noise.carve(
                &mut terrain,
                &mask,
                &c,
                &mut states,
                |st, x, y, z, fluid| {
                    let s = self.surface.as_ref()?;
                    let chunk = self.view(st, cx, cz, own, &memo);
                    let heights = heights.get_or_insert_with(|| {
                        (0..256).map(|i| chunk.surface(i % 16, i / 16)).collect()
                    });
                    let y = (y - noise.min_y) as usize;
                    s.stage(noise, &preliminary, &random)
                        .top_material(&chunk, x, y, z, cx, cz, heights, fluid)
                },
            );
        }
        let t_carve = t.elapsed().as_secs_f64() * 1000.0;
        if prof {
            eprintln!(
                "proto {cx} {cz}: борода {t_beard:.2} | шум {t_noise:.2} | биомы {t_biomes:.2} | \
                 предв. поверхность {t_prelim:.2} | поверхность {t_surface:.2} | \
                 карверы {t_carve:.2}{} = {:.2} мс, жидкость {}",
                if carved { "" } else { " (нет)" },
                t0.elapsed().as_secs_f64() * 1000.0,
                terrain.fluid_posts.len()
            );
        }
        Blocks {
            states,
            biomes,
            worldgen_heights: None,
            live_heights: None,
            shape_posts: Vec::new(),
            block_entities: Vec::new(),
            fluid_posts: std::mem::take(&mut terrain.fluid_posts),
            block_ticks: Vec::new(),
        }
    }

    /// Прото-чанк без декорации: для золотого теста генерации.
    ///
    /// Фичи зависят от порядка запросов, а рельеф и поверхность — нет,
    /// поэтому эталон ловит именно их.
    pub fn proto_for_test(&self, cx: i32, cz: i32) -> Blocks {
        self.proto(cx, cz)
    }

    /// Чанк статуса `terrain` (шум с бородой структур, поверхность,
    /// карверы) без декорации: сверка с прото-чанками ванили.
    pub fn terrain_for_test(&self, cx: i32, cz: i32) -> Blocks {
        self.proto_at(cx, cz, self.features.as_ref())
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
            clay_offset: self
                .noises
                .iter()
                .position(|(n, _)| n == "minecraft:clay_bands_offset"),
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
    chunks: [Option<Proto>; 9],
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
        Some((
            slot,
            (ry as usize * 16 + (z & 15) as usize) * 16 + (x & 15) as usize,
        ))
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
            .and_then(|(s, i)| {
                self.chunks[s]
                    .as_ref()?
                    .states
                    .get(i)
                    .copied()
                    .map(u32::from)
            })
            .unwrap_or(self.air)
    }
    fn set(&mut self, x: i32, y: i32, z: i32, state: u32) {
        let Some((s, i)) = self.index(x, y, z) else {
            return;
        };
        let Some(c) = self.chunks[s].as_mut() else {
            return;
        };
        debug_assert!(state <= u32::from(u16::MAX), "block state id fits u16");
        c.states[i] = state as u16;
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
            // Один проход по столбцу сверху вниз на все четыре карты: у
            // каждого типа своя крайняя найденная высота, ниже искать уже
            // нечего. Раньше каждая карта сканировала столбец заново — в
            // четыре раза больше работы на чанк.
            for column in 0..256usize {
                let mut found = [0i32; 4];
                // Биты ещё не найденных карт по `Heightmap::index`.
                let mut left = (1u8 << Heightmap::ALL.len()) - 1;
                for ry in (0..self.noise.height).rev() {
                    let state = u32::from(c.states[ry as usize * 256 + column]);
                    for kind in Heightmap::ALL {
                        let bit = 1 << kind.index();
                        if left & bit != 0 && self.palette.hits(kind, state) {
                            found[kind.index()] = ry + 1;
                            left &= !bit;
                        }
                    }
                    if left == 0 {
                        break;
                    }
                }
                for (k, h) in maps.iter_mut().map(|m| &mut m[column]).enumerate() {
                    *h = found[k] + min_y;
                }
            }
            maps
        });
        maps[kind.index()][column]
    }
    fn sea_level(&self) -> i32 {
        self.noise.sea_level
    }
    fn mark_shape(&mut self, x: i32, y: i32, z: i32) {
        let Some(s) = self.slot(x, z) else {
            return;
        };
        if let Some(c) = self.chunks[s].as_mut() {
            c.shape_posts.push([x, y, z]);
        }
    }
    fn schedule_fluid(&mut self, x: i32, y: i32, z: i32) {
        let Some(s) = self.slot(x, z) else {
            return;
        };
        if let Some(c) = self.chunks[s].as_mut() {
            c.fluid_posts.push([x, y, z]);
        }
    }
    fn schedule_block(&mut self, x: i32, y: i32, z: i32) {
        let Some(s) = self.slot(x, z) else {
            return;
        };
        if let Some(c) = self.chunks[s].as_mut() {
            c.block_ticks.push([x, y, z]);
        }
    }
    fn set_block_entity(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        id: &str,
        nbt: chunkwake_world::nbt::Compound,
    ) {
        let Some(s) = self.slot(x, z) else {
            return;
        };
        if let Some(c) = self.chunks[s].as_mut() {
            c.block_entities.push(([x, y, z], id.to_owned(), nbt));
        }
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
fn column_height<S: Copy + Into<u32>>(
    states: &[S],
    column: usize,
    below: i32,
    kind: Heightmap,
    palette: &feature::Palette,
) -> i32 {
    (0..below.max(0) as usize)
        .rev()
        .find(|&ry| palette.hits(kind, states[ry * 256 + column].into()))
        .map_or(0, |ry| ry as i32 + 1)
}

/// Столбцы для постановки структур: небородая высота и биомы чанка.
pub(crate) struct Cols<'a> {
    pub(crate) noise: &'a Generator,
    pub(crate) carve: CarveBlocks,
    pub(crate) stone: u32,
    pub(crate) palette: &'a feature::Palette,
    pub(crate) heights: &'a BaseHeights,
    pub(crate) bases: &'a BaseColumns,
    pub(crate) base_limit: usize,
}

impl Columns for Cols<'_> {
    fn seed(&self) -> i64 {
        self.noise.seed()
    }
    fn min_y(&self) -> i32 {
        self.noise.min_y
    }
    fn max_y(&self) -> i32 {
        self.noise.min_y + self.noise.height
    }
    fn sea_level(&self) -> i32 {
        self.noise.sea_level
    }
    fn base_height(&self, x: i32, z: i32) -> i32 {
        self.column_heights(x, z).0
    }
    fn ocean_floor(&self, x: i32, z: i32) -> i32 {
        self.column_heights(x, z).1
    }
    fn biome_at(&self, x: i32, y: i32, z: i32) -> Option<u32> {
        let biomes = self.noise.biomes(x.div_euclid(16), z.div_euclid(16))?;
        let qy = y.div_euclid(4) - self.noise.min_y.div_euclid(4);
        if qy < 0 {
            return None;
        }
        let qx = x.rem_euclid(16) >> 2;
        let qz = z.rem_euclid(16) >> 2;
        biomes.get((qy * 16 + qz * 4 + qx) as usize).copied()
    }
    fn base_block(&self, x: i32, y: i32, z: i32) -> Option<Base> {
        let ry = y - self.noise.min_y;
        if ry < 0 || ry >= self.noise.height {
            return None;
        }
        Some(match self.base_fills(x, z).get(ry as usize)? {
            Fill::Air => Base::Air,
            Fill::Solid => Base::Solid,
            _ => Base::Fluid,
        })
    }
    fn column_has(&self, x: i32, z: i32, biomes: &HashSet<u32>) -> bool {
        let Some(cells) = self.noise.biomes(x.div_euclid(16), z.div_euclid(16)) else {
            return false;
        };
        let qx = (x.rem_euclid(16) >> 2) as usize;
        let qz = (z.rem_euclid(16) >> 2) as usize;
        let quarters = (self.noise.height / 4) as usize;
        (0..quarters).any(|qy| {
            cells
                .get(qy * 16 + qz * 4 + qx)
                .is_some_and(|id| biomes.contains(id))
        })
    }
}

impl Cols<'_> {
    /// Столбец `iterateNoiseColumn`: шум и акифер без поверхности, карверов и
    /// бороды. Раньше считался весь чанк вместе со стадией поверхности
    /// (~16 мс на одну точку старта); ваниль считает один столбец, и её
    /// поверхность в высоту не входит.
    fn base_fills(&self, x: i32, z: i32) -> Arc<Vec<Fill>> {
        if let Some(hit) = self.bases.lock().unwrap().get(&(x, z)) {
            return Arc::clone(hit);
        }
        let built = Arc::new(self.noise.column_fills(x, z));
        let mut cache = self.bases.lock().unwrap();
        if cache.len() >= self.base_limit {
            cache.clear();
        }
        cache.insert((x, z), Arc::clone(&built));
        built
    }

    /// `getBaseHeight` для `WORLD_SURFACE_WG` и `OCEAN_FLOOR_WG`.
    fn column_heights(&self, x: i32, z: i32) -> (i32, i32) {
        if let Some(&hit) = self.heights.lock().unwrap().get(&(x, z)) {
            return hit;
        }
        let fills = self.base_fills(x, z);
        let c = self.carve;
        let state = |f: &Fill| match f {
            Fill::Air => c.air,
            Fill::Solid => self.stone,
            // `default_fluid` настроек: в оверворлде вода, в Аду лава.
            Fill::Liquid => c.liquid,
            Fill::Water => c.water,
            Fill::Lava => c.lava,
        };
        let top = |kind: Heightmap| {
            fills
                .iter()
                .rposition(|f| self.palette.hits(kind, state(f)))
                .map_or(0, |y| y as i32 + 1)
                + self.noise.min_y
        };
        let hit = (top(Heightmap::WorldSurfaceWg), top(Heightmap::OceanFloorWg));
        let mut cache = self.heights.lock().unwrap();
        if cache.len() >= 1 << 16 {
            cache.clear();
        }
        cache.insert((x, z), hit);
        hit
    }
}

#[cfg(test)]
mod rle_tests {
    #[test]
    fn rle_roundtrip() {
        let mut v = vec![0u16; 70_000];
        v.extend([5, 5, 7, 7, 7, 1]);
        v.extend(vec![9u16; 30_000]);
        let rle = super::rle_encode(&v);
        assert!(rle.len() < 20);
        assert_eq!(super::rle_decode(&rle), v);
        assert!(super::rle_decode(&super::rle_encode(&[])).is_empty());
    }
}
