//! Хранение мира в формате Anvil (`region/r.X.Z.mca`), совместимом с ванилой:
//! чанк — NBT-compound с секциями `block_states`/`biomes` (палитра + long-массив).

use crate::light::LightLayer;
use crate::nbt::{self, Compound, Tag};
use crate::{Biomes, ChunkColumn, Section, bits_for, emit_packed, pack};
use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// Имена состояний блоков и биомов ↔ их ID (сетевые/внутренние).
pub trait Names {
    fn block_name(&self, id: u32) -> Option<(&str, &[(String, String)])>;
    fn block_id(&self, name: &str, props: &[(String, String)]) -> Option<u32>;
    /// Состояние по умолчанию: нужно, когда в файле нет свойств блока.
    fn block_default(&self, name: &str) -> Option<u32>;
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
    // Палитра и индексы — одним разбором секции, без поиска по состояниям.
    let (palette, indices) = s.first_seen_pack();
    if palette.len() == 1 {
        return (palette, None);
    }
    let bits = bits_for(palette.len()).max(min_bits);
    let mut data = Vec::with_capacity(4096usize.div_ceil((64 / bits) as usize));
    emit_packed(4096, bits, |i| indices[i], |long| data.push(long as i64));
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
        // NBT-список однотипен: либо все записи строки (ни у одного блока нет
        // свойств), либо все компаунды `{"id", "properties"}`.
        let any_props = palette
            .iter()
            .any(|&id| names.block_name(id).is_some_and(|(_, p)| !p.is_empty()));
        let palette = palette
            .iter()
            .map(|&id| {
                let (name, props) = names
                    .block_name(id)
                    .ok_or_else(|| format!("unknown block state {id}"))?;
                if !any_props {
                    return Ok(Tag::String(name.to_owned()));
                }
                let mut e = Compound::new();
                e.insert("id".into(), string(name));
                let p = props
                    .iter()
                    .map(|(k, v)| (k.clone(), string(v)))
                    .collect::<BTreeMap<_, _>>();
                e.insert("properties".into(), Tag::Compound(p));
                Ok(Tag::Compound(e))
            })
            .collect::<Result<Vec<_>, String>>()?;
        states.insert("palette".into(), Tag::List(palette));
        if let Some(d) = data {
            states.insert("data".into(), Tag::LongArray(d));
        }
        let cells = s.biome.cells();
        let mut bpalette: Vec<&str> = Vec::new();
        let mut bidx = [0u32; 64];
        for (i, &c) in cells.iter().enumerate() {
            let name = names
                .biome_name(c)
                .ok_or_else(|| format!("unknown biome {c}"))?;
            let slot = match bpalette.iter().position(|&p| p == name) {
                Some(slot) => slot,
                None => {
                    bpalette.push(name);
                    bpalette.len() - 1
                }
            };
            bidx[i] = slot as u32;
        }
        let mut biomes = Compound::new();
        biomes.insert(
            "palette".into(),
            Tag::List(bpalette.iter().map(|n| string(n)).collect()),
        );
        if bpalette.len() > 1 {
            let bits = bits_for(bpalette.len()).max(1);
            biomes.insert(
                "data".into(),
                Tag::LongArray(pack(&bidx, bits).into_iter().map(|v| v as i64).collect()),
            );
        }

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

/// Биомы секции из `biomes`: палитра с `data` или один биом без данных.
/// Неизвестные имена — `default`, как и раньше с единственным биомом.
fn biomes_from_nbt(sec: &Compound, names: &dyn Names, default: u32) -> [u32; 64] {
    let mut out = [default; 64];
    let Some(b) = sec.get("biomes").and_then(Tag::as_compound) else {
        return out;
    };
    let Some(palette) = b.get("palette").and_then(Tag::as_list) else {
        return out;
    };
    let ids: Vec<u32> = palette
        .iter()
        .map(|e| {
            e.as_str()
                .and_then(|n| names.biome_id(n))
                .unwrap_or(default)
        })
        .collect();
    let Some(first) = ids.first().copied() else {
        return out;
    };
    out.fill(first);
    if ids.len() == 1 {
        return out;
    }
    let Some(data) = b.get("data").and_then(Tag::as_long_array) else {
        return out;
    };
    let bits = bits_for(ids.len()).max(1);
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = ids
            .get(unpack_at(data, bits, i) as usize)
            .copied()
            .unwrap_or(default);
    }
    out
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
    let default_biome = template
        .sections
        .first()
        .map_or(0, |s| s.biome.uniform().unwrap_or(0));
    // Столбец собирается заново, а не клонируется из шаблона: клон скопировал
    // бы все секции (по `Arc` на каждую) и тут же заменил их пустыми. От
    // шаблона нужны только размеры и флаги.
    let n = template.sections.len();
    let has_skylight = template.has_skylight;
    let mut c = ChunkColumn {
        sections: vec![Section::filled(0, default_biome); n],
        global_block_bits: template.global_block_bits,
        global_biome_bits: template.global_biome_bits,
        has_skylight,
        sky_light: vec![LightLayer::Uniform(if has_skylight { 15 } else { 0 }); n],
        block_light: vec![LightLayer::Uniform(0); n],
        block_entities: std::collections::BTreeMap::new(),
    };
    // Свет из файла только у чанков, сохранённых со светом (`isLightOn`).
    let mut lit = root.get("isLightOn").and_then(Tag::as_i64) == Some(1);
    let layer = |sec: &Compound, key: &str| {
        let b = sec.get(key)?.as_byte_array()?;
        LightLayer::from_i8(b)
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
        let biome = biomes_from_nbt(sec, names, default_biome);
        let Some(states) = sec.get("block_states").and_then(Tag::as_compound) else {
            *slot = Section::with_biomes(0, &biome);
            continue;
        };
        let palette: Vec<u32> = states
            .get("palette")
            .and_then(Tag::as_list)
            .ok_or("block_states without palette")?
            .iter()
            .map(|e| -> Option<u32> {
                // 26.3: без свойств — просто строка `"minecraft:stone"`,
                // со свойствами — `{"id": "…", "properties": {…}}`
                // (старый `Name`/`Properties` тоже читаем).
                let (name, props): (&str, Vec<(String, String)>) = match e {
                    Tag::String(s) => (s.as_str(), Vec::new()),
                    Tag::Compound(c) => {
                        let name = c
                            .get("")
                            .and_then(Tag::as_str)
                            .or_else(|| c.get("id").and_then(Tag::as_str))
                            .or_else(|| c.get("Name").and_then(Tag::as_str))?;
                        let props = c
                            .get("properties")
                            .or_else(|| c.get("Properties"))
                            .and_then(Tag::as_compound)
                            .map(|p| {
                                p.iter()
                                    .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned())))
                                    .collect()
                            })
                            .unwrap_or_default();
                        (name, props)
                    }
                    _ => return None,
                };
                match names.block_id(name, &props) {
                    Some(id) => Some(id),
                    // В файле нет свойств, а у блока они есть (`deepslate`):
                    // берём состояние по умолчанию, иначе блок станет воздухом.
                    None if props.is_empty() => names.block_default(name),
                    None => None,
                }
            })
            .map(|id| id.unwrap_or(0))
            .collect();

        *slot = match (
            palette.as_slice(),
            states.get("data").and_then(Tag::as_long_array),
        ) {
            ([], _) => Section::with_biomes(0, &biome),
            ([one], _) | ([one, ..], None) => Section::with_biomes(*one, &biome),
            (_, Some(data)) => {
                let bits = bits_for(palette.len()).max(4);
                let mut s = Section::from_states(biome[0], |i| {
                    let id = unpack_at(data, bits, i) as usize;
                    palette.get(id).copied().unwrap_or(0)
                });
                s.biome = Biomes::from_cells(&biome);
                s
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

/// Сколько потоков отдать сжатию: чанки независимы, но на маленьком
/// автосейве лишние потоки дороже работы.
fn pack_threads(chunks: usize) -> usize {
    let cores = std::thread::available_parallelism().map_or(1, |p| p.get());
    cores.min(chunks.max(1)).min(8)
}

/// NBT + zlib для списка чанков; порядок сохраняется.
///
/// На одном чанке потоки не заводятся: накладные расходы на пробуждение
/// больше самой работы, а автосейв изредка пишет пару блоков.
pub fn compress_chunks(list: &[(usize, &Compound)]) -> Vec<(usize, Vec<u8>)> {
    let encode = |nbt: &Compound| {
        // Автосейв каждые 30 с: быстрый уровень в 2–3 раза дешевле, файл ~10% больше.
        let mut enc = ZlibEncoder::new(vec![ZLIB], Compression::fast());
        enc.write_all(&nbt::write_root(nbt)).expect("write to Vec");
        enc.finish().expect("write to Vec")
    };
    let threads = pack_threads(list.len());
    if threads <= 1 {
        return list.iter().map(|(i, nbt)| (*i, encode(nbt))).collect();
    }
    // Пакеты по `threads`, результат собирается в исходном порядке: индекс
    // чанка в регионе важен, и перемешивание сломало бы файл.
    let batch = list.len().div_ceil(threads);
    let parts: Vec<Vec<(usize, Vec<u8>)>> = std::thread::scope(|scope| {
        let mut joins = Vec::with_capacity(threads);
        for part in list.chunks(batch) {
            joins
                .push(scope.spawn(move || part.iter().map(|(i, nbt)| (*i, encode(nbt))).collect()));
        }
        joins
            .into_iter()
            .map(|j| j.join().expect("chunk compression"))
            .collect()
    });
    parts.into_iter().flatten().collect()
}

/// Распаковка и разбор чанков региона по потокам; порядок входного списка
/// сохраняется.
///
/// Работа на чанк независима (свой zlib-поток, свой NBT), поэтому она
/// раскладывается по потокам так же, как сжатие при записи. Ошибки
/// возвращаются как есть — их соберёт вызывающий и назовёт первый.
fn decompress_parallel<T, E>(
    slices: Vec<(usize, std::ops::Range<usize>)>,
    decode: impl Fn((usize, std::ops::Range<usize>)) -> Result<T, E> + Sync,
) -> Vec<(usize, Result<T, E>)>
where
    T: Send,
    E: Send,
{
    // Потолок ниже, чем у сжатия: загрузку региона зовут из пула генерации,
    // где потоков уже почти столько же, сколько ядер. Вложенный `scope` на
    // восемь потоков там только отнимал бы ядра у тика.
    let threads = pack_threads(slices.len()).min(4);
    if threads <= 1 {
        return slices.into_iter().map(|s| (s.0, decode(s))).collect();
    }
    let batch = slices.len().div_ceil(threads);
    let parts: Vec<Vec<(usize, Result<T, E>)>> = std::thread::scope(|scope| {
        let mut joins = Vec::with_capacity(threads);
        for part in slices.chunks(batch) {
            let decode = &decode;
            joins.push(scope.spawn(move || {
                part.iter()
                    .map(|s| (s.0, decode((s.0, s.1.clone()))))
                    .collect()
            }));
        }
        joins
            .into_iter()
            .map(|j| j.join().expect("chunk decompression"))
            .collect()
    });
    parts.into_iter().flatten().collect()
}

/// Сжатие пачки чанков без записи в файл — для бенчмарка и профиля.
pub fn compress_for_bench(chunks: &[Compound]) -> usize {
    let list: Vec<(usize, &Compound)> = chunks.iter().enumerate().collect();
    compress_chunks(&list).iter().map(|(_, v)| v.len()).sum()
}

/// То же в одном потоке: с ним сравнивается параллельный вариант.
pub fn compress_for_bench_sequential(chunks: &[Compound]) -> usize {
    chunks
        .iter()
        .map(|nbt| {
            let mut enc = ZlibEncoder::new(vec![ZLIB], Compression::fast());
            enc.write_all(&nbt::write_root(nbt)).expect("write to Vec");
            enc.finish().expect("write to Vec").len()
        })
        .sum()
}

/// Чанк с координатами и его NBT-корнем.
pub type StoredChunk = ((i32, i32), Compound);

/// Почему запись по месту не удалась.
enum InPlace {
    /// Нет файла или payload не влез в свои сектора — нужна пересборка.
    NeedRebuild,
    /// Настоящая ошибка ввода-вывода.
    Failed(String),
}

/// Каталог `region/` одного измерения.
pub struct RegionStore {
    dir: PathBuf,
}

fn region_index((x, z): (i32, i32)) -> usize {
    ((x & 31) + (z & 31) * 32) as usize
}

/// Файл региона и срезы payload'ов чанков в нём.
type RegionPayloads = (Vec<u8>, Vec<(usize, std::ops::Range<usize>)>);

impl RegionStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn path(&self, rx: i32, rz: i32) -> PathBuf {
        self.dir.join(format!("r.{rx}.{rz}.mca"))
    }

    /// Сжатые данные чанков региона по индексу (0..1024), каждый — своей копией.
    ///
    /// Для пересборки файла: там payload'ы смешиваются с новыми и живут дольше
    /// одного прохода. Горячий путь чтения — [`Self::read_region_slices`].
    fn read_region(path: &Path) -> Result<BTreeMap<usize, Vec<u8>>, String> {
        let (bytes, slices) = Self::read_region_slices(path)?;
        Ok(slices
            .into_iter()
            .map(|(i, r)| (i, bytes[r].to_vec()))
            .collect())
    }

    /// Сжатые payload'ы чанков региона: общий буфер файла и срезы в нём.
    ///
    /// Раньше каждый чанк копировался в свой `Vec` — на регион 32×32 это сотни
    /// килобайт лишних копий и столько же аллокаций, хотя распаковка читает
    /// payload один раз и сразу выбрасывает.
    fn read_region_slices(path: &Path) -> Result<RegionPayloads, String> {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok((Vec::new(), Vec::new()));
            }
            Err(e) => return Err(format!("read {}: {e}", path.display())),
        };
        let mut out = Vec::new();
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
            let start = offset + 4;
            let end = start + len;
            if end > bytes.len() {
                return Err(format!("{}: truncated chunk {i}", path.display()));
            }
            out.push((i, start..end));
        }
        Ok((bytes, out))
    }

    /// Записывает чанки (NBT-корни), сохраняя остальные чанки их регионов.
    ///
    /// Изменённые чанки пишутся **по своим секторам** в существующий файл:
    /// раньше один изменённый блок читал весь регион, собирал его заново в
    /// новый буфер и переписывал целиком — три полные копии региона на
    /// автосейв. Пересборка файла нужна лишь когда payload не влезает в
    /// выделенные ему сектора.
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
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as u32);
        let mut touched: Vec<PathBuf> = Vec::new();
        for ((rx, rz), list) in by_region {
            let path = self.path(rx, rz);
            // NBT + zlib по чанкам независимы: раскладываем их по потокам,
            // а файл пишем по-прежнему последовательно. Автосейв — самое
            // дорогое после тика, и на одном потоке он занимал одно ядро
            // впустую.
            let fresh: Vec<(usize, Vec<u8>)> = compress_chunks(&list);
            match self.write_in_place(&path, &fresh, now) {
                Ok(()) => {}
                // Не влезло в свои сектора — тогда пересборка файла.
                Err(InPlace::NeedRebuild) => self.rebuild_region(&path, &fresh, now)?,
                Err(InPlace::Failed(e)) => return Err(e),
            }
            touched.push(path);
        }
        // Один `fsync` на всю пачку: регионы пишутся в общий проход.
        for path in touched {
            let file = std::fs::File::options()
                .write(true)
                .open(&path)
                .map_err(|e| format!("open {}: {e}", path.display()))?;
            file.sync_all()
                .map_err(|e| format!("sync {}: {e}", path.display()))?;
        }
        Ok(())
    }

    /// Пишет изменённые чанки прямо в их сектора. `Err(NeedRebuild)` — если
    /// для какого-то чанка места не хватило или файла ещё нет.
    fn write_in_place(
        &self,
        path: &Path,
        fresh: &[(usize, Vec<u8>)],
        now: u32,
    ) -> Result<(), InPlace> {
        let mut file = match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
        {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(InPlace::NeedRebuild),
            Err(e) => return Err(InPlace::Failed(format!("open {}: {e}", path.display()))),
        };
        let len = file
            .metadata()
            .map_err(|e| InPlace::Failed(format!("stat {}: {e}", path.display())))?
            .len();
        if len < 2 * SECTOR as u64 {
            return Err(InPlace::NeedRebuild);
        }
        let mut header = [0u8; 2 * SECTOR];
        file.read_exact(&mut header)
            .map_err(|e| InPlace::Failed(format!("read {}: {e}", path.display())))?;
        // Куда писать конец файла: за последним занятым сектором.
        let mut end = 2 * SECTOR as u64;
        for i in 0..1024 {
            let loc = u32::from_be_bytes(header[i * 4..i * 4 + 4].try_into().unwrap());
            if loc == 0 {
                continue;
            }
            let offset = (loc >> 8) as u64 * SECTOR as u64;
            let count = (loc & 0xff) as u64;
            end = end.max(offset + count * SECTOR as u64);
        }
        for (i, payload) in fresh {
            let need = (4 + payload.len()).div_ceil(SECTOR);
            if need > 255 {
                return Err(InPlace::Failed(format!(
                    "chunk {i} in {} is too large",
                    path.display()
                )));
            }
            let loc = u32::from_be_bytes(header[i * 4..i * 4 + 4].try_into().unwrap());
            let (offset, count) = ((loc >> 8) as u64 * SECTOR as u64, (loc & 0xff) as u64);
            let (at, sectors) = if offset != 0 && need as u64 <= count {
                // Место есть: пишем на старое место, хвост секторов оставляем.
                (offset, count)
            } else {
                // Места нет: берём сектора в конце файла.
                let at = end;
                end += need as u64 * SECTOR as u64;
                (at, need as u64)
            };
            let mut buf = Vec::with_capacity(4 + payload.len());
            buf.extend_from_slice(&(payload.len() as u32).to_be_bytes());
            buf.extend_from_slice(payload);
            buf.resize(need * SECTOR, 0);
            file.seek(SeekFrom::Start(at))
                .and_then(|_| file.write_all(&buf))
                .map_err(|e| InPlace::Failed(format!("write {}: {e}", path.display())))?;
            // Заголовок: где лежит чанк и сколько секторов ему выделено.
            // На старом месте остаётся исходное `count` (место уже занято),
            // на новом — ровно `need`.
            let loc = ((at / SECTOR as u64) as u32) << 8 | sectors as u32;
            file.seek(SeekFrom::Start(*i as u64 * 4))
                .and_then(|_| file.write_all(&loc.to_be_bytes()))
                .map_err(|e| InPlace::Failed(format!("header {}: {e}", path.display())))?;
            file.seek(SeekFrom::Start(SECTOR as u64 + *i as u64 * 4))
                .and_then(|_| file.write_all(&now.to_be_bytes()))
                .map_err(|e| InPlace::Failed(format!("timestamp {}: {e}", path.display())))?;
        }
        Ok(())
    }

    /// Пересобирает регион целиком: нужен только когда payload не влез в свои
    /// сектора. Файл пишется во временный и атомарно заменяет старый.
    fn rebuild_region(
        &self,
        path: &Path,
        fresh: &[(usize, Vec<u8>)],
        now: u32,
    ) -> Result<(), String> {
        let mut region = Self::read_region(path)?;
        for (i, payload) in fresh {
            region.insert(*i, payload.clone());
        }
        let mut file = vec![0u8; 2 * SECTOR];
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
        std::fs::rename(&tmp, path).map_err(|e| format!("rename {}: {e}", tmp.display()))?;
        Ok(())
    }

    /// Все чанки одного региона как NBT-корни (нет файла — пусто).
    ///
    /// Чанки распаковываются и разбираются параллельно: zlib и NBT по чанкам
    /// независимы, а порядок восстанавливается по позиции. Раньше регион
    /// читался строго по одному чанку в одном потоке, и вход игрока в
    /// исследованный мир ждал весь регион.
    pub fn load_region(&self, rx: i32, rz: i32) -> Result<Vec<StoredChunk>, String> {
        let path = self.path(rx, rz);
        let (bytes, slices) = Self::read_region_slices(&path)?;
        if slices.is_empty() {
            return Ok(Vec::new());
        }
        let name = path.display().to_string();
        let decode = |(i, range): (usize, std::ops::Range<usize>)| {
            let payload = &bytes[range];
            let (kind, data) = payload.split_first().ok_or("empty chunk")?;
            if *kind != ZLIB {
                return Err(format!("{name}: chunk {i} uses compression {kind}"));
            }
            // Распакованный NBT растёт реаллокациями, если не задать ёмкость:
            // zlib-поток знает размер только в конце, но запас в четверть
            // сжатого payload покрывает обычный коэффициент входа чанка.
            let mut raw = Vec::with_capacity(data.len().saturating_mul(4).max(4096));
            ZlibDecoder::new(data)
                .read_to_end(&mut raw)
                .map_err(|e| format!("{name}: chunk {i}: {e}"))?;
            nbt::read_root(&raw).map_err(|e| format!("{name}: {e}"))
        };
        let parsed = decompress_parallel(slices, decode);
        let mut out = Vec::with_capacity(parsed.len());
        for (i, root) in parsed {
            let root = root?;
            let pos = (rx * 32 + (i % 32) as i32, rz * 32 + (i / 32) as i32);
            out.push((pos, root));
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
        fn block_default(&self, name: &str) -> Option<u32> {
            self.block_id(name, &[])
        }
        fn biome_name(&self, id: u32) -> Option<&str> {
            match id {
                0 => Some("minecraft:plains"),
                1 => Some("minecraft:desert"),
                2 => Some("minecraft:forest"),
                _ => None,
            }
        }
        fn biome_id(&self, name: &str) -> Option<u32> {
            match name {
                "minecraft:plains" => Some(0),
                "minecraft:desert" => Some(1),
                "minecraft:forest" => Some(2),
                _ => None,
            }
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
    fn mixed_biomes_roundtrip_through_anvil() {
        let mut c = ChunkColumn::flat(&[], 1, 1, 2, 0, true);
        let mut cells = vec![1u32; 64];
        cells[3] = 2;
        c.sections[0].biome = Biomes::from_cells(&cells);
        let nbt = chunk_to_nbt(&c, (0, 0), F, &Test).unwrap();
        let sec = nbt["sections"].as_list().unwrap()[0].as_compound().unwrap();
        let biomes = sec["biomes"].as_compound().unwrap();
        assert_eq!(biomes["palette"].as_list().unwrap().len(), 2);
        assert!(biomes.get("data").is_some(), "palette of 2 needs data");
        let (back, _) = chunk_from_nbt(&nbt, &c, F, &Test).unwrap();
        assert_eq!(back.sections[0].biome, c.sections[0].biome);
        assert_eq!(back.sections[0].biome_at(3, 0, 0), 2);
        assert_eq!(back.sections[0].biome_at(0, 0, 0), 1);
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
                // 26.3: имя без свойств лежит под пустым ключом.
                let name = e
                    .get("")
                    .and_then(Tag::as_str)
                    .or_else(|| e.get("id").and_then(Tag::as_str))
                    .unwrap();
                let props: Vec<(String, String)> = e
                    .get("properties")
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
    fn in_place_save_keeps_other_chunks() {
        // Правка одного чанка не должна терять соседей и должна обновлять
        // именно его: это путь `write_in_place`, а не пересборки региона.
        let dir =
            std::env::temp_dir().join(format!("chunkwake-anvil-inplace-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = RegionStore::new(&dir);
        let c = sample();
        let a = chunk_to_nbt(&c, (0, 0), F, &Test).unwrap();
        let b = chunk_to_nbt(&c, (1, 0), F, &Test).unwrap();
        store
            .save(&[((0, 0), a.clone()), ((1, 0), b.clone())])
            .unwrap();
        // Меняем первый чанк и сохраняем только его.
        let mut changed = a.clone();
        changed.insert("LastUpdate".into(), Tag::Long(777));
        store.save(&[((0, 0), changed.clone())]).unwrap();
        let loaded = store.load_region(0, 0).unwrap();
        let by_pos: BTreeMap<_, _> = loaded.into_iter().collect();
        assert_eq!(by_pos.get(&(1, 0)), Some(&b), "сосед не должен пострадать");
        let got = by_pos.get(&(0, 0)).expect("изменённый чанк на месте");
        assert_eq!(got.get("LastUpdate").and_then(Tag::as_i64), Some(777));
        std::fs::remove_dir_all(&dir).unwrap();
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
