//! Проводник (`ConduitBlockEntity`): рамка из `#minecraft:conduit_effect_block`
//! вокруг проводника в воде даёт `Conduit Power`, полная рамка ещё и бьёт
//! враждебных мобов.
//!
//! `updateShape`: все 27 блоков куба 3×3×3 (включая сам проводник) должны быть
//! водой, а позиции рамки — кольца 5×5 в трёх плоскостях (42 уникальные
//! позиции). `MIN_ACTIVE_SIZE` = 16 (одно кольцо), `MIN_KILL_SIZE` = 42.
//! Радиус эффекта `size / 7 * 16` (32…96), эффект `conduit_power` на 13 секунд
//! (260 тиков), пересчёт раз в 40 тиков (`BLOCK_REFRESH_RATE * 20`).

use super::*;
use crate::mobs::dist2;

/// `ConduitBlockEntity.MIN_ACTIVE_SIZE`.
const MIN_ACTIVE_SIZE: usize = 16;
/// `ConduitBlockEntity.MIN_KILL_SIZE`: полная рамка для урона.
const MIN_KILL_SIZE: usize = 42;
/// `ConduitBlockEntity.KILL_RANGE`.
const KILL_RANGE: f64 = 8.0;
/// `ConduitBlockEntity.EFFECT_DURATION` в тиках (13 секунд).
const EFFECT_DURATION: i32 = 13 * 20;
/// `BLOCK_REFRESH_RATE * 20`: пересчёт рамки раз в 2 секунды.
const CHECK_INTERVAL: i64 = 40;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Conduit {
    /// Сколько блоков рамки на месте (0..=42).
    pub size: usize,
    /// `isActive`: рамка не меньше 16 и куб 3×3×3 заполнен водой.
    pub active: bool,
    /// `isHunting`: полная рамка, урон враждебным мобам.
    pub hunting: bool,
}

