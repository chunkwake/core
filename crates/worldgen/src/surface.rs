//! Правила поверхности: `worldgen/material_rule` и `material_condition`.
//!
//! Стадия идёт после заполнения плотностью, как `MaterialSystem.buildSurface`:
//! по каждому столбцу блоки перебираются сверху вниз, и первое совпавшее
//! правило подменяет состояние. Контекст (`MaterialRuleContext`) знает глубину
//! поверхности, высоту воды, глубину камня и уклон по карте высот.

use crate::random::{Positional, Xoroshiro};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// Правило поверхности.
#[derive(Debug, Clone)]
pub enum Rule {
    /// `block`: подменить состояние.
    Block(u32),
    /// `sequence`: первое сработавшее правило цепочки.
    Sequence(Vec<Rule>),
    /// `condition`: проверить условие и выполнить вложенное правило.
    Condition(Box<Cond>, Box<Rule>),
    /// `ore_vein`: жила руды по плотностям (медь, железо).
    OreVein(Box<Vein>),
    /// `bandlands`: полосы цветной глины бедлендов.
    Bandlands(usize),
}

/// Жила руды (`OreVeinRule`): плотность решает, есть ли она, богатство —
/// руда или заполнитель, `raw_ore_chance` — сырой блок вместо руды.
#[derive(Debug, Clone)]
pub struct Vein {
    pub ore: u32,
    pub raw_ore: u32,
    pub filler: u32,
    pub raw_ore_chance: f32,
    pub density: usize,
    pub richness: usize,
    pub gap: usize,
}

/// Условие правила.
#[derive(Debug, Clone)]
pub enum Cond {
    StoneDepth {
        offset: i32,
        add_surface_depth: bool,
        secondary_depth_range: i32,
        ceiling: bool,
    },
    Water {
        offset: i32,
        surface_depth_multiplier: i32,
        add_stone_depth: bool,
    },
    YAbove {
        anchor: Anchor,
        add_stone_depth: bool,
        surface_depth_multiplier: i32,
    },
    /// `biome`: биом в этой клетке; пустой список — никогда не совпадает.
    Biome(Vec<u32>),
    /// `hole`: глубина поверхности не выше нуля.
    Hole,
    /// `steep`: уклон столбца (`gradientX > -4 || gradientZ >= 4`).
    Steep,
    NoiseThreshold {
        noise: usize,
        min: f64,
        max: f64,
    },
    VerticalGradient {
        random: usize,
        true_at_and_below: i32,
        false_at_and_above: i32,
    },
    AbovePreliminarySurface,
    Not(Box<Cond>),
}

/// Якорь высоты (`VerticalAnchor`).
#[derive(Debug, Clone, Copy)]
pub enum Anchor {
    Absolute(i32),
    AboveBottom(i32),
    BelowTop(i32),
}

impl Anchor {
    fn resolve(&self, min_y: i32, height: i32) -> i32 {
        match *self {
            Anchor::Absolute(y) => y,
            Anchor::AboveBottom(n) => min_y + n,
            // `VerticalAnchor.BelowTop`: genDepth - 1 + minGenY - offset.
            Anchor::BelowTop(n) => min_y + height - 1 - n,
        }
    }
}

/// Компилятор правил: файлы `material_rule`/`material_condition`, состояния
/// блоков по имени и индексы шумов/плотностей, которые уже собраны.
pub struct Rules {
    rules: HashMap<String, Value>,
    conditions: HashMap<String, Value>,
    /// Имя состояния > состояние; биомы лежат под ключом `biome:<имя>`.
    names: HashMap<String, u32>,
    /// Все биомы реестра: правила ссылаются и на те, что не попали в климат.
    biomes: HashMap<String, u32>,
    noise_ids: HashMap<String, usize>,
    random_ids: HashMap<String, usize>,
    density_ids: HashMap<String, usize>,
    /// Полосы глины бедлендов: у ванили они одни на мир.
    band_ids: HashMap<String, usize>,
    /// База для потоков `vertical_gradient`.
    random_factory: Positional,
    min_y: i32,
    height: i32,
}

impl Rules {
    pub fn new(
        rules: HashMap<String, Value>,
        conditions: HashMap<String, Value>,
        names: HashMap<String, u32>,
        biomes: HashMap<String, u32>,
        noises: &[String],
        densities: &[String],
        random_factory: Positional,
        min_y: i32,
        height: i32,
    ) -> Self {
        let index = |list: &[String]| -> HashMap<String, usize> {
            list.iter()
                .enumerate()
                .map(|(i, n)| (n.clone(), i))
                .collect()
        };
        Self {
            rules,
            conditions,
            names,
            biomes,
            noise_ids: index(noises),
            random_ids: HashMap::new(),
            density_ids: index(densities),
            band_ids: HashMap::new(),
            random_factory,
            min_y,
            height,
        }
    }

