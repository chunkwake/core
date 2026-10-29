//! Генератор рельефа оверворлда, перенесённый с ванили 26.3 бит в бит:
//! шумы, функции плотности из `worldgen/density_function`, акиферы.

pub mod aquifer;
pub mod biome;
pub mod carver;
pub mod density;
pub mod noise;
pub mod pipeline;
pub mod random;
pub mod surface;
pub mod volume;

use aquifer::{Aquifer, Fluid, Samplers};
use density::{Compiler, Ctx, Df, S};
use noise::NormalNoise;
use random::{Positional, Xoroshiro};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use volume::Volume;

/// Блок после заполнения шумом (`doFill`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fill {
    Air,
    /// `default_block` (камень).
    Solid,
    Water,
    Lava,
}

/// Рельеф одного мира: скомпилированные функции и настройки.
pub struct Generator {
    pub min_y: i32,
    pub height: i32,
    pub sea_level: i32,
    final_density: Arc<S>,
    aquifer: Samplers,
    aquifer_random: Positional,
    /// Шесть осей климата в порядке [`biome::AXES`].
    climate: [Arc<S>; 6],
    biomes: biome::BiomeSource,
    /// Предварительная поверхность (`chunk_surface_level`) для правил.
    preliminary: Arc<S>,
    /// Файлы правил и условий поверхности.
    rules: HashMap<String, Value>,
    conditions: HashMap<String, Value>,
    /// Плотности жил в порядке, который ожидает [`surface::Vein`].
    densities: Vec<Arc<S>>,
    /// Случайность стадии поверхности (`minecraft:surface`, `minecraft:deepslate`).
    random: Positional,
    /// Параметры шумов поверхности: из них стадия собирает движок шумов.
    surface_noise_params: HashMap<String, NormalNoise>,
    /// Мир шумов: нужен, чтобы собрать недостающие шумы поверхности.
    world: Positional,
    /// Сид мира: от него `setLargeFeatureSeed` карверов.
    seed: i64,
    /// Карверы `worldgen/carver` в порядке первой ссылки из биомов.
    carvers: Vec<carver::Carver>,
    /// Имя биома → номера его карверов в [`Self::carvers`] по порядку.
    biome_carvers: HashMap<String, Vec<usize>>,
    /// Карверы по чанку-источнику (`carverBiome`): одна точка климата
    /// на чанк, но её спрашивают 289 соседей.
    source_biomes: std::sync::Mutex<HashMap<ChunkPos, Option<Arc<[usize]>>>>,
    /// `BiomeManager.obfuscateSeed`: сид «дрожания» границ биомов.
    zoom_seed: i64,
    /// Биомы уже посчитанных чанков: «дрожание» границы заглядывает к соседу,
    /// а соседа всё равно генерировать — как биомы в чанках региона ванили.
    biome_cache: std::sync::Mutex<HashMap<ChunkPos, Arc<[u32]>>>,
}


/// Координаты чанка `(cx, cz)`.
type ChunkPos = (i32, i32);
/// Чанк после заполнения шумом: блоки плюс контекст и акифер, нужные
/// карверам (`noiseChunk.aquifer()` той же стадии).
pub struct Terrain<'a> {
    pub fills: Vec<Fill>,
    ctx: Ctx,
    aquifer: Aquifer<'a>,
    cx: i32,
    cz: i32,
}

/// Биомы соседних чанков на время одного чанка: «дрожание» границы
/// заглядывает только в окно 3×3.
#[derive(Default)]
pub struct BiomeMemo {
    near: [Option<Arc<[u32]>>; 9],
    /// По родительской клетке зума: биом, если все 8 кандидатов совпали,
    /// [`MIXED`] — если нет, [`UNKNOWN`] — ещё не смотрели.
    parents: Vec<u32>,
}

const UNKNOWN: u32 = u32::MAX;
const MIXED: u32 = u32::MAX - 1;

/// Состояния блоков, с которыми работает вырезание.
#[derive(Clone, Copy, Debug)]
pub struct CarveBlocks {
    pub air: u32,
    pub water: u32,
    pub lava: u32,
    /// `#minecraft:uncarvable`.
    pub bedrock: u32,
    pub grass: u32,
    pub mycelium: u32,
    pub dirt: u32,
}

/// Каталог `worldgen` одним файлом: сотня мелких открытий на холодном диске
/// стоит дороже, чем разбор уже прочитанного текста.
const CACHE_DIR: &str = "chunkwake-derived-v2";
const CACHE_FILE: &str = "worldgen.v1.bin";
const CACHE_MAGIC: u32 = 0x434B_5747; // "CKWG"
const CACHE_SCHEMA: u32 = 2;