/// Позиции рамки: три кольца 5×5 (XY, XZ, YZ) без повторов — 42 блока.
fn frame_positions([x, y, z]: [i32; 3]) -> Vec<[i32; 3]> {
    let mut out = Vec::with_capacity(42);
    for a in -2i32..=2 {
        for b in -2i32..=2 {
            if a.abs().max(b.abs()) != 2 {
                continue;
            }
            out.push([x + a, y + b, z]);
            out.push([x + a, y, z + b]);
            out.push([x, y + a, z + b]);
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

impl PlayPlan {
    /// `#minecraft:conduit_effect_block`.
    fn conduit_frame_block(&self, state: u32) -> bool {
        self.survival
            .block_tags
            .get("minecraft:conduit_effect_block")
            .is_some_and(|set| set.contains(self.state_name(state)))
    }

    fn conduit_water(&self, pos: [i32; 3]) -> bool {
        self.block(pos)
            .and_then(|s| crate::fluids::state_fluid(&self.survival.placement, s))
            .is_some_and(|f| f.kind == crate::fluids::Kind::Water)
    }

    /// `ConduitBlockEntity.updateShape`: вода вокруг и число блоков рамки.
    fn conduit_shape(&self, pos: [i32; 3]) -> (usize, bool) {
        let water = (-1..=1).all(|dx| {
            (-1..=1).all(|dy| {
                (-1..=1).all(|dz| self.conduit_water([pos[0] + dx, pos[1] + dy, pos[2] + dz]))
            })
        });
        let size = frame_positions(pos)
            .into_iter()
            .filter(|p| self.block(*p).is_some_and(|s| self.conduit_frame_block(s)))
            .count();
        (size, water && size >= MIN_ACTIVE_SIZE)
    }

    /// `ConduitBlockEntity.applyEffects`: `conduit_power` игрокам в кубе радиуса.
    fn conduit_effects(&self, pos: [i32; 3], size: usize) {
        let Some(&id) = self.survival.effects.get("minecraft:conduit_power") else {
            return;
        };
        let range = (size / 7 * 16) as f64;
        let dim = self.active_dim();
        let low = [
            f64::from(pos[0]) - range,
            f64::from(pos[1]) - range,
            f64::from(pos[2]) - range,
        ];
        let high = [
            f64::from(pos[0]) + 1.0 + range,
            f64::from(pos[1]) + 1.0 + range,
            f64::from(pos[2]) + 1.0 + range,
        ];
        let players: Vec<Uuid> = self
            .live
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, l)| {
                l.dimension == dim
                    && l.health > 0.0
                    && (0..3).all(|i| l.pos[i] >= low[i] && l.pos[i] <= high[i])
            })
            .map(|(&uuid, _)| uuid)
            .collect();
        for uuid in players {
            let mut e = survival::Effect::plain(id, 0, EFFECT_DURATION);
            e.ambient = true;
            self.push_control(
                uuid,
                Control::Effect {
                    effect: e,
                    force: false,
                    command: false,
                },
            );
        }
    }

    /// `ConduitBlockEntity.updateAndAttackTarget`: 4 урона враждебному мобу
    /// в 8 блоках (ближайшему).
    fn conduit_attack(&self, pos: [i32; 3]) {
        let center = [
            f64::from(pos[0]) + 0.5,
            f64::from(pos[1]) + 0.5,
            f64::from(pos[2]) + 0.5,
        ];
        let low = [
            f64::from(pos[0]) - KILL_RANGE,
            f64::from(pos[1]) - KILL_RANGE,
            f64::from(pos[2]) - KILL_RANGE,
        ];
        let high = [
            f64::from(pos[0]) + 1.0 + KILL_RANGE,
            f64::from(pos[1]) + 1.0 + KILL_RANGE,
            f64::from(pos[2]) + 1.0 + KILL_RANGE,
        ];
        let target = {
            let mobs = self.sim().mobs.lock().unwrap();
            mobs.values()
                .filter(|e| {
                    e.mob.alive()
                        && e.mob.species.info().category == crate::mobs::Category::Monster
                        && (0..3).all(|i| e.mob.pos[i] >= low[i] && e.mob.pos[i] <= high[i])
                })
                .min_by(|a, b| {
                    let da = dist2(a.mob.pos, center);
                    let db = dist2(b.mob.pos, center);
                    da.total_cmp(&db)
                })
                .map(|e| e.mob.uuid)
        };
        let Some(uuid) = target else {
            return;
        };
        let dead = {
            let mut mobs = self.sim().mobs.lock().unwrap();
            let Some(entry) = mobs.values_mut().find(|e| e.mob.uuid == uuid) else {
                return;
            };
            crate::mobs::hurt(
                &mut entry.mob,
                4.0,
                crate::mobs::Source::at(center, None),
                0.0,
            )
        };
        if matches!(dead, Some(true)) {
            self.play_sound(
                None,
                "minecraft:block.conduit.attack",
                crate::sounds::BLOCKS,
                center,
                1.0,
                1.0,
            );
        }
    }

    /// Тик проводников: раз в 40 тиков пересчитать рамку, выдать эффект и урон.
    pub(super) fn tick_conduits(&self) {
        let pending = std::mem::take(&mut *self.sim().pending_conduits.lock().unwrap());
        for pos in pending {
            self.sim().conduits.lock().unwrap().entry(pos).or_default();
        }
        let simulated = self.simulated_set();
        let positions: Vec<[i32; 3]> = self
            .sim()
            .conduits
            .lock()
            .unwrap()
            .keys()
            .copied()
            .collect();
        for pos in positions {
            if !simulated.is_empty() && !simulated.contains(&(pos[0] >> 4, pos[2] >> 4)) {
                continue;
            }
            self.tick_conduit(pos);
        }
    }

    fn tick_conduit(&self, pos: [i32; 3]) {
        if !self
            .block(pos)
            .is_some_and(|s| self.state_name(s) == "minecraft:conduit")
        {
            self.sim().conduits.lock().unwrap().remove(&pos);
            return;
        }
        if self.game_time.load(Ordering::Relaxed) % CHECK_INTERVAL != 0 {
            return;
        }
        let (size, active) = self.conduit_shape(pos);
        let hunting = size >= MIN_KILL_SIZE;
        let old = {
            let mut map = self.sim().conduits.lock().unwrap();
            let c = map.entry(pos).or_default();
            let old = *c;
            *c = Conduit {
                size,
                active,
                hunting,
            };
            old
        };
        if active {
            self.conduit_effects(pos, size);
            self.play_sound(
                None,
                "minecraft:block.conduit.ambient",
                crate::sounds::BLOCKS,
                [
                    f64::from(pos[0]) + 0.5,
                    f64::from(pos[1]) + 0.5,
                    f64::from(pos[2]) + 0.5,
                ],
                1.0,
                1.0,
            );
            if hunting {
                self.conduit_attack(pos);
            }
        }
        if old.active != active {
            let sound = if active {
                "minecraft:block.conduit.activate"
            } else {
                "minecraft:block.conduit.deactivate"
            };
            self.play_sound(
                None,
                sound,
                crate::sounds::BLOCKS,
                [
                    f64::from(pos[0]) + 0.5,
                    f64::from(pos[1]) + 0.5,
                    f64::from(pos[2]) + 0.5,
                ],
                1.0,
                1.0,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_has_42_positions_in_three_rings() {
        let blocks = frame_positions([10, 64, -3]);
        assert_eq!(blocks.len(), 42);
        // Концы осей общие для двух колец и входят один раз.
        assert_eq!(blocks.iter().filter(|p| **p == [12, 64, -3]).count(), 1);
        assert!(!blocks.contains(&[10, 64, -3]));
        // Полная рамка даёт максимальный радиус 96.
        assert_eq!(42 / 7 * 16, 96);
        assert_eq!(16 / 7 * 16, 32);
    }
}
