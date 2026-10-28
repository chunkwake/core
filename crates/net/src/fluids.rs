//! Жидкости 26.3 (`FlowingFluid`, `WaterFluid`, `LavaFluid` в верхнем мире):
//! уровни, растекание с поиском спуска, бесконечные источники воды, падение,
//! смывание растений, встреча лавы и воды.

use crate::placement::{Placement, step};

/// Вода или лава.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Water,
    Lava,
}

impl Kind {
    pub fn block(self) -> &'static str {
        match self {
            Kind::Water => "minecraft:water",
            Kind::Lava => "minecraft:lava",
        }
    }

    /// Потеря уровня за блок (`getDropOff`).
    fn drop_off(self) -> u8 {
        match self {
            Kind::Water => 1,
            Kind::Lava => 2,
        }
    }

    /// Глубина поиска спуска (`getSlopeFindDistance`).
    fn slope(self) -> u32 {
        match self {
            Kind::Water => 4,
            Kind::Lava => 2,
        }
    }

    /// Тиков между шагами (`getTickDelay`).
    pub fn delay(self) -> u64 {
        match self {
            Kind::Water => 5,
            Kind::Lava => 30,
        }
    }
}

/// Состояние жидкости: количество 1–8, источник, падающая.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Fluid {
    pub kind: Kind,
    pub amount: u8,
    pub source: bool,
    pub falling: bool,
}

impl Fluid {
    pub fn source(kind: Kind) -> Self {
        Fluid {
            kind,
            amount: 8,
            source: true,
            falling: false,
        }
    }

    fn flowing(kind: Kind, amount: u8, falling: bool) -> Self {
        Fluid {
            kind,
            amount,
            source: false,
            falling,
        }
    }

    /// `level` блока (`createLegacyBlock`): источник 0, падающая 8, иначе 8 − количество.
    pub fn level(self) -> u8 {
        if self.source {
            0
        } else if self.falling {
            8
        } else {
            8 - self.amount
        }
    }
}

/// Жидкость блока: вода, лава, пузырьковый столб (источник воды).
pub fn fluid_of(name: &str, level: Option<&str>) -> Option<Fluid> {
    let kind = match name {
        "minecraft:water" | "minecraft:bubble_column" => Kind::Water,
        "minecraft:lava" => Kind::Lava,
        _ => return None,
    };
    let l: u8 = level.and_then(|v| v.parse().ok()).unwrap_or(0);
    Some(if l == 0 {
        Fluid::source(kind)
    } else if l >= 8 {
        Fluid::flowing(kind, 8, true)
    } else {
        Fluid::flowing(kind, 8 - l, false)
    })
}

/// Жидкость состояния: сам блок жидкости или заводнённый блок (`waterlogged=true` — источник воды).
pub fn state_fluid(pl: &Placement, s: u32) -> Option<Fluid> {
    if pl.prop(s, "waterlogged") == Some("true") {
        return Some(Fluid::source(Kind::Water));
    }
    fluid_of(pl.name(s).unwrap_or(""), pl.prop(s, "level"))
}

/// Что поменять в мире.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Change {
    /// Поставить жидкость (`None` — воздух); `wash` — смыть блок с дропом.
    Set {
        pos: [i32; 3],
        fluid: Option<Fluid>,
        wash: bool,
    },
    /// Лава встретила воду: блок вместо лавы и шипение.
    Solidify { pos: [i32; 3], block: &'static str },
}

pub struct Ctx<'a> {
    pub pl: &'a Placement,
    pub at: &'a dyn Fn([i32; 3]) -> u32,
}

const SIDES: [usize; 4] = [2, 3, 4, 5];

fn add(p: [i32; 3], d: usize) -> [i32; 3] {
    let o = step(d);
    [p[0] + o[0], p[1] + o[1], p[2] + o[2]]
}

