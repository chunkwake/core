//! Мир на диске: `<level-name>/region/*.mca` (Anvil), загрузка при старте,
//! автосохранение и сохранение при остановке.

use crate::players::PlayerFiles;
use chunkwake_data::world::Blocks;
use chunkwake_net::Uuid;
use chunkwake_net::play::{ChunkSource, LoadedChunk, PlayPlan, PlayerSource};
use chunkwake_net::survival::PlayerSave;
use chunkwake_world::ChunkColumn;
use chunkwake_world::anvil::{self, Format, Names, RegionStore};
use chunkwake_world::light::{self, LightTable};
use chunkwake_world::nbt::{Compound, Tag};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

const AUTOSAVE: Duration = Duration::from_secs(30);
/// Чанков в одной записи: NBT пачки живёт в памяти целиком.
const SAVE_BATCH: usize = 128;
/// Как часто выгружать столбцы вдали от игроков.
const UNLOAD_EVERY: Duration = Duration::from_secs(5);

/// Каталог регионов измерения. Мир 26.3 держит чанки в
/// `dimensions/minecraft/<измерение>/region`; для оверворлда, если такого
/// каталога ещё нет, а старый `region/` есть — читаем старый, чтобы не
/// потерять мир.
fn region_dir(world_dir: &Path, dimension: &str) -> std::path::PathBuf {
    // В ванили `dimensions/minecraft/overworld/region`: двоеточие в ключе
    // становится разделителем каталогов.
    let path = dimension.replace(':', "/");
    let dimension_dir = world_dir.join(format!("dimensions/{path}/region"));
    if dimension_dir.is_dir() || dimension != "minecraft:overworld" {
        dimension_dir
    } else if world_dir.join("region").is_dir() {
        world_dir.join("region")
    } else {
        dimension_dir
    }
}

pub struct WorldStorage {
    /// Хранилище на измерение: в ванили у каждого `ServerLevel` своё.
    dimensions: std::sync::RwLock<std::collections::BTreeMap<String, DimStore>>,
    /// Шаблон и минимум по Y активного измерения (оверворлда при открытии).
    template: ChunkColumn,
    light: Arc<LightTable>,
    format: Format,
    names: StateNames,
    players: PlayerFiles,
    save_lock: Mutex<()>,
    save_notify: Arc<tokio::sync::Notify>,
    world_dir: std::path::PathBuf,
}

/// Хранилище одного измерения: чанки и сущности.
pub struct DimStore {
    store: RegionStore,
    entities: RegionStore,
    /// Шаблон столбца и формат измерения: у Ада 8 секций от Y 0, у Края 16.
    /// `None` — как у оверворлда.
    layout: Option<(ChunkColumn, Format)>,
}

struct StateNames {
    blocks: Blocks,
    biomes: Vec<String>,
    biome_ids: HashMap<String, u32>,
}

impl Names for StateNames {
    fn block_name(&self, id: u32) -> Option<(&str, &[(String, String)])> {
        self.blocks.state(id)
    }
    fn block_id(&self, name: &str, props: &[(String, String)]) -> Option<u32> {
        self.blocks.state_id(name, props)
    }
    fn block_default(&self, name: &str) -> Option<u32> {
        self.blocks.default_state(name).ok().map(|s| s as u32)
    }
    fn biome_name(&self, id: u32) -> Option<&str> {
        self.biomes.get(id as usize).map(String::as_str)
    }
    fn biome_id(&self, name: &str) -> Option<u32> {
        self.biome_ids.get(name).copied()
    }
}

