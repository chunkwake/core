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
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

const AUTOSAVE: Duration = Duration::from_secs(30);

pub struct WorldStorage {
    store: RegionStore,
    template: ChunkColumn,
    light: Arc<LightTable>,
    format: Format,
    names: StateNames,
    players: PlayerFiles,
    save_lock: Mutex<()>,
    save_notify: Arc<tokio::sync::Notify>,
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
        Self {
            store: RegionStore::new(world_dir.join("region")),
            players: PlayerFiles::new(world_dir, data_version, items, effects),
            save_lock: Mutex::new(()),
            save_notify: Arc::new(tokio::sync::Notify::new()),
            template,
            light,
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

    /// Записывает изменённые чанки; возвращает их число.
    pub fn save(&self, plan: &PlayPlan) -> Result<usize, String> {
        let _guard = self.save_lock.lock().unwrap();
        let mut errors = save_player_batch(
            plan.pending_player_saves(),
            |uuid, s| self.players.save(uuid, s),
            |uuid, s| plan.mark_player_saved(uuid, s),
        );
        let unsaved = plan.unsaved();
        let mut saved = 0;
        if !unsaved.is_empty() {
            let nbt = unsaved
                .iter()
                .map(|(pos, c, _)| {
                    Ok((
                        *pos,
                        anvil::chunk_to_nbt(c, *pos, self.format, &self.names)?,
                    ))
                })
                .collect::<Result<Vec<_>, String>>();
            match nbt.and_then(|nbt| self.store.save(&nbt).map(|()| nbt.len())) {
                Ok(n) => {
                    plan.mark_saved(unsaved.iter().map(|(pos, _, rev)| (*pos, *rev)));
                    saved = n;
                }
                Err(e) => errors.push(format!("save chunks: {e}")),
            }
        }
        if errors.is_empty() {
            Ok(saved)
        } else {
            Err(errors.join("\n"))
        }
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
                let (storage, plan) = (storage.clone(), plan.clone());
                match tokio::task::spawn_blocking(move || storage.save(&plan)).await {
                    Ok(Ok(0)) => {}
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
        self.store
            .load_region(rx, rz)?
            .iter()
            .map(|(pos, nbt)| {
                // Чанк без света (сохранён до его появления) освещаем сами.
                anvil::chunk_from_nbt(nbt, &self.template, self.format, &self.names)
                    .map(|(mut c, lit)| {
                        if !lit {
                            light::relight_column(&mut c, &self.light);
                        }
                        (*pos, c)
                    })
                    .map_err(|e| format!("chunk {pos:?}: {e}"))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::save_player_batch;
    use chunkwake_net::Uuid;
    use chunkwake_net::survival::PlayerState;
    use std::collections::HashMap;

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