impl Ctx<'_> {
    fn name(&self, s: u32) -> &str {
        self.pl.name(s).unwrap_or("")
    }

    pub fn fluid(&self, p: [i32; 3]) -> Option<Fluid> {
        state_fluid(self.pl, (self.at)(p))
    }

    fn air(&self, s: u32) -> bool {
        matches!(
            self.name(s),
            "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air" | ""
        )
    }

    /// `canHoldAnyFluid`: воздух, жидкость или блок без коллизии, кроме
    /// дверей, табличек, лестниц, тростника, порталов.
    fn holds(&self, s: u32) -> bool {
        if self.air(s) || fluid_of(self.name(s), None).is_some() {
            return true;
        }
        // Заводняемые блоки (сундуки, ступени, фонари…) жидкость не смывает.
        if self.pl.prop(s, "waterlogged").is_some() {
            return false;
        }
        let n = self.name(s);
        let path = n.strip_prefix("minecraft:").unwrap_or(n);
        let solidish = path.ends_with("_door")
            || path.contains("sign")
            || path.ends_with("_fence")
            || path.ends_with("_wall")
            || path.ends_with("_pane")
            || path.ends_with("_trapdoor")
            || path.ends_with("_fence_gate")
            || path.ends_with("_carpet")
            || path.ends_with("_pressure_plate")
            || path.ends_with("_bed")
            || path.ends_with("_slab")
            || path.ends_with("_stairs")
            || matches!(
                path,
                "ladder"
                    | "sugar_cane"
                    | "nether_portal"
                    | "end_portal"
                    | "structure_void"
                    | "iron_bars"
                    | "chain"
                    | "snow"
                    | "scaffolding"
                    | "cobweb"
                    | "piston_head"
                    | "moving_piston"
                    | "lectern"
                    | "hopper"
                    | "repeater"
                    | "comparator"
                    | "daylight_detector"
                    | "cake"
                    | "flower_pot"
                    | "lantern"
                    | "soul_lantern"
                    | "end_rod"
                    | "lightning_rod"
                    | "bell"
                    | "anvil"
                    | "enchanting_table"
            );
        !solidish && (0..6).all(|f| !self.pl.sturdy(s, f))
    }

    /// `canPassThroughWall`: грань между `p` и соседом по `d` не закрыта
    /// ни одним из блоков (заводнённая ступень не льёт сквозь спинку).
    fn wall_open(&self, p: [i32; 3], d: usize) -> bool {
        let (a, b) = ((self.at)(p), (self.at)(add(p, d)));
        if self.pl.sturdy(a, d) || self.pl.sturdy(b, d ^ 1) {
            return false;
        }
        // `mergedFaceOccludes`: нижняя и верхняя половины боковой грани вместе — целая.
        !matches!((self.half(a, d), self.half(b, d)), (Some(x), Some(y)) if x != y)
    }

    /// Боковая грань плиты: `Some(true)` — верхняя половина, `Some(false)` — нижняя.
    fn half(&self, s: u32, d: usize) -> Option<bool> {
        if d < 2 || !self.name(s).ends_with("_slab") {
            return None;
        }
        match self.pl.prop(s, "type")? {
            "top" => Some(true),
            "bottom" => Some(false),
            _ => None,
        }
    }

    /// `isSourceBlockOfThisType`.
    fn own_source(&self, p: [i32; 3], kind: Kind) -> bool {
        self.fluid(p).is_some_and(|f| f.kind == kind && f.source)
    }

    /// `canMaybePassThrough`.
    fn passes(&self, to: [i32; 3], kind: Kind) -> bool {
        !self.own_source(to, kind) && self.holds((self.at)(to))
    }

    /// `isWaterHole`: снизу та же жидкость или место для неё.
    fn hole(&self, below: [i32; 3], kind: Kind) -> bool {
        self.fluid(below).is_some_and(|f| f.kind == kind) || self.holds((self.at)(below))
    }

    /// `canBeReplacedWith`: чем можно заменить жидкость в `to`, приходя по `dir`.
    fn replaceable(&self, to: [i32; 3], with: Kind, dir: usize) -> bool {
        match self.fluid(to) {
            None => true,
            Some(f) if f.kind == Kind::Water => dir == 0 && with != Kind::Water,
            Some(f) => with == Kind::Water && (f.amount as f32 / 9.0) >= 0.444_444_45,
        }
    }

    /// `getNewLiquid`: каким должно быть место `p` по соседям.
    pub fn new_liquid(&self, p: [i32; 3], kind: Kind) -> Option<Fluid> {
        let mut max = 0u8;
        let mut sources = 0;
        for d in SIDES {
            if let Some(f) = self
                .fluid(add(p, d))
                .filter(|f| f.kind == kind && self.wall_open(p, d))
            {
                if f.source {
                    sources += 1;
                }
                max = max.max(f.amount);
            }
        }
        // Вода: два источника рядом над твёрдым или над источником — новый источник.
        if kind == Kind::Water && sources >= 2 {
            let below = add(p, 0);
            let bs = (self.at)(below);
            // Верхняя грань блока снизу (`isFaceSturdy` вверх) или источник под клеткой.
            if self.pl.sturdy(bs, 1) || self.own_source(below, kind) {
                return Some(Fluid::source(kind));
            }
        }
        if self.wall_open(p, 1) && self.fluid(add(p, 1)).is_some_and(|f| f.kind == kind) {
            return Some(Fluid::flowing(kind, 8, true));
        }
        let k = max.saturating_sub(kind.drop_off());
        (k > 0).then(|| Fluid::flowing(kind, k, false))
    }

    /// `getSlopeDistance`: шагов до спуска (1000 — нет в пределах поиска).
    fn slope(&self, p: [i32; 3], depth: u32, from: usize, kind: Kind) -> u32 {
        let mut best = 1000;
        for d in SIDES {
            if d == from {
                continue;
            }
            let rel = add(p, d);
            if self.wall_open(p, d) && self.passes(rel, kind) {
                if self.wall_open(rel, 0) && self.hole(add(rel, 0), kind) {
                    return depth;
                }
                if depth < kind.slope() {
                    best = best.min(self.slope(rel, depth + 1, d ^ 1, kind));
                }
            }
        }
        best
    }

    /// `getSpread`: стороны, куда течь (ближайшие к спуску).
    fn spread_dirs(&self, p: [i32; 3], kind: Kind) -> Vec<([i32; 3], Fluid)> {
        let mut best = 1000;
        let mut out = Vec::new();
        for d in SIDES {
            let rel = add(p, d);
            if !self.wall_open(p, d) || !self.passes(rel, kind) {
                continue;
            }
            let Some(nf) = self.new_liquid(rel, kind) else {
                continue;
            };
            let dist = if self.wall_open(rel, 0) && self.hole(add(rel, 0), kind) {
                0
            } else {
                self.slope(rel, 1, d ^ 1, kind)
            };
            if dist < best {
                out.clear();
            }
            if dist <= best {
                if self.replaceable(rel, kind, d) {
                    out.push((rel, nf));
                }
                best = dist;
            }
        }
        out
    }

    fn spread_to(&self, out: &mut Vec<Change>, to: [i32; 3], dir: usize, f: Fluid) {
        let s = (self.at)(to);
        let dest = self.fluid(to);
        // Вода в лаву: источник — обсидиан, течение — булыжник.
        if f.kind == Kind::Water
            && let Some(lava) = dest.filter(|d| d.kind == Kind::Lava)
        {
            out.push(Change::Solidify {
                pos: to,
                block: if lava.source {
                    "minecraft:obsidian"
                } else {
                    "minecraft:cobblestone"
                },
            });
            return;
        }
        // Лава, падая в воду, — камень.
        if dir == 0 && f.kind == Kind::Lava && dest.is_some_and(|w| w.kind == Kind::Water) {
            out.push(Change::Solidify {
                pos: to,
                block: "minecraft:stone",
            });
            return;
        }
        let wash = !self.air(s) && fluid_of(self.name(s), None).is_none();
        out.push(Change::Set {
            pos: to,
            fluid: Some(f),
            wash,
        });
    }

    /// `FlowingFluid.tick`: пересчитать текущую жидкость и растечься.
    pub fn tick(&self, p: [i32; 3]) -> Vec<Change> {
        let mut out = Vec::new();
        let Some(mut f) = self.fluid(p) else {
            return out;
        };
        if !f.source {
            match self.new_liquid(p, f.kind) {
                None => {
                    out.push(Change::Set {
                        pos: p,
                        fluid: None,
                        wash: false,
                    });
                    return out;
                }
                Some(n) if n != f => {
                    out.push(Change::Set {
                        pos: p,
                        fluid: Some(n),
                        wash: false,
                    });
                    f = n;
                }
                _ => {}
            }
        }
        // `spread`: сперва вниз, потом в стороны.
        let below = add(p, 0);
        if self.wall_open(p, 0) && self.passes(below, f.kind) && self.replaceable(below, f.kind, 0)
        {
            let falling = Fluid::flowing(f.kind, 8, true);
            self.spread_to(&mut out, below, 0, falling);
            let sources = SIDES
                .iter()
                .filter(|&&d| self.own_source(add(p, d), f.kind))
                .count();
            if sources >= 3 {
                self.sides(&mut out, p, f);
            }
            return out;
        }
        if f.source || !(self.wall_open(p, 0) && self.hole(below, f.kind)) {
            self.sides(&mut out, p, f);
        }
        out
    }

    fn sides(&self, out: &mut Vec<Change>, p: [i32; 3], f: Fluid) {
        let amount = if f.falling {
            7
        } else {
            f.amount.saturating_sub(f.kind.drop_off())
        };
        if amount == 0 {
            return;
        }
        for (rel, nf) in self.spread_dirs(p, f.kind) {
            let d = SIDES
                .iter()
                .copied()
                .find(|&d| add(p, d) == rel)
                .unwrap_or(2);
            self.spread_to(out, rel, d, nf);
        }
    }

    /// `LiquidBlock.shouldSpreadLiquid` для лавы: вода рядом (не снизу) —
    /// обсидиан из источника, булыжник из текущей; над песком душ у синего льда — базальт.
    pub fn lava_meets(&self, p: [i32; 3]) -> Option<&'static str> {
        let f = self.fluid(p).filter(|f| f.kind == Kind::Lava)?;
        let soul = self.name((self.at)(add(p, 0))) == "minecraft:soul_soil";
        for d in [1, 2, 3, 4, 5] {
            let rel = add(p, d);
            if self.fluid(rel).is_some_and(|w| w.kind == Kind::Water) {
                return Some(if f.source {
                    "minecraft:obsidian"
                } else {
                    "minecraft:cobblestone"
                });
            }
            if soul && self.name((self.at)(rel)) == "minecraft:blue_ice" {
                return Some("minecraft:basalt");
            }
        }
        None
    }
}

