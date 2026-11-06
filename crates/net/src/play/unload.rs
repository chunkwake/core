//! Выгрузка столбцов вдали от игроков (`ChunkMap` у ванили).
//!
//! Раньше каждый сгенерированный или прочитанный столбец жил в `edited` до
//! остановки сервера: полёт на обзоре 32 за минуту набирал 13 тыс. столбцов
//! и больше гигабайта секций и света. Теперь столбец за пределом обзора
//! всех игроков (с запасом) пишется на диск, если там его ещё нет, и
//! уходит из памяти; регион помечается, и следующее обращение читает его
//! блоки заново.

use super::*;

/// Запас вокруг обзора: соседи края ещё нужны свету и потоку отправки.
const KEEP_MARGIN: i32 = 3;

/// Столбец к записи перед выгрузкой.
pub type UnloadWrite = ((i32, i32), ChunkColumn);

impl PlayPlan {
    /// Чанки, которые выгрузка не трогает: квадрат обзора с запасом вокруг
    /// каждого игрока измерения и симулируемые чанки.
    fn keep_test(&self, dim: Dimension) -> impl Fn((i32, i32)) -> bool + use<> {
        let r = self.view_radius.load(Ordering::Relaxed).max(2) + KEEP_MARGIN;
        let centers: Vec<(i32, i32)> = self
            .hub
            .players
            .lock()
            .unwrap()
            .values()
            .filter(|p| p.dimension == dim)
            .map(|p| ((p.pos[0].floor() as i32) >> 4, (p.pos[2].floor() as i32) >> 4))
            .collect();
        let simulated = self.dims.get(&dim).map(|s| s.simulated.lock().unwrap().clone());
        move |c: (i32, i32)| {
            centers.iter().any(|o| (c.0 - o.0).abs() <= r && (c.1 - o.1).abs() <= r)
                || simulated.as_ref().is_some_and(|s| s.contains(&c))
        }
    }

    /// Выгружает далёкие столбцы измерения `dim`. `write` пишет на диск те,
    /// чьей версии там нет (сгенерированные и правленые после сейва), и
    /// возвращает ошибку — тогда регион остаётся в памяти. Каждый регион
    /// идёт под замком своего шарда загрузки: чтение того же региона не
    /// застанет файл до записи, а столбцы ещё в памяти. Возвращает число
    /// выгруженных столбцов.
    pub fn unload_far(
        &self,
        dim: Dimension,
        write: &dyn Fn(&[UnloadWrite]) -> Result<(), String>,
    ) -> usize {
        // Без диска выгруженное не вернуть.
        if self.source.get().is_none() {
            return 0;
        }
        let Some(state) = self.dims.get(&dim) else {
            return 0;
        };
        let keep = self.keep_test(dim);
        let mut regions: FastMap<(i32, i32), Vec<(i32, i32)>> = FastMap::default();
        for pos in state.edited.read().unwrap().keys() {
            if !keep(*pos) {
                regions.entry((pos.0 >> 5, pos.1 >> 5)).or_default().push(*pos);
            }
        }
        let mut total = 0;
        for (region, list) in regions {
            let _guard = self.region_load[region_shard(region)].lock().unwrap();
            // Игрок мог подойти, пока шли предыдущие регионы.
            let keep = self.keep_test(dim);
            let (writes, revs) = {
                let world = state.edited.read().unwrap();
                let saved = state.saved.lock().unwrap();
                let playing = state.sim.jukeboxes.lock().unwrap();
                let mut writes = Vec::new();
                let mut revs = Vec::new();
                for pos in list {
                    let Some((column, rev)) = world.get(&pos) else { continue };
                    if keep(pos) {
                        continue;
                    }
                    if saved.get(&pos) != Some(rev) {
                        let mut c = column.clone();
                        jukebox::stamp_ticks(&mut c, &playing);
                        writes.push((pos, c));
                    }
                    revs.push((pos, *rev));
                }
                (writes, revs)
            };
            if revs.is_empty() {
                continue;
            }
            if !writes.is_empty()
                && let Err(e) = write(&writes)
            {
                tracing::error!("unload region {region:?}: {e}");
                continue;
            }
            drop(writes);
            total += self.unload_commit(state, region, &revs);
        }
        total
    }

    /// Снимает записанные столбцы, если их не правили за время записи.
    fn unload_commit(&self, state: &DimState, region: (i32, i32), revs: &[((i32, i32), u64)]) -> usize {
        let mut world = state.edited.write().unwrap();
        // Отметка до снятия и под тем же замком: читатель, не нашедший
        // столбца, уже увидит регион невычитанным.
        state.evicted.lock().unwrap().insert(region);
        state.region_epoch.fetch_add(1, Ordering::AcqRel);
        let mut frames = state.chunk_frames.lock().unwrap();
        let mut saved = state.saved.lock().unwrap();
        let mut dirty = state.dirty.lock().unwrap();
        let mut n = 0;
        for &(pos, rev) in revs {
            if world.get(&pos).is_none_or(|e| e.1 != rev) {
                continue;
            }
            world.remove(&pos);
            frames.remove(&pos);
            saved.remove(&pos);
            dirty.remove(&pos);
            n += 1;
        }
        n
    }
}
