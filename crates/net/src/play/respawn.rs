//! Точка возрождения: якорь возрождения, кровать вне оверворлда и выбор места
//! после смерти (`ServerPlayer.findRespawnPositionAndUseSpawnBlock`).

use super::{AIR, Dimension, PlayPlan, SpawnPoint, with_dim};
use crate::Uuid;

const ANCHOR: &str = "minecraft:respawn_anchor";

/// `RespawnAnchorBlock.RESPAWN_HORIZONTAL_OFFSETS`, затем они же ниже, выше
/// и клетка над якорем.
fn anchor_offsets() -> impl Iterator<Item = [i32; 3]> {
    const H: [[i32; 3]; 8] = [
        [0, 0, -1],
        [-1, 0, 0],
        [0, 0, 1],
        [1, 0, 0],
        [-1, 0, -1],
        [1, 0, -1],
        [-1, 0, 1],
        [1, 0, 1],
    ];
    H.into_iter()
        .chain(H.map(|[x, _, z]| [x, -1, z]))
        .chain(H.map(|[x, _, z]| [x, 1, z]))
        .chain(std::iter::once([0, 1, 0]))
}

/// `ServerPlayer.RespawnPosAngle.calculateLookAtYaw`: взгляд на низ центра блока.
fn look_at_yaw(from: [f64; 3], block: [i32; 3]) -> f32 {
    let dx = block[0] as f64 + 0.5 - from[0];
    let dz = block[2] as f64 + 0.5 - from[2];
    let len = (dx * dx + (block[1] as f64 - from[1]).powi(2) + dz * dz).sqrt();
    if len < 1.0e-5 {
        return -90.0;
    }
    wrap((dz / len).atan2(dx / len) * 57.295_776_367_187_5 - 90.0) as f32
}

fn wrap(a: f64) -> f64 {
    let w = a % 360.0;
    if w >= 180.0 {
        w - 360.0
    } else if w < -180.0 {
        w + 360.0
    } else {
        w
    }
}

/// Куда вернуть игрока: измерение, точка, взгляд; `missing` — блока точки
/// больше нет (`TeleportTransition.missingRespawnBlock`).
pub(super) struct RespawnAt {
    pub dim: Dimension,
    pub pos: [f64; 3],
    pub look: (f32, f32),
    pub missing: bool,
}

/// Чем кончилось использование якоря без светокамня.
pub(super) enum AnchorUse {
    /// Заряда нет — действие предмета.
    Pass,
    /// Точка та же: ничего.
    Same,
    /// Новая точка: сообщение игроку.
    Set,
    /// Не Ад: якорь взорвался.
    Exploded,
}

impl PlayPlan {
    /// Коробки коллизии; без `shapes.csv` твёрдое — полный куб.
    fn collision_boxes(&self, state: u32) -> Vec<[f64; 6]> {
        if let Some(p) = self.survival.physics.as_deref() {
            return p.boxes_of(state).to_vec();
        }
        if chunkwake_world::has_flag(state, chunkwake_world::flags::SOLID) {
            vec![[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]]
        } else {
            Vec::new()
        }
    }

    /// `EntityType.isBlockDangerous` для игрока: горящее, колючее, снег.
    fn dangerous_for_player(&self, state: u32) -> bool {
        let name = self.state_name(state);
        matches!(
            name,
            "minecraft:fire"
                | "minecraft:soul_fire"
                | "minecraft:lava"
                | "minecraft:magma_block"
                | "minecraft:lava_cauldron"
                | "minecraft:wither_rose"
                | "minecraft:sweet_berry_bush"
                | "minecraft:cactus"
                | "minecraft:powder_snow"
        ) || (name.ends_with("campfire")
            && self.survival.placement.prop(state, "lit") == Some("true"))
    }

