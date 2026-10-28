//! Поршни 26.3: `PistonStructureResolver` (до 12 блоков, слизь и мёд тянут
//! соседей, хрупкие блоки ломаются), условие срабатывания с квази-связностью.

use crate::placement::{Placement, step};

/// Больше двигать нельзя (`MAX_PUSH_DEPTH`).
pub const MAX_PUSH: usize = 12;
/// Тики анимации до установки блоков на место (`MovingPistonBlockEntity`).
pub const MOVE_TICKS: u64 = 3;

fn opposite(d: usize) -> usize {
    d ^ 1
}

fn add(p: [i32; 3], d: usize) -> [i32; 3] {
    let o = step(d);
    [p[0] + o[0], p[1] + o[1], p[2] + o[2]]
}

pub fn is_piston(name: &str) -> bool {
    name == "minecraft:piston" || name == "minecraft:sticky_piston"
}

/// Как блок реагирует на толчок (`PushReaction`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Reaction {
    Normal,
    Destroy,
    Block,
    PushOnly,
}

/// Блоки с блок-сущностью: поршень их не двигает.
fn has_entity(name: &str) -> bool {
    let p = name.strip_prefix("minecraft:").unwrap_or(name);
    matches!(
        p,
        "chest"
            | "trapped_chest"
            | "ender_chest"
            | "barrel"
            | "furnace"
            | "blast_furnace"
            | "smoker"
            | "hopper"
            | "dispenser"
            | "dropper"
            | "brewing_stand"
            | "enchanting_table"
            | "beacon"
            | "spawner"
            | "jukebox"
            | "lectern"
            | "beehive"
            | "bee_nest"
            | "campfire"
            | "soul_campfire"
            | "conduit"
            | "daylight_detector"
            | "chiseled_bookshelf"
            | "crafter"
            | "trial_spawner"
            | "vault"
            | "sculk_sensor"
            | "calibrated_sculk_sensor"
            | "sculk_shrieker"
            | "sculk_catalyst"
            | "creaking_heart"
            | "end_gateway"
            | "end_portal"
    )
}

pub struct Ctx<'a> {
    pub pl: &'a Placement,
    pub at: &'a dyn Fn([i32; 3]) -> u32,
    pub hardness: &'a dyn Fn(u32) -> f32,
    pub min_y: i32,
    pub max_y: i32,
}

impl Ctx<'_> {
    fn name(&self, s: u32) -> &str {
        self.pl.name(s).unwrap_or("")
    }

    fn air(&self, s: u32) -> bool {
        matches!(
            self.name(s),
            "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air" | ""
        )
    }

    /// `getPistonPushReaction`.
    pub fn reaction(&self, s: u32) -> Reaction {
        let name = self.name(s);
        let path = name.strip_prefix("minecraft:").unwrap_or(name);
        if matches!(
            path,
            "obsidian"
                | "crying_obsidian"
                | "respawn_anchor"
                | "reinforced_deepslate"
                | "piston_head"
                | "moving_piston"
        ) {
            return Reaction::Block;
        }
        if path.ends_with("glazed_terracotta") {
            return Reaction::PushOnly;
        }
        // Шалкер едет вместе с содержимым, даже если грани не прочные.
        if path.ends_with("shulker_box") {
            return Reaction::Normal;
        }
        let keeps = path.ends_with("_fence")
            || path.ends_with("_wall")
            || path.ends_with("_pane")
            || path.ends_with("_trapdoor")
            || path.ends_with("_fence_gate")
            || matches!(
                path,
                "iron_bars" | "chain" | "ladder" | "piston" | "sticky_piston" | "hopper"
            );
        let fragile = path.ends_with("_bed")
            || path.ends_with("_head")
            || path.ends_with("_skull")
            || matches!(
                path,
                "decorated_pot"
                    | "cobweb"
                    | "dragon_egg"
                    | "scaffolding"
                    | "comparator"
                    | "repeater"
            );
        if fragile || (!keeps && (0..6).all(|f| !self.pl.sturdy(s, f))) {
            return Reaction::Destroy;
        }
        Reaction::Normal
    }

    /// `PistonBaseBlock.isPushable`: `along` — направление проверки.
    fn pushable(&self, s: u32, pos: [i32; 3], push: usize, destroy: bool, along: usize) -> bool {
        if pos[1] < self.min_y || pos[1] > self.max_y {
            return false;
        }
        if self.air(s) {
            return true;
        }
        if (push == 0 && pos[1] == self.min_y) || (push == 1 && pos[1] == self.max_y) {
            return false;
        }
        if (self.hardness)(s) < 0.0 {
            return false;
        }
        let name = self.name(s);
        if is_piston(name) {
            return self.pl.prop(s, "extended") != Some("true");
        }
        match self.reaction(s) {
            Reaction::Block => false,
            Reaction::Destroy => destroy,
            Reaction::PushOnly => push == along,
            Reaction::Normal => !has_entity(name),
        }
    }

    fn sticky(&self, s: u32) -> bool {
        matches!(
            self.name(s),
            "minecraft:slime_block" | "minecraft:honey_block"
        )
    }

    fn stick(&self, a: u32, b: u32) -> bool {
        let (na, nb) = (self.name(a), self.name(b));
        if (na == "minecraft:honey_block" && nb == "minecraft:slime_block")
            || (na == "minecraft:slime_block" && nb == "minecraft:honey_block")
        {
            return false;
        }
        self.sticky(a) || self.sticky(b)
    }

    /// Сигнал поршня (`getNeighborSignal`): соседи, кроме лицевой стороны, и
    /// соседи блока сверху (квази-связность).
    pub fn powered(&self, w: &crate::redstone::World, pos: [i32; 3], facing: usize) -> bool {
        for d in 0..6 {
            if d != facing && w.signal(add(pos, d), d, true) > 0 {
                return true;
            }
        }
        let above = add(pos, 1);
        (0..6)
            .filter(|&d| d != 0)
            .any(|d| w.signal(add(above, d), d, true) > 0)
    }
}