/// Луч взгляда по блокам (`ClipContext`): первый блок, где `hit` истинно,
/// и грань входа (0–5). `None` — в пределах `range` ничего.
pub fn trace(
    eye: [f64; 3],
    dir: [f64; 3],
    range: f64,
    hit: impl Fn([i32; 3]) -> bool,
) -> Option<([i32; 3], usize)> {
    let mut p = eye.map(|v| v.floor() as i32);
    let mut t_max = [0.0f64; 3];
    let mut t_delta = [f64::INFINITY; 3];
    let mut stepv = [0i32; 3];
    for a in 0..3 {
        if dir[a] > 0.0 {
            stepv[a] = 1;
            t_delta[a] = 1.0 / dir[a];
            t_max[a] = (p[a] as f64 + 1.0 - eye[a]) * t_delta[a];
        } else if dir[a] < 0.0 {
            stepv[a] = -1;
            t_delta[a] = -1.0 / dir[a];
            t_max[a] = (eye[a] - p[a] as f64) * t_delta[a];
        } else {
            t_max[a] = f64::INFINITY;
        }
    }
    // Грань входа по оси: x → запад/восток (4/5), y → низ/верх, z → север/юг.
    let face = |a: usize, s: i32| match (a, s > 0) {
        (0, true) => 4,
        (0, false) => 5,
        (1, true) => 0,
        (1, false) => 1,
        (_, true) => 2,
        (_, false) => 3,
    };
    let mut entered = 1;
    loop {
        if hit(p) {
            return Some((p, entered));
        }
        let a = if t_max[0] < t_max[1] {
            if t_max[0] < t_max[2] { 0 } else { 2 }
        } else if t_max[1] < t_max[2] {
            1
        } else {
            2
        };
        if t_max[a] > range {
            return None;
        }
        p[a] += stepv[a];
        t_max[a] += t_delta[a];
        entered = face(a, stepv[a]);
    }
}

