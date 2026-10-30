//! chunkwake-world — чанки и их сетевое кодирование (формат 26.3).

pub mod anvil;
pub mod light;
pub mod nbt;

use chunkwake_protocol::Writer;
use light::LightLayer;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, OnceLock};

/// Блок-сущность (сундук, бочка…): `id` и NBT как в Anvil (без `id`/`x`/`y`/`z`).
#[derive(Clone, Debug, PartialEq)]
pub struct BlockEntity {
    pub id: String,
    pub nbt: nbt::Compound,
}

/// Номера `block_entity_type` по имени (ставятся при старте): нужны пакету чанка.
pub static BLOCK_ENTITY_TYPES: OnceLock<HashMap<String, i32>> = OnceLock::new();

/// Сколько бит нужно, чтобы записать значения `0..n`.
pub fn bits_for(n: usize) -> u32 {
    usize::BITS - n.saturating_sub(1).leading_zeros()
}

/// Упаковка как в `SimpleBitStorage`: значения не пересекают границы long,
/// младшие биты — первое значение.
pub fn pack(values: &[u32], bits: u32) -> Vec<u64> {
    let per_long = (64 / bits) as usize;
    values
        .chunks(per_long)
        .map(|chunk| {
            chunk
                .iter()
                .enumerate()
                .fold(0u64, |acc, (i, &v)| acc | (v as u64) << (i as u32 * bits))
        })
        .collect()
}

/// Каждый long `SimpleBitStorage` для `n` значений `at(0..n)`.
/// Расклад тот же, что у [`pack`]: частичный последний long дополнен нулями.
pub(crate) fn emit_packed(
    n: usize,
    bits: u32,
    mut at: impl FnMut(usize) -> u32,
    mut emit: impl FnMut(u64),
) {
    let per_long = (64 / bits) as usize;
    let mut i = 0;
    while i < n {
        let mut acc = 0u64;
        let mut slot = 0;
        while slot < per_long && i < n {
            acc |= u64::from(at(i)) << (slot as u32 * bits);
            slot += 1;
            i += 1;
        }
        emit(acc);
    }
}

/// Состояния в порядке первого появления, не во внутреннем порядке слотов.
/// `limit` — порог сети: как только записей стало больше, хвост секции не смотрится
/// (прямая палитра). `None` — обойти все 4096, как Anvil.
pub(crate) fn first_seen_palette(section: &Section, limit: Option<usize>) -> Vec<u32> {
    let mut palette = Vec::new();
    for i in 0..4096 {
        let v = section.get(i);
        if palette.contains(&v) {
            continue;
        }
        palette.push(v);
        if limit.is_some_and(|n| palette.len() > n) {
            break;
        }
    }
    palette
}

pub(crate) fn palette_index(palette: &[u32], state: u32) -> u32 {
    palette.iter().position(|&p| p == state).unwrap() as u32
}

/// Одно значение: бит на запись = 0, палитра из одной записи, без long-массива.
fn write_single(w: &mut Writer, state: u32) {
    w.u8(0).var_int(state as i32);
}

/// Биомы секции в сети: контейнер на 64 записи, минимум 1 бит на запись
/// (`Strategy.createForBiomes`). Порядок клеток — `x + 4*z + 16*y`.
fn write_biome_container(w: &mut Writer, biomes: &Biomes, global_bits: u32) {
    if let Some(one) = biomes.uniform() {
        write_single(w, one);
        return;
    }
    let cells = biomes.cells();
    let palette: Vec<u32> = {
        let mut p = Vec::new();
        for &c in &cells {
            if !p.contains(&c) {
                p.push(c);
            }
        }
        p
    };
    if palette.len() == 1 {
        write_single(w, palette[0]);
        return;
    }
    let bits = bits_for(palette.len()).max(1);
    if palette.len() <= 256 && bits <= 8 {
        w.u8(bits as u8).var_int(palette.len() as i32);
        for &b in &palette {
            w.var_int(b as i32);
        }
        emit_packed(
            64,
            bits,
            |i| palette_index(&palette, cells[i]),
            |long| w.buf.extend_from_slice(&long.to_be_bytes()),
        );
    } else {
        w.u8(global_bits as u8);
        emit_packed(
            64,
            global_bits,
            |i| cells[i],
            |long| w.buf.extend_from_slice(&long.to_be_bytes()),
        );
    }
}

/// Блоки секции в формате paletted container. Байты те же, что у массива из 4096 состояний:
/// однородная секция пишется сразу, смешанная — двумя проходами по индексам.
fn write_block_container(w: &mut Writer, section: &Section, min_bits: u32, global_bits: u32) {
    if section.is_uniform() {
        write_single(w, section.get(0));
        return;
    }
    let palette = first_seen_palette(section, Some(256));
    if palette.len() == 1 {
        write_single(w, palette[0]);
        return;
    }
    let indirect_bits = bits_for(palette.len()).max(min_bits);
    if palette.len() <= 256 && indirect_bits <= 8 {
        w.u8(indirect_bits as u8).var_int(palette.len() as i32);
        for &state in &palette {
            w.var_int(state as i32);
        }
        let bits = indirect_bits;
        emit_packed(
            4096,
            bits,
            |i| palette_index(&palette, section.get(i)),
            |long| w.buf.extend_from_slice(&long.to_be_bytes()),
        );
    } else {
        w.u8(global_bits as u8);
        emit_packed(
            4096,
            global_bits,
            |i| section.get(i),
            |long| w.buf.extend_from_slice(&long.to_be_bytes()),
        );
    }
}

