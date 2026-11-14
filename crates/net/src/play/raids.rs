//! Рейды (`Raid`): Bad Omen у деревни запускает волны разбойников;
//! после последней волны игроки получают статистику `raid_win`.
//!
//! Упрощения относительно ванили: волны фиксированные (3/5/7 по сложности),
//! в волне `2 + номер` разбойников, между волнами 5 секунд; состав групп,
//! вызовы и «герой деревни» не реализованы. Босс-бар `event.minecraft.raid`
//! показывается игрокам в 32 блоках и заполняется по мере зачистки волн.

use super::*;
use crate::mobs::dist2;
use std::collections::{HashMap, HashSet};

/// Активный рейд (центр деревни, волна, живые разбойники).
pub(super) struct Raid {
    pub dim: Dimension,
    pub center: [f64; 3],
    /// Текущая волна (1..=waves).
    pub wave: i32,
    /// Всего волн (3/5/7 по сложности).
    pub waves: i32,
    /// UUID разбойников текущей волны.
    pub raiders: Vec<u128>,
    /// Когда спавнить следующую волну (после зачистки текущей).
    pub next_wave: Option<Instant>,
    /// UUID босс-бара (`Raid.raidBossBar`).
    bar: Uuid,
    /// Игроки, которым сейчас показан бар.
    bar_players: HashSet<Uuid>,
    /// Суммарное здоровье разбойников текущей волны.
    wave_health: f32,
    /// Последний отправленный прогресс.
    bar_progress: f32,
}

impl PlayPlan {
    /// `PoiManager.getInRange` по кроватям (`minecraft:home`).
    fn raid_bed_near(&self, origin: [i32; 3], radius: i32) -> bool {
        let (cx, cz) = (origin[0] >> 4, origin[2] >> 4);
        let reach = radius.div_euclid(16) + 1;
        let map = self.dim_state().pois.lock().unwrap();
        for ((x, z), list) in map.iter() {
            if (x - cx).abs() > reach || (z - cz).abs() > reach {
                continue;
            }
            for (pos, kind) in list {
                if *kind != "minecraft:home" {
                    continue;
                }
                let d = (pos[0] as i64 - origin[0] as i64).pow(2)
                    + (pos[1] as i64 - origin[1] as i64).pow(2)
                    + (pos[2] as i64 - origin[2] as i64).pow(2);
                if d <= (radius as i64).pow(2) {
                    return true;
                }
            }
        }
        false
    }

    /// UUID босс-бара рейда: уникальный для каждого запуска.
    fn raid_bar_uuid(&self) -> Uuid {
        let n = self.next_entity_id.fetch_add(1, Ordering::Relaxed) as u128;
        Uuid::from_u128(0x7261_6964_5a1d_4e70_8000_0000_0000_0000 ^ n)
    }

    /// `Raid.updateBossbar`: доля зачищенных волн плюс доля здоровья текущей.
    fn raid_bar_progress(raid: &Raid, alive_health: f32) -> f32 {
        let frac = if raid.wave_health > 0.0 {
            (1.0 - alive_health / raid.wave_health).clamp(0.0, 1.0)
        } else {
            1.0
        };
        ((raid.wave - 1) as f32 + frac) / raid.waves as f32
    }

    /// `Raid.createOrExtendRaid` + `Player.awardStat(RAID_TRIGGER)`.
    fn raid_start(&self, uuid: Uuid, center: [f64; 3], bad_omen: i32) {
        let waves = match self.difficulty() {
            survival::Difficulty::Easy => 3,
            survival::Difficulty::Normal => 5,
            survival::Difficulty::Hard => 7,
            survival::Difficulty::Peaceful => 3,
        };
        let (raiders, wave_health) = self.raid_spawn_wave(center, 1);
        let raid = Raid {
            dim: Dimension::Overworld,
            center,
            wave: 1,
            waves,
            raiders,
            next_wave: None,
            bar: self.raid_bar_uuid(),
            bar_players: HashSet::new(),
            wave_health,
            bar_progress: -1.0,
        };
        self.raids.lock().unwrap().push(raid);
        self.award_stat(uuid, "minecraft:custom", "minecraft:raid_trigger", 1);
        // `Raid` забирает эффект: игрок получает `Bad Omen` только один раз.
        self.push_control(
            uuid,
            Control::ClearEffect {
                id: Some(bad_omen),
                command: false,
            },
        );
    }