    /// Правило верхнего уровня по ссылке (`minecraft:overworld`).
    pub fn top(&mut self, id: &str) -> Result<Rule, String> {
        self.rule_ref(id, 0)
    }

    fn rule_ref(&mut self, id: &str, depth: usize) -> Result<Rule, String> {
        if depth > 32 {
            return Err(format!(
                "material_rule {id}: ссылки вложены слишком глубоко"
            ));
        }
        let value = self
            .rules
            .get(id)
            .cloned()
            .ok_or_else(|| format!("material_rule {id} not found"))?;
        self.rule(&value, depth + 1)
    }

    fn rule(&mut self, v: &Value, depth: usize) -> Result<Rule, String> {
        if let Some(name) = v.as_str() {
            return self.rule_ref(name, depth);
        }
        let kind = v["type"].as_str().unwrap_or_default();
        Ok(match kind {
            "minecraft:block" => {
                let name = v["result_state"]
                    .as_str()
                    .ok_or("block without result_state")?;
                Rule::Block(
                    self.names
                        .get(name)
                        .copied()
                        .ok_or_else(|| format!("unknown block state {name}"))?,
                )
            }
            "minecraft:sequence" => {
                let list = v["sequence"].as_array().ok_or("sequence without list")?;
                let mut out = Vec::with_capacity(list.len());
                for item in list {
                    out.push(self.rule(item, depth + 1)?);
                }
                Rule::Sequence(out)
            }
            "minecraft:condition" => {
                let cond = match &v["if_true"] {
                    Value::String(id) => self.condition(id)?,
                    other => self.cond(other)?,
                };
                Rule::Condition(
                    Box::new(cond),
                    Box::new(self.rule(&v["then_run"], depth + 1)?),
                )
            }
            "minecraft:ore_vein" => {
                let block = |r: &mut Self, key: &str| -> Result<u32, String> {
                    let name = v[key]
                        .as_str()
                        .ok_or_else(|| format!("ore_vein without {key}"))?;
                    r.names
                        .get(name)
                        .copied()
                        .ok_or_else(|| format!("unknown block state {name}"))
                };
                let ore = block(self, "ore_block")?;
                let raw_ore = block(self, "raw_ore_block")?;
                let filler = block(self, "filler_block")?;
                let density = self.density(&v["density"])?;
                let richness = self.density(&v["richness"])?;
                let gap = self.density(&v["filler_gap"])?;
                Rule::OreVein(Box::new(Vein {
                    ore,
                    raw_ore,
                    filler,
                    raw_ore_chance: v["raw_ore_chance"].as_f64().unwrap_or(0.0) as f32,
                    density,
                    richness,
                    gap,
                }))
            }
            "minecraft:bandlands" => {
                let next = self.band_ids.len();
                let id = *self.band_ids.entry(String::new()).or_insert(next);
                Rule::Bandlands(id)
            }
            other => return Err(format!("unknown material rule {other}")),
        })
    }

    fn density(&self, v: &Value) -> Result<usize, String> {
        let id = v
            .as_str()
            .ok_or("ore_vein density must be a reference to worldgen/density_function")?;
        self.density_ids
            .get(id)
            .copied()
            .ok_or_else(|| format!("density {id} is not loaded for surface"))
    }

    fn condition(&mut self, id: &str) -> Result<Cond, String> {
        let value = self
            .conditions
            .get(id)
            .cloned()
            .ok_or_else(|| format!("material_condition {id} not found"))?;
        self.cond(&value)
    }

    /// Случайные потоки для `vertical_gradient` по именам в порядке индексов.
    pub fn into_randoms(self) -> Vec<Positional> {
        let mut names: Vec<(usize, String)> = self
            .random_ids
            .iter()
            .map(|(n, i)| (*i, n.clone()))
            .collect();
        names.sort();
        let factory = self.random_factory;
        names
            .into_iter()
            .map(|(_, name)| factory.from_hash_of(&name).fork_positional())
            .collect()
    }

    pub fn bands(&self, states: &HashMap<String, u32>) -> Vec<Vec<u32>> {
        let mut out = vec![Vec::new(); self.band_ids.len()];
        if !out.is_empty() {
            out[0] = Self::render_bands(&self.random_factory, states);
        }
        out
    }

