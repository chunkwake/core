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

    /// `EnderDragonFight.setDragonKilled`: активный выходной портал и
    /// драконье яйцо после первого убийства.
    pub(super) fn dragon_killed(&self) {
        if self.active_dim() != Dimension::End {
            return;
        }
        let first = {
            let mut fight = self.dragon_fight.lock().unwrap();
            if fight.killed {
                return;
            }
            fight.killed = true;
            let first = !fight.has_previously_killed;
            fight.has_previously_killed = true;
            first
        };
        self.save_dragon_fight();
        let pos = self.exit_portal_pos();
        self.end_podium(pos, true);
        if first {
            self.named_block([pos[0], pos[1] + 4, pos[2]], "minecraft:dragon_egg");
        }
    }

    /// `EnderDragonFight.spawnExitPortal`: подиум на высоте (0, 0),
    /// над бедроком не выше 63 и не ниже дна мира.
    fn exit_portal_pos(&self) -> [i32; 3] {
        let mut y = self.motion_blocking_top(0, 0).unwrap_or(self.min_y() + 1) - 1;
        while y > 63
            && self
                .block([0, y, 0])
                .is_some_and(|s| self.state_name(s) == "minecraft:bedrock")
        {
            y -= 1;
        }
        [0, y.max(self.min_y() + 1), 0]
    }

    /// `EndPodiumFeature.place`: сфера из бедрока/эндер-камня с колонной,
    /// активный центр — `end_portal`, по бокам настенные факелы.
    fn end_podium(&self, [x, y, z]: [i32; 3], active: bool) {
        for dy in -1..=32 {
            for dx in -4..=4 {
                for dz in -4..=4 {
                    let d2 = (dx * dx + dy * dy + dz * dz) as f64;
                    let inner = d2 < 2.5 * 2.5;
                    let outer = d2 < 3.5 * 3.5;
                    if !inner && !outer {
                        continue;
                    }
                    let p = [x + dx, y + dy, z + dz];
                    let name = if p[1] < y {
                        if inner {
                            "minecraft:bedrock"
                        } else {
                            "minecraft:end_stone"
                        }
                    } else if p[1] > y {
                        "minecraft:air"
                    } else if !inner {
                        "minecraft:bedrock"
                    } else if active {
                        "minecraft:end_portal"
                    } else {
                        "minecraft:air"
                    };
                    self.named_block(p, name);
                }
            }
        }
        // Колонна 4 блока в центре (перекрывает центральный портал).
        for i in 0..4 {
            self.named_block([x, y + i, z], "minecraft:bedrock");
        }
        // Настенные факелы на `pos.above(2)` смотрят наружу.
        for (dx, dz, facing) in [
            (0, -1, "north"),
            (0, 1, "south"),
            (-1, 0, "west"),
            (1, 0, "east"),
        ] {
            let Some(&base) = self.survival.block_defaults.get("minecraft:wall_torch") else {
                continue;
            };
            let state = self
                .survival
                .placement
                .with_prop(base, "facing", facing)
                .unwrap_or(base);
            self.set_block([x + dx, y + 2, z + dz], state);
        }
    }

    /// Поставить блок по имени, если состояние уже такое — не трогать.
    fn named_block(&self, pos: [i32; 3], name: &str) {
        let Some(&state) = self.survival.block_defaults.get(name) else {
            return;
        };
        if self.block(pos) == Some(state) {
            return;
        }
        self.set_block(pos, state);
    }
}
