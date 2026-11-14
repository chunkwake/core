//! Визер (`WitherBoss`): босс-бар `entity.minecraft.wither`, полёт и черепа.
//!
//! Босс-бар видят игроки в 48 блоках, прогресс — `health / 300`; при смерти
//! визера бар снимается. Череп (`minecraft:wither_skull`) летит со скоростью
//! 0.4 без гравитации, при попадании в блок/сущность взрывается (сила 1,
//! `mobGriefing`) и наносит 8 урона прямой цели.

use super::*;
use crate::mobs::dist2;
use std::collections::HashSet;

/// `WitherBoss` — максимальное здоровье.
const WITHER_HEALTH: f32 = 300.0;
/// Дальность, на которой виден босс-бар.
const BAR_RANGE: f64 = 48.0;
/// Скорость черепа за тик.
const SKULL_SPEED: f64 = 0.4;
/// Предел жизни черепа (тиков).
const SKULL_LIFE: i32 = 100;

pub(crate) struct WitherBar {
    bar: Uuid,
    players: HashSet<Uuid>,
    progress: f32,
}

pub(crate) struct WitherSkull {
    pos: [f64; 3],
    vel: [f64; 3],
    owner: u128,
    life: i32,
}

impl PlayPlan {
    /// Тик визеров: босс-бары и черепа.
    pub(super) fn tick_withers(&self) {
        self.tick_wither_bars();
        self.tick_wither_skulls();
    }