    /// Спавн волны: `2 + wave` разбойников вокруг центра.
    ///
    /// Возвращает UUID разбойников и суммарное здоровье волны.
    fn raid_spawn_wave(&self, center: [f64; 3], wave: i32) -> (Vec<u128>, f32) {
        let count = (2 + wave).clamp(1, 8);
        let mut out = Vec::new();
        let mut health = 0.0;
        let (x, z) = (center[0].floor() as i32, center[2].floor() as i32);
        let base = center[1].floor() as i32;
        let ground = (0..8)
            .rev()
            .find(|dy| {
                self.block([x, base + dy, z])
                    .is_some_and(|s| !self.state_name(s).ends_with(":air"))
            })
            .map_or(base, |dy| base + dy + 1);
        for i in 0..count {
            let angle = (i as f64) * std::f64::consts::TAU / count as f64;
            let pos = [
                center[0] + angle.cos() * 6.0,
                f64::from(ground),
                center[2] + angle.sin() * 6.0,
            ];
            if let Some(id) = self.spawn_mob(
                crate::mobs::Species::Pillager,
                pos,
                0.0f32,
                false,
                Some("RAID"),
            ) && let Some(entry) = self.sim().mobs.lock().unwrap().get(&id)
            {
                out.push(entry.mob.uuid);
                health += entry.mob.health;
            }
        }
        (out, health)
    }

