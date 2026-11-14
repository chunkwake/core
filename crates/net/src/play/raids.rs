//! Рейды (`Raid`): Bad Omen у деревни запускает волны разбойников;
//! после последней волны игроки получают статистику `raid_win`.
//!
//! Упрощения относительно ванили: волны фиксированные (3/5/7 по сложности),
//! в волне `2 + номер` разбойников, между волнами 5 секунд; состав групп,
//! вызовы и «герой деревни» не реализованы.

use super::*;
use crate::mobs::dist2;

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

    /// `Raid.createOrExtendRaid` + `Player.awardStat(RAID_TRIGGER)`.
    fn raid_start(&self, uuid: Uuid, center: [f64; 3], bad_omen: i32) {
        let waves = match self.difficulty() {
            survival::Difficulty::Easy => 3,
            survival::Difficulty::Normal => 5,
            survival::Difficulty::Hard => 7,
            survival::Difficulty::Peaceful => 3,
        };
        let mut raid = Raid {
            dim: Dimension::Overworld,
            center,
            wave: 1,
            waves,
            raiders: Vec::new(),
            next_wave: None,
        };
        raid.raiders = self.raid_spawn_wave(center, 1);
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
    fn raid_spawn_wave(&self, center: [f64; 3], wave: i32) -> Vec<u128> {
        let count = (2 + wave).clamp(1, 8);
        let mut out = Vec::new();
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
            }
        }
        out
    }

    /// Тик рейдов: старт по Bad Omen у кроватей, волны, `raid_win`.
    pub(super) fn raid_tick(&self) {
        if self.difficulty() == survival::Difficulty::Peaceful {
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

            // 2) Прогресс волн.
            let alive: std::collections::HashSet<u128> = self
                .sim()
                .mobs
                .lock()
                .unwrap()
                .values()
                .filter(|e| e.mob.alive())
                .map(|e| e.mob.uuid)
                .collect();
            let now = Instant::now();
            let mut spawn = Vec::new();
            let mut won = Vec::new();
            {
                let mut raids = self.raids.lock().unwrap();
                let mut i = 0;
                while i < raids.len() {
                    let raid = &mut raids[i];
                    if raid.raiders.iter().any(|u| alive.contains(u)) {
                        i += 1;
                        continue;
                    }
                    if raid.wave >= raid.waves {
                        won.push((raid.center, raid.dim));
                        raids.remove(i);
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
            for (center, wave, index) in spawn {
                let raiders = self.raid_spawn_wave(center, wave);
                if let Some(raid) = self.raids.lock().unwrap().get_mut(index) {
                    raid.raiders = raiders;
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
