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
use std::sync::{Arc, Condvar, Mutex};

/// Плотности жил в порядке [`surface::Vein`].
const VEIN_DENSITIES: [&str; 4] = [
    "minecraft:overworld/ore_vein/copper_density",
    "minecraft:overworld/ore_vein/iron_density",
    "minecraft:overworld/ore_vein/richness",
    "minecraft:overworld/ore_vein/gap",
];

/// Блоки чанка по индексу `((y - min_y) * 16 + z) * 16 + x` и его биомы
/// по клеткам 4×4×4 (`None` — климата нет, мир из одного биома).
#[derive(Clone, Default)]
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
    protos: HashMap<(i32, i32), Blocks>,
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

impl Area {
    /// Выбрасывает прото-чанки сверх предела, но только чистые: ни одна
    /// декорация в них ещё не писала, и построенный заново будет тем же.
    /// Такие копятся за краем прорисовки (область 5×5 запроса шире радиуса)
    /// и у чанков, выпавших из очереди, когда игрок улетел.
    ///
    /// Обход всех ключей дорогой: на радиусе 24 `protos` держит больше
    /// двух тысяч столбцов, и вызов на каждый чанк превращался в проход по
    /// всему словарю под общим замком. Поэтому чистим, только когда запас
    /// вырос заметно, и сразу до половины предела.
    fn evict(&mut self) {
        if self.protos.len() <= AREA_SOFT_LIMIT + AREA_EVICT_SLACK {
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
        let excess = self.protos.len() - AREA_SOFT_LIMIT / 2;
        for p in clean.into_iter().take(excess) {
            self.protos.remove(&p);
        }
    }
}

/// Прото-чанков сверх этого — повод выбросить чистые (по ~393 КБ каждый).
const AREA_SOFT_LIMIT: usize = 2048;
/// Запас сверх предела, после которого чистка того стоит: обход словаря
/// на каждый чанк дороже, чем подержать лишнюю тысячу столбцов.
const AREA_EVICT_SLACK: usize = 1024;

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
            wake: Condvar::new(),
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
        area.evict();
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
        let around = |r: i32| (-r..=r).flat_map(move |dz| (-r..=r).map(move |dx| (cx + dx, cz + dz)));
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
                let b = self.proto(p.0, p.1);
                area = self.area.lock().unwrap();
                area.building.remove(&p);
                area.protos.entry(p).or_insert(b);
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
            for n in around(1) {
                // Своя область 3×3 должна быть на месте: её может держать
                // чужая декорация.
                loop {
                    if area.decorated.contains(&n) {
                        break;
                    }
                    let region = (-1..=1).flat_map(|dz| (-1..=1).map(move |dx| (n.0 + dx, n.1 + dz)));
                    if region.clone().any(|p| area.busy.contains(&p)) {
                        area = self.wake.wait(area).unwrap();
                        continue;
                    }
                    area.decorated.insert(n);
                    area.decorating.insert(n);
                    let at = |i: usize| (n.0 + i as i32 % 3 - 1, n.1 + i as i32 / 3 - 1);
                    let mut chunks: [Option<Blocks>; 9] = Default::default();
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
                    break;
                }
            }
            // Декорации соседей, начатые другими потоками, должны закончиться.
            while around(1).any(|p| area.decorating.contains(&p)) {
                area = self.wake.wait(area).unwrap();
            }
            // Тот же чанк мог отдать параллельный запрос, пока этот ждал.
            if area.done.insert((cx, cz)) {
                return Some(area.protos.remove(&(cx, cz)).expect("own proto chunk stays in the area"));
            }
        }
        // Повторный запрос отданного чанка: декорация уже ушла с ним, вызов
        // вернёт только основу (вызывающий держит готовый столбец и её отбросит).
        None
    }

    /// `applyBiomeDecoration` чанка `(cx, cz)` над его областью 3×3,
    /// забранной из общей области.
    fn decorate(&self, features: &Features, mut chunks: [Option<Blocks>; 9], (cx, cz): (i32, i32)) -> [Option<Blocks>; 9] {
        for c in chunks.iter_mut().flatten() {
            if c.worldgen_heights.is_none() {
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
        region.chunks
    }

    /// Прото-чанк: шум, поверхность, карверы.
    ///
    /// `CHUNKWAKE_PROF=1` печатает время стадий в stderr: без этого видно
    /// только «20 мс на чанк» и непонятно, что оптимизировать.
    fn proto(&self, cx: i32, cz: i32) -> Blocks {
        let prof = std::env::var_os("CHUNKWAKE_PROF").is_some();
        let t0 = std::time::Instant::now();
        let noise = &*self.noise;
        let mut terrain = noise.terrain(cx, cz);
        let t_noise = t0.elapsed().as_secs_f64() * 1000.0;
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
        if let Some(s) = &self.surface {
            let mut chunk = self.view(&mut states, cx, cz, own, &memo);
            s.stage(noise, &preliminary, &random).build(&mut chunk, cx, cz);
        }
        let t_surface = t.elapsed().as_secs_f64() * 1000.0;
        // Карверы после поверхности (`generateCarvers`): тоннели и каньоны,
        // залитые акифером того же чанка.
        let t = std::time::Instant::now();
        let mut carved = false;
        if let Some(mask) = noise.carving_mask(cx, cz) {
            carved = true;
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
        let t_carve = t.elapsed().as_secs_f64() * 1000.0;
        if prof {
            eprintln!(
                "proto {cx} {cz}: шум {t_noise:.2} | биомы {t_biomes:.2} | \
                 предв. поверхность {t_prelim:.2} | поверхность {t_surface:.2} | \
                 карверы {t_carve:.2}{} = {:.2} мс",
                if carved { "" } else { " (нет)" },
                t0.elapsed().as_secs_f64() * 1000.0
            );
        }
        Blocks { states, biomes, worldgen_heights: None }
    }

    /// Прото-чанк без декорации: для золотого теста генерации.
    ///
    /// Фичи зависят от порядка запросов, а рельеф и поверхность — нет,
    /// поэтому эталон ловит именно их.
    pub fn proto_for_test(&self, cx: i32, cz: i32) -> Blocks {
        self.proto(cx, cz)
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
            // Один проход по столбцу сверху вниз на все четыре карты: у
            // каждого типа своя крайняя найденная высота, ниже искать уже
            // нечего. Раньше каждая карта сканировала столбец заново — в
            // четыре раза больше работы на чанк.
            for column in 0..256usize {
                let mut found = [0i32; 4];
                let mut left: Vec<Heightmap> = Heightmap::ALL.to_vec();
                for ry in (0..self.noise.height).rev() {
                    let state = c.states[ry as usize * 256 + column];
                    let mut i = 0;
                    while i < left.len() {
                        if self.palette.hits(left[i], state) {
                            found[left[i].index()] = ry + 1;
                            left.swap_remove(i);
                        } else {
                            i += 1;
                        }
                    }
                    if left.is_empty() {
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
