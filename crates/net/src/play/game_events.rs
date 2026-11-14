//! `gameEvent` (`GameEventDispatcher`, `GameEventListenerRegistry`): вибрации
//! идут к скалк-сенсорам по секциям 16×16×16 с задержкой `distance` тиков,
//! затеняются блоками `#minecraft:occludes_vibration_signals` (шерсть) и
//! рисуют частицу `minecraft:vibration` от источника к сенсору.
//!
//! За тик сенсор обрабатывает только ближайшую вибрацию, а пока к нему летит
//! сигнал, новые не принимаются (как в ванили: «other vibrations cannot be
//! detected when a signal is already travelling»).

use super::*;
use chunkwake_protocol::Writer;
use std::collections::HashMap;

/// Ожидающая доставки вибрация (запись `GameEventListenerRegistry`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct PendingVibration {
    /// Тик доставки: `game_time + distance` (1 тик на блок).
    pub at: i64,
    pub sensor: [i32; 3],
    pub origin: [f64; 3],
    pub frequency: i32,
}

/// За тик сенсор получает только ближайшую из созревших вибраций.
fn pick_nearest(due: Vec<PendingVibration>) -> Vec<PendingVibration> {
    let mut best: HashMap<[i32; 3], PendingVibration> = HashMap::new();
    for v in due {
        let at = center(v.sensor);
        let d = (at[0] - v.origin[0]).powi(2)
            + (at[1] - v.origin[1]).powi(2)
            + (at[2] - v.origin[2]).powi(2);
        match best.get(&v.sensor) {
            Some(old) => {
                let o = center(old.sensor);
                let od = (o[0] - old.origin[0]).powi(2)
                    + (o[1] - old.origin[1]).powi(2)
                    + (o[2] - old.origin[2]).powi(2);
                if d < od {
                    best.insert(v.sensor, v);
                }
            }
            None => {
                best.insert(v.sensor, v);
            }
        }
    }
    best.into_values().collect()
}

/// Центр блока.
fn center(pos: [i32; 3]) -> [f64; 3] {
    [
        f64::from(pos[0]) + 0.5,
        f64::from(pos[1]) + 0.5,
        f64::from(pos[2]) + 0.5,
    ]
}

impl PlayPlan {
    /// Сенсоры секции: `GameEventListenerRegistry` строится один раз и
    /// сбрасывается при изменении блоков (`after_block_change`).
    fn vibration_section(&self, section: [i32; 3]) -> Vec<[i32; 3]> {
        if let Some(list) = self.sim().sculk_sections.lock().unwrap().get(&section) {
            return list.clone();
        }
        let base = [section[0] * 16, section[1] * 16, section[2] * 16];
        let found = self.with_world(|w| {
            let mut out = Vec::new();
            for x in base[0]..base[0] + 16 {
                for y in base[1]..base[1] + 16 {
                    for z in base[2]..base[2] + 16 {
                        let Some(state) = w.block([x, y, z]) else {
                            continue;
                        };
                        if crate::redstone::is_sculk_sensor(self.state_name(state)) {
                            out.push([x, y, z]);
                        }
                    }
                }
            }
            out
        });
        self.sim()
            .sculk_sections
            .lock()
            .unwrap()
            .insert(section, found.clone());
        found
    }

    /// Есть ли между источником и сенсором блок `#occludes_vibration_signals`.
    fn vibration_occluded(&self, from: [f64; 3], to: [f64; 3]) -> bool {
        let Some(tag) = self
            .survival
            .block_tags
            .get("minecraft:occludes_vibration_signals")
        else {
            return false;
        };
        let delta = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
        let len = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
        if len <= 0.0 {
            return false;
        }
        let steps = (len / 0.25).ceil() as i32;
        // Первый блок (источник) пропускаем: вибрация рождается внутри него.
        for i in 1..steps {
            let t = f64::from(i) / f64::from(steps);
            let p = [
                (from[0] + delta[0] * t).floor() as i32,
                (from[1] + delta[1] * t).floor() as i32,
                (from[2] + delta[2] * t).floor() as i32,
            ];
            if self
                .block(p)
                .is_some_and(|s| tag.contains(self.state_name(s)))
            {
                return true;
            }
        }
        false
    }

    /// Частица `minecraft:vibration`: источник → сенсор за `ticks` тиков.
    fn vibration_particle(&self, from: [f64; 3], sensor: [i32; 3], ticks: i32) {
        let Some(&particle) = self
            .particle_types
            .lock()
            .unwrap()
            .get("minecraft:vibration")
        else {
            return;
        };
        let mut body = Writer::new();
        // `PositionSource`: тип 0 — блок, затем упакованная позиция.
        body.var_int(0)
            .i64(chunkwake_protocol::play::pack_block_pos(sensor))
            .var_int(ticks);
        self.hub.send(
            None,
            chunkwake_protocol::play::level_particles_with(
                self.ids.level_particles,
                particle,
                Some(&body.buf),
                false,
                from,
                [0.0; 3],
                0.0,
                1,
            ),
        );
    }