/// Индексы палитры: байт, пока палитра ≤ 256 записей, иначе u16.
#[derive(Clone, Debug)]
enum Indices {
    Narrow(Box<[u8; 4096]>),
    Wide(Box<[u16; 4096]>),
}

impl Indices {
    fn get(&self, i: usize) -> usize {
        match self {
            Indices::Narrow(a) => a[i] as usize,
            Indices::Wide(a) => a[i] as usize,
        }
    }
}

/// Неоднородная секция: палитра, число блоков каждой записи и индексы.
#[derive(Clone, Debug)]
struct Paletted {
    palette: Vec<u32>,
    counts: Vec<u16>,
    idx: Indices,
}

impl Paletted {
    fn entry(&mut self, state: u32) -> usize {
        if let Some(i) = self.palette.iter().position(|&p| p == state) {
            return i;
        }
        // Переиспользуем запись, которой больше нет в секции.
        if let Some(i) = self.counts.iter().position(|&c| c == 0) {
            self.palette[i] = state;
            return i;
        }
        self.palette.push(state);
        self.counts.push(0);
        if self.palette.len() > 256
            && let Indices::Narrow(a) = &self.idx
        {
            let mut wide = Box::new([0u16; 4096]);
            for (w, &n) in wide.iter_mut().zip(a.iter()) {
                *w = n as u16;
            }
            self.idx = Indices::Wide(wide);
        }
        self.palette.len() - 1
    }
}

#[derive(Clone, Debug)]
enum Storage {
    Single(u32),
    Paletted(Arc<Paletted>),
}

