//! chunkwake-world — чанки и их сетевое кодирование (формат 26.3).

use chunkwake_protocol::Writer;

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

/// Paletted container: бит на запись, палитра, long-массив без префикса длины.
fn write_container(w: &mut Writer, values: &[u32], min_bits: u32, global_bits: u32) {
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
    let (bits, data) = if palette.len() <= 256 && indirect_bits <= 8 {
        w.u8(indirect_bits as u8).var_int(palette.len() as i32);
        for &p in &palette {
            w.var_int(p as i32);
        }
        let idx: Vec<u32> = values
            .iter()
            .map(|v| palette.iter().position(|p| p == v).unwrap() as u32)
            .collect();
        (indirect_bits, pack(&idx, indirect_bits))
    } else {
        w.u8(global_bits as u8);
        (global_bits, pack(values, global_bits))
    };
    debug_assert!(bits > 0);
    for l in data {
        w.buf.extend_from_slice(&l.to_be_bytes());
    }
}

/// Секция 16×16×16: состояния блоков (индекс `(y*16+z)*16+x`) и один биом.
/// Однородная секция хранит одно значение, иначе — все 4096.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    blocks: Vec<u32>,
    pub biome: u32,
}

impl Section {
    pub fn filled(state: u32, biome: u32) -> Self {
        Self {
            blocks: vec![state],
            biome,
        }
    }

    /// Из полного массива 4096 состояний (сжимается, если однородный).
    pub fn from_blocks(blocks: Vec<u32>, biome: u32) -> Self {
        assert_eq!(blocks.len(), 4096);
        let mut s = Self { blocks, biome };
        s.compact();
        s
    }

    pub fn get(&self, i: usize) -> u32 {
        match self.blocks.as_slice() {
            [b] => *b,
            all => all[i],
        }
    }

    /// Ставит состояние, возвращает прежнее.
    pub fn set(&mut self, i: usize, state: u32) -> u32 {
        let old = self.get(i);
        if old != state {
            if self.blocks.len() == 1 {
                self.blocks = vec![old; 4096];
            }
            self.blocks[i] = state;
            self.compact();
        }
        old
    }

    fn compact(&mut self) {
        if self.blocks.len() > 1 && self.blocks.iter().all(|&b| b == self.blocks[0]) {
            self.blocks.truncate(1);
            self.blocks.shrink_to_fit();
        }
    }

    /// Все 4096 состояний.
    pub fn blocks(&self) -> std::borrow::Cow<'_, [u32]> {
        match self.blocks.as_slice() {
            [b] => std::borrow::Cow::Owned(vec![*b; 4096]),
            all => std::borrow::Cow::Borrowed(all),
        }
    }

    fn non_air(&self) -> usize {
        match self.blocks.as_slice() {
            [b] if *b == AIR => 0,
            [_] => 4096,
            all => all.iter().filter(|&&b| b != AIR).count(),
        }
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
                let mut blocks = vec![AIR; 4096];
                for y in 0..16 {
                    blocks[y * 256..(y + 1) * 256].fill(column[i * 16 + y]);
                }
                Section::from_blocks(blocks, biome)
            })
            .collect();
        Self {
            sections,
            global_block_bits,
            global_biome_bits,
            has_skylight,
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
    fn heightmap(&self) -> Vec<u32> {
        (0..256)
            .map(|xz| {
                (0..self.height())
                    .rev()
                    .find(|&y| self.sections[y / 16].get((y % 16) * 256 + xz) != AIR)
                    .map_or(0, |y| y as u32 + 1)
            })
            .collect()
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
        let mut data = Writer::new();
        for s in &self.sections {
            data.u16(s.non_air() as u16).u16(0);
            write_container(&mut data, &s.blocks(), 4, self.global_block_bits);
            write_container(&mut data, &[s.biome; 64], 1, self.global_biome_bits);
        }
        w.byte_array(&data.buf);
        w.var_int(0); // block entities

        // Свет: секции -1..=n (n+2 бит). Небо — полный свет, блочного нет.
        let light_sections = self.sections.len() + 2;
        let all = bit_set(light_sections);
        let none: &[u8] = &[];
        let (sky, empty_sky) = if self.has_skylight {
            (all.as_slice(), none)
        } else {
            (none, all.as_slice())
        };
        // В 26.3 BitSet кодируется как byte array (BitSet::toByteArray).
        for mask in [sky, none, empty_sky, all.as_slice()] {
            w.byte_array(mask);
        }
        if self.has_skylight {
            w.var_int(light_sections as i32);
            for _ in 0..light_sections {
                w.byte_array(&[0xff; 2048]);
            }
        } else {
            w.var_int(0);
        }
        w.var_int(0); // block light arrays
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
    use chunkwake_protocol::Reader;

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
        for expect in [
            &[0xff, 0xff, 0xff, 0x03][..],
            &[],
            &[],
            &[0xff, 0xff, 0xff, 0x03],
        ] {
            let n = r.var_int().unwrap() as usize;
            assert_eq!(r.bytes(n).unwrap(), expect, "light mask");
        }
        assert_eq!(r.var_int().unwrap(), 26, "sky arrays");
        for _ in 0..26 {
            assert_eq!(r.var_int().unwrap(), 2048);
            r.bytes(2048).unwrap();
        }
        assert_eq!(r.var_int().unwrap(), 0, "block light arrays");
        assert!(r.remaining().is_empty());
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
        assert_eq!(c.sections[1].blocks.len(), 4096);
        c.set_block(0, 20, 0, AIR);
        assert_eq!(c.sections[1].blocks.len(), 1, "снова однородна");
    }
}