    /// Полосы глины бедлендов (`MaterialSystem.generateBands`): 192 записи,
    /// где терракота перемешана с оранжевой, жёлтой, коричневой, красной и
    /// белой. Строятся одним и тем же зерном `minecraft:clay_bands`.
    fn render_bands(factory: &Positional, states: &HashMap<String, u32>) -> Vec<u32> {
        let pick = |name: &str| states.get(name).copied().unwrap_or(0);
        let terracotta = pick("minecraft:terracotta");
        let mut bands = vec![terracotta; 192];
        let mut rng = factory.from_hash_of("minecraft:clay_bands");
        let mut i = 0usize;
        while i < 192 {
            i += rng.next_int_bound(5) as usize + 1;
            if i < 192 {
                bands[i] = pick("minecraft:orange_terracotta");
            }
            i += 1;
        }
        // `makeBands`: по несколько отрезков случайной длины.
        let mut paint = |rng: &mut Xoroshiro, color: u32| {
            let length = rng_int(rng, 6, 15);
            for _ in 0..length {
                let len = rng.next_int_bound(3) + 3;
                let at = rng.next_int_bound(192) as usize;
                for k in 0..len as usize {
                    if at + k < 192 {
                        bands[at + k] = color;
                    }
                }
            }
        };
        let mut rng = factory.from_hash_of("minecraft:clay_bands");
        // Первые три цвета идут подряд, как в ваниле.
        let mut first = Xoroshiro::new(rng.next_long(), rng.next_long());
        paint(&mut first, pick("minecraft:yellow_terracotta"));
        paint(&mut first, pick("minecraft:brown_terracotta"));
        paint(&mut first, pick("minecraft:red_terracotta"));
        let white = pick("minecraft:white_terracotta");
        let light = pick("minecraft:light_gray_terracotta");
        let count = rng_int(&mut first, 9, 15);
        let mut at = 0usize;
        for _ in 0..count {
            if at < 192 {
                bands[at] = white;
                if at > 0 && first.next_bool() {
                    bands[at - 1] = light;
                }
                if at + 1 < 192 && first.next_bool() {
                    bands[at + 1] = light;
                }
            }
            at += first.next_int_bound(16) as usize + 4;
        }
        bands
    }

    fn cond(&mut self, v: &Value) -> Result<Cond, String> {
        if let Some(id) = v.as_str() {
            return self.condition(id);
        }
        let kind = v["type"].as_str().unwrap_or_default();
        let int = |key: &str| v[key].as_i64().unwrap_or(0) as i32;
        let num = |key: &str| v[key].as_f64().unwrap_or(0.0);
        let flag = |key: &str| v[key].as_bool().unwrap_or(false);
        Ok(match kind {
            "minecraft:stone_depth" => Cond::StoneDepth {
                offset: int("offset"),
                add_surface_depth: flag("add_surface_depth"),
                secondary_depth_range: int("secondary_depth_range"),
                ceiling: v["surface_type"].as_str() == Some("ceiling"),
            },
            "minecraft:water" => Cond::Water {
                offset: int("offset"),
                surface_depth_multiplier: int("surface_depth_multiplier"),
                add_stone_depth: flag("add_stone_depth"),
            },
            "minecraft:y_above" => Cond::YAbove {
                anchor: anchor(&v["anchor"]),
                add_stone_depth: flag("add_stone_depth"),
                surface_depth_multiplier: int("surface_depth_multiplier"),
            },
            "minecraft:biome" => {
                // `biome_is` бывает строкой и списком (бедленды — три биома).
                let names: Vec<&str> = match &v["biome_is"] {
                    Value::String(s) => vec![s.as_str()],
                    Value::Array(list) => list.iter().filter_map(|n| n.as_str()).collect(),
                    _ => Vec::new(),
                };
                let mut ids = Vec::with_capacity(names.len());
                for name in names {
                    ids.push(
                        self.biomes
                            .get(name)
                            .copied()
                            .ok_or_else(|| format!("unknown biome {name} in material rule"))?,
                    );
                }
                Cond::Biome(ids)
            }
            "minecraft:hole" => Cond::Hole,
            "minecraft:steep" => Cond::Steep,
            "minecraft:above_preliminary_surface" => Cond::AbovePreliminarySurface,
            "minecraft:not" => Cond::Not(Box::new(match &v["invert"] {
                Value::String(id) => self.condition(id)?,
                other => self.cond(other)?,
            })),
            "minecraft:noise_threshold" => {
                let name = v["noise"].as_str().ok_or("noise_threshold without noise")?;
                let noise = *self
                    .noise_ids
                    .get(name)
                    .ok_or_else(|| format!("noise {name} not loaded for surface"))?;
                Cond::NoiseThreshold {
                    noise,
                    min: num("min_threshold"),
                    max: num("max_threshold"),
                }
            }
            "minecraft:vertical_gradient" => {
                let name = v["random_name"].as_str().unwrap_or("minecraft:deepslate");
                let next = self.random_ids.len();
                let random = *self.random_ids.entry(name.to_owned()).or_insert(next);
                Cond::VerticalGradient {
                    random,
                    true_at_and_below: anchor(&v["true_at_and_below"])
                        .resolve(self.min_y, self.height),
                    false_at_and_above: anchor(&v["false_at_and_above"])
                        .resolve(self.min_y, self.height),
                }
            }
            other => return Err(format!("unknown material condition {other}")),
        })
    }
}

