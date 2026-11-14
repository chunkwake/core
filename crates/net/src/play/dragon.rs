//! Дракон Края: дыхание (`dragon_breath`) и лечение кристаллами.
//!
//! Дыхание — облако радиусом 2.5 блока на 3 секунды: 3 HP/с магического
//! урона игрокам и мобам внутри, частицы `minecraft:dragon_breath`.
//! Кристаллы Края лечат дракона на 1 HP раз в 10 тиков в 32 блоках и
//! показывают белый луч (`DATA_BEAM_TARGET`, индекс 8, сериализатор 11).

use super::*;
use crate::mobs::dist2;

/// Облако дыхания дракона.
pub(crate) struct Breath {
    pub pos: [f64; 3],
    pub until: i64,
    pub next_damage: i64,
}

const BREATH_RADIUS: f64 = 2.5;
const BREATH_TICKS: i64 = 60;
const BREATH_DAMAGE_TICKS: i64 = 10;
const BREATH_DAMAGE: f32 = 1.5;
const HEAL_RANGE: f64 = 32.0;
const HEAL_TICKS: i64 = 10;
const HEAL_AMOUNT: f32 = 1.0;

impl PlayPlan {
    /// Дыхание дракона в точке цели (`EnderDragon`).
    pub(super) fn spawn_dragon_breath(&self, target: Uuid) {
        let Some(feet) = self.hub.players.lock().unwrap().get(&target).map(|p| p.pos) else {
            return;
        };
        let now = self.game_time.load(Ordering::Relaxed);
        self.sim().breaths.lock().unwrap().push(Breath {
            pos: feet,
            until: now + BREATH_TICKS,
            next_damage: now + BREATH_DAMAGE_TICKS,
        });
        self.play_sound(
            None,
            "minecraft:entity.dragon_fireball.explode",
            crate::sounds::HOSTILE,
            feet,
            1.0,
            1.0,
        );
    }

    /// Тик дыхания и лечения кристаллами.
    pub(super) fn tick_dragons(&self) {
        self.tick_dragon_breaths();
        self.tick_dragon_crystals();
    }

    fn tick_dragon_breaths(&self) {
        let now = self.game_time.load(Ordering::Relaxed);
        let mut due = Vec::new();
        let clouds: Vec<[f64; 3]> = {
            let mut breaths = self.sim().breaths.lock().unwrap();
            breaths.retain(|b| b.until > now);
            for b in breaths.iter_mut() {
                if now >= b.next_damage {
                    b.next_damage = now + BREATH_DAMAGE_TICKS;
                    due.push(b.pos);
                }
            }
            breaths.iter().map(|b| b.pos).collect()
        };
        if clouds.is_empty() {
            return;
        }
        if let Some(&particle) = self
            .particle_types
            .lock()
            .unwrap()
            .get("minecraft:dragon_breath")
        {
            for at in &clouds {
                self.hub.send(
                    None,
                    play::level_particles(
                        self.ids.level_particles,
                        particle,
                        false,
                        *at,
                        [1.2, 0.1, 1.2],
                        0.01,
                        12,
                    ),
                );
            }
        }
        if due.is_empty() {
            return;
        }
        let players: Vec<(Uuid, [f64; 3])> = self
            .live
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, l)| l.health > 0.0)
            .map(|(&uuid, l)| (uuid, l.pos))
            .collect();
        for at in &due {
            for (uuid, pos) in &players {
                if (pos[0] - at[0]).abs() <= BREATH_RADIUS
                    && (pos[2] - at[2]).abs() <= BREATH_RADIUS
                    && (pos[1] - at[1]).abs() <= 2.0
                {
                    self.push_control(
                        *uuid,
                        Control::Hurt(BREATH_DAMAGE, Cause::Magic, Some(*at), 0.0),
                    );
                }
            }
        }
        let mut mobs = self.sim().mobs.lock().unwrap();
        for at in &due {
            for entry in mobs.values_mut() {
                if entry.mob.alive()
                    && (entry.mob.pos[0] - at[0]).abs() <= BREATH_RADIUS
                    && (entry.mob.pos[2] - at[2]).abs() <= BREATH_RADIUS
                    && (entry.mob.pos[1] - at[1]).abs() <= 2.0
                {
                    crate::mobs::hurt(
                        &mut entry.mob,
                        BREATH_DAMAGE,
                        crate::mobs::Source::at(*at, None),
                        0.0,
                    );
                }
            }
        }
    }

    /// `EndCrystal`: лечение дракона в 32 блоках и белый луч к нему.
    fn tick_dragon_crystals(&self) {
        if self.active_dim() != Dimension::End {
            return;
        }
        let now = self.game_time.load(Ordering::Relaxed);
        if now.rem_euclid(HEAL_TICKS) != 0 {
            return;
        }
        let crystals: Vec<(i32, [f64; 3])> = self
            .sim()
            .mobs
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, e)| e.mob.species == crate::mobs::Species::EndCrystal && e.mob.alive())
            .map(|(&id, e)| (id, e.mob.pos))
            .collect();
        let dragons: Vec<(u128, [f64; 3], f32)> = self
            .sim()
            .mobs
            .lock()
            .unwrap()
            .values()
            .filter(|e| e.mob.species == crate::mobs::Species::EnderDragon && e.mob.alive())
            .map(|e| (e.mob.uuid, e.mob.pos, e.mob.health))
            .collect();
        let mut packets = Vec::new();
        let mut heals = Vec::new();
        for (id, cpos) in &crystals {
            let target = dragons
                .iter()
                .filter(|d| dist2(d.1, *cpos) <= HEAL_RANGE * HEAL_RANGE)
                .min_by(|a, b| dist2(a.1, *cpos).total_cmp(&dist2(b.1, *cpos)));
            let beam = target.map(|d| {
                [
                    d.1[0].floor() as i32,
                    d.1[1].floor() as i32,
                    d.1[2].floor() as i32,
                ]
            });
            if let Some(d) = target
                && d.2 < crate::mobs::Species::EnderDragon.max_health()
            {
                heals.push((d.0, HEAL_AMOUNT));
            }
            packets.push((
                *id,
                play::entity_meta(
                    self.ids.set_entity_data,
                    *id,
                    &[(8, chunkwake_protocol::play::Meta::OptBlockPos(beam))],
                ),
            ));
        }
        if !heals.is_empty() {
            let mut mobs = self.sim().mobs.lock().unwrap();
            for (uuid, amount) in heals {
                if let Some(entry) = mobs.values_mut().find(|e| e.mob.uuid == uuid) {
                    let max = entry.mob.species.max_health();
                    entry.mob.health = (entry.mob.health + amount).min(max);
                }
            }
        }
        for (id, packet) in packets {
            self.hub.send_about(id, packet);
        }
    }
}
