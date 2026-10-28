//! Хранение мира в формате Anvil (`region/r.X.Z.mca`), совместимом с ванилой:
//! чанк — NBT-compound с секциями `block_states`/`biomes` (палитра + long-массив).

use crate::light::LightLayer;
use crate::nbt::{self, Compound, Tag};
use crate::{ChunkColumn, Section, bits_for, emit_packed, first_seen_palette, palette_index};
use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Имена состояний блоков и биомов ↔ их ID (сетевые/внутренние).
pub trait Names {
    fn block_name(&self, id: u32) -> Option<(&str, &[(String, String)])>;
    fn block_id(&self, name: &str, props: &[(String, String)]) -> Option<u32>;
    fn biome_name(&self, id: u32) -> Option<&str>;
    fn biome_id(&self, name: &str) -> Option<u32>;
}

/// `DataVersion` из `version.json` (`world_version`) и нижняя секция мира.
#[derive(Clone, Copy, Debug)]
pub struct Format {
    pub data_version: i32,
    pub min_section: i32,
}

/// Одно значение `SimpleBitStorage` (обратное к `pack`).
fn unpack_at(data: &[i64], bits: u32, i: usize) -> u32 {
    let per_long = (64 / bits) as usize;
    let mask = (1u64 << bits) - 1;
    let l = data.get(i / per_long).copied().unwrap_or(0) as u64;
    ((l >> ((i % per_long) as u32 * bits)) & mask) as u32
}

fn string(s: &str) -> Tag {
    Tag::String(s.to_owned())
}

/// Палитра + данные, как в `PalettedContainer` ванилы. `None` для данных — одно значение.
/// Порядок палитры — первое появление в секции, не порядок внутренних слотов.
fn palettize(s: &Section, min_bits: u32) -> (Vec<u32>, Option<Vec<i64>>) {
    if s.is_uniform() {
        return (vec![s.get(0)], None);
    }
    let palette = first_seen_palette(s, None);
    if palette.len() == 1 {
        return (palette, None);
    }
    let bits = bits_for(palette.len()).max(min_bits);
    let mut data = Vec::with_capacity(4096usize.div_ceil((64 / bits) as usize));
    emit_packed(
        4096,
        bits,
        |i| palette_index(&palette, s.get(i)),
        |long| data.push(long as i64),
    );
    (palette, Some(data))
}

pub fn chunk_to_nbt(
    c: &ChunkColumn,
    (x, z): (i32, i32),
    f: Format,
    names: &dyn Names,
) -> Result<Compound, String> {
    let mut sections = Vec::with_capacity(c.sections.len());
    for (i, s) in c.sections.iter().enumerate() {
        let (palette, data) = palettize(s, 4);
        let mut states = Compound::new();
        let palette = palette
            .iter()
            .map(|&id| {
                let (name, props) = names
                    .block_name(id)
                    .ok_or_else(|| format!("unknown block state {id}"))?;
                let mut e = Compound::new();
                e.insert("Name".into(), string(name));
                if !props.is_empty() {
                    let p = props
                        .iter()
                        .map(|(k, v)| (k.clone(), string(v)))
                        .collect::<BTreeMap<_, _>>();
                    e.insert("Properties".into(), Tag::Compound(p));
                }
                Ok(Tag::Compound(e))
            })
            .collect::<Result<Vec<_>, String>>()?;
        states.insert("palette".into(), Tag::List(palette));
        if let Some(d) = data {
            states.insert("data".into(), Tag::LongArray(d));
        }
        let biome = names
            .biome_name(s.biome)
            .ok_or_else(|| format!("unknown biome {}", s.biome))?;
        let mut biomes = Compound::new();
        biomes.insert("palette".into(), Tag::List(vec![string(biome)]));

        let mut sec = Compound::new();
        sec.insert("Y".into(), Tag::Byte((f.min_section + i as i32) as i8));
        sec.insert("block_states".into(), Tag::Compound(states));
        sec.insert("biomes".into(), Tag::Compound(biomes));
        // Небо пишем всегда (у ванилы отсутствие = «как секция выше»),
        // блочный — только ненулевой.
        let bytes = |l: &LightLayer| {
            let mut raw = Vec::with_capacity(2048);
            l.write_i8(&mut raw);
            Tag::ByteArray(raw)
        };
        if c.has_skylight {
            sec.insert("SkyLight".into(), bytes(&c.sky_light[i]));
        }
        if !c.block_light[i].is_zero() {
            sec.insert("BlockLight".into(), bytes(&c.block_light[i]));
        }
        sections.push(Tag::Compound(sec));
    }
    let mut root = Compound::new();
    root.insert("DataVersion".into(), Tag::Int(f.data_version));
    root.insert("xPos".into(), Tag::Int(x));
    root.insert("zPos".into(), Tag::Int(z));
    root.insert("yPos".into(), Tag::Int(f.min_section));
    root.insert("Status".into(), string("minecraft:full"));
    root.insert("LastUpdate".into(), Tag::Long(0));
    root.insert("InhabitedTime".into(), Tag::Long(0));
    root.insert("isLightOn".into(), Tag::Byte(1));
    root.insert("sections".into(), Tag::List(sections));
    let block_entities = c
        .block_entities
        .iter()
        .map(|(&[x, y, z], be)| {
            let mut t = be.nbt.clone();
            t.insert("id".into(), string(&be.id));
            t.insert("x".into(), Tag::Int(x));
            t.insert("y".into(), Tag::Int(y));
            t.insert("z".into(), Tag::Int(z));
            t.insert("keepPacked".into(), Tag::Byte(0));
            Tag::Compound(t)
        })
        .collect();
    root.insert("block_entities".into(), Tag::List(block_entities));
    Ok(root)
}