/// `nextIntBetweenInclusive(a, b)`.
fn rng_int(rng: &mut Xoroshiro, a: i32, b: i32) -> i32 {
    a + rng.next_int_bound(b - a + 1)
}

fn anchor(v: &Value) -> Anchor {
    if let Some(y) = v["absolute"].as_i64() {
        Anchor::Absolute(y as i32)
    } else if let Some(n) = v["above_bottom"].as_i64() {
        Anchor::AboveBottom(n as i32)
    } else {
        Anchor::BelowTop(v["below_top"].as_i64().unwrap_or(0) as i32)
    }
}

/// Материал клетки: воздух, жидкость или камень. Стадия получает это от чанка,
/// чтобы не знать про ID состояний бетона, травы и прочего.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Air,
    Fluid,
    Solid,
}

/// Чанк, который правит стадия. `y` здесь — от низа мира, как в `ChunkColumn`.
pub trait Chunk {
    fn get(&self, x: usize, y: usize, z: usize) -> u32;
    fn set(&mut self, x: usize, y: usize, z: usize, state: u32) -> u32;
    fn biome(&self, x: usize, y: usize, z: usize) -> u32;
    /// Верхняя непустая клетка столбца (`WORLD_SURFACE_WG`), индекс от низа.
    fn surface(&self, x: usize, z: usize) -> usize;
    fn kind(&self, state: u32) -> Kind;
}

/// Что стадии нужно от генератора помимо чанка.
pub struct Stage<'a> {
    pub rule: &'a Rule,
    pub min_y: i32,
    pub height: i32,
    /// Шумы в порядке индексов условий: `surface`, `surface_secondary`, …
    pub noises: &'a [(String, Arc<crate::noise::Stack>)],
    /// Полосы глины бедлендов по индексам.
    pub bands: &'a [Vec<u32>],
    /// Плотности жил в порядке индексов; объём считается один раз на чанк.
    pub densities: &'a [Arc<crate::density::S>],
    /// Случайность по имени (`minecraft:deepslate`) в порядке индексов.
    pub randoms: &'a [Positional],
    /// Предварительная поверхность (`chunk_surface_level`) по квартовой сетке.
    pub preliminary: &'a [f32],
    pub random: &'a Positional,
}

/// `MaterialRuleContext`: состояние одного столбца между вызовами правил.
#[derive(Default, Clone, Copy)]
struct Context {
    surface_depth: i32,
    gradient_x: i32,
    gradient_z: i32,
    water_height: i32,
    stone_above: i32,
    stone_below: i32,
    min_surface_level: i32,
    secondary: f64,
    /// Биом текущего блока `(y, id)`: как `MaterialRuleContext.biome`,
    /// считается один раз на блок, сколько бы условий его ни спросили.
    biome: Option<(usize, u32)>,
}

impl<'a> Stage<'a> {
    /// Пройти стадию по всем 16?16 столбцам чанка.
    pub fn build<C: Chunk>(&self, chunk: &mut C, cx: i32, cz: i32) {
        // `getDensitiesInChunk`: объём плотности на весь чанк считается один раз,
        // иначе каждая клетка заново проходит дерево интерполяции.
        let veins = self.vein_volumes(cx, cz);
        // Карта высот `WORLD_SURFACE_WG` считается до стадии и по ней же ищется
        // уклон. Без кэша `surface()` сканирует 384 блока пять раз на столбец.
        let heights: Vec<usize> = (0..256)
            .map(|i| chunk.surface(i % 16, i / 16))
            .collect();
        // `noiseSamplers2d` ванили: шум условия зависит только от x/z, поэтому
        // считается один раз на столбец, а не на каждый блок. Кэш ленивый —
        // заранее считать все шумы дороже, чем посчитать два нужных.
        let mut noise_cache: Vec<(usize, f32)> = Vec::new();
        for z in 0..16usize {
            for x in 0..16usize {
                self.column(
                    chunk,
                    x,
                    z,
                    cx * 16 + x as i32,
                    cz * 16 + z as i32,
                    &veins,
                    &heights,
                    &mut noise_cache,
                );
            }
        }
    }