fn cache_path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join(CACHE_DIR).join(CACHE_FILE)
}

/// `worldgen` целиком: `noise_settings/overworld`, параметры шумов и функции
/// плотности. Кэш только для этого мира, поэтому читается лишь для `normal`.
fn worldgen_text(data_dir: &Path) -> Result<Vec<(String, String)>, String> {
    let wg = data_dir.join("data/minecraft/worldgen");
    let mut out = vec![(
        "@noise_settings/overworld".to_owned(),
        std::fs::read_to_string(wg.join("noise_settings/overworld.json"))
            .map_err(|e| format!("noise_settings/overworld.json: {e}"))?,
    )];
    for (name, text) in read_tree(&wg.join("density_function"))? {
        out.push((format!("@density_function/{name}"), text));
    }
    for (name, text) in read_tree(&wg.join("noise"))? {
        out.push((format!("@noise/{name}"), text));
    }
    for (name, text) in read_tree(&wg.join("material_rule"))? {
        out.push((format!("@material_rule/{name}"), text));
    }
    for (name, text) in read_tree(&wg.join("material_condition"))? {
        out.push((format!("@material_condition/{name}"), text));
    }
    for (name, text) in read_tree(&wg.join("carver"))? {
        out.push((format!("@carver/{name}"), text));
    }
    // Из биома карверам нужен только список: весь файл в кэш не кладём.
    for (name, text) in read_tree(&wg.join("biome"))? {
        let v: Value = serde_json::from_str(&text).map_err(|e| format!("biome {name}: {e}"))?;
        let carvers = serde_json::json!({ "carvers": v["carvers"] });
        out.push((format!("@biome/{name}"), carvers.to_string()));
    }
    Ok(out)
}

/// Дерево JSON как тексты: `minecraft:<путь без .json>`.
fn read_tree(root: &Path) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for e in entries.flatten() {
            let path = e.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|x| x == "json") {
                let rel = path
                    .strip_prefix(root)
                    .unwrap()
                    .with_extension("")
                    .to_string_lossy()
                    .replace('\\', "/");
                let text = std::fs::read_to_string(&path)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                out.push((format!("minecraft:{rel}"), text));
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Кэш `worldgen`; `None` — его нет, он от другой версии или повреждён.
fn read_cache(path: &Path, world_version: i32) -> Option<Vec<(String, String)>> {
    let bytes = std::fs::read(path).ok()?;
    let mut at = 0usize;
    let mut word = || -> Option<u32> {
        let w = bytes.get(at..at + 4)?;
        at += 4;
        Some(u32::from_le_bytes(w.try_into().ok()?))
    };
    let string = |at: &mut usize, bytes: &[u8]| -> Option<String> {
        let n = u32::from_le_bytes(bytes.get(*at..*at + 4)?.try_into().ok()?) as usize;
        *at += 4;
        String::from_utf8(bytes.get(*at..*at + n)?.to_vec())
            .ok()
            .inspect(|_| *at += n)
    };
    if word()? != CACHE_MAGIC || word()? != CACHE_SCHEMA || word()? != world_version as u32 {
        return None;
    }
    let n = word()? as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push((string(&mut at, &bytes)?, string(&mut at, &bytes)?));
    }
    // Кэш старого формата собрали до правил поверхности: без них мир будет
    // голым камнем, поэтому такой файл считаем устаревшим.
    if !out.iter().any(|(name, _)| name.starts_with("@material_rule/")) {
        return None;
    }
    Some(out)
}

