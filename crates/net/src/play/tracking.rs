//! Слежение за сущностями (`ChunkMap.TrackedEntity`): кто какую сущность видит.
//!
//! Ванила держит у каждой сущности `seenBy` и раз в тик решает для игроков
//! (`updatePlayer`): видна, если по горизонтали не дальше
//! `min(обзор × 16, clientTrackingRange × 16)` и чанк сущности уже у клиента
//! (`isChunkTracked`). Появилась — `sendPairingData` (спавн и данные), ушла —
//! `remove_entities` на неё одну. Пакеты о сущности (`ServerEntity.broadcast`)
//! получают только видящие.
//!
//! Здесь мир после тика публикует снимок сущностей ([`Tracked`]) по
//! измерению, а сессия раз в тик сверяет его со своим набором `visible`.
//! Новая сущность приходит видящим сразу (`EntityEvent::Spawn`), пакеты о
//! ней — с её ID (`EntityEvent::Of`, `Many`), удаление — `GoneAll`.

use super::*;

impl PlayPlan {
    /// Дальность слежения типа в блоках (`clientTrackingRange` × 16).
    pub(super) fn tracking_range(&self, entity_type: i32) -> i32 {
        self.survival
            .tracking_ranges
            .get(&entity_type)
            .copied()
            .unwrap_or(DEFAULT_TRACKING_RANGE)
    }

    /// Снимок сущностей измерения после тика.
    pub(super) fn publish_tracked(&self) {
        let mut out = Vec::new();
        self.track_mobs(&mut out);
        self.track_drops(&mut out);
        self.track_orbs(&mut out);
        self.track_boats(&mut out);
        self.track_minecarts(&mut out);
        self.track_item_frames(&mut out);
        *self.dim_state().tracked.lock().unwrap() = Arc::new(out);
    }

    /// Пакеты спавна сущности для нового зрителя.
    fn tracked_spawn(&self, id: i32) -> Option<Vec<Vec<u8>>> {
        self.mob_spawn(id)
            .or_else(|| self.drop_spawn(id))
            .or_else(|| self.orb_spawn(id))
            .or_else(|| self.boat_spawn(id))
            .or_else(|| self.minecart_spawn(id))
            .or_else(|| self.item_frame_spawn(id))
    }

    /// Тик слежения одного игрока (`TrackedEntity.updatePlayer` по всем
    /// сущностям его мира): спавн вошедших в дальность, удаление вышедших.
    pub(super) fn update_tracking(
        &self,
        view: &stream::ChunkStream,
        visible: &mut HashSet<i32>,
        out: &mut Vec<u8>,
    ) {
        let snapshot = self.dim_state().tracked.lock().unwrap().clone();
        for t in snapshot.iter() {
            let see = view.sees(t.pos, t.range);
            match (see, visible.contains(&t.id)) {
                (true, false) => {
                    if let Some(packets) = self.tracked_spawn(t.id) {
                        for p in packets {
                            out.extend(self.hub.frame(&p));
                        }
                        visible.insert(t.id);
                    }
                }
                (false, true) => {
                    out.extend(self.remove_frame(t.id));
                    visible.remove(&t.id);
                }
                _ => {}
            }
        }
    }
}