/// Биомы секции: одно значение на все 64 клетки 4×4×4 или палитра.
/// Порядок клеток — как у `PalettedContainer` для биомов: `x + 4*z + 16*y`.
#[derive(Clone, Debug, PartialEq)]
pub enum Biomes {
    Single(u32),
    Paletted(Arc<BiomePalette>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct BiomePalette {
    pub palette: Vec<u32>,
    /// Индекс палитры на клетку, 64 записи.
    pub idx: [u8; 64],
}

impl Biomes {
    pub fn get(&self, i: usize) -> u32 {
        match self {
            Biomes::Single(b) => *b,
            Biomes::Paletted(p) => p.palette[p.idx[i] as usize],
        }
    }

    pub fn uniform(&self) -> Option<u32> {
        match self {
            Biomes::Single(b) => Some(*b),
            Biomes::Paletted(_) => None,
        }
    }

    /// Все 64 клетки.
    pub fn cells(&self) -> Vec<u32> {
        match self {
            Biomes::Single(b) => vec![*b; 64],
            Biomes::Paletted(p) => (0..64).map(|i| p.palette[p.idx[i] as usize]).collect(),
        }
    }

    /// Все 64 клетки одним значением, если они совпадают.
    pub fn from_cells(cells: &[u32]) -> Self {
        assert_eq!(cells.len(), 64, "biomes: expected 64 cells");
        let first = cells[0];
        if cells.iter().all(|&c| c == first) {
            return Biomes::Single(first);
        }
        let mut palette: Vec<u32> = Vec::new();
        let mut idx = [0u8; 64];
        for (i, &c) in cells.iter().enumerate() {
            let slot = match palette.iter().position(|&p| p == c) {
                Some(slot) => slot,
                None => {
                    palette.push(c);
                    palette.len() - 1
                }
            };
            idx[i] = slot as u8;
        }
        Biomes::Paletted(Arc::new(BiomePalette { palette, idx }))
    }
}

/// Секция 16×16×16: состояния блоков (индекс `(y*16+z)*16+x`) и биомы 4×4×4.
/// Однородная — одно значение; иначе палитра с индексами u8/u16 за `Arc`:
/// клон столбца не копирует блоки, запись копирует только свою секцию.
#[derive(Clone, Debug)]
pub struct Section {
    storage: Storage,
    non_air: u16,
    pub biome: Biomes,
    /// Закодированный для сети контейнер блоков: клоны делят его, `set` сбрасывает.
    net: Arc<std::sync::OnceLock<Vec<u8>>>,
}

impl PartialEq for Section {
    fn eq(&self, other: &Self) -> bool {
        self.biome == other.biome && self.blocks() == other.blocks()
    }
}

impl Section {
    pub fn filled(state: u32, biome: u32) -> Self {
        Self {
            storage: Storage::Single(state),
            non_air: if state == AIR { 0 } else { 4096 },
            biome: Biomes::Single(biome),
            net: Default::default(),
        }
    }

    /// Состояние и биомы секции: `biomes` — 64 клетки 4×4×4.
    pub fn with_biomes(state: u32, biomes: &[u32]) -> Self {
        let mut s = Self::filled(state, 0);
        s.biome = Biomes::from_cells(biomes);
        s
    }

    /// Биом клетки 4×4×4 по локальным `x, y, z` в 0..4.
    pub fn biome_at(&self, x: usize, y: usize, z: usize) -> u32 {
        self.biome.get(x + 4 * z + 16 * y)
    }

    /// Из полного массива 4096 состояний (сжимается, если однородный).
    pub fn from_blocks(blocks: Vec<u32>, biome: u32) -> Self {
        assert_eq!(blocks.len(), 4096);
        Self::from_states(biome, |i| blocks[i])
    }

    /// 4096 состояний одним проходом. `get(i)` равен `at(i)`, одно значение — `filled`.
    pub(crate) fn from_states(biome: u32, mut at: impl FnMut(usize) -> u32) -> Self {
        // Палитра секции — единицы элементов (камень, воздух, вода, руда),
        // поэтому линейный `position` по `Vec` дешевле хэша; вырожденные
        // секции одного состояния отсекаются ниже отдельным проходом.
        let mut palette = Vec::new();
        let mut counts = Vec::new();
        let mut idx = Box::new([0u16; 4096]);
        let mut non_air = 0u16;
        let first = at(0);
        let mut uniform = true;
        for (i, slot) in idx.iter_mut().enumerate() {
            let state = if i == 0 { first } else { at(i) };
            if state != first {
                uniform = false;
            }
            if state != AIR {
                non_air += 1;
            }
            let s = match palette.iter().position(|&p| p == state) {
                Some(s) => s,
                None => {
                    palette.push(state);
                    counts.push(0usize);
                    palette.len() - 1
                }
            };
            counts[s] += 1;
            *slot = s as u16;
        }
        if uniform {
            return Self::filled(first, biome);
        }
        let indices = if palette.len() <= 256 {
            let mut narrow = Box::new([0u8; 4096]);
            for (dst, &src) in narrow.iter_mut().zip(idx.iter()) {
                *dst = src as u8;
            }
            Indices::Narrow(narrow)
        } else {
            Indices::Wide(idx)
        };
        Self {
            storage: Storage::Paletted(Arc::new(Paletted {
                palette,
                counts: counts.into_iter().map(|c| c as u16).collect(),
                idx: indices,
            })),
            non_air,
            biome: Biomes::Single(biome),
            net: Default::default(),
        }
    }

    pub fn get(&self, i: usize) -> u32 {
        match &self.storage {
            Storage::Single(b) => *b,
            Storage::Paletted(p) => p.palette[p.idx.get(i)],
        }
    }

    pub fn is_uniform(&self) -> bool {
        matches!(self.storage, Storage::Single(_))
    }

    /// Ставит состояние, возвращает прежнее. O(1), кроме роста палитры.
    pub fn set(&mut self, i: usize, state: u32) -> u32 {
        let old = self.get(i);
        if old == state {
            return old;
        }
        self.non_air = (self.non_air as i32 + (state != AIR) as i32 - (old != AIR) as i32) as u16;
        self.net = Default::default();
        if let Storage::Single(b) = self.storage {
            self.storage = Storage::Paletted(Arc::new(Paletted {
                palette: vec![b],
                counts: vec![4096],
                idx: Indices::Narrow(Box::new([0; 4096])),
            }));
        }
        let Storage::Paletted(arc) = &mut self.storage else {
            unreachable!()
        };
        let p = Arc::make_mut(arc);
        let from = p.idx.get(i);
        let to = p.entry(state);
        p.counts[from] -= 1;
        p.counts[to] += 1;
        match &mut p.idx {
            Indices::Narrow(a) => a[i] = to as u8,
            Indices::Wide(a) => a[i] = to as u16,
        }
        if p.counts[to] == 4096 {
            self.storage = Storage::Single(state);
        }
        old
    }

    /// Все 4096 состояний.
    pub fn blocks(&self) -> Vec<u32> {
        match &self.storage {
            Storage::Single(b) => vec![*b; 4096],
            Storage::Paletted(p) => (0..4096).map(|i| p.palette[p.idx.get(i)]).collect(),
        }
    }

    fn non_air(&self) -> usize {
        self.non_air as usize
    }

    /// Секция из одного состояния, включая воздух.
    fn uniform_state(&self) -> Option<u32> {
        match &self.storage {
            Storage::Single(state) => Some(*state),
            Storage::Paletted(_) => None,
        }
    }

    /// Блоки с жидкостью (`LevelChunkSection.fluidCount`): вода, лава и
    /// затопленные. Без таблицы [`set_fluid_states`] — ноль.
    fn fluids(&self) -> u16 {
        let Some(table) = FLUID_STATES.get() else {
            return 0;
        };
        let is = |s: u32| table.get(s as usize).copied().unwrap_or(false);
        match &self.storage {
            Storage::Single(s) => {
                if is(*s) {
                    4096
                } else {
                    0
                }
            }
            Storage::Paletted(_) => {
                if self.non_air == 0 {
                    return 0;
                }
                (0..4096).filter(|&i| is(self.get(i))).count() as u16
            }
        }
    }
}

static FLUID_STATES: std::sync::OnceLock<Vec<bool>> = std::sync::OnceLock::new();

/// Биты класса карты высот по ID состояния.
/// 1 — `WORLD_SURFACE` (не воздух), 2 — `MOTION_BLOCKING`, 4 — без листвы.
static HEIGHT_CLASS: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();

/// Таблица для трёх клиентских карт высот. Без неё все три совпадают с
/// «любой не-воздух»: так устроены тесты, пока сервер не загрузил теги.
pub fn set_height_classes(table: Vec<u8>) {
    let _ = HEIGHT_CLASS.set(table);
}

/// Таблица «в состоянии есть жидкость» по ID состояния: клиент 26.x читает
/// число таких блоков в секции и по нему решает, есть ли в ней жидкость.
pub fn set_fluid_states(table: Vec<bool>) {
    let _ = FLUID_STATES.set(table);
}

/// Биты карты высот состояния. Без таблицы не-воздух считается поверхностью.
pub fn height_bits(state: u32) -> u8 {
    HEIGHT_CLASS
        .get()
        .and_then(|table| table.get(state as usize).copied())
        .unwrap_or(if state == AIR { 0 } else { 1 })
}

/// В состоянии есть жидкость (вода, лава, водолаз).
pub fn is_fluid_state(state: u32) -> bool {
    FLUID_STATES
        .get()
        .and_then(|table| table.get(state as usize).copied())
        .unwrap_or(false)
}

/// Столбец чанка. Считается, что воздух — состояние 0 (`minecraft:air`).
#[derive(Clone, Debug)]
pub struct ChunkColumn {
    pub sections: Vec<Section>,
    /// Бит для прямой (глобальной) палитры блоков: ceil(log2(число состояний)).
    pub global_block_bits: u32,
    pub global_biome_bits: u32,
    pub has_skylight: bool,
    /// Свет по секциям мира (без секций над/под миром).
    pub sky_light: Vec<LightLayer>,
    pub block_light: Vec<LightLayer>,
    /// Блок-сущности по мировым координатам.
    pub block_entities: BTreeMap<[i32; 3], BlockEntity>,
}

const AIR: u32 = 0;

impl ChunkColumn {
    /// Плоский мир: слои снизу вверх `(состояние, толщина)`, начиная с `min_y`.
    pub fn flat(
        layers: &[(u32, usize)],
        sections: usize,
        biome: u32,
        global_block_bits: u32,
        global_biome_bits: u32,
        has_skylight: bool,
    ) -> Self {
        let mut column: Vec<u32> = layers
            .iter()
            .flat_map(|&(s, n)| std::iter::repeat_n(s, n))
            .collect();
        column.resize(sections * 16, AIR);
        let sections = (0..sections)
            .map(|i| {
                let base = i * 16;
                let state = column[base];
                if column[base..base + 16].iter().all(|&s| s == state) {
                    Section::filled(state, biome)
                } else {
                    Section::from_states(biome, |idx| column[base + idx / 256])
                }
            })
            .collect();
        let n = column.len() / 16;
        // До `light::relight_column` — небо всюду (как раньше), блочного нет.
        Self {
            sections,
            global_block_bits,
            global_biome_bits,
            has_skylight,
            sky_light: vec![LightLayer::Uniform(if has_skylight { 15 } else { 0 }); n],
            block_light: vec![LightLayer::Uniform(0); n],
            block_entities: BTreeMap::new(),
        }
    }

    /// То же, но с биомами по клеткам 4×4×4: по 64 на секцию, порядок
    /// `x + 4*z + 16*y`. Длина — `секции * 64`.
    pub fn from_blocks_with_biomes(
        states: &[u32],
        biomes: &[u32],
        global_block_bits: u32,
        global_biome_bits: u32,
        has_skylight: bool,
    ) -> Self {
        let mut c = Self::from_blocks(
            states,
            biomes.first().copied().unwrap_or(0),
            global_block_bits,
            global_biome_bits,
            has_skylight,
        );
        if biomes.len() == c.sections.len() * 64 {
            for (s, cells) in c.sections.iter_mut().zip(biomes.chunks(64)) {
                s.biome = Biomes::from_cells(cells);
            }
        }
        c
    }

    /// Столбец из состояний длины `высота * 256`. Индекс `((y * 16 + z) * 16 + x)`,
    /// `y` от низа мира — тот же порядок, что у `Generator::fill`. Одинаковая
    /// секция сжимается. Свет до вызова `light::relight_column` — полное небо.
    pub fn from_blocks(
        states: &[u32],
        biome: u32,
        global_block_bits: u32,
        global_biome_bits: u32,
        has_skylight: bool,
    ) -> Self {
        assert!(
            !states.is_empty() && states.len().is_multiple_of(4096),
            "from_blocks: expected full sections, got {}",
            states.len()
        );
        let n = states.len() / 4096;
        let sections = states
            .chunks(4096)
            .map(|sec| {
                let state = sec[0];
                if sec.iter().all(|&s| s == state) {
                    Section::filled(state, biome)
                } else {
                    Section::from_blocks(sec.to_vec(), biome)
                }
            })
            .collect();
        Self {
            sections,
            global_block_bits,
            global_biome_bits,
            has_skylight,
            sky_light: vec![LightLayer::Uniform(if has_skylight { 15 } else { 0 }); n],
            block_light: vec![LightLayer::Uniform(0); n],
            block_entities: BTreeMap::new(),
        }
    }

    /// Состояние блока; `y` — от низа мира (0..height), `x`/`z` — 0..16.
    pub fn block(&self, x: usize, y: usize, z: usize) -> u32 {
        self.sections[y / 16].get(((y % 16) * 16 + z) * 16 + x)
    }

    /// Ставит блок, возвращает прежнее состояние.
    pub fn set_block(&mut self, x: usize, y: usize, z: usize, state: u32) -> u32 {
        self.sections[y / 16].set(((y % 16) * 16 + z) * 16 + x, state)
    }

    /// Биом клетки 4×4×4. `x`/`z` — 0..16, `y` — от низа мира. Возвращает прежний id.
    pub fn set_biome(&mut self, x: usize, y: usize, z: usize, biome: u32) -> u32 {
        let section = &mut self.sections[y / 16];
        let index = (x / 4) + 4 * (z / 4) + 16 * ((y % 16) / 4);
        let old = section.biome.get(index);
        if old == biome {
            return old;
        }
        let mut cells = section.biome.cells();
        cells[index] = biome;
        section.biome = Biomes::from_cells(&cells);
        old
    }

    /// Биомы секций в том виде, в каком их ждёт пакет `chunks_biomes`.
    pub fn encode_biomes(&self) -> Vec<u8> {
        let mut data = Writer::new();
        for section in &self.sections {
            write_biome_container(&mut data, &section.biome, self.global_biome_bits);
        }
        data.buf
    }

    pub fn height(&self) -> usize {
        self.sections.len() * 16
    }

    /// Высота столбца: y верхнего непустого блока - min_y + 1 (0 — пустой).
    /// Целиком пустая секция пропускается, целиком занятая закрывает все ещё
    /// пустые столбцы. Ноль в результате значит «блок не найден», найденная
    /// высота всегда ≥ 1.
    fn heightmap(&self) -> Vec<u32> {
        self.classified_heightmaps(|_| 0b111)[0].clone()
    }

    /// Три клиентские карты: поверхность, движение, движение без листвы.
    /// Без таблицы классов лист считается землёй, и миникарта рисует пещеру.
    fn client_heightmaps(&self) -> [Vec<u32>; 3] {
        if let Some(class) = HEIGHT_CLASS.get() {
            return self.classified_heightmaps(|state| {
                class.get(state as usize).copied().unwrap_or(0)
            });
        }
        let hm = self.heightmap();
        [hm.clone(), hm.clone(), hm]
    }

    fn classified_heightmaps(&self, class_of: impl Fn(u32) -> u8) -> [Vec<u32>; 3] {
        let mut out = [vec![0u32; 256], vec![0u32; 256], vec![0u32; 256]];
        let mut pending = [256usize; 3];
        let bits_of = |state: u32| if state == AIR { 0 } else { class_of(state) };
        for si in (0..self.sections.len()).rev() {
            if pending.iter().all(|&n| n == 0) {
                break;
            }
            let section = &self.sections[si];
            if let Some(state) = section.uniform_state() {
                let bits = bits_of(state);
                if bits == 0 {
                    continue;
                }
                let h = (si * 16 + 16) as u32;
                for kind in 0..3 {
                    if bits & (1 << kind) == 0 {
                        continue;
                    }
                    for hgt in &mut out[kind] {
                        if *hgt == 0 {
                            *hgt = h;
                            pending[kind] -= 1;
                        }
                    }
                }
                continue;
            }
            if section.non_air() == 0 {
                continue;
            }
            for local_y in (0..16).rev() {
                if pending.iter().all(|&n| n == 0) {
                    break;
                }
                let base = local_y * 256;
                let h = (si * 16 + local_y + 1) as u32;
                for xz in 0..256 {
                    let bits = bits_of(section.get(base + xz));
                    if bits == 0 {
                        continue;
                    }
                    for kind in 0..3 {
                        if bits & (1 << kind) == 0 || out[kind][xz] != 0 {
                            continue;
                        }
                        out[kind][xz] = h;
                        pending[kind] -= 1;
                    }
                }
            }
        }
        out
    }

    /// Тело `level_chunk_with_light` после координат: ChunkData + LightData.
    pub fn encode_body(&self) -> Vec<u8> {
        let mut w = Writer::new();

        // Heightmaps: map<VarInt тип, long[]>; клиенту нужны типы с Usage.CLIENT.
        // Листва входит только в WORLD_SURFACE. Иначе миникарта считает игрока
        // под землёй и рисует пещеру по нулевому блочному свету.
        let bits = bits_for(self.height() + 1);
        const KINDS: [i32; 3] = [1, 4, 5];
        w.var_int(3);
        for (kind, hm) in KINDS.into_iter().zip(self.client_heightmaps()) {
            let packed = pack(&hm, bits);
            w.var_int(kind).var_int(packed.len() as i32);
            for l in packed {
                w.buf.extend_from_slice(&l.to_be_bytes());
            }
        }

        // Секции: short блоков, short жидкостей (26.x), блоки, биомы.
        let mut data = Writer::new();
        data.buf
            .reserve(self.sections.len().saturating_mul(2048 + 64));
        for s in &self.sections {
            data.u16(s.non_air() as u16).u16(s.fluids());
            // Неизменённая секция кодируется один раз: после правки блока
            // пересобирается только её секция, а не все 24.
            let bits = self.global_block_bits;
            let bytes = s.net.get_or_init(|| {
                let mut w = Writer::new();
                write_block_container(&mut w, s, 4, bits);
                w.buf
            });
            data.buf.extend_from_slice(bytes);
            write_biome_container(&mut data, &s.biome, self.global_biome_bits);
        }
        w.byte_array(&data.buf);
        // Блок-сущности: упакованные x/z, y, тип и NBT.
        // Сундуку клиенту данные не нужны. Костру нужны `Items`, иначе еда не видна.
        let types = BLOCK_ENTITY_TYPES.get();
        let known: Vec<(&BlockEntity, i32, [i32; 3])> = self
            .block_entities
            .iter()
            .filter_map(|(pos, be)| Some((be, *types?.get(&be.id)?, *pos)))
            .collect();
        w.var_int(known.len() as i32);
        for (be, kind, [x, y, z]) in known {
            w.u8(((x & 15) << 4 | (z & 15)) as u8)
                .u16(y as i16 as u16)
                .var_int(kind);
            if be.id == "minecraft:campfire" {
                w.buf.extend(nbt::write_network(&be.nbt));
            } else {
                w.u8(10).u8(0);
            }
        }

        // Свет: секции -1..=n (n+2 бит): под миром темно, над миром — небо.
        let above = LightLayer::Uniform(if self.has_skylight { 15 } else { 0 });
        let dark = LightLayer::Uniform(0);
        let sky: Vec<&LightLayer> = std::iter::once(&dark)
            .chain(&self.sky_light)
            .chain(std::iter::once(&above))
            .collect();
        let block: Vec<&LightLayer> = std::iter::once(&dark)
            .chain(&self.block_light)
            .chain(std::iter::once(&dark))
            .collect();
        let masks = |layers: &[&LightLayer]| {
            let lit: Vec<bool> = layers.iter().map(|l| !l.is_zero()).collect();
            let set = |want: bool| {
                let mut bytes = vec![0u8; lit.len().div_ceil(8)];
                for (i, &l) in lit.iter().enumerate() {
                    if l == want {
                        bytes[i / 8] |= 1 << (i % 8);
                    }
                }
                // BitSet::toByteArray без хвостовых нулей.
                while bytes.last() == Some(&0) {
                    bytes.pop();
                }
                bytes
            };
            (set(true), set(false))
        };
        let (sky_mask, empty_sky) = if self.has_skylight {
            masks(&sky)
        } else {
            (Vec::new(), bit_set(sky.len()))
        };
        let (block_mask, empty_block) = masks(&block);
        // В 26.3 BitSet кодируется как byte array (BitSet::toByteArray).
        for mask in [&sky_mask, &block_mask, &empty_sky, &empty_block] {
            w.byte_array(mask);
        }
        let no_sky = Vec::new();
        for layers in [if self.has_skylight { &sky } else { &no_sky }, &block] {
            let lit: Vec<_> = layers.iter().filter(|l| !l.is_zero()).collect();
            w.var_int(lit.len() as i32);
            for l in lit {
                w.var_int(2048);
                l.write_bytes(&mut w.buf);
            }
        }
        w.buf
    }
}

/// `java.util.BitSet::toByteArray` для битов `0..n`: little-endian, без хвостовых нулей.
fn bit_set(n: usize) -> Vec<u8> {
    (0..n.div_ceil(8))
        .map(|i| match n - i * 8 {
            8.. => 0xff,
            bits => (1u8 << bits) - 1,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chunkwake_protocol::{Reader, Writer};

    #[test]
    fn bits_and_packing() {
        assert_eq!(bits_for(1), 0);
        assert_eq!(bits_for(2), 1);
        assert_eq!(bits_for(4), 2);
        assert_eq!(bits_for(5), 3);
        assert_eq!(bits_for(385), 9);
        // 9 бит: 7 значений на long, без переноса через границу.
        let packed = pack(&[1; 8], 9);
        assert_eq!(packed.len(), 2);
        assert_eq!(packed[1], 1);
        assert_eq!(bit_set(26), [0xff, 0xff, 0xff, 0x03]);
        assert_eq!(bit_set(16), [0xff, 0xff]);
    }

    #[test]
    fn from_blocks_follows_fill_order_and_blocks_sky() {
        // Одна секция. Камень (1) в (x=1, y=3, z=2), остальное воздух.
        let mut states = vec![0u32; 4096];
        states[((3 * 16 + 2) * 16 + 1) as usize] = 1;
        let mut c = ChunkColumn::from_blocks(&states, 7, 4, 2, true);
        assert_eq!(c.block(1, 3, 2), 1);
        assert_eq!(c.block(0, 0, 0), 0);
        assert_eq!(c.sections[0].biome, Biomes::Single(7));
        assert!(!c.sections[0].is_uniform());
        let air = ChunkColumn::from_blocks(&[0; 4096], 7, 4, 2, true);
        assert!(air.sections[0].is_uniform());
        // 0 — воздух, небо проходит; 1 — камень, небо стоит.
        let table = light::LightTable::new([(0, 1, true), (0, 15, false)]);
        light::relight_column(&mut c, &table);
        let at = |x: i32, y: i32, z: i32| {
            c.light(light::Kind::Sky)[0].get(((((y & 15) << 8) | (z << 4) | x) as usize) & 4095)
        };
        assert_eq!(at(1, 4, 2), 15);
        assert_eq!(at(1, 3, 2), 0);
        assert_eq!(at(0, 0, 0), 15);
    }

    #[test]
    fn mixed_biomes_survive_network_and_anvil() {
        let mut biomes = vec![1u32; 24 * 64];
        // Третья секция (y 32..47), клетка (x=1, z=2, y=3) — пустыня.
        biomes[2 * 64 + 1 + 4 * 2 + 16 * 3] = 9;
        let states = vec![0u32; 24 * 4096];
        let c = ChunkColumn::from_blocks_with_biomes(&states, &biomes, 15, 7, true);
        assert_eq!(c.sections[0].biome, Biomes::Single(1));
        assert_eq!(c.sections[2].biome_at(1, 3, 2), 9);
        assert_eq!(c.sections[2].biome_at(0, 0, 0), 1);
        // Сеть: в теле чанка у третьей секции палитра из двух биомов.
        let body = c.encode_body();
        let mut r = Reader::new(&body);
        let maps = r.var_int().unwrap();
        for _ in 0..maps {
            r.var_int().unwrap();
            let n = r.var_int().unwrap() as usize;
            r.bytes(n * 8).unwrap();
        }
        let len = r.var_int().unwrap() as usize;
        let data = r.bytes(len).unwrap().to_vec();
        let mut d = Reader::new(&data);
        for _ in 0..2 {
            d.u16().unwrap();
            d.u16().unwrap();
            skip_block_container(&mut d);
            skip_biome_container(&mut d);
        }
        d.u16().unwrap();
        d.u16().unwrap();
        skip_block_container(&mut d);
        // Третья секция: палитра 1|9 и 64 записи.
        assert_eq!(d.u8().unwrap(), 1, "one bit per biome cell");
        assert_eq!(d.var_int().unwrap(), 2);
        assert_eq!(d.var_int().unwrap(), 1);
        assert_eq!(d.var_int().unwrap(), 9);
    }

    /// Пропуск контейнера блоков секции (4..8 бит, палитра или глобальная).
    fn skip_block_container(r: &mut Reader<'_>) {
        let bits = r.u8().unwrap();
        if bits == 0 {
            r.var_int().unwrap();
            return;
        }
        if bits <= 8 {
            let n = r.var_int().unwrap() as usize;
            for _ in 0..n {
                r.var_int().unwrap();
            }
            let longs = 4096usize.div_ceil(64 / bits as usize);
            r.bytes(longs * 8).unwrap();
        } else {
            let longs = 4096usize.div_ceil(64 / bits as usize);
            r.bytes(longs * 8).unwrap();
        }
    }

    fn skip_biome_container(r: &mut Reader<'_>) {
        let bits = r.u8().unwrap();
        if bits == 0 {
            r.var_int().unwrap();
            return;
        }
        if bits <= 8 {
            let n = r.var_int().unwrap() as usize;
            for _ in 0..n {
                r.var_int().unwrap();
            }
        }
        let longs = 64usize.div_ceil(64 / bits as usize);
        r.bytes(longs * 8).unwrap();
    }

    #[test]
    fn flat_chunk_layout() {
        // bedrock, 2×dirt, grass; 24 секции (overworld).
        let c = ChunkColumn::flat(&[(88, 1), (10, 2), (9, 1)], 24, 7, 15, 7, true);
        assert_eq!(c.heightmap()[0], 4);
        let body = c.encode_body();
        let mut r = Reader::new(&body);

        assert_eq!(r.var_int().unwrap(), 3, "heightmaps");
        for _ in 0..3 {
            r.var_int().unwrap();
            let n = r.var_int().unwrap() as usize;
            assert_eq!(n, 256usize.div_ceil(7));
            r.bytes(n * 8).unwrap();
        }
        let len = r.var_int().unwrap() as usize;
        let mut s = Reader::new(r.bytes(len).unwrap());
        // Нижняя секция: 4 слоя → косвенная палитра на 4 бита.
        assert_eq!(s.u16().unwrap(), 4 * 256);
        assert_eq!(s.u16().unwrap(), 0);
        assert_eq!(s.u8().unwrap(), 4);
        assert_eq!(s.var_int().unwrap(), 4); // bedrock, dirt, grass, air
        for _ in 0..4 {
            s.var_int().unwrap();
        }
        s.bytes(256 * 8).unwrap(); // 4096 значений × 4 бита
        assert_eq!((s.u8().unwrap(), s.var_int().unwrap()), (0, 7), "biome");
        // Остальные 23 секции — один воздух.
        for _ in 1..24 {
            assert_eq!(s.u16().unwrap(), 0);
            s.u16().unwrap();
            assert_eq!((s.u8().unwrap(), s.var_int().unwrap()), (0, 0));
            assert_eq!((s.u8().unwrap(), s.var_int().unwrap()), (0, 7));
        }
        assert!(s.remaining().is_empty());
        assert_eq!(r.var_int().unwrap(), 0, "block entities");
        // Небо в секциях 0..=24 (бит 0 — под миром, тёмная), блочного света нет.
        for expect in [
            &[0xfe, 0xff, 0xff, 0x03][..],
            &[],
            &[0x01],
            &[0xff, 0xff, 0xff, 0x03],
        ] {
            let n = r.var_int().unwrap() as usize;
            assert_eq!(r.bytes(n).unwrap(), expect, "light mask");
        }
        assert_eq!(r.var_int().unwrap(), 25, "sky arrays");
        for _ in 0..25 {
            assert_eq!(r.var_int().unwrap(), 2048);
            r.bytes(2048).unwrap();
        }
        assert_eq!(r.var_int().unwrap(), 0, "block light arrays");
        assert!(r.remaining().is_empty());
    }

    /// Старый кодировщик: эталон байтов, с которым сверяется путь без массива на 4096.
    fn legacy_container(w: &mut Writer, values: &[u32], min_bits: u32, global_bits: u32) {
        let mut palette: Vec<u32> = Vec::new();
        for &v in values {
            if !palette.contains(&v) {
                palette.push(v);
                if palette.len() > 256 {
                    break;
                }
            }
        }
        if palette.len() == 1 {
            w.u8(0).var_int(palette[0] as i32);
            return;
        }
        let indirect_bits = bits_for(palette.len()).max(min_bits);
        let data = if palette.len() <= 256 && indirect_bits <= 8 {
            w.u8(indirect_bits as u8).var_int(palette.len() as i32);
            for &p in &palette {
                w.var_int(p as i32);
            }
            let idx: Vec<u32> = values
                .iter()
                .map(|v| palette.iter().position(|p| p == v).unwrap() as u32)
                .collect();
            pack(&idx, indirect_bits)
        } else {
            w.u8(global_bits as u8);
            pack(values, global_bits)
        };
        for l in data {
            w.buf.extend_from_slice(&l.to_be_bytes());
        }
    }

    fn section_bytes(body: &[u8]) -> Vec<u8> {
        let mut r = Reader::new(body);
        let maps = r.var_int().unwrap();
        for _ in 0..maps {
            r.var_int().unwrap();
            let n = r.var_int().unwrap() as usize;
            r.bytes(n * 8).unwrap();
        }
        let len = r.var_int().unwrap() as usize;
        r.bytes(len).unwrap().to_vec()
    }

    fn legacy_sections(c: &ChunkColumn) -> Vec<u8> {
        let mut w = Writer::new();
        for s in &c.sections {
            w.u16(s.non_air() as u16).u16(0);
            legacy_container(&mut w, &s.blocks(), 4, c.global_block_bits);
            legacy_container(&mut w, &s.biome.cells(), 1, c.global_biome_bits);
        }
        w.buf
    }

    fn assert_sections_match(c: &ChunkColumn) {
        assert_eq!(section_bytes(&c.encode_body()), legacy_sections(c));
    }

    #[test]
    fn containers_match_expanded_scan() {
        assert_sections_match(&ChunkColumn::flat(
            &[(88, 1), (10, 2), (9, 1)],
            24,
            7,
            15,
            7,
            true,
        ));

        // Слот состояния 1 обнуляется и занимается заново: внутренний порядок
        // палитры (0, 1, 2, 3) не совпадает с первым появлением (3, 2, 1, 0).
        let mut reused = ChunkColumn::flat(&[], 1, 7, 15, 6, false);
        reused.set_block(0, 0, 0, 1);
        reused.set_block(1, 0, 0, 2);
        reused.set_block(0, 0, 0, 3);
        reused.set_block(2, 0, 0, 1);
        assert_sections_match(&reused);

        // Ровно 256 состояний — ещё косвенная палитра на 8 бит.
        let mut indirect = ChunkColumn::flat(&[], 1, 3, 15, 6, false);
        for i in 0..255 {
            indirect.set_block(i % 16, 0, i / 16, (i + 1) as u32);
        }
        assert_sections_match(&indirect);

        // 257 состояний — прямая палитра, в long'ах сами id.
        let mut direct = ChunkColumn::flat(&[], 1, 3, 9, 6, false);
        for i in 0..256 {
            direct.set_block(i % 16, 0, i / 16, (i + 1) as u32);
        }
        assert_sections_match(&direct);
    }

    /// Старый поклеточный проход: эталон тех же 256 высот.
    fn legacy_heightmap(c: &ChunkColumn) -> Vec<u32> {
        (0..256)
            .map(|xz| {
                (0..c.height())
                    .rev()
                    .find(|&y| c.sections[y / 16].get((y % 16) * 256 + xz) != AIR)
                    .map_or(0, |y| y as u32 + 1)
            })
            .collect()
    }

    #[test]
    fn heightmap_skips_uniform_sections() {
        let flat = ChunkColumn::flat(&[(88, 1), (10, 2), (9, 1)], 24, 7, 15, 7, true);
        let hm = flat.heightmap();
        assert_eq!(hm, legacy_heightmap(&flat));
        assert!(hm.iter().all(|&h| h == 4));

        // Верхняя секция смешанная, нижняя целиком камень.
        // Уже найденная высота не затирается высотой сплошной секции.
        let mut c = ChunkColumn::flat(&[(1, 16)], 2, 0, 4, 1, false);
        c.set_block(0, 16, 0, 2);
        c.set_block(1, 31, 0, 2);
        let hm = c.heightmap();
        assert_eq!(hm, legacy_heightmap(&c));
        assert_eq!(hm[0], 17);
        assert_eq!(hm[1], 32);
        assert_eq!(hm[2], 16);

        let mut mixed = ChunkColumn::flat(&[], 2, 0, 4, 1, false);
        mixed.set_block(3, 5, 4, 9);
        mixed.set_block(3, 20, 4, 9);
        let hm = mixed.heightmap();
        assert_eq!(hm, legacy_heightmap(&mixed));
        assert_eq!(hm[4 * 16 + 3], 21);
        assert_eq!(hm[0], 0);
    }

    #[test]
    fn motion_heightmap_ignores_leaves() {
        let mut c = ChunkColumn::flat(&[(1, 4)], 2, 0, 4, 1, false);
        c.set_block(0, 10, 0, 2);
        let maps = c.classified_heightmaps(|state| match state {
            1 => 0b111,
            2 => 0b001,
            _ => 0,
        });
        assert_eq!(maps[0][0], 11, "листва входит в поверхность");
        assert_eq!(maps[1][0], 4, "движение останавливается на камне");
        assert_eq!(maps[2][0], 4);
        assert_eq!(maps[0][1], 4);
    }

    #[test]
    fn from_states_matches_sets() {
        let stepwise = |blocks: &[u32]| {
            let mut s = Section::filled(blocks[0], 7);
            for (i, &b) in blocks.iter().enumerate() {
                s.set(i, b);
            }
            s
        };
        let mut blocks = vec![0u32; 4096];
        blocks[0] = 3;
        blocks[1] = 2;
        blocks[2] = 1;
        blocks[100] = 3;
        let built = Section::from_blocks(blocks.clone(), 7);
        let step = stepwise(&blocks);
        assert_eq!(built, step);
        assert_eq!(built.non_air(), step.non_air());

        for (i, block) in blocks.iter_mut().take(300).enumerate() {
            *block = (i + 1) as u32;
        }
        let built = Section::from_blocks(blocks.clone(), 7);
        let step = stepwise(&blocks);
        assert_eq!(built, step);
        assert_eq!(built.non_air(), step.non_air());
    }
}

#[cfg(test)]
mod edit_tests {
    use super::*;

    #[test]
    fn set_and_compact() {
        let mut c = ChunkColumn::flat(&[(88, 1), (10, 2), (9, 1)], 24, 7, 15, 7, true);
        assert_eq!(
            c.sections[1],
            Section::filled(AIR, 7),
            "пустая секция однородна"
        );
        assert_eq!(c.block(3, 3, 5), 9);
        assert_eq!(c.set_block(3, 3, 5, AIR), 9);
        assert_eq!(c.block(3, 3, 5), AIR);
        assert_eq!(c.set_block(0, 20, 0, 1), AIR);
        assert!(!c.sections[1].is_uniform());
        c.set_block(0, 20, 0, AIR);
        assert!(c.sections[1].is_uniform(), "снова однородна");
        // Клон делит блоки; запись в копию не трогает оригинал.
        c.set_block(0, 20, 0, 1);
        let before = c.clone();
        c.set_block(1, 20, 0, 1);
        assert_eq!(before.block(1, 20, 0), AIR);
        assert_eq!(c.block(1, 20, 0), 1);
    }
}