impl WorldStorage {
    /// `world_dir` — каталог мира; `data_version` — `world_version` из version.json.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        world_dir: &Path,
        blocks: Blocks,
        biomes: Vec<String>,
        data_version: i32,
        min_y: i32,
        template: ChunkColumn,
        light: Arc<LightTable>,
        items: HashMap<String, i32>,
        effects: HashMap<String, i32>,
    ) -> Self {
        let biome_ids = biomes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.clone(), i as u32))
            .collect();
        // С 26.3 чанки лежат в `dimensions/minecraft/<измерение>/region`.
        // Старый `region/` читаем как запасной путь: миры до переезда живы.
        let region = region_dir(world_dir, "minecraft:overworld");
        let entities = region.with_file_name("entities");
        let mut dimensions = std::collections::BTreeMap::new();
        dimensions.insert(
            "minecraft:overworld".to_owned(),
            DimStore {
                store: RegionStore::new(region),
                entities: RegionStore::new(entities),
                layout: None,
            },
        );
        Self {
            dimensions: std::sync::RwLock::new(dimensions),
            players: PlayerFiles::new(world_dir, data_version, items, effects),
            save_lock: Mutex::new(()),
            save_notify: Arc::new(tokio::sync::Notify::new()),
            template,
            light,
            world_dir: world_dir.to_path_buf(),
            format: Format {
                data_version,
                min_section: min_y.div_euclid(16),
            },
            names: StateNames {
                blocks,
                biomes,
                biome_ids,
            },
        }
    }

    /// Включает измерение: создаёт каталоги регионов по требованию.
    ///
    /// В ванили `ServerLevel` создаётся сразу; здесь хранилище появляется при
    /// первом обращении — так мир без Ада не обрастает пустыми каталогами.
    pub fn enable_dimension(&self, dimension: &str) {
        let mut dims = self.dimensions.write().unwrap();
        dims.entry(dimension.to_owned()).or_insert_with(|| {
            let region = region_dir(&self.world_dir, dimension);
            DimStore {
                store: RegionStore::new(region.clone()),
                entities: RegionStore::new(region.with_file_name("entities")),
                layout: None,
            }
        });
    }

    /// Включает измерение со своим шаблоном столбца и нижней границей.
    pub fn enable_dimension_with(&self, dimension: &str, template: ChunkColumn, min_y: i32) {
        self.enable_dimension(dimension);
        let mut dims = self.dimensions.write().unwrap();
        if dimension != "minecraft:overworld"
            && let Some(dim) = dims.get_mut(dimension)
        {
            let format = Format {
                data_version: self.format.data_version,
                min_section: min_y.div_euclid(16),
            };
            dim.layout = Some((template, format));
        }
    }

    /// Шаблон и формат измерения.
    fn layout<'a>(&'a self, dim: &'a DimStore) -> (&'a ChunkColumn, Format) {
        match &dim.layout {
            Some((template, format)) => (template, *format),
            None => (&self.template, self.format),
        }
    }

    /// Хранилище измерения с загрузкой при необходимости.
    fn dim(
        &self,
        dimension: &str,
    ) -> std::sync::RwLockReadGuard<'_, std::collections::BTreeMap<String, DimStore>> {
        self.enable_dimension(dimension);
        self.dimensions.read().unwrap()
    }

    /// Дописывает `Heightmaps` в чанки мира, где раздела нет. Возвращает число
    /// обновлённых чанков.
    ///
    /// Зачем: раньше сервер не писал `Heightmaps` вообще, и внешние читатели
    /// мира (BlueMap, редакторы карт, ванильные инструменты) не находили
    /// поверхность — BlueMap из-за этого не рисовал чанк. Блоки не трогаем:
    /// чанк читается, heightmaps считаются по его секциям и пишутся рядом.
    ///
    /// Работает один раз: чанки, у которых раздел уже есть, пропускаются.
    pub fn backfill_heightmaps(&self) -> Result<usize, String> {
        let _guard = self.save_lock.lock().unwrap();
        let stored = self
            .dim("minecraft:overworld")
            .get("minecraft:overworld")
            .unwrap()
            .store
            .load_all()?;
        let mut updated = Vec::new();
        for (pos, root) in stored {
            // `Compound` — псевдоним `BTreeMap`, поэтому `contains_key` есть,
            // но clippy-совет про него здесь верен: раздел маленький.
            if root.contains_key("Heightmaps") {
                continue;
            }
            // Считаем по самому чанку: `chunk_from_nbt` даёт секции, а
            // `heightmaps_nbt` — готовый раздел из них.
            let Ok((column, _)) =
                anvil::chunk_from_nbt(&root, &self.template, self.format, &self.names)
            else {
                continue;
            };
            let mut fixed = root;
            fixed.insert("Heightmaps".into(), anvil::heightmaps_for_nbt(&column));
            updated.push((pos, fixed));
        }
        if updated.is_empty() {
            return Ok(0);
        }
        let n = updated.len();
        self.dim("minecraft:overworld")
            .get("minecraft:overworld")
            .unwrap()
            .store
            .save(&updated)?;
        Ok(n)
    }

    /// Записывает изменённые чанки; возвращает их число.
    pub fn save(&self, plan: &PlayPlan) -> Result<usize, String> {
        let _guard = self.save_lock.lock().unwrap();
        let mut errors = save_player_batch(
            plan.pending_player_saves(),
            |uuid, s| self.players.save(uuid, s),
            |uuid, s| plan.mark_player_saved(uuid, s),
        );
        if let Err(e) = plan.save_maps(self.format.data_version) {
            errors.push(format!("save maps: {e}"));
        }
        let mut saved = 0;
        for (dim, _, _) in plan.dimension_layouts() {
            let unsaved = plan.unsaved_in(dim);
            if unsaved.is_empty() {
                continue;
            }
            let dims = self.dim(dim.key());
            let Some(store) = dims.get(dim.key()) else {
                continue;
            };
            let (_, format) = self.layout(store);
            // Пачками: NBT-деревья всех несохранённых чанков разом — сотни
            // мегабайт пика после полёта, когда правлены тысячи столбцов.
            for batch in unsaved.chunks(SAVE_BATCH) {
                let nbt = batch
                    .iter()
                    .map(|(pos, c, _)| Ok((*pos, anvil::chunk_to_nbt(c, *pos, format, &self.names)?)))
                    .collect::<Result<Vec<_>, String>>();
                match nbt.and_then(|nbt| store.store.save(&nbt).map(|()| nbt.len())) {
                    Ok(n) => {
                        plan.mark_saved_in(dim, batch.iter().map(|(pos, _, rev)| (*pos, *rev)));
                        saved += n;
                    }
                    Err(e) => {
                        errors.push(format!("save chunks {}: {e}", dim.key()));
                        break;
                    }
                }
            }
        }
        let mobs = plan.unsaved_entities();
        if !mobs.is_empty() {
            let nbt: Vec<_> = mobs
                .iter()
                .map(|(pos, list)| {
                    let mut root = Compound::new();
                    root.insert("DataVersion".into(), Tag::Int(self.format.data_version));
                    root.insert("Position".into(), Tag::IntArray(vec![pos.0, pos.1]));
                    root.insert(
                        "Entities".into(),
                        Tag::List(list.iter().cloned().map(Tag::Compound).collect()),
                    );
                    (*pos, root)
                })
                .collect();
            match self
                .dim("minecraft:overworld")
                .get("minecraft:overworld")
                .unwrap()
                .entities
                .save(&nbt)
            {
                Ok(()) => plan.mark_entities_saved(&mobs),
                Err(e) => errors.push(format!("save entities: {e}")),
            }
        }
        if errors.is_empty() {
            Ok(saved)
        } else {
            Err(errors.join("\n"))
        }
    }

    /// Выгружает столбцы вдали от игроков, дописывая на диск те, которых
    /// там ещё нет. Под замком сейва: запись не перемешается с автосейвом.
    pub fn unload(&self, plan: &PlayPlan) -> usize {
        let _guard = self.save_lock.lock().unwrap();
        let mut total = 0;
        for (dim, _, _) in plan.dimension_layouts() {
            let dims = self.dim(dim.key());
            let Some(store) = dims.get(dim.key()) else {
                continue;
            };
            let (_, format) = self.layout(store);
            let write = |cols: &[chunkwake_net::play::UnloadWrite]| {
                for batch in cols.chunks(SAVE_BATCH) {
                    let nbt = batch
                        .iter()
                        .map(|(pos, c)| Ok((*pos, anvil::chunk_to_nbt(c, *pos, format, &self.names)?)))
                        .collect::<Result<Vec<_>, String>>()?;
                    store.store.save(&nbt)?;
                }
                Ok(())
            };
            total += plan.unload_far(dim, &write);
        }
        total
    }

    /// Выгрузка раз в несколько секунд, в блокирующем пуле.
    pub fn spawn_unloader(self: &Arc<Self>, plan: Arc<PlayPlan>) -> tokio::task::JoinHandle<()> {
        let storage = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(UNLOAD_EVERY);
            tick.tick().await;
            loop {
                tick.tick().await;
                let (storage, plan) = (storage.clone(), plan.clone());
                let started = std::time::Instant::now();
                match tokio::task::spawn_blocking(move || storage.unload(&plan)).await {
                    Ok(0) => {}
                    Ok(n) => tracing::debug!("unloaded {n} chunks ({} ms)", started.elapsed().as_millis()),
                    Err(e) => tracing::error!("unload task: {e}"),
                }
            }
        })
    }

    /// Автосохранение каждые 30 с в блокирующем пуле (сжатие и диск не держат сеть).
    pub fn spawn_autosave(self: &Arc<Self>, plan: Arc<PlayPlan>) -> tokio::task::JoinHandle<()> {
        let storage = self.clone();
        let notify = self.save_notify.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(AUTOSAVE);
            tick.tick().await;
            loop {
                tokio::select! {
                    _ = tick.tick() => {},
                    _ = notify.notified() => {},
                }
                // `World.save()` и `/save-all` просят запись через флаг:
                // без этого они ставили флаг, а читать его было некому.
                let requested = plan.take_save_request();
                let (storage, plan) = (storage.clone(), plan.clone());
                let started = std::time::Instant::now();
                match tokio::task::spawn_blocking(move || storage.save(&plan)).await {
                    Ok(Ok(0)) => {}
                    Ok(Ok(n)) if requested => {
                        tracing::info!(
                            "saved {n} chunks on request ({} ms)",
                            started.elapsed().as_millis()
                        )
                    }
                    Ok(Ok(n)) => tracing::info!("autosaved {n} chunks"),
                    Ok(Err(e)) => tracing::error!("autosave failed: {e}"),
                    Err(e) => tracing::error!("autosave task: {e}"),
                }
            }
        })
    }
}