/// Чанк из NBT. Размеры и биты берутся из `template`; секции, которых нет в
/// файле, остаются пустыми (воздух), неизвестные блоки становятся воздухом.
/// Второе значение — `true`, если свет прочитан из файла; иначе его надо
/// пересчитать (`light::relight_column`).
pub fn chunk_from_nbt(
    root: &Compound,
    template: &ChunkColumn,
    f: Format,
    names: &dyn Names,
) -> Result<(ChunkColumn, bool), String> {
    let mut c = template.clone();
    let default_biome = template.sections.first().map_or(0, |s| s.biome);
    for s in &mut c.sections {
        *s = Section::filled(0, default_biome);
    }
    // Свет из файла только у чанков, сохранённых со светом (`isLightOn`).
    let mut lit = root.get("isLightOn").and_then(Tag::as_i64) == Some(1);
    let sky_default = LightLayer::Uniform(if c.has_skylight { 15 } else { 0 });
    let n = c.sections.len();
    c.sky_light = vec![sky_default; n];
    c.block_light = vec![LightLayer::Uniform(0); n];
    let layer = |sec: &Compound, key: &str| {
        let b = sec.get(key)?.as_byte_array()?;
        LightLayer::from_bytes(&b.iter().map(|&x| x as u8).collect::<Vec<_>>())
    };
    let sections = root
        .get("sections")
        .and_then(Tag::as_list)
        .ok_or("chunk without sections")?;
    for sec in sections.iter().filter_map(Tag::as_compound) {
        let y = sec
            .get("Y")
            .and_then(Tag::as_i64)
            .ok_or("section without Y")? as i32;
        let si = (y - f.min_section) as usize;
        let Some(slot) = c.sections.get_mut(si) else {
            continue; // секции света над/под миром
        };
        if let Some(l) = layer(sec, "BlockLight") {
            c.block_light[si] = l;
        }
        if c.has_skylight {
            match layer(sec, "SkyLight") {
                Some(l) => c.sky_light[si] = l,
                None => lit = false,
            }
        }
        let biome = sec
            .get("biomes")
            .and_then(Tag::as_compound)
            .and_then(|b| b.get("palette")?.as_list()?.first()?.as_str())
            .and_then(|n| names.biome_id(n))
            .unwrap_or(default_biome);
        let Some(states) = sec.get("block_states").and_then(Tag::as_compound) else {
            *slot = Section::filled(0, biome);
            continue;
        };
        let palette: Vec<u32> = states
            .get("palette")
            .and_then(Tag::as_list)
            .ok_or("block_states without palette")?
            .iter()
            .map(|e| {
                let e = e.as_compound()?;
                let name = e.get("Name")?.as_str()?;
                let props: Vec<(String, String)> = e
                    .get("Properties")
                    .and_then(Tag::as_compound)
                    .map(|p| {
                        p.iter()
                            .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned())))
                            .collect()
                    })
                    .unwrap_or_default();
                names.block_id(name, &props)
            })
            .map(|id| id.unwrap_or(0))
            .collect();
        *slot = match (
            palette.as_slice(),
            states.get("data").and_then(Tag::as_long_array),
        ) {
            ([], _) => Section::filled(0, biome),
            ([one], _) | ([one, ..], None) => Section::filled(*one, biome),
            (_, Some(data)) => {
                let bits = bits_for(palette.len()).max(4);
                Section::from_states(biome, |i| {
                    let id = unpack_at(data, bits, i) as usize;
                    palette.get(id).copied().unwrap_or(0)
                })
            }
        };
    }
    c.block_entities.clear();
    for be in root
        .get("block_entities")
        .and_then(Tag::as_list)
        .unwrap_or(&[])
        .iter()
        .filter_map(Tag::as_compound)
    {
        let int = |k: &str| be.get(k).and_then(Tag::as_i64).map(|v| v as i32);
        let (Some(id), Some(x), Some(y), Some(z)) = (
            be.get("id").and_then(Tag::as_str),
            int("x"),
            int("y"),
            int("z"),
        ) else {
            continue;
        };
        let mut nbt = be.clone();
        for k in ["id", "x", "y", "z", "keepPacked"] {
            nbt.remove(k);
        }
        c.block_entities.insert(
            [x, y, z],
            crate::BlockEntity {
                id: id.to_owned(),
                nbt,
            },
        );
    }
    Ok((c, lit))
}