/// Что сдвинет поршень: двигаемые блоки и ломаемые.
#[derive(Debug, Default, PartialEq)]
pub struct Plan {
    pub push: Vec<[i32; 3]>,
    pub destroy: Vec<[i32; 3]>,
}

/// `PistonStructureResolver.resolve`: `None` — не двигается.
pub fn resolve(c: &Ctx, piston: [i32; 3], facing: usize, extending: bool) -> Option<Plan> {
    let push = if extending { facing } else { opposite(facing) };
    let start = if extending {
        add(piston, facing)
    } else {
        add(add(piston, facing), facing)
    };
    let mut r = Resolver {
        c,
        piston,
        push,
        plan: Plan::default(),
    };
    let s = (c.at)(start);
    if !c.pushable(s, start, push, false, facing) {
        if extending && c.reaction(s) == Reaction::Destroy {
            r.plan.destroy.push(start);
            return Some(r.plan);
        }
        return None;
    }
    if !r.line(start, push) {
        return None;
    }
    let mut i = 0;
    while i < r.plan.push.len() {
        let p = r.plan.push[i];
        if c.sticky((c.at)(p)) && !r.branch(p) {
            return None;
        }
        i += 1;
    }
    Some(r.plan)
}

struct Resolver<'a, 'b> {
    c: &'a Ctx<'b>,
    piston: [i32; 3],
    push: usize,
    plan: Plan,
}

impl Resolver<'_, '_> {
    fn back(&self, p: [i32; 3], n: i32) -> [i32; 3] {
        let o = step(opposite(self.push));
        [p[0] + o[0] * n, p[1] + o[1] * n, p[2] + o[2] * n]
    }

    /// `addBlockLine`.
    fn line(&mut self, start: [i32; 3], along: usize) -> bool {
        let c = self.c;
        let mut s = (c.at)(start);
        if c.air(s)
            || !c.pushable(s, start, self.push, false, along)
            || start == self.piston
            || self.plan.push.contains(&start)
        {
            return true;
        }
        let mut count = 1usize;
        if count + self.plan.push.len() > MAX_PUSH {
            return false;
        }
        // Липкий блок тянет за собой цепочку позади.
        while c.sticky(s) {
            let p = self.back(start, count as i32);
            let prev = s;
            s = (c.at)(p);
            if c.air(s)
                || !c.stick(prev, s)
                || !c.pushable(s, p, self.push, false, opposite(self.push))
                || p == self.piston
            {
                break;
            }
            count += 1;
            if count + self.plan.push.len() > MAX_PUSH {
                return false;
            }
        }
        let mut pushed = 0usize;
        for i in (0..count).rev() {
            self.plan.push.push(self.back(start, i as i32));
            pushed += 1;
        }
        let mut i = 1;
        loop {
            let o = step(self.push);
            let p = [
                start[0] + o[0] * i,
                start[1] + o[1] * i,
                start[2] + o[2] * i,
            ];
            if let Some(idx) = self.plan.push.iter().position(|&q| q == p) {
                self.reorder(pushed, idx);
                for k in 0..=idx + pushed {
                    if k >= self.plan.push.len() {
                        break;
                    }
                    let q = self.plan.push[k];
                    if c.sticky((c.at)(q)) && !self.branch(q) {
                        return false;
                    }
                }
                return true;
            }
            let s = (c.at)(p);
            if c.air(s) {
                return true;
            }
            if !c.pushable(s, p, self.push, true, self.push) || p == self.piston {
                return false;
            }
            if c.reaction(s) == Reaction::Destroy {
                self.plan.destroy.push(p);
                return true;
            }
            if self.plan.push.len() >= MAX_PUSH {
                return false;
            }
            self.plan.push.push(p);
            pushed += 1;
            i += 1;
        }
    }