fn write_cache(path: &Path, rows: &[(String, String)], world_version: i32) {
    let mut bytes = Vec::new();
    for w in [
        CACHE_MAGIC,
        CACHE_SCHEMA,
        world_version as u32,
        rows.len() as u32,
    ] {
        bytes.extend_from_slice(&w.to_le_bytes());
    }
    for (name, text) in rows {
        for s in [name, text] {
            bytes.extend_from_slice(&(s.len() as u32).to_le_bytes());
            bytes.extend_from_slice(s.as_bytes());
        }
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, &bytes).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

impl Generator {
    /// `data_dir` — корень сгенерированных данных (с `data/minecraft/worldgen`).
    pub fn load(data_dir: &Path, seed: i64) -> Result<Self, String> {
        Self::load_with_version(data_dir, seed, None)
    }

    /// То же, но `world_version` позволяет переиспользовать кэш `worldgen`:
    /// без него дерево JSON читается заново на каждый запуск.
    pub fn load_with_version(
        data_dir: &Path,
        seed: i64,
        world_version: Option<i32>,
    ) -> Result<Self, String> {
        let rows = match world_version.and_then(|v| read_cache(&cache_path(data_dir), v)) {
            Some(rows) => rows,
            None => {
                let rows = worldgen_text(data_dir)?;
                if let Some(v) = world_version {
                    write_cache(&cache_path(data_dir), &rows, v);
                }
                rows
            }
        };
        let mut settings = Value::Null;
        let mut registry = HashMap::new();
        let mut noises = HashMap::new();
        let mut rules = HashMap::new();
        let mut conditions = HashMap::new();
        let mut carver_files = HashMap::new();
        let mut biome_files = HashMap::new();
        for (name, text) in rows {
            let value: Value =
                serde_json::from_str(&text).map_err(|e| format!("worldgen {name}: {e}"))?;
            if name == "@noise_settings/overworld" {
                settings = value;
            } else if let Some(id) = name.strip_prefix("@noise/") {
                noises.insert(id.to_owned(), NormalNoise::from_json(&value)?);
            } else if let Some(id) = name.strip_prefix("@density_function/") {
                registry.insert(id.to_owned(), value);
            } else if let Some(id) = name.strip_prefix("@material_rule/") {
                rules.insert(id.to_owned(), value);
            } else if let Some(id) = name.strip_prefix("@material_condition/") {
                conditions.insert(id.to_owned(), value);
            } else if let Some(id) = name.strip_prefix("@carver/") {
                carver_files.insert(id.to_owned(), value);
            } else if let Some(id) = name.strip_prefix("@biome/") {
                biome_files.insert(id.to_owned(), value);
            }
        }
        if settings.is_null() {
            return Err("worldgen: no noise_settings/overworld".into());
        }
        let mut g = Self::assemble(
            data_dir, seed, settings, registry, noises, rules, conditions,
        )?;
        g.bind_carvers(&carver_files, &biome_files)?;
        Ok(g)
    }

    /// Карверы и их списки по биомам. Карвер, на который ссылается хоть один
    /// биом, обязан разбираться: молча пропущенный дал бы мир без пещер.
    fn bind_carvers(
        &mut self,
        files: &HashMap<String, Value>,
        biomes: &HashMap<String, Value>,
    ) -> Result<(), String> {
        let mut index: HashMap<String, usize> = HashMap::new();
        let mut names: Vec<&String> = biomes.keys().collect();
        names.sort();
        for biome in names {
            let list = match &biomes[biome]["carvers"] {
                Value::String(one) => vec![one.clone()],
                Value::Array(a) => a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect(),
                _ => Vec::new(),
            };
            let mut ids = Vec::with_capacity(list.len());
            for name in list {
                let id = match index.get(&name) {
                    Some(&i) => i,
                    None => {
                        let file = files
                            .get(&name)
                            .ok_or_else(|| format!("biome {biome}: no carver {name}"))?;
                        let c = carver::Carver::parse(file, self.min_y, self.height)
                            .map_err(|e| format!("carver {name}: {e}"))?;
                        self.carvers.push(c);
                        index.insert(name, self.carvers.len() - 1);
                        self.carvers.len() - 1
                    }
                };
                ids.push(id);
            }
            self.biome_carvers.insert(biome.clone(), ids);
        }
        Ok(())
    }

    fn assemble(
        data_dir: &Path,
        seed: i64,
        settings: Value,
        registry: HashMap<String, Value>,
        noises: HashMap<String, NormalNoise>,
        rules: HashMap<String, Value>,
        conditions: HashMap<String, Value>,
    ) -> Result<Self, String> {
        let world = Xoroshiro::from_seed(seed).fork_positional();
        // Плотности жил берутся из того же каталога, что рельеф: копия нужна,
        // потому что компилятор забирает реестр себе.
        let density_values = registry.clone();
        let noise_params = noises.clone();
        let mut c = Compiler::new(registry, noises, world);
        let mut get = |v: &Value| Df::parse(v).and_then(|d| c.sampler(&d));
        let aq = &settings["aquifers"];
        let aquifer = Samplers {
            barrier: get(&aq["barrier"])?,
            floodedness: get(&aq["fluid_level_floodedness"])?,
            spread: get(&aq["fluid_level_spread"])?,
            lava: get(&aq["lava"])?,
            exclusion: get(&aq["exclusion"])?,
            surface: get(&aq["surface_level"])?,
        };
        let final_density = get(&settings["noise_router"]["final_density"])?;
        // Оси климата из роутера: `humidity` там называется `vegetation`,
        // `continentalness` — `continents`, `weirdness` — `ridges`.
        let mut climate = Vec::with_capacity(6);
        for axis in biome::AXES {
            let key = match axis {
                "humidity" => "vegetation",
                "continentalness" => "continents",
                "weirdness" => "ridges",
                other => other,
            };
            climate.push(get(&settings["noise_router"][key])?);
        }
        let climate: [Arc<S>; 6] = climate.try_into().expect("six climate axes");
        let biomes = read_biomes(data_dir)?;
        // Предварительная поверхность и стадия поверхности.
        let preliminary = get(&settings["noise_router"]["chunk_surface_level"])?;
        // Плотности жил: нужны поштучно на каждую клетку, а не объёмом.
        let vein_densities = [
            "minecraft:overworld/ore_vein/copper_density",
            "minecraft:overworld/ore_vein/iron_density",
            "minecraft:overworld/ore_vein/richness",
            "minecraft:overworld/ore_vein/gap",
        ];
        let mut densities = Vec::with_capacity(vein_densities.len());
        for name in vein_densities {
            let value = density_values
                .get(name)
                .cloned()
                .ok_or_else(|| format!("no density function {name}"))?;
            densities.push(get(&value)?);
        }
        let int = |v: &Value| v.as_i64().map(|x| x as i32);
        let min_y = int(&settings["noise"]["min_y"]).ok_or("noise_settings: no min_y")?;
        let height = int(&settings["noise"]["height"]).ok_or("noise_settings: no height")?;
        // Шумы поверхности: параметры уже прочитаны из `worldgen/noise`.
        let mut surface_noise_params = HashMap::new();
        for (name, params) in &noise_params {
            surface_noise_params.insert(name.clone(), params.clone());
        }
        Ok(Self {
            min_y,
            height,
            sea_level: int(&settings["sea_level"]).ok_or("noise_settings: no sea_level")?,
            final_density,
            aquifer,
            aquifer_random: world.from_hash_of("minecraft:aquifer").fork_positional(),
            climate,
            biomes,
            preliminary,
            rules,
            conditions,
            densities,
            // `MaterialSystem.noiseRandom` — сама позиционная фабрика мира
            // (`RandomState.random`), без отдельной ветки по имени.
            random: world,
            // Параметры шумов: движок собирается при первом запросе стадии.
            surface_noise_params,
            world,
            seed,
            carvers: Vec::new(),
            biome_carvers: HashMap::new(),
            source_biomes: std::sync::Mutex::new(HashMap::new()),
            zoom_seed: biome::zoom_seed(seed),
            biome_cache: std::sync::Mutex::new(HashMap::new()),
        })
    }

    /// Заполнить чанк шумом (`NoiseBasedChunkGenerator.doFill`). Индекс —
    /// `((y - min_y) * 16 + z) * 16 + x`.
    pub fn fill(&self, cx: i32, cz: i32) -> Vec<Fill> {
        self.terrain(cx, cz).fills
    }

    /// То же, что [`Self::fill`], но с акифером для стадии карверов.
    pub fn terrain(&self, cx: i32, cz: i32) -> Terrain<'_> {
        let v = Volume::blocks([16, self.height, 16], [cx * 16, self.min_y, cz * 16]);
        let mut ctx = Ctx::default();
        let mut aquifer = Aquifer::new(
            &self.aquifer,
            self.aquifer_random,
            self.sea_level,
            &mut ctx,
            &v,
        );
        let mut density = vec![0f32; v.len()];
        self.final_density.volume(&mut ctx, &mut density, &v);
        let mut out = vec![Fill::Air; v.len()];
        for z in 0..16 {
            for x in 0..16 {
                for y in (0..self.height).rev() {
                    let d = density[v.index(x, y, z)];
                    let (bx, by, bz) = (v.block_x(x), v.block_y(y), v.block_z(z));
                    let fill = match aquifer.substance(&mut ctx, bx, by, bz, d as f64) {
                        None => Fill::Solid,
                        Some(Fluid::Air) => Fill::Air,
                        Some(Fluid::Water) => Fill::Water,
                        Some(Fluid::Lava) => Fill::Lava,
                    };
                    out[((y * 16 + z) * 16 + x) as usize] = fill;
                }
            }
        }
        Terrain {
            fills: out,
            ctx,
            aquifer,
            cx,
            cz,
        }
    }

    /// Карверы чанка-источника по биому в квартовой клетке его угла на y=0
    /// (`getBiomeGenerationSettingsForCarver`).
    fn source_carvers(&self, sx: i32, sz: i32) -> Option<Arc<[usize]>> {
        if let Some(hit) = self.source_biomes.lock().unwrap().get(&(sx, sz)) {
            return hit.clone();
        }
        let p = self.climate_at(sx * 4, 0, sz * 4);
        let mut q = [0i64; 6];
        for (q, v) in q.iter_mut().zip(&p) {
            *q = biome::quantize(*v);
        }
        let found = self
            .biomes
            .name(&q)
            .and_then(|n| self.biome_carvers.get(n))
            .filter(|l| !l.is_empty())
            .map(|l| Arc::from(l.as_slice()));
        let mut cache = self.source_biomes.lock().unwrap();
        // Кэш ограничен: игрок, улетевший далеко, не должен копить его вечно.
        if cache.len() > 1 << 16 {
            cache.clear();
        }
        cache.insert((sx, sz), found.clone());
        found
    }

    /// Маска вырезов чанка (`generateCarvers`): карверы всех чанков в
    /// радиусе 8. Верхние 7 блоков мира защищены, как у не обновляемого чанка.
    pub fn carving_mask(&self, cx: i32, cz: i32) -> Option<carver::Mask> {
        if self.carvers.is_empty() || self.biomes.is_empty() {
            return None;
        }
        let max_y = self.min_y + self.height - 1 - 7;
        let mut mask = carver::Mask::new(self.min_y + 1, max_y);
        let mut r = random::Legacy::new(0);
        for dx in -8..=8 {
            for dz in -8..=8 {
                let (sx, sz) = (cx + dx, cz + dz);
                let Some(list) = self.source_carvers(sx, sz) else {
                    continue;
                };
                for (index, &id) in list.iter().enumerate() {
                    let c = &self.carvers[id];
                    r.set_large_feature_seed(self.seed.wrapping_add(index as i64), sx, sz);
                    if c.is_start(&mut r) {
                        c.carve(&mut r, cx, cz, sx, sz, self.min_y, self.height, &mut mask);
                    }
                }
            }
        }
        (!mask.is_empty()).then_some(mask)
    }

    /// `applyCarvingMask`: вырез по маске с заливкой акифером. Индекс
    /// `states` — как у [`Self::fill`]. `top(состояния, x, y, z, под_жидкостью)` —
    /// `topMaterial` поверхности для земли под вырезанной травой.
    pub fn carve(
        &self,
        terrain: &mut Terrain,
        mask: &carver::Mask,
        b: &CarveBlocks,
        states: &mut [u32],
        mut top: impl FnMut(&mut [u32], usize, i32, usize, bool) -> Option<u32>,
    ) {
        let (bx0, bz0) = (terrain.cx * 16, terrain.cz * 16);
        let min_y = self.min_y;
        let at = |x: usize, y: i32, z: usize| ((y - min_y) as usize * 16 + z) * 16 + x;
        mask.visit(|x, z, bottom, top_y| {
            let mut has_grass = false;
            for wy in (bottom..=top_y).rev() {
                let i = at(x, wy, z);
                let old = states[i];
                if old == b.bedrock {
                    continue;
                }
                if old == b.grass || old == b.mycelium {
                    has_grass = true;
                }
                let Some(fluid) = terrain.aquifer.substance(
                    &mut terrain.ctx,
                    bx0 + x as i32,
                    wy,
                    bz0 + z as i32,
                    0.0,
                ) else {
                    continue;
                };
                let state = match fluid {
                    Fluid::Air => b.air,
                    Fluid::Water => b.water,
                    Fluid::Lava => b.lava,
                };
                states[i] = state;
                if has_grass {
                    let below = at(x, wy - 1, z);
                    if states[below] == b.dirt
                        && let Some(t) = top(states, x, wy - 1, z, fluid != Fluid::Air)
                    {
                        states[below] = t;
                    }
                }
            }
        });
    }

    /// `chunk_surface_level` по квартовой сетке 4×4: по одному значению на
    /// клетку столбца, как интерполяция ванили.
    pub fn preliminary(&self, cx: i32, cz: i32) -> Vec<f32> {
        // Как `MaterialRuleContext.preliminarySurfaceVolume`: объём 16×1×16
        // с шагом 1 в y=0, значение на каждый блок столбца.
        let v = Volume::new([16, 1, 16], [cx * 16, 0, cz * 16], [1, 1, 1]);
        let mut ctx = Ctx::default();
        let mut buf = vec![0f32; v.len()];
        self.preliminary.volume(&mut ctx, &mut buf, &v);
        buf
    }

    /// Файлы правил поверхности и условий: их компилирует сервер, у которого
    /// есть реестр блоков (имя состояния → ID).
    pub fn surface_files(&self) -> (&HashMap<String, Value>, &HashMap<String, Value>) {
        (&self.rules, &self.conditions)
    }

    /// Плотности жил в порядке [`surface::Vein`].
    pub fn vein_densities(&self) -> &[Arc<S>] {
        &self.densities
    }

    /// Случайность стадии поверхности: столбцы и имена (`deepslate`).
    pub fn surface_random(&self) -> Positional {
        self.random
    }

    /// Есть ли правила поверхности в загруженных данных.
    pub fn has_surface(&self) -> bool {
        !self.rules.is_empty()
    }

    /// Шумы правил поверхности в порядке, который ожидает [`surface::Stage`].
    /// Список берётся из самих правил: ванила ссылается и на `surface_swamp`,
    /// и на шумы айсбергов, которых в фиксированном перечне не было.
    pub fn surface_noises(&self) -> Vec<(String, Arc<noise::Stack>)> {
        let mut names: Vec<String> = Vec::new();
        for rule in self.rules.values().chain(self.conditions.values()) {
            collect_noises(rule, &mut names);
        }
        names.sort();
        names.dedup();
        // `surface` и `surface_secondary` нужны всегда: стадия берёт их по
        // индексам 0 и 1 — для глубины поверхности и вторичного шума.
        for first in ["minecraft:surface", "minecraft:surface_secondary"] {
            names.retain(|n| n != first);
        }
        names.insert(0, "minecraft:surface_secondary".to_owned());
        names.insert(0, "minecraft:surface".to_owned());
        names
            .into_iter()
            .filter_map(|n| {
                let params = self.surface_noise_params.get(&n)?;
                Some((
                    n.clone(),
                    Arc::new(noise::instantiate(params, &self.world, &n)),
                ))
            })
            .collect()
    }

    /// Привязка имён биомов из `biomes.csv` к сетевым ID этого сервера.
    pub fn bind_biomes(&mut self, ids: HashMap<String, u32>) {
        self.biomes.bind(ids);
    }
    pub fn has_biomes(&self) -> bool {
        !self.biomes.is_empty()
    }

    /// Биомы чанка по клеткам 4×4×4 (`ChunkGenerator.createBiomes`): по 64 на
    /// секцию, индекс внутри секции `x + 4*z + 16*y`, как у палитры биомов.
    /// `None` — климатические данные не выгружены, мир остаётся одним биомом.
    pub fn biomes(&self, cx: i32, cz: i32) -> Option<Vec<u32>> {
        self.cached_biomes(cx, cz).map(|b| b.to_vec())
    }

    fn cached_biomes(&self, cx: i32, cz: i32) -> Option<Arc<[u32]>> {
        if let Some(hit) = self.biome_cache.lock().unwrap().get(&(cx, cz)) {
            return Some(hit.clone());
        }
        let ids: Arc<[u32]> = self.compute_biomes(cx, cz)?.into();
        let mut cache = self.biome_cache.lock().unwrap();
        // Около 6 КиБ на чанк: держим не больше пары тысяч.
        if cache.len() >= 2048 {
            cache.clear();
        }
        cache.insert((cx, cz), ids.clone());
        Some(ids)
    }

    fn compute_biomes(&self, cx: i32, cz: i32) -> Option<Vec<u32>> {
        let points = self.climate(cx, cz)?;
        let mut ids = Vec::with_capacity(points.len());
        for p in &points {
            let mut q = [0i64; 6];
            for (q, v) in q.iter_mut().zip(p) {
                *q = biome::quantize(*v);
            }
            ids.push(self.biomes.biome(&q).unwrap_or(u32::MAX));
        }
        Some(ids)
    }

    /// Сырой климат чанка по квартовой решётке (шаг 4 блока), в порядке клеток
    /// секции. `None` — климатических данных нет. Для сверки с ванилью.
    pub fn climate(&self, cx: i32, cz: i32) -> Option<Vec<[f32; 6]>> {
        if self.biomes.is_empty() {
            return None;
        }
        let quarters = (self.height / 4) as i32;
        // Шаг 4 блока — квартовая решётка ванили (`QuartPos.toBlock`).
        let v = Volume::new([4, quarters, 4], [cx * 16, self.min_y, cz * 16], [4, 4, 4]);
        let mut ctx = Ctx::default();
        let mut axes = Vec::with_capacity(6);
        for s in &self.climate {
            let mut buf = vec![0f32; v.len()];
            s.volume(&mut ctx, &mut buf, &v);
            axes.push(buf);
        }
        let mut points = vec![[0f32; 6]; v.len()];
        for qy in 0..quarters as usize {
            for qz in 0..4usize {
                for qx in 0..4usize {
                    let i = v.index(qx as i32, qy as i32, qz as i32);
                    // Индекс клетки секции: `x + 4*z + 16*y`.
                    let at = qy * 16 + qz * 4 + qx;
                    for (axis, buf) in axes.iter().enumerate() {
                        points[at][axis] = buf[i];
                    }
                }
            }
        }
        Some(points)
    }

    /// Климат в одной квартовой клетке без кэшей чанка
    /// (`BiomeSource.createUncachedResolver`): так ванила берёт биом карвера.
    pub fn climate_at(&self, qx: i32, qy: i32, qz: i32) -> [f32; 6] {
        let mut ctx = Ctx::default();
        let mut p = [0f32; 6];
        for (p, s) in p.iter_mut().zip(&self.climate) {
            *p = s.value(&mut ctx, qx * 4, qy * 4, qz * 4);
        }
        p
    }

    /// Биом блока, как его видит стадия поверхности (`BiomeManager.getBiome`):
    /// клетка выбирается с «дрожанием» и может лежать в соседнем чанке.
    /// `own` — биомы этого чанка по [`Self::biomes`], `memo` — кэш соседних
    /// клеток на время одного чанка.
    pub fn zoomed_biome(
        &self,
        cx: i32,
        cz: i32,
        own: &[u32],
        x: i32,
        y: i32,
        z: i32,
        memo: &mut BiomeMemo,
    ) -> u32 {
        // `ChunkAccess.getNoiseBiome` прижимает y к высоте мира.
        let min_q = self.min_y >> 2;
        let max_q = min_q + (self.height >> 2) - 1;
        let mut cell = |qx: i32, qy: i32, qz: i32| {
            let qy = qy.clamp(min_q, max_q);
            let i = ((qy - min_q) * 16 + (qz & 3) * 4 + (qx & 3)) as usize;
            if qx >> 2 == cx && qz >> 2 == cz {
                return own.get(i).copied().unwrap_or(0);
            }
            let (dx, dz) = ((qx >> 2) - cx, (qz >> 2) - cz);
            let slot = &mut memo.near[((dz + 1) * 3 + dx + 1) as usize];
            if slot.is_none() {
                *slot = self.cached_biomes(qx >> 2, qz >> 2);
            }
            slot.as_ref().and_then(|b| b.get(i).copied()).unwrap_or(0)
        };
        // Все восемь кандидатов одного биома — «дрожание» ничего не решает.
        // Родитель общий у блоков 4×4×4, поэтому ответ кэшируется по нему.
        let (px, py, pz) = ((x - 2) >> 2, (y - 2) >> 2, (z - 2) >> 2);
        let span = (self.height >> 2) + 2;
        let slot = {
            let (rx, ry, rz) = (px - cx * 4 + 1, py - min_q + 1, pz - cz * 4 + 1);
            (0..5).contains(&rx) && (0..span).contains(&ry) && (0..5).contains(&rz)
        }
        .then(|| (((pz - cz * 4 + 1) * 5 + px - cx * 4 + 1) * span + py - min_q + 1) as usize);
        let known = match slot {
            Some(i) => {
                if memo.parents.is_empty() {
                    memo.parents = vec![UNKNOWN; (25 * span) as usize];
                }
                memo.parents[i]
            }
            None => UNKNOWN,
        };
        let verdict = if known != UNKNOWN {
            known
        } else {
            let first = cell(px, py, pz);
            let uniform = (1..8).all(|i| {
                cell(px + (i >> 2 & 1), py + (i >> 1 & 1), pz + (i & 1)) == first
            });
            let v = if uniform { first } else { MIXED };
            if let Some(i) = slot {
                memo.parents[i] = v;
            }
            v
        };
        if verdict != MIXED {
            return verdict;
        }
        let (qx, qy, qz) = biome::zoom(self.zoom_seed, x, y, z);
        cell(qx, qy, qz)
    }

    /// Биом квартовой клетки по климату; `None` — климата нет.
    pub fn biome_at(&self, qx: i32, qy: i32, qz: i32) -> Option<u32> {
        if self.biomes.is_empty() {
            return None;
        }
        let p = self.climate_at(qx, qy, qz);
        let mut q = [0i64; 6];
        for (q, v) in q.iter_mut().zip(&p) {
            *q = biome::quantize(*v);
        }
        self.biomes.biome(&q)
    }

    /// Имя биома для сырой точки климата — для тестов и сверки.
    pub fn biome_name(&self, point: &[f32; 6]) -> Option<&str> {
        let mut q = [0i64; 6];
        for (q, v) in q.iter_mut().zip(point) {
            *q = biome::quantize(*v);
        }
        self.biomes.name(&q)
    }

    /// Одна ось климата чанка — для замера, какая из шести дороже.
    pub fn axis_ms(&self, cx: i32, cz: i32, axis: &str) -> f64 {
        let Some(i) = biome::AXES.iter().position(|a| *a == axis) else {
            return 0.0;
        };
        let quarters = (self.height / 4) as i32;
        let v = Volume::new([4, quarters, 4], [cx * 16, self.min_y, cz * 16], [4, 4, 4]);
        let mut ctx = Ctx::default();
        let mut buf = vec![0f32; v.len()];
        let t = std::time::Instant::now();
        self.climate[i].volume(&mut ctx, &mut buf, &v);
        t.elapsed().as_secs_f64() * 1000.0
    }
}

