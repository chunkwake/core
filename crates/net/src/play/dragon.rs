//! Дракон Края: дыхание (`dragon_breath`) и лечение кристаллами.
//!
//! Дыхание — настоящее `AreaEffectCloud` (радиус 5, 200 тиков, частица
//! `minecraft:dragon_breath`, эффект `instant_damage`), из которого флакон
//! набирает `minecraft:dragon_breath`. Кристаллы Края лечат дракона на 1 HP
//! раз в 10 тиков в 32 блоках и показывают белый луч (`DATA_BEAM_TARGET`,
//! индекс 8, сериализатор 11).

use super::*;
use crate::mobs::dist2;

const HEAL_RANGE: f64 = 32.0;
const HEAL_TICKS: i64 = 10;
const HEAL_AMOUNT: f32 = 1.0;

impl PlayPlan {
    /// Дыхание дракона в точке цели (`DragonSittingFlamingPhase`).
    pub(super) fn spawn_dragon_breath(&self, target: Uuid) {
        let Some(feet) = self.hub.players.lock().unwrap().get(&target).map(|p| p.pos) else {
            return;
        };
        self.spawn_dragon_breath_cloud(feet);
        self.play_sound(
            None,
            "minecraft:entity.dragon_fireball.explode",
            crate::sounds::HOSTILE,
            feet,
            1.0,
            1.0,
        );
    }

    /// Тик дыхания (облака) и лечения кристаллами.
    pub(super) fn tick_dragons(&self) {
        self.tick_dragon_crystals();
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
