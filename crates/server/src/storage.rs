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
use std::time::Duration;

const AUTOSAVE: Duration = Duration::from_secs(30);

pub struct WorldStorage {
    store: RegionStore,
    template: ChunkColumn,
    light: Arc<LightTable>,
    format: Format,
    names: StateNames,
    players: PlayerFiles,
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
        // Игроки: ошибка одного не мешает остальным и чанкам.
        for (uuid, s) in plan.take_player_saves() {
            if let Err(e) = self.players.save(uuid, &s) {
                tracing::error!("save player {uuid}: {e}");
            }
        }
        let unsaved = plan.unsaved();
        if unsaved.is_empty() {
            return Ok(0);
        }
        let nbt = unsaved
            .iter()
            .map(|(pos, c, _)| {
                Ok((
                    *pos,
                    anvil::chunk_to_nbt(c, *pos, self.format, &self.names)?,
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        self.store.save(&nbt)?;
        plan.mark_saved(unsaved.iter().map(|(pos, _, rev)| (*pos, *rev)));
        Ok(nbt.len())
    }

    /// Автосохранение каждые 30 с в блокирующем пуле (сжатие и диск не держат сеть).
    pub fn spawn_autosave(self: &Arc<Self>, plan: Arc<PlayPlan>) {
        let storage = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(AUTOSAVE);
            tick.tick().await;
            loop {
                tick.tick().await;
                let (storage, plan) = (storage.clone(), plan.clone());
                match tokio::task::spawn_blocking(move || storage.save(&plan)).await {
                    Ok(Ok(0)) => {}
                    Ok(Ok(n)) => tracing::info!("autosaved {n} chunks"),
                    Ok(Err(e)) => tracing::error!("autosave failed: {e}"),
                    Err(e) => tracing::error!("autosave task: {e}"),
                }
            }
        });
    }
}

impl PlayerSource for WorldStorage {
    fn load_player(&self, uuid: Uuid) -> Option<PlayerSave> {
        self.players
            .load(uuid)
            .inspect_err(|e| tracing::error!("load player {uuid}: {e}"))
            .ok()
            .flatten()
    }

    fn save_player(&self, uuid: Uuid, save: &PlayerSave) {
        if let Err(e) = self.players.save(uuid, save) {
            tracing::error!("save player {uuid}: {e}");
        }
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