/// Все имена шумов, на которые ссылаются правила (`"noise": "minecraft:x"`),
/// плюс шумы внутри `noise_threshold` в условиях.
fn collect_noises(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => {
            // Ссылку на шум видно по ключу `noise`, поэтому строки не берём.
            let _ = s;
        }
        Value::Object(map) => {
            if let Some(name) = map.get("noise").and_then(|n| n.as_str())
                && name.starts_with("minecraft:")
            {
                out.push(name.to_owned());
            }
            if let Some(name) = map.get("random_name").and_then(|n| n.as_str()) {
                let _ = name;
            }
            for child in map.values() {
                collect_noises(child, out);
            }
        }
        Value::Array(list) => {
            for child in list {
                collect_noises(child, out);
            }
        }
        _ => {}
    }
}

/// `reports/biomes.csv` из выгрузки `ChunkwakeBiomes`. Нет файла — пустой
/// источник: старый кэш данных продолжает работать на одном биоме.
fn read_biomes(data_dir: &Path) -> Result<biome::BiomeSource, String> {
    let path = data_dir.join("reports/biomes.csv");
    match std::fs::read_to_string(&path) {
        Ok(text) => biome::BiomeSource::parse(&text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(biome::BiomeSource::default()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("chunkwake-worldgen-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("worldgen.v1.bin")
    }

    fn rows(with_rules: bool) -> Vec<(String, String)> {
        let mut rows = vec![
            ("@noise_settings/overworld".to_owned(), "{}".to_owned()),
            ("@density_function/x".to_owned(), "{}".to_owned()),
        ];
        if with_rules {
            rows.push(("@material_rule/overworld".to_owned(), "{}".to_owned()));
        }
        rows
    }

    #[test]
    fn cache_without_surface_rules_is_stale() {
        // Старый кэш собирали до правил поверхности: без них мир был бы
        // голым камнем, поэтому такой файл читать нельзя.
        let path = temp("norules");
        write_cache(&path, &rows(false), 5023);
        assert!(read_cache(&path, 5023).is_none());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn cache_with_surface_rules_is_used() {
        let path = temp("withrules");
        write_cache(&path, &rows(true), 5023);
        let rows = read_cache(&path, 5023).expect("кэш с правилами годен");
        assert_eq!(rows.len(), 3);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn cache_of_another_schema_is_ignored() {
        let path = temp("oldschema");
        write_cache(&path, &rows(true), 5023);
        // Подменяем номер схемы на прежний, как в файле от прошлой сборки.
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[4..8].copy_from_slice(&1u32.to_le_bytes());
        std::fs::write(&path, &bytes).unwrap();
        assert!(read_cache(&path, 5023).is_none());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn cache_of_another_data_version_is_ignored() {
        let path = temp("oldversion");
        write_cache(&path, &rows(true), 5023);
        assert!(read_cache(&path, 4900).is_none());
        std::fs::remove_file(&path).ok();
    }
}