    fn tick_wither_bars(&self) {
        let dim = self.active_dim();
        let withers: Vec<(u128, [f64; 3], f32)> = self
            .sim()
            .mobs
            .lock()
            .unwrap()
            .values()
            .filter(|e| e.mob.species == crate::mobs::Species::Wither && e.mob.alive())
            .map(|e| (e.mob.uuid, e.mob.pos, e.mob.health))
            .collect();
        let players: Vec<(Uuid, [f64; 3])> = self
            .live
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, l)| l.dimension == dim && l.health > 0.0)
            .map(|(&uuid, l)| (uuid, l.pos))
            .collect();
        let mut packets: Vec<(Uuid, Vec<u8>)> = Vec::new();
        let mut bars = self.sim().wither_bars.lock().unwrap();
        for (uuid, pos, health) in &withers {
            let bar = bars.entry(*uuid).or_insert_with(|| WitherBar {
                bar: Uuid::from_u128(*uuid ^ 0x5749_5448_4552_0000_0000_0000_0000_0000),
                players: HashSet::new(),
                progress: -1.0,
            });
            let progress = (health / WITHER_HEALTH).clamp(0.0, 1.0);
            let near: HashSet<Uuid> = players
                .iter()
                .filter(|(_, p)| dist2(*p, *pos) <= BAR_RANGE * BAR_RANGE)
                .map(|(u, _)| *u)
                .collect();
            let lost: Vec<Uuid> = bar.players.difference(&near).copied().collect();
            for player in lost {
                bar.players.remove(&player);
                packets.push((
                    player,
                    chunkwake_protocol::play::boss_event_remove(self.ids.boss_event, bar.bar),
                ));
            }
            for player in near {
                if bar.players.insert(player) {
                    packets.push((
                        player,
                        chunkwake_protocol::play::boss_event_add(
                            self.ids.boss_event,
                            bar.bar,
                            &serde_json::json!({"translate": "entity.minecraft.wither"}),
                            progress,
                            5,
                            0,
                        ),
                    ));
                }
            }
            if (progress - bar.progress).abs() > 0.001 {
                bar.progress = progress;
                for player in &bar.players {
                    packets.push((
                        *player,
                        chunkwake_protocol::play::boss_event_progress(
                            self.ids.boss_event,
                            bar.bar,
                            progress,
                        ),
                    ));
                }
            }
        }
        let alive: HashSet<u128> = withers.iter().map(|w| w.0).collect();
        let gone: Vec<u128> = bars
            .keys()
            .filter(|u| !alive.contains(u))
            .copied()
            .collect();
        for uuid in gone {
            if let Some(bar) = bars.remove(&uuid) {
                for player in bar.players {
                    packets.push((
                        player,
                        chunkwake_protocol::play::boss_event_remove(self.ids.boss_event, bar.bar),
                    ));
                }
            }
        }
        drop(bars);
        for (player, packet) in packets {
            self.send_to(player, packet);
        }
    }

    /// `WitherBoss.performRangedAttack`: череп из глаз визера в цель.
    pub(super) fn mob_shoot_skull(&self, owner: i32, target: Uuid) {
        let Some((uuid, eye)) = self
            .sim()
            .mobs
            .lock()
            .unwrap()
            .get(&owner)
            .map(|e| (e.mob.uuid, e.mob.eye()))
        else {
            return;
        };
        let Some(feet) = self.hub.players.lock().unwrap().get(&target).map(|p| p.pos) else {
            return;
        };
        let to = [feet[0], feet[1] + crate::mobs::PLAYER_SIZE.1 * 0.6, feet[2]];
        self.spawn_wither_skull(eye, to, uuid);
    }

    fn spawn_wither_skull(&self, from: [f64; 3], to: [f64; 3], owner: u128) {
        let Some(&ty) = self.survival.entity_types.get("minecraft:wither_skull") else {
            return;
        };
        let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-6);
        let vel = [
            d[0] / len * SKULL_SPEED,
            d[1] / len * SKULL_SPEED,
            d[2] / len * SKULL_SPEED,
        ];
        let (yaw, pitch) = crate::arrows::rotation(vel);
        let id = self.next_entity_id.fetch_add(1, Ordering::Relaxed);
        let uuid = Uuid::from_u128(rand::random());
        self.hub.send(
            None,
            play::add_entity_moving(self.ids.add_entity, id, uuid, ty, from, vel, yaw, pitch, 0),
        );
        self.sim().wither_skulls.lock().unwrap().insert(
            id,
            WitherSkull {
                pos: from,
                vel,
                owner,
                life: 0,
            },
        );
    }

    fn tick_wither_skulls(&self) {
        let dim = self.active_dim();
        let players: Vec<(Uuid, [f64; 3])> = self
            .live
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, l)| l.dimension == dim && l.health > 0.0)
            .map(|(&uuid, l)| (uuid, l.pos))
            .collect();
        let mobs: Vec<(u128, [f64; 3], (f64, f64))> = self
            .sim()
            .mobs
            .lock()
            .unwrap()
            .values()
            .filter(|e| e.mob.alive())
            .map(|e| (e.mob.uuid, e.mob.pos, e.mob.size()))
            .collect();
        let ids: Vec<i32> = self
            .sim()
            .wither_skulls
            .lock()
            .unwrap()
            .keys()
            .copied()
            .collect();
        let mut packets: Vec<Vec<u8>> = Vec::new();
        let mut blasts: Vec<[f64; 3]> = Vec::new();
        let mut hits: Vec<(Uuid, [f64; 3])> = Vec::new();
        let mut mob_hits: Vec<(u128, [f64; 3])> = Vec::new();
        for id in ids {
            let (pos, vel, owner, life) = {
                let map = self.sim().wither_skulls.lock().unwrap();
                let Some(s) = map.get(&id) else { continue };
                (s.pos, s.vel, s.owner, s.life)
            };
            let next = [pos[0] + vel[0], pos[1] + vel[1], pos[2] + vel[2]];
            let block = [
                next[0].floor() as i32,
                next[1].floor() as i32,
                next[2].floor() as i32,
            ];
            let solid = self.block(block).is_some_and(|s| {
                crate::fluids::state_fluid(&self.survival.placement, s).is_none()
                    && !chunkwake_world::has_flag(s, chunkwake_world::flags::AIR)
            });
            let player = players.iter().find(|(_, p)| {
                (next[0] - p[0]).abs() <= 0.4
                    && next[1] >= p[1]
                    && next[1] <= p[1] + crate::mobs::PLAYER_SIZE.1
                    && (next[2] - p[2]).abs() <= 0.4
            });
            let mob = mobs.iter().find(|(uuid, p, size)| {
                *uuid != owner
                    && (next[0] - p[0]).abs() <= size.0 / 2.0
                    && next[1] >= p[1]
                    && next[1] <= p[1] + size.1
                    && (next[2] - p[2]).abs() <= size.0 / 2.0
            });
            if solid || player.is_some() || mob.is_some() || life >= SKULL_LIFE {
                self.sim().wither_skulls.lock().unwrap().remove(&id);
                packets.push(play::remove_entities(self.ids.remove_entities, &[id]));
                if let Some((uuid, _)) = player {
                    hits.push((*uuid, next));
                }
                if let Some((uuid, _, _)) = mob {
                    mob_hits.push((*uuid, next));
                }
                if solid || player.is_some() || mob.is_some() {
                    blasts.push(next);
                }
                continue;
            }
            {
                let mut map = self.sim().wither_skulls.lock().unwrap();
                if let Some(s) = map.get_mut(&id) {
                    s.pos = next;
                    s.vel = [vel[0] * 0.95, vel[1] * 0.95, vel[2] * 0.95];
                    s.life += 1;
                }
            }
            let (yaw, pitch) = crate::arrows::rotation(vel);
            packets.push(play::teleport_entity(
                self.ids.teleport_entity,
                id,
                next,
                yaw,
                pitch,
                false,
            ));
        }
        for packet in packets {
            self.hub.send(None, packet);
        }
        for (uuid, at) in hits {
            self.push_control(uuid, Control::Hurt(8.0, Cause::Magic, Some(at), 0.0));
        }
        if !mob_hits.is_empty() {
            let mut mobs = self.sim().mobs.lock().unwrap();
            for (uuid, at) in mob_hits {
                if let Some(e) = mobs.values_mut().find(|e| e.mob.uuid == uuid) {
                    crate::mobs::hurt(&mut e.mob, 8.0, crate::mobs::Source::at(at, None), 0.0);
                }
            }
        }
        for at in blasts {
            self.explode_mob(at, 1.0, "minecraft:wither_skull");
        }
    }
}