    /// Тик рейдов: старт по Bad Omen у кроватей, волны, босс-бар, `raid_win`.
    pub(super) fn raid_tick(&self) {
        if self.difficulty() == survival::Difficulty::Peaceful {
            // Мирная сложность снимает рейды вместе с их барами.
            let old: Vec<Raid> = std::mem::take(&mut *self.raids.lock().unwrap());
            for raid in old {
                for uuid in raid.bar_players {
                    self.send_to(
                        uuid,
                        chunkwake_protocol::play::boss_event_remove(self.ids.boss_event, raid.bar),
                    );
                }
            }
            return;
        }
        with_dim(Dimension::Overworld, || {
            let Some(bad_omen) = self.survival.effects.get("minecraft:bad_omen").copied() else {
                return;
            };
            // 1) Старт новых рейдов.
            let candidates: Vec<(Uuid, [f64; 3])> = self
                .live
                .lock()
                .unwrap()
                .iter()
                .filter(|(_, l)| {
                    l.dimension == Dimension::Overworld
                        && l.effects.iter().any(|e| e.id == bad_omen)
                })
                .map(|(uuid, l)| (*uuid, l.pos))
                .collect();
            for (uuid, pos) in candidates {
                let feet = [
                    pos[0].floor() as i32,
                    pos[1].floor() as i32,
                    pos[2].floor() as i32,
                ];
                if !self.raid_bed_near(feet, 32) {
                    continue;
                }
                if self
                    .raids
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|r| dist2(r.center, pos) < 96.0 * 96.0)
                {
                    continue;
                }
                self.raid_start(uuid, pos, bad_omen);
            }

            // 2) Прогресс волн и босс-бар.
            let (alive, raider_health): (HashSet<u128>, HashMap<u128, f32>) = {
                let mobs = self.sim().mobs.lock().unwrap();
                let mut alive = HashSet::new();
                let mut health = HashMap::new();
                for entry in mobs.values() {
                    if entry.mob.alive() {
                        alive.insert(entry.mob.uuid);
                        health.insert(entry.mob.uuid, entry.mob.health);
                    }
                }
                (alive, health)
            };
            let players: Vec<(Uuid, [f64; 3], Dimension)> = self
                .live
                .lock()
                .unwrap()
                .iter()
                .map(|(uuid, l)| (*uuid, l.pos, l.dimension))
                .collect();
            let now = Instant::now();
            let mut spawn = Vec::new();
            let mut won = Vec::new();
            let mut packets: Vec<(Uuid, Vec<u8>)> = Vec::new();
            {
                let mut raids = self.raids.lock().unwrap();
                let mut i = 0;
                while i < raids.len() {
                    let raid = &mut raids[i];
                    if !raid.raiders.iter().any(|u| alive.contains(u)) && raid.wave >= raid.waves {
                        // Победа: бар снимается, `raid_win` — участникам рядом.
                        for uuid in raid.bar_players.drain() {
                            packets.push((
                                uuid,
                                chunkwake_protocol::play::boss_event_remove(
                                    self.ids.boss_event,
                                    raid.bar,
                                ),
                            ));
                        }
                        won.push((raid.center, raid.dim));
                        raids.remove(i);
                        continue;
                    }
                    // Бар: прогресс по здоровью текущей волны и зрители в 32 блоках.
                    let alive_health: f32 = raid
                        .raiders
                        .iter()
                        .filter_map(|u| raider_health.get(u).copied())
                        .sum();
                    let progress = Self::raid_bar_progress(raid, alive_health);
                    if (progress - raid.bar_progress).abs() > 0.0005 {
                        raid.bar_progress = progress;
                        for uuid in &raid.bar_players {
                            packets.push((
                                *uuid,
                                chunkwake_protocol::play::boss_event_progress(
                                    self.ids.boss_event,
                                    raid.bar,
                                    progress,
                                ),
                            ));
                        }
                    }
                    let in_range: HashSet<Uuid> = players
                        .iter()
                        .filter(|(_, pos, dim)| {
                            *dim == raid.dim && dist2(*pos, raid.center) <= 32.0 * 32.0
                        })
                        .map(|(uuid, _, _)| *uuid)
                        .collect();
                    let lost: Vec<Uuid> = raid.bar_players.difference(&in_range).copied().collect();
                    for uuid in lost {
                        raid.bar_players.remove(&uuid);
                        packets.push((
                            uuid,
                            chunkwake_protocol::play::boss_event_remove(
                                self.ids.boss_event,
                                raid.bar,
                            ),
                        ));
                    }
                    for uuid in in_range {
                        if raid.bar_players.insert(uuid) {
                            packets.push((
                                uuid,
                                chunkwake_protocol::play::boss_event_add(
                                    self.ids.boss_event,
                                    raid.bar,
                                    &serde_json::json!({"translate": "event.minecraft.raid"}),
                                    progress,
                                    2,
                                    0,
                                ),
                            ));
                        }
                    }

                    if raid.raiders.iter().any(|u| alive.contains(u)) {
                        i += 1;
                        continue;
                    }
                    match raid.next_wave {
                        None => {
                            raid.next_wave = Some(now + Duration::from_secs(5));
                            i += 1;
                        }
                        Some(at) if now >= at => {
                            raid.next_wave = None;
                            raid.wave += 1;
                            spawn.push((raid.center, raid.wave, i));
                            i += 1;
                        }
                        Some(_) => i += 1,
                    }
                }
            }
            for (uuid, packet) in packets {
                self.send_to(uuid, packet);
            }
            for (center, wave, index) in spawn {
                let (raiders, health) = self.raid_spawn_wave(center, wave);
                if let Some(raid) = self.raids.lock().unwrap().get_mut(index) {
                    raid.raiders = raiders;
                    raid.wave_health = health;
                }
            }
            // 3) Победа: `RaidWin` всем игрокам рядом (ваниль: участники рейда).
            for (center, _) in won {
                let players: Vec<Uuid> = self
                    .live
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|(_, l)| {
                        l.dimension == Dimension::Overworld && dist2(l.pos, center) <= 48.0 * 48.0
                    })
                    .map(|(uuid, _)| *uuid)
                    .collect();
                for uuid in players {
                    self.award_stat(uuid, "minecraft:custom", "minecraft:raid_win", 1);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raid(wave: i32, waves: i32, wave_health: f32) -> Raid {
        Raid {
            dim: Dimension::Overworld,
            center: [0.0; 3],
            wave,
            waves,
            raiders: Vec::new(),
            next_wave: None,
            bar: Uuid::nil(),
            bar_players: HashSet::new(),
            wave_health,
            bar_progress: -1.0,
        }
    }

    /// Прогресс бара: волна заполняется по здоровью разбойников, `waves` равных
    /// долей дают полный бар к победе.
    #[test]
    fn raid_bar_progress_fills_by_waves() {
        let r = raid(1, 3, 36.0);
        assert_eq!(PlayPlan::raid_bar_progress(&r, 36.0), 0.0);
        assert!((PlayPlan::raid_bar_progress(&r, 18.0) - 1.0 / 6.0).abs() < 1e-6);
        assert!((PlayPlan::raid_bar_progress(&r, 0.0) - 1.0 / 3.0).abs() < 1e-6);
        let r = raid(2, 3, 36.0);
        assert!((PlayPlan::raid_bar_progress(&r, 36.0) - 1.0 / 3.0).abs() < 1e-6);
        let r = raid(3, 3, 36.0);
        assert_eq!(PlayPlan::raid_bar_progress(&r, 0.0), 1.0);
    }
}