const SECTOR: usize = 4096;
const ZLIB: u8 = 2;

/// Чанк с координатами и его NBT-корнем.
pub type StoredChunk = ((i32, i32), Compound);

/// Каталог `region/` одного измерения.
pub struct RegionStore {
    dir: PathBuf,
}

fn region_index((x, z): (i32, i32)) -> usize {
    ((x & 31) + (z & 31) * 32) as usize
}

impl RegionStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn path(&self, rx: i32, rz: i32) -> PathBuf {
        self.dir.join(format!("r.{rx}.{rz}.mca"))
    }

    /// Сжатые данные чанков региона по индексу (0..1024).
    fn read_region(path: &Path) -> Result<BTreeMap<usize, Vec<u8>>, String> {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
            Err(e) => return Err(format!("read {}: {e}", path.display())),
        };
        let mut out = BTreeMap::new();
        if bytes.len() < 2 * SECTOR {
            return Err(format!("{}: truncated region header", path.display()));
        }
        for i in 0..1024 {
            let loc = u32::from_be_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
            let (offset, count) = ((loc >> 8) as usize * SECTOR, (loc & 0xff) as usize);
            if offset == 0 && count == 0 {
                continue;
            }
            if offset < 2 * SECTOR || count == 0 || offset + 5 > bytes.len() {
                return Err(format!("{}: invalid chunk {i} location", path.display()));
            }
            let len = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
            let Some(payload) = bytes.get(offset + 4..offset + 4 + len) else {
                return Err(format!("{}: truncated chunk {i}", path.display()));
            };
            out.insert(i, payload.to_vec());
        }
        Ok(out)
    }

    /// Записывает чанки (NBT-корни), сохраняя остальные чанки их регионов.
    /// Файл пишется во временный и атомарно заменяет старый.
    pub fn save(&self, chunks: &[StoredChunk]) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir)
            .map_err(|e| format!("create {}: {e}", self.dir.display()))?;
        let mut by_region: BTreeMap<(i32, i32), Vec<(usize, &Compound)>> = BTreeMap::new();
        for (pos, nbt) in chunks {
            by_region
                .entry((pos.0 >> 5, pos.1 >> 5))
                .or_default()
                .push((region_index(*pos), nbt));
        }
        for ((rx, rz), list) in by_region {
            let path = self.path(rx, rz);
            let mut region = Self::read_region(&path)?;
            for (i, nbt) in list {
                let mut enc = ZlibEncoder::new(vec![ZLIB], Compression::default());
                enc.write_all(&nbt::write_root(nbt)).expect("write to Vec");
                region.insert(i, enc.finish().expect("write to Vec"));
            }
            let mut file = vec![0u8; 2 * SECTOR];
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as u32);
            for (i, payload) in &region {
                let sectors = (4 + payload.len()).div_ceil(SECTOR);
                if sectors > 255 {
                    return Err(format!("chunk {i} in {} is too large", path.display()));
                }
                let offset = file.len() / SECTOR;
                file[i * 4..i * 4 + 4]
                    .copy_from_slice(&(((offset as u32) << 8) | sectors as u32).to_be_bytes());
                file[SECTOR + i * 4..SECTOR + i * 4 + 4].copy_from_slice(&now.to_be_bytes());
                file.extend_from_slice(&(payload.len() as u32).to_be_bytes());
                file.extend_from_slice(payload);
                file.resize(file.len().div_ceil(SECTOR) * SECTOR, 0);
            }
            let tmp = path.with_extension("mca.tmp");
            std::fs::write(&tmp, &file).map_err(|e| format!("write {}: {e}", tmp.display()))?;
            std::fs::File::options()
                .write(true)
                .open(&tmp)
                .and_then(|f| f.sync_all())
                .map_err(|e| format!("sync {}: {e}", tmp.display()))?;
            std::fs::rename(&tmp, &path).map_err(|e| format!("rename {}: {e}", tmp.display()))?;
        }
        Ok(())
    }

    /// Все чанки одного региона как NBT-корни (нет файла — пусто).
    pub fn load_region(&self, rx: i32, rz: i32) -> Result<Vec<StoredChunk>, String> {
        let path = self.path(rx, rz);
        let name = path.display();
        let mut out = Vec::new();
        for (i, payload) in Self::read_region(&path)? {
            let pos = (rx * 32 + (i % 32) as i32, rz * 32 + (i / 32) as i32);
            let (kind, data) = payload.split_first().ok_or("empty chunk")?;
            if *kind != ZLIB {
                return Err(format!("{name}: chunk {pos:?} uses compression {kind}"));
            }
            let mut raw = Vec::new();
            ZlibDecoder::new(data)
                .read_to_end(&mut raw)
                .map_err(|e| format!("{name}: chunk {pos:?}: {e}"))?;
            out.push((
                pos,
                nbt::read_root(&raw).map_err(|e| format!("{name}: {e}"))?,
            ));
        }
        Ok(out)
    }

    /// Координаты существующих регионов.
    pub fn regions(&self) -> Result<Vec<(i32, i32)>, String> {
        let rd = match std::fs::read_dir(&self.dir) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(format!("read {}: {e}", self.dir.display())),
        };
        Ok(rd
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                let (a, b) = name
                    .strip_prefix("r.")?
                    .strip_suffix(".mca")?
                    .split_once('.')?;
                Some((a.parse().ok()?, b.parse().ok()?))
            })
            .collect())
    }

    /// Все чанки всех регионов как NBT-корни.
    pub fn load_all(&self) -> Result<Vec<StoredChunk>, String> {
        let mut out = Vec::new();
        for (rx, rz) in self.regions()? {
            out.extend(self.load_region(rx, rz)?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 0 = air, 1 = stone, 2/3 = grass_block snowy=false/true; биом 0 = plains.
    struct Test;
    impl Names for Test {
        fn block_name(&self, id: u32) -> Option<(&str, &[(String, String)])> {
            static SNOWY: std::sync::OnceLock<[Vec<(String, String)>; 2]> =
                std::sync::OnceLock::new();
            let p = SNOWY.get_or_init(|| {
                [
                    vec![("snowy".into(), "false".into())],
                    vec![("snowy".into(), "true".into())],
                ]
            });
            Some(match id {
                0 => ("minecraft:air", &[][..]),
                1 => ("minecraft:stone", &[][..]),
                2 => ("minecraft:grass_block", &p[0][..]),
                3 => ("minecraft:grass_block", &p[1][..]),
                _ => return None,
            })
        }
        fn block_id(&self, name: &str, props: &[(String, String)]) -> Option<u32> {
            (0..4).find(|&id| self.block_name(id) == Some((name, props)))
        }
        fn biome_name(&self, id: u32) -> Option<&str> {
            (id == 0).then_some("minecraft:plains")
        }
        fn biome_id(&self, name: &str) -> Option<u32> {
            (name == "minecraft:plains").then_some(0)
        }
    }

    const F: Format = Format {
        data_version: 5023,
        min_section: -4,
    };

    fn sample() -> ChunkColumn {
        let mut c = ChunkColumn::flat(&[(1, 1), (2, 2)], 24, 0, 2, 0, true);
        c.set_block(5, 3, 7, 3);
        c.set_block(0, 300, 15, 1);
        c
    }

    #[test]
    fn chunk_roundtrip() {
        let c = sample();
        let nbt = chunk_to_nbt(&c, (3, -2), F, &Test).unwrap();
        assert_eq!(nbt["yPos"], Tag::Int(-4));
        let (back, lit) = chunk_from_nbt(&nbt, &c, F, &Test).unwrap();
        assert_eq!(back.sections, c.sections);
        assert!(lit);
        assert_eq!(back.sky_light, c.sky_light);
        assert_eq!(back.block_light, c.block_light);
    }

    #[test]
    fn reused_slot_palette_is_first_seen() {
        let mut c = ChunkColumn::flat(&[], 1, 0, 2, 0, true);
        c.set_block(0, 0, 0, 1);
        c.set_block(1, 0, 0, 2);
        c.set_block(0, 0, 0, 3);
        c.set_block(2, 0, 0, 1);
        let nbt = chunk_to_nbt(&c, (0, 0), F, &Test).unwrap();
        let sec = nbt["sections"].as_list().unwrap()[0].as_compound().unwrap();
        let states = sec["block_states"].as_compound().unwrap();
        let ids: Vec<u32> = states["palette"]
            .as_list()
            .unwrap()
            .iter()
            .map(|e| {
                let e = e.as_compound().unwrap();
                let name = e["Name"].as_str().unwrap();
                let props: Vec<(String, String)> = e
                    .get("Properties")
                    .and_then(Tag::as_compound)
                    .map(|p| {
                        p.iter()
                            .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
                            .collect()
                    })
                    .unwrap_or_default();
                Test.block_id(name, &props).unwrap()
            })
            .collect();
        assert_eq!(ids, [3, 2, 1, 0]);
        let (back, _) = chunk_from_nbt(&nbt, &c, F, &Test).unwrap();
        assert_eq!(back.sections, c.sections);
    }

    #[test]
    fn region_roundtrip() {
        let dir = std::env::temp_dir().join(format!("chunkwake-anvil-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = RegionStore::new(&dir);
        let c = sample();
        let a = chunk_to_nbt(&c, (3, -2), F, &Test).unwrap();
        let b = chunk_to_nbt(&c, (40, 1), F, &Test).unwrap();
        store.save(&[((3, -2), a.clone())]).unwrap();
        // Второе сохранение в другой регион и повтор первого не теряют данные.
        store
            .save(&[((40, 1), b.clone()), ((3, -2), a.clone())])
            .unwrap();
        let mut all = store.load_all().unwrap();
        all.sort_by_key(|(p, _)| *p);
        assert_eq!(all, vec![((3, -2), a), ((40, 1), b)]);
        assert!(dir.join("r.0.-1.mca").is_file() && dir.join("r.1.0.mca").is_file());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn truncated_region_is_reported_instead_of_silently_dropping_chunks() {
        let dir =
            std::env::temp_dir().join(format!("chunkwake-anvil-corrupt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = RegionStore::new(&dir);
        std::fs::write(dir.join("r.0.0.mca"), [0u8; 12]).unwrap();
        assert!(
            store
                .load_region(0, 0)
                .unwrap_err()
                .contains("truncated region header")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