fn save_player_batch(
    saves: Vec<(Uuid, PlayerSave)>,
    mut write: impl FnMut(Uuid, &PlayerSave) -> Result<(), String>,
    mut acknowledge: impl FnMut(Uuid, &PlayerSave),
) -> Vec<String> {
    let mut errors = Vec::new();
    for (uuid, save) in saves {
        match write(uuid, &save) {
            Ok(()) => acknowledge(uuid, &save),
            Err(error) => errors.push(format!("save player {uuid}: {error}")),
        }
    }
    errors
}

impl PlayerSource for WorldStorage {
    fn load_player(&self, uuid: Uuid) -> Option<PlayerSave> {
        self.players
            .load(uuid)
            .inspect_err(|e| tracing::error!("load player {uuid}: {e}"))
            .ok()
            .flatten()
    }

    fn schedule_save(&self) {
        self.save_notify.notify_one();
    }
}

impl ChunkSource for WorldStorage {
    fn load_region(&self, rx: i32, rz: i32) -> Result<Vec<LoadedChunk>, String> {
        self.load_region_in("minecraft:overworld", rx, rz)
    }

    fn load_region_in(
        &self,
        dimension: &str,
        rx: i32,
        rz: i32,
    ) -> Result<Vec<LoadedChunk>, String> {
        let dims = self.dim(dimension);
        let store = dims
            .get(dimension)
            .ok_or_else(|| format!("no dimension {dimension}"))?;
        let (template, format) = self.layout(store);
        let mut out: Vec<((i32, i32), ChunkColumn, bool)> = store
            .store
            .load_region(rx, rz)?
            .iter()
            .map(|(pos, nbt)| {
                anvil::chunk_from_nbt(nbt, template, format, &self.names)
                    .map(|(c, lit)| (*pos, c, lit))
                    .map_err(|e| format!("chunk {pos:?}: {e}"))
            })
            .collect::<Result<_, _>>()?;
        // Чанк без света в файле (сохранён до его появления, `isLightOn=0`)
        // нельзя освещать поодиночке: `relight_column` не видит соседей и
        // оставляет тёмные грани. Пока такие есть, светим весь регион сразу —
        // `spread_wave` для них и пролив от соседей для остальных.
        let dark: Vec<(i32, i32)> = out
            .iter()
            .filter(|(_, _, lit)| !lit)
            .map(|(pos, _, _)| *pos)
            .collect();
        if !dark.is_empty() {
            let mut columns: HashMap<(i32, i32), ChunkColumn> =
                out.drain(..).map(|(pos, c, _)| (pos, c)).collect();
            let origins: std::collections::HashSet<(i32, i32)> = dark.iter().copied().collect();
            light::spread_wave(&mut columns, &origins, &self.light);
            // Соседи, у которых свет был, тоже получили пролив от тёмных —
            // их столбцы в `columns` уже обновлены.
            out = columns.into_iter().map(|(pos, c)| (pos, c, true)).collect();
        }
        Ok(out.into_iter().map(|(pos, c, _)| (pos, c)).collect())
    }

