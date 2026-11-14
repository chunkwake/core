//! Скалк-катализатор (`SculkCatalystBlockEntity`, `SculkSpreader`).
//!
//! Смерть моба в 8 блоках от катализатора при включённом `mobGriefing`
//! расползает скалк: BFS по `#minecraft:sculk_replaceable` от точки смерти
//! (до 8 блоков), блоки становятся `minecraft:sculk`, а на соседних
//! воздушных/водных гранях появляется `minecraft:sculk_vein`. Число
//! блоков зависит от опыта моба (4 опыта → 16 блоков, ограничено 120).

use super::*;
use std::collections::HashSet;

/// Радиус обнаружения смерти (`SculkCatalystBlockEntity.LISTENER_RADIUS`).
const LISTENER_RADIUS: f64 = 8.0;
/// Ограничение расползания от точки смерти.
const SPREAD_RADIUS: i32 = 8;

impl PlayPlan {
    /// Смерть моба рядом с катализатором: `SculkCatalystBlockEntity.bloom`.
    pub(super) fn sculk_catalyst_death(&self, pos: [f64; 3], charge: i32) {
        if !self.vars.flag("mob_griefing") {
            return;
        }
        let center = [
            pos[0].floor() as i32,
            pos[1].floor() as i32,
            pos[2].floor() as i32,
        ];
        let Some(catalyst) = self.sculk_catalyst_near(center) else {
            return;
        };
        let Some(tag) = self
            .survival
            .block_tags
            .get("minecraft:sculk_replaceable")
            .cloned()
        else {
            return;
        };
        let Some(sculk) = self.survival.placement.default_state("minecraft:sculk") else {
            return;
        };
        let vein = self
            .survival
            .placement
            .default_state("minecraft:sculk_vein");
        let count = (charge.max(1) * 4).clamp(4, 120) as usize;
        if let Some(state) = self.block(catalyst)
            && let Some(bloom) = self.survival.placement.with_prop(state, "bloom", "true")
        {
            self.set_block(catalyst, bloom);
        }
        let mut seen = HashSet::new();
        seen.insert(center);
        let mut frontier = vec![center];
        let mut converted = 0usize;
        while converted < count {
            let Some(p) = frontier.pop() else {
                break;
            };
            for dir in 0..6 {
                let q = crate::redstone::place(p, dir, 1);
                if !seen.insert(q) {
                    continue;
                }
                if (q[0] - center[0])
                    .abs()
                    .max((q[1] - center[1]).abs())
                    .max((q[2] - center[2]).abs())
                    > SPREAD_RADIUS
                {
                    continue;
                }
                let Some(state) = self.block(q) else {
                    continue;
                };
                if !tag.contains(self.state_name(state)) {
                    continue;
                }
                self.set_block(q, sculk);
                converted += 1;
                if let Some(base) = vein {
                    for d in 0..6 {
                        let at = crate::redstone::place(q, d, 1);
                        let Some(s) = self.block(at) else {
                            continue;
                        };
                        let name = self.state_name(s);
                        let water = crate::fluids::state_fluid(&self.survival.placement, s)
                            .is_some_and(|f| f.kind == crate::fluids::Kind::Water);
                        if (name.ends_with(":air") || water) && rand::random::<f32>() < 0.25 {
                            self.place_sculk_vein(at, base);
                        }
                    }
                }
                frontier.push(q);
                if converted >= count {
                    break;
                }
            }
        }
        if let Some(state) = self.block(catalyst)
            && let Some(bloom) = self.survival.placement.with_prop(state, "bloom", "false")
        {
            self.set_block(catalyst, bloom);
        }
    }

    /// Ближайший `minecraft:sculk_catalyst` в 8 блоках.
    fn sculk_catalyst_near(&self, center: [i32; 3]) -> Option<[i32; 3]> {
        let radius = LISTENER_RADIUS as i32;
        let mut best: Option<([i32; 3], i64)> = None;
        for dx in -radius..=radius {
            for dy in -radius..=radius {
                for dz in -radius..=radius {
                    let at = [center[0] + dx, center[1] + dy, center[2] + dz];
                    if self
                        .block(at)
                        .is_none_or(|s| self.state_name(s) != "minecraft:sculk_catalyst")
                    {
                        continue;
                    }
                    let d = i64::from(dx) * i64::from(dx)
                        + i64::from(dy) * i64::from(dy)
                        + i64::from(dz) * i64::from(dz);
                    if d <= (LISTENER_RADIUS as i64) * (LISTENER_RADIUS as i64)
                        && best.is_none_or(|(_, old)| d < old)
                    {
                        best = Some((at, d));
                    }
                }
            }
        }
        best.map(|(at, _)| at)
    }

    /// `SculkVeinBlock`: жилка с креплениями по соседним блокам.
    fn place_sculk_vein(&self, at: [i32; 3], mut state: u32) {
        let pl = &self.survival.placement;
        for (dir, name) in [
            (0, "down"),
            (1, "up"),
            (2, "north"),
            (3, "south"),
            (4, "west"),
            (5, "east"),
        ] {
            let n = crate::redstone::place(at, dir, 1);
            let supported = self.block(n).is_some_and(|s| {
                !chunkwake_world::has_flag(s, chunkwake_world::flags::AIR)
                    && crate::fluids::state_fluid(pl, s).is_none()
            });
            if let Some(next) = pl.with_prop(state, name, if supported { "true" } else { "false" })
            {
                state = next;
            }
        }
        let water = self
            .block(at)
            .and_then(|s| crate::fluids::state_fluid(pl, s))
            .is_some_and(|f| f.kind == crate::fluids::Kind::Water);
        if let Some(next) = pl.with_prop(state, "waterlogged", if water { "true" } else { "false" })
        {
            state = next;
        }
        self.set_block(at, state);
    }
}
