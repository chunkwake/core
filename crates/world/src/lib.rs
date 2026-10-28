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

/// Секция 16×16×16: состояния блоков (индекс `(y*16+z)*16+x`) и один биом.
/// Однородная — одно значение; иначе палитра с индексами u8/u16 за `Arc`:
/// клон столбца не копирует блоки, запись копирует только свою секцию.
#[derive(Clone, Debug)]
pub struct Section {
    storage: Storage,
    non_air: u16,
    pub biome: u32,
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
            biome,
            net: Default::default(),
        }
    }

    /// Из полного массива 4096 состояний (сжимается, если однородный).
    pub fn from_blocks(blocks: Vec<u32>, biome: u32) -> Self {
        assert_eq!(blocks.len(), 4096);
        Self::from_states(biome, |i| blocks[i])
    }

    /// 4096 состояний одним проходом. `get(i)` равен `at(i)`, одно значение — `filled`.
    pub(crate) fn from_states(biome: u32, mut at: impl FnMut(usize) -> u32) -> Self {
        let mut palette = Vec::new();
        let mut counts = Vec::new();
        let mut idx = Box::new([0u16; 4096]);
        let mut non_air = 0u16;
        for i in 0..4096 {
            let state = at(i);
            if state != AIR {
                non_air += 1;
            }
            let slot = match palette.iter().position(|&p| p == state) {
                Some(slot) => slot,
                None => {
                    palette.push(state);
                    counts.push(0);
                    palette.len() - 1
                }
            };
            counts[slot] += 1;
            idx[i] = slot as u16;
        }
        if palette.len() == 1 {
            return Self::filled(palette[0], biome);
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
                counts,
                idx: indices,
            })),
            non_air,
            biome,
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

    /// Состояние блока; `y` — от низа мира (0..height), `x`/`z` — 0..16.
    pub fn block(&self, x: usize, y: usize, z: usize) -> u32 {
        self.sections[y / 16].get(((y % 16) * 16 + z) * 16 + x)
    }

    /// Ставит блок, возвращает прежнее состояние.
    pub fn set_block(&mut self, x: usize, y: usize, z: usize, state: u32) -> u32 {
        self.sections[y / 16].set(((y % 16) * 16 + z) * 16 + x, state)
    }

    pub fn height(&self) -> usize {
        self.sections.len() * 16
    }

    /// Высота столбца: y верхнего непустого блока - min_y + 1 (0 — пустой).
    /// Целиком пустая секция пропускается, целиком занятая закрывает все ещё
    /// пустые столбцы. Ноль в результате значит «блок не найден», найденная
    /// высота всегда ≥ 1.
    fn heightmap(&self) -> Vec<u32> {
        let mut out = vec![0u32; 256];
        let mut pending = 256usize;
        for si in (0..self.sections.len()).rev() {
            if pending == 0 {
                break;
            }
            let section = &self.sections[si];
            match section.non_air() {
                0 => continue,
                4096 => {
                    let h = (si * 16 + 16) as u32;
                    for hgt in &mut out {
                        if *hgt == 0 {
                            *hgt = h;
                        }
                    }
                    break;
                }
                _ => {
                    for local_y in (0..16).rev() {
                        if pending == 0 {
                            break;
                        }
                        let base = local_y * 256;
                        for (xz, hgt) in out.iter_mut().enumerate() {
                            if *hgt != 0 {
                                continue;
                            }
                            if section.get(base + xz) != AIR {
                                *hgt = (si * 16 + local_y + 1) as u32;
                                pending -= 1;
                            }
                        }
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
        let bits = bits_for(self.height() + 1);
        let hm = pack(&self.heightmap(), bits);
        const WORLD_SURFACE: i32 = 1;
        const MOTION_BLOCKING: i32 = 4;
        const MOTION_BLOCKING_NO_LEAVES: i32 = 5;
        w.var_int(3);
        for kind in [WORLD_SURFACE, MOTION_BLOCKING, MOTION_BLOCKING_NO_LEAVES] {
            w.var_int(kind).var_int(hm.len() as i32);
            for l in &hm {
                w.buf.extend_from_slice(&l.to_be_bytes());
            }
        }

        // Секции: short блоков, short жидкостей (26.x), блоки, биомы.
        // Биом секции один, поэтому контейнер биомов — всегда одна запись.
        let mut data = Writer::new();
        data.buf
            .reserve(self.sections.len().saturating_mul(2048 + 64));
        for s in &self.sections {
            data.u16(s.non_air() as u16).u16(0);
            // Неизменённая секция кодируется один раз: после правки блока
            // пересобирается только её секция, а не все 24.
            let bits = self.global_block_bits;
            let bytes = s.net.get_or_init(|| {
                let mut w = Writer::new();
                write_block_container(&mut w, s, 4, bits);
                w.buf
            });
            data.buf.extend_from_slice(bytes);
            write_single(&mut data, s.biome);
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
            legacy_container(&mut w, &[s.biome; 64], 1, c.global_biome_bits);
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