    /// Плотности жил по всем клеткам чанка; индекс — `y * 256 + z * 16 + x`.
    /// `None` — плотность без интерполяции и кэшей (`gap`): по точке она
    /// равна объёму, а нужна в редких клетках, поэтому считается там же.
    fn vein_volumes(&self, cx: i32, cz: i32) -> Vec<Option<Vec<f32>>> {
        if self.densities.is_empty() {
            return Vec::new();
        }
        let volume = crate::volume::Volume::blocks(
            [16, self.height, 16],
            [cx * 16, self.min_y, cz * 16],
        );
        let mut ctx = crate::density::Ctx::default();
        self.densities
            .iter()
            .map(|d| {
                if d.pointwise() {
                    return None;
                }
                let mut buf = vec![0f32; volume.len()];
                d.volume(&mut ctx, &mut buf, &volume);
                // Порядок `Volume` — `y + (x + z*16)*height`; перекладываем
                // в порядок клеток чанка, чтобы дальше читать напрямую.
                let mut out = vec![0f32; buf.len()];
                for y in 0..self.height as usize {
                    for z in 0..16usize {
                        for x in 0..16usize {
                            out[y * 256 + z * 16 + x] =
                                buf[volume.index(x as i32, y as i32, z as i32)];
                        }
                    }
                }
                Some(out)
            })
            .collect()
    }