    /// `GameEventDispatcher.post`: разложить вибрацию по секциям-регистраторам.
    pub(super) fn dispatch_vibration(&self, origin: [f64; 3], event: &str) {
        let frequency = crate::redstone::vibration_frequency(event);
        if frequency == 0 {
            return;
        }
        let range = crate::redstone::SCULK_LISTENER_RANGE;
        let now = self.game_time.load(Ordering::Relaxed);
        let mut add = Vec::new();
        {
            let queue = self.sim().vibrations.lock().unwrap();
            // Границы куба в секциях.
            let min = [
                (origin[0].floor() as i32 - range).div_euclid(16),
                (origin[1].floor() as i32 - range).div_euclid(16),
                (origin[2].floor() as i32 - range).div_euclid(16),
            ];
            let max = [
                (origin[0].floor() as i32 + range).div_euclid(16),
                (origin[1].floor() as i32 + range).div_euclid(16),
                (origin[2].floor() as i32 + range).div_euclid(16),
            ];
            for sx in min[0]..=max[0] {
                for sy in min[1]..=max[1] {
                    for sz in min[2]..=max[2] {
                        for sensor in self.vibration_section([sx, sy, sz]) {
                            let at = center(sensor);
                            let dx = at[0] - origin[0];
                            let dy = at[1] - origin[1];
                            let dz = at[2] - origin[2];
                            let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                            if dist > f64::from(range) {
                                continue;
                            }
                            if self.vibration_occluded(origin, at) {
                                continue;
                            }
                            // Сигнал уже летит к этому сенсору.
                            if queue.iter().any(|v| v.sensor == sensor) {
                                continue;
                            }
                            let ticks = dist.ceil() as i32;
                            add.push(PendingVibration {
                                at: now + i64::from(ticks),
                                sensor,
                                origin,
                                frequency,
                            });
                            self.vibration_particle(origin, sensor, ticks.max(1));
                        }
                    }
                }
            }
        }
        if !add.is_empty() {
            self.sim().vibrations.lock().unwrap().extend(add);
        }
    }

    /// Тик вибраций: доставить созревшие и выбрать ближайшую на сенсор.
    pub(super) fn tick_vibrations(&self) {
        let now = self.game_time.load(Ordering::Relaxed);
        let due: Vec<PendingVibration> = {
            let mut queue = self.sim().vibrations.lock().unwrap();
            if queue.is_empty() {
                return;
            }
            let mut due = Vec::new();
            let mut rest = Vec::with_capacity(queue.len());
            for v in queue.drain(..) {
                if v.at <= now {
                    due.push(v);
                } else {
                    rest.push(v);
                }
            }
            *queue = rest;
            due
        };
        if due.is_empty() {
            return;
        }
        for v in pick_nearest(due) {
            self.sculk_hears(v.sensor, v.origin, v.frequency);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_of_negative_coordinates() {
        // `div_euclid` не уводит секцию в ноль у отрицательных координат.
        assert_eq!((-1i32).div_euclid(16), -1);
        assert_eq!((-16i32).div_euclid(16), -1);
        assert_eq!((-17i32).div_euclid(16), -2);
    }

    #[test]
    fn pending_is_delivered_after_distance_ticks() {
        // 1 тик на блок: потолок расстояния.
        assert_eq!(3.0f64.ceil() as i64, 3);
        assert_eq!(3.2f64.ceil() as i64, 4);
    }

    #[test]
    fn nearest_vibration_wins_per_sensor() {
        let sensor = [0, 0, 0];
        let near = PendingVibration {
            at: 10,
            sensor,
            origin: [2.5, 0.5, 0.5],
            frequency: 1,
        };
        let far = PendingVibration {
            at: 10,
            sensor,
            origin: [7.5, 0.5, 0.5],
            frequency: 13,
        };
        let other_sensor = PendingVibration {
            at: 10,
            sensor: [20, 0, 0],
            origin: [20.5, 0.5, 0.5],
            frequency: 7,
        };
        let picked = pick_nearest(vec![far, near, other_sensor]);
        assert_eq!(picked.len(), 2);
        let near_pick = picked.iter().find(|v| v.sensor == sensor).unwrap();
        assert_eq!(near_pick.frequency, 1);
        assert!(picked.iter().any(|v| v.sensor == [20, 0, 0]));
    }
}
