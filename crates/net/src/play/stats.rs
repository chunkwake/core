//! Статистика (`award_stat`): тип, значение и пакет `award_stats`.
//!
//! Ключ — (`stat_type`, значение): для `minecraft:custom` это
//! `custom_stat` (например `minecraft:traded_with_villager`), для
//! `minecraft:used` — id предмета. Пакет передаёт только изменившуюся
//! запись; полный набор отправляется при входе.

use chunkwake_protocol::Writer;

use super::*;

impl PlayPlan {
    /// `(stat_type, значение)` → id реестров.
    fn stat_id(&self, kind: &str, value: &str) -> Option<(i32, i32)> {
        let stat = *self.survival.stat_types.get(kind)?;
        let value = match kind {
            "minecraft:custom" => *self.survival.custom_stats.get(value)?,
            "minecraft:killed" | "minecraft:killed_by" => *self.survival.entity_types.get(value)?,
            "minecraft:mined" => *self.survival.block_types.get(value)?,
            _ => self.item_id(value)?,
        };
        Some((stat, value))
    }

    /// `Player.awardStat`: увеличить счётчик и отправить `award_stats`.
    pub(crate) fn award_stat(&self, uuid: Uuid, kind: &str, value: &str, amount: i32) {
        let Some((stat, id)) = self.stat_id(kind, value) else {
            return;
        };
        let total = {
            let mut all = self.stats.lock().unwrap();
            let entry = all
                .entry(uuid)
                .or_default()
                .entry((kind.to_owned(), value.to_owned()))
                .or_insert(0);
            *entry += amount;
            *entry
        };
        let packet = award_stats(self.ids.award_stats, &[(stat, id, total)]);
        self.send_to(uuid, packet);
    }

    /// `ServerStatsCounter.setValue`: сбросить счётчик (например
    /// `time_since_death`) и отправить новое значение.
    pub(crate) fn reset_stat(&self, uuid: Uuid, kind: &str, value: &str) {
        let Some((stat, id)) = self.stat_id(kind, value) else {
            return;
        };
        {
            let mut all = self.stats.lock().unwrap();
            all.entry(uuid)
                .or_default()
                .insert((kind.to_owned(), value.to_owned()), 0);
        }
        let packet = award_stats(self.ids.award_stats, &[(stat, id, 0)]);
        self.send_to(uuid, packet);
    }

    /// `ServerPlayer.checkMovementStatistics`: шаг, спринт, вода, подъём,
    /// полёт и прыжок. `to` — позиция из пакета движения.
    pub(crate) fn movement_stats(
        &self,
        uuid: Uuid,
        to: Option<[f64; 3]>,
        on_ground: bool,
        sprint: bool,
        sneak: bool,
        glide: bool,
    ) {
        let Some(to) = to else {
            return;
        };
        let Some((old, old_ground)) = self
            .hub
            .players
            .lock()
            .unwrap()
            .get(&uuid)
            .map(|p| (p.pos, p.on_ground))
        else {
            return;
        };
        let (dx, dy, dz) = (to[0] - old[0], to[1] - old[1], to[2] - old[2]);
        if dx == 0.0 && dy == 0.0 && dz == 0.0 {
            return;
        }
        // `ServerGamePacketListenerImpl.handleMovePlayer`: прыжок — отрыв от
        // земли вверх после кадра «на земле».
        if old_ground && !on_ground && dy > 0.0 {
            self.award_stat(uuid, "minecraft:custom", "minecraft:jump", 1);
        }
        let feet = [
            to[0].floor() as i32,
            to[1].floor() as i32,
            to[2].floor() as i32,
        ];
        let in_water = self.standing_in_water(to);
        let eye_water = self.standing_in_water([to[0], to[1] + 1.62, to[2]]);
        let climbable = self.block(feet).is_some_and(|s| {
            matches!(
                self.state_name(s),
                "minecraft:ladder"
                    | "minecraft:vine"
                    | "minecraft:scaffolding"
                    | "minecraft:twisting_vines"
                    | "minecraft:twisting_vines_plant"
                    | "minecraft:weeping_vines"
                    | "minecraft:weeping_vines_plant"
                    | "minecraft:cave_vines"
                    | "minecraft:cave_vines_plant"
            )
        });
        let horizontal = ((dx * dx + dz * dz).sqrt() * 100.0).round() as i32;
        let full = ((dx * dx + dy * dy + dz * dz).sqrt() * 100.0).round() as i32;
        if in_water {
            if sprint {
                if full > 0 {
                    self.award_stat(uuid, "minecraft:custom", "minecraft:swim_one_cm", full);
                }
            } else if eye_water {
                if full > 0 {
                    self.award_stat(
                        uuid,
                        "minecraft:custom",
                        "minecraft:walk_under_water_one_cm",
                        full,
                    );
                }
            } else if horizontal > 0 {
                self.award_stat(
                    uuid,
                    "minecraft:custom",
                    "minecraft:walk_on_water_one_cm",
                    horizontal,
                );
            }
            return;
        }
        if climbable {
            if dy > 0.0 {
                let climb = (dy * 100.0).round() as i32;
                if climb > 0 {
                    self.award_stat(uuid, "minecraft:custom", "minecraft:climb_one_cm", climb);
                }
            }
            return;
        }
        if on_ground {
            if horizontal <= 0 {
                return;
            }
            let key = if sprint {
                "minecraft:sprint_one_cm"
            } else if sneak {
                "minecraft:crouch_one_cm"
            } else {
                "minecraft:walk_one_cm"
            };
            self.award_stat(uuid, "minecraft:custom", key, horizontal);
            return;
        }
        if glide {
            if full > 0 {
                self.award_stat(uuid, "minecraft:custom", "minecraft:aviate_one_cm", full);
            }
        } else if horizontal > 25 {
            self.award_stat(uuid, "minecraft:custom", "minecraft:fly_one_cm", horizontal);
        }
    }