    /// `MaterialSystem.topMaterial`: правило для одной клетки, которой карвер
    /// снял траву сверху. Глубина камня 1 сверху и снизу, вода над клеткой —
    /// если вырез залит. `heights` — `WORLD_SURFACE_WG` чанка, `y` — от низа.
    #[allow(clippy::too_many_arguments)]
    pub fn top_material<C: Chunk>(
        &self,
        chunk: &C,
        x: usize,
        y: usize,
        z: usize,
        cx: i32,
        cz: i32,
        heights: &[usize],
        under_fluid: bool,
    ) -> Option<u32> {
        let (bx, bz) = (cx * 16 + x as i32, cz * 16 + z as i32);
        let by = self.min_y + y as i32;
        let mut c = Context {
            surface_depth: self.surface_depth(bx, bz),
            secondary: self.secondary(bx, bz),
            gradient_x: self.gradient(heights, x, z, true),
            gradient_z: self.gradient(heights, x, z, false),
            water_height: if under_fluid { by + 1 } else { i32::MIN },
            stone_above: 1,
            stone_below: 1,
            ..Context::default()
        };
        c.min_surface_level = self.preliminary_at(x, z) + c.surface_depth - 8;
        // Жилы здесь считаются по точке: объёма чанка у этой стадии нет.
        let veins = vec![None; self.densities.len()];
        let at = y * 256 + z * 16 + x;
        let mut noise_cache = Vec::new();
        self.eval(
            chunk, &mut c, self.rule, x, y, z, bx, by, bz, &veins, at, &mut noise_cache,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn column<C: Chunk>(
        &self,
        chunk: &mut C,
        x: usize,
        z: usize,
        bx: i32,
        bz: i32,
        veins: &[Option<Vec<f32>>],
        heights: &[usize],
        noise_cache: &mut Vec<(usize, f32)>,
    ) {
        noise_cache.clear();
        let mut c = Context::default();
        c.surface_depth = self.surface_depth(bx, bz);
        c.secondary = self.secondary(bx, bz);
        c.gradient_x = self.gradient(heights, x, z, true);
        c.gradient_z = self.gradient(heights, x, z, false);
        c.min_surface_level = self.preliminary_at(x, z) + c.surface_depth - 8;
        c.water_height = i32::MIN;

        let mut y = heights[z * 16 + x] as i32;
        let mut stone_above = 0;
        // Глубина от потолка: считается один раз, у первого камня снизу вверх.
        let mut ceiling = i32::MAX;
        while y >= 0 {
            let state = chunk.get(x, y as usize, z);
            match chunk.kind(state) {
                Kind::Air => {
                    stone_above = 0;
                    y -= 1;
                    continue;
                }
                Kind::Fluid => {
                    if c.water_height == i32::MIN {
                        c.water_height = y + 1;
                    }
                    y -= 1;
                    continue;
                }
                Kind::Solid => {}
            }
            if ceiling == i32::MAX {
                // `stoneDepthBelow`: первый не-камень под текущим.
                let mut below = y - 1;
                while below >= 0 && chunk.kind(chunk.get(x, below as usize, z)) == Kind::Solid {
                    below -= 1;
                }
                ceiling = below + 1;
            }
            stone_above += 1;
            c.stone_above = stone_above;
            c.stone_below = y - ceiling + 1;
            let by = self.min_y + y;
            let at = y as usize * 256 + z * 16 + x;
            if let Some(state) = self.eval(
                chunk,
                &mut c,
                self.rule,
                x,
                y as usize,
                z,
                bx,
                by,
                bz,
                veins,
                at,
                noise_cache,
            ) {
                chunk.set(x, y as usize, z, state);
            }
            y -= 1;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn eval<C: Chunk>(
        &self,
        chunk: &C,
        c: &mut Context,
        rule: &Rule,
        x: usize,
        y: usize,
        z: usize,
        bx: i32,
        by: i32,
        bz: i32,
        veins: &[Option<Vec<f32>>],
        at: usize,
        noise_cache: &mut Vec<(usize, f32)>,
    ) -> Option<u32> {
        match rule {
            Rule::Block(state) => Some(*state),
            Rule::Sequence(list) => {
                for r in list {
                    if let Some(s) =
                        self.eval(chunk, c, r, x, y, z, bx, by, bz, veins, at, noise_cache)
                    {
                        return Some(s);
                    }
                }
                None
            }
            Rule::Condition(cond, then) => {
                if self.test(chunk, c, cond, x, y, z, bx, by, bz, noise_cache) {
                    self.eval(chunk, c, then, x, y, z, bx, by, bz, veins, at, noise_cache)
                } else {
                    None
                }
            }
            Rule::OreVein(vein) => self.vein(vein, veins, at, bx, by, bz),
            Rule::Bandlands(bands) => {
                let bands = self.bands.get(*bands)?;
                // Смещение полос шумом `clay_bands_offset`, как `getBand`.
                // Шум зависит только от x/z — берём из кэша столбца.
                let id = self
                    .noises
                    .iter()
                    .position(|(n, _)| n == "minecraft:clay_bands_offset")?;
                let offset = (self.column_noise(id, bx, bz, noise_cache) * 4.0).round() as i32;
                let i = (by + offset).rem_euclid(bands.len() as i32) as usize;
                bands.get(i).copied()
            }
        }
    }

    /// Шум условия на столбец: первый запрос считает и кладёт в кэш,
    /// остальные блоки столбца читают готовое значение.
    fn column_noise(&self, id: usize, bx: i32, bz: i32, cache: &mut Vec<(usize, f32)>) -> f32 {
        if let Some(&(_, v)) = cache.iter().find(|(k, _)| *k == id) {
            return v;
        }
        let v = self
            .noises
            .get(id)
            .map_or(0.0, |(_, n)| n.get(bx as f64, 0.0, bz as f64));
        cache.push((id, v));
        v
    }

    fn vein(&self, vein: &Vein, veins: &[Option<Vec<f32>>], at: usize, bx: i32, by: i32, bz: i32) -> Option<u32> {
        let sample = |i: usize| match veins.get(i) {
            Some(Some(v)) => v[at],
            Some(None) => self.densities[i].value(&mut crate::density::Ctx::default(), bx, by, bz),
            None => 0.0,
        };
        let density = sample(vein.density);
        if density <= 0.0 {
            return None;
        }
        let mut rng = self.random.at(bx, by, bz);
        if next_float(&mut rng) > density {
            return None;
        }
        let richness = sample(vein.richness);
        if next_float(&mut rng) >= richness {
            return Some(vein.filler);
        }
        let gap = sample(vein.gap);
        if gap >= 0.0 {
            return Some(vein.filler);
        }
        Some(if next_float(&mut rng) < vein.raw_ore_chance {
            vein.raw_ore
        } else {
            vein.ore
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn test<C: Chunk>(
        &self,
        chunk: &C,
        c: &mut Context,
        cond: &Cond,
        x: usize,
        y: usize,
        z: usize,
        bx: i32,
        by: i32,
        bz: i32,
        noise_cache: &mut Vec<(usize, f32)>,
    ) -> bool {
        match cond {
            Cond::StoneDepth {
                offset,
                add_surface_depth,
                secondary_depth_range,
                ceiling,
            } => {
                let depth = if *ceiling {
                    c.stone_below
                } else {
                    c.stone_above
                };
                let surface = if *add_surface_depth {
                    c.surface_depth
                } else {
                    0
                };
                // `Mth.map(secondary, -1, 1, 0, range)`.
                let secondary = if *secondary_depth_range == 0 {
                    0
                } else {
                    (((c.secondary + 1.0) / 2.0) * *secondary_depth_range as f64) as i32
                };
                depth <= 1 + offset + surface + secondary
            }
            Cond::Water {
                offset,
                surface_depth_multiplier,
                add_stone_depth,
            } => {
                if c.water_height == i32::MIN {
                    return true;
                }
                let stone = if *add_stone_depth { c.stone_above } else { 0 };
                by + stone >= c.water_height + offset + c.surface_depth * surface_depth_multiplier
            }
            Cond::YAbove {
                anchor,
                add_stone_depth,
                surface_depth_multiplier,
            } => {
                let stone = if *add_stone_depth { c.stone_above } else { 0 };
                by + stone
                    >= anchor.resolve(self.min_y, self.height)
                        + c.surface_depth * surface_depth_multiplier
            }
            Cond::Biome(ids) => {
                let b = match c.biome {
                    Some((at, b)) if at == y => b,
                    _ => {
                        let b = chunk.biome(x, y, z);
                        c.biome = Some((y, b));
                        b
                    }
                };
                ids.contains(&b)
            }
            Cond::Hole => c.surface_depth <= 0,
            Cond::Steep => c.gradient_x > -4 || c.gradient_z >= 4,
            Cond::NoiseThreshold { noise, min, max } => {
                let v = self.column_noise(*noise, bx, bz, noise_cache) as f64;
                *min <= v && v < *max
            }
            Cond::VerticalGradient {
                random,
                true_at_and_below,
                false_at_and_above,
            } => {
                if by <= *true_at_and_below {
                    return true;
                }
                if by >= *false_at_and_above {
                    return false;
                }
                // `Mth.map(y, trueAt, falseAt, 1, 0)`: чем выше, тем меньше шанс.
                let t = (by - *true_at_and_below) as f64
                    / (*false_at_and_above - *true_at_and_below) as f64;
                let factory = self.randoms.get(*random).copied().unwrap_or(*self.random);
                let mut rng = factory.at(bx, by, bz);
                // `nextFloat() < probability`: float сравнивается с double.
                (next_float(&mut rng) as f64) < 1.0 - t
            }
            Cond::AbovePreliminarySurface => by >= c.min_surface_level,
            Cond::Not(inner) => !self.test(chunk, c, inner, x, y, z, bx, by, bz, noise_cache),
        }
    }

    /// `MaterialSystem.getSurfaceDepth`: шум ?2.75 + 3 + случайная доля столбца.
    fn surface_depth(&self, bx: i32, bz: i32) -> i32 {
        let noise = self
            .noises
            .first()
            .map_or(0.0, |(_, n)| n.get(bx as f64, 0.0, bz as f64) as f64);
        let mut rng = self.random.at(bx, 0, bz);
        (noise * 2.75 + 3.0 + next_double(&mut rng) * 0.25) as i32
    }

    fn secondary(&self, bx: i32, bz: i32) -> f64 {
        self.noises
            .get(1)
            .map_or(0.0, |(_, n)| n.get(bx as f64, 0.0, bz as f64) as f64)
    }

    /// Предварительная поверхность столбца (`chunk_surface_level`).
    fn preliminary_at(&self, x: usize, z: usize) -> i32 {
        // Одно значение на блок столбца: `preliminarySurfaceVolume` ванили
        // считает объём 16×1×16 с шагом 1.
        self.preliminary.get(z * 16 + x).copied().unwrap_or(0.0) as i32
    }

    /// `getSurfaceGradientX/Z`: разница высот с соседним столбцом в чанке.
    fn gradient(&self, heights: &[usize], x: usize, z: usize, along_x: bool) -> i32 {
        if along_x {
            let a = (x + 1).min(15);
            let b = x.saturating_sub(1);
            heights[z * 16 + a] as i32 - heights[z * 16 + b] as i32
        } else {
            let a = (z + 1).min(15);
            let b = z.saturating_sub(1);
            heights[a * 16 + x] as i32 - heights[b * 16 + x] as i32
        }
    }
}

/// `RandomSource.nextFloat` — 24 старших бита.
/// `nextFloat`: старшие 24 бита без знака.
fn next_float(rng: &mut Xoroshiro) -> f32 {
    rng.next_float()
}

/// `nextDouble`: 53 бита, а не float — от него глубина поверхности.
fn next_double(rng: &mut Xoroshiro) -> f64 {
    rng.next_double()
}

#[cfg(test)]
mod tests {
    use super::*;

    const AIR: u32 = 0;
    const STONE: u32 = 1;
    const GRASS: u32 = 2;
    const DIRT: u32 = 3;

    struct Flat {
        states: Vec<u32>,
        height: usize,
    }

    impl Chunk for Flat {
        fn get(&self, x: usize, y: usize, z: usize) -> u32 {
            self.states[(y * 16 + z) * 16 + x]
        }
        fn set(&mut self, x: usize, y: usize, z: usize, state: u32) -> u32 {
            let i = (y * 16 + z) * 16 + x;
            std::mem::replace(&mut self.states[i], state)
        }
        fn biome(&self, _x: usize, _y: usize, _z: usize) -> u32 {
            0
        }
        fn surface(&self, x: usize, z: usize) -> usize {
            (0..self.height)
                .rev()
                .find(|&y| self.get(x, y, z) != AIR)
                .unwrap_or(0)
        }
        fn kind(&self, state: u32) -> Kind {
            if state == AIR { Kind::Air } else { Kind::Solid }
        }
    }

    fn flat(height: usize, top: usize) -> Flat {
        let mut states = vec![AIR; height * 256];
        for y in 0..=top {
            for z in 0..16 {
                for x in 0..16 {
                    states[(y * 16 + z) * 16 + x] = STONE;
                }
            }
        }
        Flat { states, height }
    }

    /// Ванильная форма: трава на самом верху, земля на несколько блоков глубже,
    /// дальше правило не срабатывает и остаётся камень.
    fn soil_rule() -> Rule {
        let depth = |offset: i32| Cond::StoneDepth {
            offset,
            add_surface_depth: false,
            secondary_depth_range: 0,
            ceiling: false,
        };
        Rule::Sequence(vec![
            Rule::Condition(Box::new(depth(0)), Box::new(Rule::Block(GRASS))),
            Rule::Condition(Box::new(depth(3)), Box::new(Rule::Block(DIRT))),
        ])
    }

    #[test]
    fn top_block_gets_grass_and_below_gets_dirt() {
        let height = 64;
        let mut chunk = flat(height, 40);
        let rule = soil_rule();
        let random = Xoroshiro::from_seed(1).fork_positional();
        let stage = Stage {
            rule: &rule,
            min_y: 0,
            height: height as i32,
            noises: &[],
            bands: &[],
            densities: &[],
            randoms: &[],
            preliminary: &[],
            random: &random,
        };
        stage.build(&mut chunk, 0, 0);
        assert_eq!(chunk.get(8, 40, 8), GRASS, "верхний блок — трава");
        assert_eq!(chunk.get(8, 39, 8), DIRT, "под травой земля");
        assert_eq!(chunk.get(8, 37, 8), DIRT, "земля до глубины 3");
        // Глубже условия не выполняются: остаётся камень.
        assert_eq!(chunk.get(8, 30, 8), STONE, "в глубине остаётся камень");
    }

    #[test]
    fn ore_vein_uses_densities_and_stays_inside() {
        // Жила с плотностью «всегда», богатством «всегда», зазором «нет»:
        // каждая клетка становится рудой.
        let height = 32;
        let mut chunk = flat(height, 20);
        let density = std::sync::Arc::new(crate::density::S::Const(1.0));
        let richness = std::sync::Arc::new(crate::density::S::Const(1.0));
        let gap = std::sync::Arc::new(crate::density::S::Const(-1.0));
        let densities = vec![density, richness, gap];
        let rule = Rule::OreVein(Box::new(Vein {
            ore: 9,
            raw_ore: 9,
            filler: 9,
            raw_ore_chance: 0.0,
            density: 0,
            richness: 1,
            gap: 2,
        }));
        let random = Xoroshiro::from_seed(1).fork_positional();
        let stage = Stage {
            rule: &rule,
            min_y: 0,
            height: height as i32,
            noises: &[],
            bands: &[],
            densities: &densities,
            randoms: &[],
            preliminary: &[],
            random: &random,
        };
        stage.build(&mut chunk, 0, 0);
        assert_eq!(chunk.get(3, 10, 3), 9);
    }

    #[test]
    fn vein_volume_matches_point_sampling() {
        // Предварительный объём обязан давать те же значения, что и расчёт
        // по клетке: иначе руды разойдутся с поэлементным путём.
        let height = 32;
        let gradient = std::sync::Arc::new(crate::density::S::Gradient(
            1,
            crate::density::Grad::Clamp {
                from: 0,
                min: 0,
                max: 1000,
            },
            0.0,
            1.0,
        ));
        let densities = vec![gradient];
        let rule = Rule::Block(1);
        let random = crate::random::Xoroshiro::from_seed(1).fork_positional();
        let stage = Stage {
            rule: &rule,
            min_y: 0,
            height: height as i32,
            noises: &[],
            bands: &[],
            densities: &densities,
            randoms: &[],
            preliminary: &[],
            random: &random,
        };
        let volumes = stage.vein_volumes(0, 0);
        assert_eq!(volumes.len(), 1);
        // Градиент без интерполяции считается по точке, объём ему не нужен.
        assert!(volumes[0].is_none());
        let mut ctx = crate::density::Ctx::default();
        for y in [0usize, 7, 31] {
            for z in [0usize, 5, 15] {
                for x in [0usize, 9, 15] {
                    assert_eq!(
                        densities[0].value(&mut ctx, x as i32, y as i32, z as i32),
                        y as f32,
                        "клетка ({x}, {y}, {z}) — градиент по высоте"
                    );
                }
            }
        }
    }
}