    /// `reorderListAtCollision`.
    fn reorder(&mut self, pushed: usize, idx: usize) {
        let v = &self.plan.push;
        let n = v.len();
        let mut out = v[..idx].to_vec();
        out.extend_from_slice(&v[n - pushed..]);
        out.extend_from_slice(&v[idx..n - pushed]);
        self.plan.push = out;
    }

    /// `addBranchingBlocks`: соседи липкого блока поперёк движения.
    fn branch(&mut self, p: [i32; 3]) -> bool {
        let c = self.c;
        let s = (c.at)(p);
        for d in 0..6 {
            if d / 2 != self.push / 2 {
                let rel = add(p, d);
                if c.stick((c.at)(rel), s) && !self.line(rel, d) {
                    return false;
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn table() -> (Placement, HashMap<&'static str, u32>) {
        let names = [
            "minecraft:air",
            "minecraft:stone",
            "minecraft:slime_block",
            "minecraft:obsidian",
            "minecraft:torch",
            "minecraft:shulker_box",
        ];
        let states = names
            .iter()
            .map(|n| Some((n.to_string(), Vec::new())))
            .collect();
        let mut sturdy = vec![63u8; names.len()];
        sturdy[0] = 0;
        sturdy[4] = 0;
        sturdy[5] = 0;
        let ids = HashMap::from([
            ("air", 0),
            ("stone", 1),
            ("slime", 2),
            ("obsidian", 3),
            ("torch", 4),
            ("shulker", 5),
        ]);
        (Placement::new(states, sturdy, Default::default()), ids)
    }

    fn ctx<'a>(pl: &'a Placement, at: &'a dyn Fn([i32; 3]) -> u32) -> Ctx<'a> {
        Ctx {
            pl,
            at,
            hardness: &|_| 1.0,
            min_y: -64,
            max_y: 319,
        }
    }

    #[test]
    fn pushes_line_and_breaks_torch() {
        let (pl, id) = table();
        // Поршень в 0,0,0 смотрит на восток (5): три камня и факел.
        let w: HashMap<[i32; 3], u32> = HashMap::from([
            ([1, 0, 0], id["stone"]),
            ([2, 0, 0], id["stone"]),
            ([3, 0, 0], id["stone"]),
            ([4, 0, 0], id["torch"]),
        ]);
        let at = |p: [i32; 3]| w.get(&p).copied().unwrap_or(0);
        let c = ctx(&pl, &at);
        let plan = resolve(&c, [0, 0, 0], 5, true).unwrap();
        assert_eq!(plan.push, vec![[1, 0, 0], [2, 0, 0], [3, 0, 0]]);
        assert_eq!(plan.destroy, vec![[4, 0, 0]]);
        // 13 блоков — слишком много; обсидиан — не двигается.
        let long: HashMap<[i32; 3], u32> = (1..=13).map(|x| ([x, 0, 0], id["stone"])).collect();
        let at2 = |p: [i32; 3]| long.get(&p).copied().unwrap_or(0);
        assert!(resolve(&ctx(&pl, &at2), [0, 0, 0], 5, true).is_none());
        let obs = HashMap::from([([1, 0, 0], id["stone"]), ([2, 0, 0], id["obsidian"])]);
        let at3 = |p: [i32; 3]| obs.get(&p).copied().unwrap_or(0);
        assert!(resolve(&ctx(&pl, &at3), [0, 0, 0], 5, true).is_none());
    }

    #[test]
    fn slime_drags_neighbours() {
        let (pl, id) = table();
        // Слизь перед поршнем и камень над ней — едут вместе.
        let w = HashMap::from([([1, 0, 0], id["slime"]), ([1, 1, 0], id["stone"])]);
        let at = |p: [i32; 3]| w.get(&p).copied().unwrap_or(0);
        let plan = resolve(&ctx(&pl, &at), [0, 0, 0], 5, true).unwrap();
        assert_eq!(plan.push, vec![[1, 0, 0], [1, 1, 0]]);
        // Липкий поршень втягивает слизь через одну клетку.
        let w2 = HashMap::from([([2, 0, 0], id["slime"]), ([2, 1, 0], id["stone"])]);
        let at2 = |p: [i32; 3]| w2.get(&p).copied().unwrap_or(0);
        let plan = resolve(&ctx(&pl, &at2), [0, 0, 0], 5, false).unwrap();
        assert_eq!(plan.push, vec![[2, 0, 0], [2, 1, 0]]);
    }

    #[test]
    fn shulker_moves_with_the_piston() {
        let (pl, id) = table();
        let w = HashMap::from([([1, 0, 0], id["shulker"])]);
        let at = |p: [i32; 3]| w.get(&p).copied().unwrap_or(0);
        let plan = resolve(&ctx(&pl, &at), [0, 0, 0], 5, true).unwrap();
        assert_eq!(plan.push, vec![[1, 0, 0]]);
        assert!(plan.destroy.is_empty());
    }
}