    /// `DismountHelper.findSafeDismountLocation` для игрока (0,6 × 1,8).
    fn safe_dismount(&self, pos: [i32; 3], check_dangerous: bool) -> Option<[f64; 3]> {
        let state = self.block(pos)?;
        if check_dangerous && self.dangerous_for_player(state) {
            return None;
        }
        // `DismountHelper.blockFloorHeight`.
        let top = |s: u32| {
            self.collision_boxes(s)
                .iter()
                .map(|b| b[4])
                .fold(f64::NEG_INFINITY, f64::max)
        };
        let here = top(state);
        let floor = if here.is_finite() {
            here
        } else {
            let below = top(self.block([pos[0], pos[1] - 1, pos[2]])?);
            if below >= 1.0 {
                below - 1.0
            } else {
                f64::NEG_INFINITY
            }
        };
        if !floor.is_finite() || floor >= 1.0 {
            return None;
        }
        if check_dangerous
            && floor <= 0.0
            && self
                .block([pos[0], pos[1] - 1, pos[2]])
                .is_some_and(|s| self.dangerous_for_player(s))
        {
            return None;
        }
        let at = [pos[0] as f64 + 0.5, pos[1] as f64 + floor, pos[2] as f64 + 0.5];
        let lo = [at[0] - 0.3, at[1], at[2] - 0.3];
        let hi = [at[0] + 0.3, at[1] + 1.8, at[2] + 0.3];
        const EPS: f64 = 1.0e-7;
        for x in lo[0].floor() as i32..=hi[0].floor() as i32 {
            for z in lo[2].floor() as i32..=hi[2].floor() as i32 {
                for y in lo[1].floor() as i32 - 1..=hi[1].floor() as i32 {
                    let s = self.block([x, y, z])?;
                    let hit = self.collision_boxes(s).iter().any(|b| {
                        let (bx, by, bz) = (x as f64, y as f64, z as f64);
                        bx + b[0] < hi[0] - EPS
                            && bx + b[3] > lo[0] + EPS
                            && by + b[1] < hi[1] - EPS
                            && by + b[4] > lo[1] + EPS
                            && bz + b[2] < hi[2] - EPS
                            && bz + b[5] > lo[2] + EPS
                    });
                    if hit {
                        return None;
                    }
                }
            }
        }
        // `BlockTags.INVALID_SPAWN_INSIDE`.
        let inside = |p: [i32; 3]| {
            self.block(p).is_some_and(|s| {
                matches!(self.state_name(s), "minecraft:end_portal" | "minecraft:end_gateway")
            })
        };
        if inside(pos) || inside([pos[0], pos[1] + 1, pos[2]]) {
            return None;
        }
        Some(at)
    }

    /// `RespawnAnchorBlock.findStandUpPosition`: сначала без опасных блоков.
    fn anchor_stand_up(&self, anchor: [i32; 3]) -> Option<[f64; 3]> {
        [true, false].into_iter().find_map(|safe| {
            anchor_offsets().find_map(|[dx, dy, dz]| {
                self.safe_dismount([anchor[0] + dx, anchor[1] + dy, anchor[2] + dz], safe)
            })
        })
    }

    /// `Block.isPossibleToRespawnInThis`: не твёрдое и не жидкость.
    fn can_respawn_in(&self, pos: [i32; 3]) -> bool {
        self.block(pos).is_some_and(|s| {
            !chunkwake_world::has_flag(s, chunkwake_world::flags::SOLID)
                && !matches!(self.state_name(s), "minecraft:water" | "minecraft:lava")
        })
    }

    /// `ServerPlayer.findRespawnAndUseSpawnBlock` в измерении точки.
    fn find_respawn(
        &self,
        uuid: Uuid,
        point: SpawnPoint,
        use_charge: bool,
    ) -> Option<([f64; 3], (f32, f32))> {
        let pos = point.pos;
        let state = self.block(pos)?;
        let name = self.state_name(state);
        if name == ANCHOR
            && point.dim == Dimension::Nether
            && let Some((s, charge)) = self.anchor_charge(pos)
            && (point.forced || charge > 0)
        {
            let at = self.anchor_stand_up(pos);
            if !point.forced
                && use_charge
                && at.is_some()
                && let Some(next) = self
                    .survival
                    .placement
                    .with_prop(s, "charges", &(charge - 1).to_string())
            {
                self.set_block(pos, next);
                self.redstone_update(&crate::redstone::around(pos));
            }
            return at.map(|v| (v, (look_at_yaw(v, pos), 0.0)));
        }
        if name.ends_with("_bed") && point.dim == Dimension::Overworld {
            return self
                .bed_spawn(pos, uuid)
                .map(|v| (v, (look_at_yaw(v, pos), 0.0)));
        }
        if !point.forced {
            return None;
        }
        (self.can_respawn_in(pos) && self.can_respawn_in([pos[0], pos[1] + 1, pos[2]])).then(|| {
            (
                [pos[0] as f64 + 0.5, pos[1] as f64 + 0.1, pos[2] as f64 + 0.5],
                (point.angle[0], point.angle[1]),
            )
        })
    }

    /// Место возрождения. `use_block` — после смерти (якорь тратит заряд),
    /// при выходе из Края — нет.
    pub(super) fn respawn_target(&self, uuid: Uuid, use_block: bool) -> RespawnAt {
        let world = RespawnAt {
            dim: Dimension::Overworld,
            pos: self.vars.world_spawn().unwrap_or(self.spawn),
            look: (0.0, 0.0),
            missing: false,
        };
        let point = self.spawn_points.lock().unwrap().get(&uuid).copied().flatten();
        let Some(point) = point.filter(|p| self.dims.contains_key(&p.dim)) else {
            return world;
        };
        match with_dim(point.dim, || self.find_respawn(uuid, point, use_block)) {
            Some((pos, look)) => RespawnAt {
                dim: point.dim,
                pos,
                look,
                missing: false,
            },
            None => RespawnAt {
                missing: true,
                ..world
            },
        }
    }