    /// Загруженные счётчики игрока после входа.
    pub(crate) fn load_stats(&self, uuid: Uuid, stats: Vec<((String, String), i32)>) {
        self.stats
            .lock()
            .unwrap()
            .insert(uuid, stats.into_iter().collect());
    }

    /// Все счётчики для сохранения в `stats/<uuid>.json`.
    pub(crate) fn collect_stats(&self, uuid: Uuid) -> Vec<((String, String), i32)> {
        self.stats
            .lock()
            .unwrap()
            .get(&uuid)
            .map(|m| m.iter().map(|(k, &n)| (k.clone(), n)).collect())
            .unwrap_or_default()
    }

    /// Полный набор при входе (`ServerPlayer.sendStats`).
    pub(crate) fn stats_packet(&self, uuid: Uuid) -> Option<Vec<u8>> {
        let all = self.stats.lock().unwrap();
        let stats = all.get(&uuid)?;
        let mut rows = Vec::new();
        for ((kind, value), &count) in stats {
            if let Some((stat, id)) = self.stat_id(kind, value) {
                rows.push((stat, id, count));
            }
        }
        (!rows.is_empty()).then(|| award_stats(self.ids.award_stats, &rows))
    }

    pub(crate) fn forget_stats(&self, uuid: Uuid) {
        self.stats.lock().unwrap().remove(&uuid);
    }
}

/// `ClientboundAwardStatsPacket`: map `Stat<?> → int`.
///
/// `Stat.STREAM_CODEC` пишет id `stat_type`, затем id значения в реестре
/// этого типа; `ByteBufCodecs.map` — количество и пары.
pub fn award_stats(id: i32, stats: &[(i32, i32, i32)]) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(stats.len() as i32);
    for &(kind, value, count) in stats {
        w.var_int(kind).var_int(value).var_int(count);
    }
    w.buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use chunkwake_protocol::Reader;

    #[test]
    fn award_stats_encodes_type_value_and_count() {
        let p = award_stats(7, &[(8, 38, 2), (2, 900, 5)]);
        let mut r = Reader::new(&p);
        assert_eq!(r.var_int().unwrap(), 7);
        assert_eq!(r.var_int().unwrap(), 2);
        assert_eq!(r.var_int().unwrap(), 8);
        assert_eq!(r.var_int().unwrap(), 38);
        assert_eq!(r.var_int().unwrap(), 2);
        assert_eq!(r.var_int().unwrap(), 2);
        assert_eq!(r.var_int().unwrap(), 900);
        assert_eq!(r.var_int().unwrap(), 5);
    }
}