/// `LevelEvent.LAVA_FIZZ`.
pub const FIZZ: i32 = 1501;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn table() -> (Placement, HashMap<String, u32>) {
        let mut states = vec![
            Some(("minecraft:air".to_owned(), Vec::new())),
            Some(("minecraft:stone".to_owned(), Vec::new())),
        ];
        let mut ids = HashMap::from([("air".to_owned(), 0), ("stone".to_owned(), 1)]);
        for k in ["water", "lava"] {
            for l in 0..16 {
                ids.insert(format!("{k}{l}"), states.len() as u32);
                states.push(Some((
                    format!("minecraft:{k}"),
                    vec![("level".to_owned(), l.to_string())],
                )));
            }
        }
        ids.insert("logged".to_owned(), states.len() as u32);
        states.push(Some((
            "minecraft:oak_stairs".to_owned(),
            vec![("waterlogged".to_owned(), "true".to_owned())],
        )));
        ids.insert("top".to_owned(), states.len() as u32);
        states.push(Some(("minecraft:stone_slab".to_owned(), Vec::new())));
        ids.insert("back".to_owned(), states.len() as u32);
        states.push(Some((
            "minecraft:stone_stairs".to_owned(),
            vec![("waterlogged".to_owned(), "true".to_owned())],
        )));
        for (k, t) in [("low", "bottom"), ("high", "top")] {
            ids.insert(k.to_owned(), states.len() as u32);
            states.push(Some((
                "minecraft:oak_slab".to_owned(),
                vec![
                    ("type".to_owned(), t.to_owned()),
                    ("waterlogged".to_owned(), "true".to_owned()),
                ],
            )));
        }
        let mut sturdy = vec![0u8; states.len()];
        sturdy[1] = 63;
        sturdy[ids["top"] as usize] = 1 << 1;
        // Спинка ступени — на восток, низ целый.
        sturdy[ids["back"] as usize] = 1 | 1 << 5;
        sturdy[ids["low"] as usize] = 1;
        sturdy[ids["high"] as usize] = 1 << 1;
        (Placement::new(states, sturdy, Default::default()), ids)
    }

    #[test]
    fn source_spreads_over_floor() {
        let (pl, id) = table();
        let mut w: HashMap<[i32; 3], u32> = HashMap::new();
        for x in -10..10 {
            for z in -10..10 {
                w.insert([x, -1, z], id["stone"]);
            }
        }
        w.insert([0, 0, 0], id["water0"]);
        let at = |p: [i32; 3]| w.get(&p).copied().unwrap_or(0);
        let c = Ctx { pl: &pl, at: &at };
        let ch = c.tick([0, 0, 0]);
        // Ровный пол: во все четыре стороны уровнем 7.
        assert_eq!(ch.len(), 4);
        assert!(ch.iter().all(
            |c| matches!(c, Change::Set { fluid: Some(f), .. } if f.amount == 7 && !f.source)
        ));
        assert_eq!(Fluid::flowing(Kind::Water, 7, false).level(), 1);
    }

    #[test]
    fn falls_and_finds_holes() {
        let (pl, id) = table();
        let mut w: HashMap<[i32; 3], u32> = HashMap::new();
        for x in -10..10 {
            for z in -10..10 {
                w.insert([x, -1, z], id["stone"]);
            }
        }
        // Яма в двух блоках к востоку: течёт только туда.
        w.remove(&[2, -1, 0]);
        w.insert([0, 0, 0], id["water0"]);
        let at = |p: [i32; 3]| w.get(&p).copied().unwrap_or(0);
        let c = Ctx { pl: &pl, at: &at };
        let ch = c.tick([0, 0, 0]);
        assert_eq!(
            ch,
            vec![Change::Set {
                pos: [1, 0, 0],
                fluid: Some(Fluid::flowing(Kind::Water, 7, false)),
                wash: false
            }]
        );
        // Над пустотой — падает.
        let w2 = HashMap::from([([0, 5, 0], id["water0"])]);
        let at2 = |p: [i32; 3]| w2.get(&p).copied().unwrap_or(0);
        let c2 = Ctx { pl: &pl, at: &at2 };
        assert_eq!(
            c2.tick([0, 5, 0]),
            vec![Change::Set {
                pos: [0, 4, 0],
                fluid: Some(Fluid::flowing(Kind::Water, 8, true)),
                wash: false
            }]
        );
    }

    #[test]
    fn floor_simulation() {
        let (pl, id) = table();
        let mut w: HashMap<[i32; 3], u32> = HashMap::new();
        for x in 28..=38 {
            for z in -4..=4 {
                w.insert([x, 199, z], id["stone"]);
            }
        }
        w.insert([33, 200, 0], id["water0"]);
        let mut due = vec![[33, 200, 0]];
        for _ in 0..40 {
            let mut all = Vec::new();
            for p in std::mem::take(&mut due) {
                let at = |q: [i32; 3]| w.get(&q).copied().unwrap_or(0);
                all.extend(Ctx { pl: &pl, at: &at }.tick(p));
            }
            for c in all {
                if let Change::Set { pos, fluid, .. } = c {
                    let s = fluid.map_or(0, |f| id[&format!("water{}", f.level())]);
                    if w.get(&pos).copied().unwrap_or(0) != s {
                        w.insert(pos, s);
                        for d in 0..6 {
                            due.push(add(pos, d));
                        }
                        due.push(pos);
                    }
                }
            }
            due.retain(|p| w.get(p).is_some_and(|&s| s >= 2));
            due.sort();
            due.dedup();
        }
        let lvl = |p: [i32; 3]| w.get(&p).copied().unwrap_or(0);
        // Как в ванили: спуск по z (край в 5 шагах) ближе, чем по x — течёт только туда.
        assert_eq!(lvl([34, 200, 0]), 0);
        assert_eq!(
            (1..=4).map(|z| lvl([33, 200, z])).collect::<Vec<_>>(),
            [1, 2, 3, 4].map(|l| id[&format!("water{l}")])
        );
        assert_eq!(lvl([33, 199, 5]), id["water8"]);
    }

    #[test]
    fn waterlogged_respects_faces() {
        let (pl, id) = table();
        let w = HashMap::from([([0, 0, 0], id["back"])]);
        let at = |p: [i32; 3]| w.get(&p).copied().unwrap_or(0);
        let ch = Ctx { pl: &pl, at: &at }.tick([0, 0, 0]);
        let to: Vec<[i32; 3]> = ch
            .iter()
            .filter_map(|c| match c {
                Change::Set { pos, .. } => Some(*pos),
                _ => None,
            })
            .collect();
        // Вниз нельзя (целый низ), на восток — спинка; остальные стороны текут.
        assert!(
            !to.contains(&[0, -1, 0]) && !to.contains(&[1, 0, 0]),
            "{to:?}"
        );
        assert!(to.contains(&[-1, 0, 0]), "{to:?}");
    }

    #[test]
    fn opposite_slab_halves_close_the_face() {
        let (pl, id) = table();
        // Заводнённая нижняя плита, к востоку — верхняя: вместе грань закрыта.
        let w = HashMap::from([
            ([0, -1, 0], id["stone"]),
            ([0, 0, 0], id["low"]),
            ([1, 0, 0], id["high"]),
        ]);
        let at = |p: [i32; 3]| w.get(&p).copied().unwrap_or(0);
        let ctx = Ctx { pl: &pl, at: &at };
        assert!(!ctx.wall_open([0, 0, 0], 5));
        assert!(ctx.wall_open([0, 0, 0], 4));
    }

    #[test]
    fn waterlogged_is_source() {
        let (pl, id) = table();
        let w = HashMap::from([
            ([0, -1, 0], id["stone"]),
            ([1, -1, 0], id["stone"]),
            ([0, 0, 0], id["logged"]),
        ]);
        let at = |p: [i32; 3]| w.get(&p).copied().unwrap_or(0);
        let ch = Ctx { pl: &pl, at: &at }.tick([0, 0, 0]);
        assert!(
            ch.contains(&Change::Set {
                pos: [0, 0, 1],
                fluid: Some(Fluid::flowing(Kind::Water, 7, false)),
                wash: false
            }),
            "{ch:?}"
        );
    }

    #[test]
    fn trace_hits_face() {
        let hit = trace([0.5, 1.6, 0.5], [0.0, 0.0, -1.0], 5.0, |p| p == [0, 1, -3]).unwrap();
        assert_eq!(hit, ([0, 1, -3], 3));
        assert_eq!(
            trace([0.5, 1.6, 0.5], [0.0, -1.0, 0.0], 5.0, |p| p[1] == -1),
            Some(([0, -1, 0], 1))
        );
        assert!(trace([0.5, 1.6, 0.5], [1.0, 0.0, 0.0], 3.0, |p| p[0] == 9).is_none());
    }

    #[test]
    fn infinite_source_and_lava_meets_water() {
        let (pl, id) = table();
        let w = HashMap::from([
            ([0, -1, 0], id["stone"]),
            ([-1, 0, 0], id["water0"]),
            ([1, 0, 0], id["water0"]),
            ([5, 0, 0], id["lava0"]),
            ([6, 0, 0], id["water3"]),
            ([5, 0, 3], id["lava4"]),
            ([5, 0, 4], id["water0"]),
        ]);
        let at = |p: [i32; 3]| w.get(&p).copied().unwrap_or(0);
        let c = Ctx { pl: &pl, at: &at };
        assert_eq!(
            c.new_liquid([0, 0, 0], Kind::Water),
            Some(Fluid::source(Kind::Water))
        );
        assert_eq!(c.lava_meets([5, 0, 0]), Some("minecraft:obsidian"));
        assert_eq!(c.lava_meets([5, 0, 3]), Some("minecraft:cobblestone"));
        // Остальные стороны закрыты камнем, иначе вода уходит в ближайший спуск.
        let into = HashMap::from([
            ([0, -1, 0], id["stone"]),
            ([0, 0, 0], id["water0"]),
            ([1, 0, 0], id["lava0"]),
            ([-1, 0, 0], id["stone"]),
            ([0, 0, 1], id["stone"]),
            ([0, 0, -1], id["stone"]),
            ([3, -1, 0], id["stone"]),
            ([3, 0, 0], id["water0"]),
            ([4, 0, 0], id["lava1"]),
            ([2, 0, 0], id["stone"]),
            ([3, 0, 1], id["stone"]),
            ([3, 0, -1], id["stone"]),
        ]);
        let at = |p: [i32; 3]| into.get(&p).copied().unwrap_or(0);
        let c = Ctx { pl: &pl, at: &at };
        let spread = c.tick([0, 0, 0]);
        assert!(spread.iter().any(|c| matches!(
            c,
            Change::Solidify {
                pos: [1, 0, 0],
                block: "minecraft:obsidian"
            }
        )));
        let spread = c.tick([3, 0, 0]);
        assert!(spread.iter().any(|c| matches!(
            c,
            Change::Solidify {
                pos: [4, 0, 0],
                block: "minecraft:cobblestone"
            }
        )));
        // Прочная только сверху — источник всё равно образуется.
        let w = HashMap::from([
            ([0, -1, 0], id["top"]),
            ([-1, 0, 0], id["water0"]),
            ([1, 0, 0], id["water0"]),
        ]);
        let at = |p: [i32; 3]| w.get(&p).copied().unwrap_or(0);
        let c = Ctx { pl: &pl, at: &at };
        assert_eq!(
            c.new_liquid([0, 0, 0], Kind::Water),
            Some(Fluid::source(Kind::Water))
        );
    }
}