    /// Якорь в точке игрока: после возрождения у него звук разрядки.
    pub(super) fn anchor_at_spawn(&self, uuid: Uuid) -> Option<[i32; 3]> {
        let point = self.spawn_points.lock().unwrap().get(&uuid).copied().flatten()?;
        with_dim(point.dim, || {
            self.block(point.pos)
                .is_some_and(|s| self.state_name(s) == ANCHOR)
                .then_some(point.pos)
        })
    }

    /// `RespawnAnchorBlock.useWithoutItem`.
    pub(super) fn use_anchor(&self, uuid: Uuid, pos: [i32; 3]) -> AnchorUse {
        let Some((_, charge)) = self.anchor_charge(pos) else {
            return AnchorUse::Pass;
        };
        if charge == 0 {
            return AnchorUse::Pass;
        }
        let dim = self.active_dim();
        if dim != Dimension::Nether {
            self.explode_anchor(pos);
            return AnchorUse::Exploded;
        }
        let mut points = self.spawn_points.lock().unwrap();
        let same = points
            .get(&uuid)
            .copied()
            .flatten()
            .is_some_and(|p| p.dim == dim && p.pos == pos);
        if same {
            return AnchorUse::Same;
        }
        points.insert(
            uuid,
            Some(SpawnPoint {
                dim,
                pos,
                angle: [0.0, 0.0],
                forced: false,
            }),
        );
        drop(points);
        self.block_sound(None, "minecraft:block.respawn_anchor.set_spawn", pos, 1.0, 1.0);
        AnchorUse::Set
    }

    /// `RespawnAnchorBlock.explode`: блок убран без дропа, взрыв 5 с огнём.
    /// Вода рядом, которая хлынет в яму, гасит лучи в клетке якоря.
    fn explode_anchor(&self, pos: [i32; 3]) {
        let water_at = |p: [i32; 3]| {
            self.block(p)
                .and_then(|s| crate::fluids::state_fluid(&self.survival.placement, s))
                .filter(|f| f.kind == crate::fluids::Kind::Water)
        };
        // `isWaterThatWouldFlow`: источник или уровень ≥ 2 без воды под ним.
        let would_flow = |p: [i32; 3]| {
            water_at(p).is_some_and(|f| {
                f.source || (f.amount >= 2 && water_at([p[0], p[1] - 1, p[2]]).is_none())
            })
        };
        let near = [[1, 0, 0], [-1, 0, 0], [0, 0, 1], [0, 0, -1]]
            .iter()
            .any(|d| would_flow([pos[0] + d[0], pos[1], pos[2] + d[2]]));
        let shield = near || water_at([pos[0], pos[1] + 1, pos[2]]).is_some();
        self.set_block_entity(pos, None);
        self.set_block(pos, AIR);
        self.redstone_update(&crate::redstone::around(pos));
        let center = pos.map(|v| v as f64 + 0.5);
        self.explode_with(center, 5.0, false, true, shield.then_some(pos));
    }

    /// `BedBlock.destroyOnUse`: обе половины убраны, взрыв 5 с огнём в изголовье.
    pub(super) fn explode_bed(&self, head: [i32; 3], foot: [i32; 3]) {
        for p in [head, foot] {
            self.wake_bed(p);
            self.set_block_entity(p, None);
            self.set_block(p, AIR);
        }
        let mut touched = crate::redstone::around(head).to_vec();
        touched.extend_from_slice(&crate::redstone::around(foot));
        self.redstone_update(&touched);
        let center = head.map(|v| v as f64 + 0.5);
        self.explode_with(center, 5.0, false, true, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchor_offsets_follow_vanilla_order() {
        let all: Vec<_> = anchor_offsets().collect();
        assert_eq!(all.len(), 25);
        assert_eq!(all[0], [0, 0, -1]);
        assert_eq!(all[8], [0, -1, -1]);
        assert_eq!(all[16], [0, 1, -1]);
        assert_eq!(all[24], [0, 1, 0]);
    }

    #[test]
    fn look_at_faces_the_block() {
        // Игрок к западу от блока смотрит на восток (−90).
        assert!((look_at_yaw([-0.5, 64.0, 0.5], [0, 64, 0]) + 90.0).abs() < 1e-3);
        // К северу (меньше z) — на юг (0).
        assert!(look_at_yaw([0.5, 64.0, -0.5], [0, 64, 0]).abs() < 1e-3);
    }
}