    fn load_entities(
        &self,
        rx: i32,
        rz: i32,
    ) -> Result<Vec<chunkwake_net::play::StoredEntities>, String> {
        self.load_entities_in("minecraft:overworld", rx, rz)
    }

    fn load_entities_in(
        &self,
        dimension: &str,
        rx: i32,
        rz: i32,
    ) -> Result<Vec<chunkwake_net::play::StoredEntities>, String> {
        self.dim(dimension)
            .get(dimension)
            .ok_or_else(|| format!("no dimension {dimension}"))?
            .entities
            .load_region(rx, rz)
    }
}

#[cfg(test)]
mod tests {
    use super::{region_dir, save_player_batch};
    use chunkwake_net::Uuid;
    use chunkwake_net::survival::PlayerState;
    use std::collections::HashMap;

    /// Мир 26.3 держит чанки в `dimensions/minecraft/<измерение>/region`;
    /// старый `region/` берётся только когда нового каталога ещё нет.
    #[test]
    fn region_lives_in_dimension_folder() {
        let base = std::env::temp_dir().join(format!("chunkwake-region-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("dimensions/minecraft/overworld/region")).unwrap();
        let fresh = region_dir(&base, "minecraft:overworld");
        assert!(
            fresh.ends_with("dimensions/minecraft/overworld/region"),
            "{fresh:?}"
        );

        // Мир до переезда: только `region/`, его и берём.
        let legacy = base.join("legacy");
        std::fs::create_dir_all(legacy.join("region")).unwrap();
        let old = region_dir(&legacy, "minecraft:overworld");
        assert!(old.ends_with("region") && !old.to_string_lossy().contains("dimensions"));

        // Пустой мир: пишем сразу в ванильный путь.
        let empty = base.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        let next = region_dir(&empty, "minecraft:overworld");
        assert!(
            next.ends_with("dimensions/minecraft/overworld/region"),
            "{next:?}"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn failed_player_write_is_reported_and_retried() {
        let uuid = Uuid::nil();
        let save = PlayerState::default().save([1.0, 64.0, 1.0], 0.0, 0.0);
        let mut pending = HashMap::from([(uuid, save.clone())]);
        let errors = save_player_batch(
            pending
                .iter()
                .map(|(id, save)| (*id, save.clone()))
                .collect(),
            |_, _| Err("disk full".into()),
            |id, _| {
                pending.remove(&id);
            },
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("disk full"));
        assert_eq!(pending.get(&uuid), Some(&save));
        let errors = save_player_batch(
            pending
                .iter()
                .map(|(id, save)| (*id, save.clone()))
                .collect(),
            |_, _| Ok(()),
            |id, _| {
                pending.remove(&id);
            },
        );
        assert!(errors.is_empty());
        assert!(pending.is_empty());
    }
}
