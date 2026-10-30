//! `TreeFeature`: стволы, кроны, корни мангров, декораторы и пересчёт
//! `distance` листвы. Множества позиций ведут себя как `HashSet<BlockPos>`
//! Java — порядок их обхода решает, в каком порядке декораторы тратят
//! случайные числа.

use super::{add, block_set, Features, Level, Palette, Placed, Provider, DIRECTION_NAMES};
use crate::carver::{cos, IntP};
use crate::random::{Bits, Worldgen};
use serde_json::Value;
use std::sync::Arc;

type Pos = [i32; 3];

/// `java.util.HashSet<BlockPos>` с порядком обхода как у Java: корзины по
/// `hashCode`, внутри корзины — порядок вставки; ёмкость растёт вдвое при
/// заполнении на три четверти (деревья корзин не моделируются).
#[derive(Clone)]
pub(super) struct JavaSet {
    buckets: Vec<Vec<Pos>>,
    len: usize,
}

impl JavaSet {
    pub(super) fn new() -> Self {
        Self { buckets: vec![Vec::new(); 16], len: 0 }
    }

    fn hash(&[x, y, z]: &Pos) -> u32 {
        let h = y.wrapping_add(z.wrapping_mul(31)).wrapping_mul(31).wrapping_add(x) as u32;
        h ^ (h >> 16)
    }

    fn slot(&self, p: &Pos) -> usize {
        Self::hash(p) as usize & (self.buckets.len() - 1)
    }

    pub(super) fn insert(&mut self, p: Pos) -> bool {
        let s = self.slot(&p);
        if self.buckets[s].contains(&p) {
            return false;
        }
        self.buckets[s].push(p);
        self.len += 1;
        if self.len > self.buckets.len() * 3 / 4 {
            let cap = self.buckets.len() * 2;
            let mut next = vec![Vec::new(); cap];
            for b in std::mem::take(&mut self.buckets) {
                for p in b {
                    next[Self::hash(&p) as usize & (cap - 1)].push(p);
                }
            }
            self.buckets = next;
        }
        true
    }

    pub(super) fn contains(&self, p: &Pos) -> bool {
        self.buckets[self.slot(p)].contains(p)
    }

    pub(super) fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// `iterator().next()` с удалением.
    fn pop_first(&mut self) -> Option<Pos> {
        let b = self.buckets.iter_mut().find(|b| !b.is_empty())?;
        self.len -= 1;
        Some(b.remove(0))
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = &Pos> {
        self.buckets.iter().flatten()
    }

    /// `new ObjectArrayList(set)` с устойчивой сортировкой по y.
    fn sorted_by_y(&self) -> Vec<Pos> {
        let mut v: Vec<Pos> = self.iter().copied().collect();
        v.sort_by_key(|p| p[1]);
        v
    }
}

/// Горизонтальные направления в порядке `Plane.HORIZONTAL`: север, восток, юг, запад.
const HORIZONTAL: [Pos; 4] = [[0, 0, -1], [1, 0, 0], [0, 0, 1], [-1, 0, 0]];
/// Номер направления в `DIRS` (порядок `Direction`) для `HORIZONTAL[i]`.
const HORIZONTAL_DIR: [usize; 4] = [2, 5, 3, 4];

fn up(p: Pos, n: i32) -> Pos {
    [p[0], p[1] + n, p[2]]
}

fn step(p: Pos, d: Pos, n: i32) -> Pos {
    [p[0] + d[0] * n, p[1] + d[1] * n, p[2] + d[2] * n]
}

fn manhattan(a: Pos, b: Pos) -> i32 {
    (a[0] - b[0]).abs() + (a[1] - b[1]).abs() + (a[2] - b[2]).abs()
}

/// Ось направления `HORIZONTAL[i]`.
fn axis_of(i: usize) -> &'static str {
    if i.is_multiple_of(2) { "z" } else { "x" }
}

/// `Mth.floor` для float.
fn floor_f(v: f32) -> i32 {
    let i = v as i32;
    if v < i as f32 { i - 1 } else { i }
}

fn floor_d(v: f64) -> i32 {
    let i = v as i32;
    if v < i as f64 { i - 1 } else { i }
}

/// Теги и блоки, которые читают деревья.
#[derive(Debug)]
pub(super) struct Tags {
    logs: Arc<Vec<bool>>,
    leaves: Arc<Vec<bool>>,
    replaceable: Arc<Vec<bool>>,
    no_decay: Arc<Vec<bool>>,
    vine: u32,
    bee_nest: u32,
    cocoa: u32,
    creaking_heart: u32,
    shelf_mushroom: u32,
    hanging_moss: u32,
}

impl Tags {
    fn load(pal: &mut Palette) -> Result<Self, String> {
        let def = |pal: &Palette, n: &str| {
            pal.defaults
                .get(n)
                .copied()
                .ok_or_else(|| format!("no block {n}"))
        };
        Ok(Self {
            logs: pal.tag("minecraft:logs")?,
            leaves: pal.tag("minecraft:leaves")?,
            replaceable: pal.tag("minecraft:replaceable_by_trees")?,
            no_decay: pal.tag("minecraft:prevents_nearby_leaf_decay")?,
            vine: def(pal, "minecraft:vine")?,
            bee_nest: def(pal, "minecraft:bee_nest")?,
            cocoa: def(pal, "minecraft:cocoa")?,
            creaking_heart: def(pal, "minecraft:creaking_heart")?,
            shelf_mushroom: def(pal, "minecraft:shelf_mushroom").unwrap_or(u32::MAX),
            hanging_moss: def(pal, "minecraft:pale_hanging_moss")?,
        })
    }
}

#[derive(Clone, Debug)]
enum Size {
    Two { limit: i32, lower: i32, upper: i32 },
    Three { limit: i32, upper_limit: i32, lower: i32, middle: i32, upper: i32 },
}

impl Size {
    fn at(&self, h: i32, yo: i32) -> i32 {
        match *self {
            Self::Two { limit, lower, upper } => {
                if yo < limit { lower } else { upper }
            }
            Self::Three { limit, upper_limit, lower, middle, upper } => {
                if yo < limit {
                    lower
                } else if yo >= h - upper_limit {
                    upper
                } else {
                    middle
                }
            }
        }
    }
}

#[derive(Debug)]
enum TrunkKind {
    Straight,
    Fancy,
    Forking,
    DarkOak,
    Giant,
    MegaJungle,
    Bending { min_leaves: i32, bend: IntP },
    Upwards { steps: IntP, chance: f32, length: IntP, through: Arc<Vec<bool>> },
    Cherry { count: IntP, horizontal: IntP, start: (i32, i32), end: IntP },
    Poplar { above: IntP, branches: IntP },
}

#[derive(Debug)]
struct Trunk {
    base: i32,
    a: i32,
    b: i32,
    kind: TrunkKind,
}

#[derive(Debug)]
enum FoliageKind {
    Blob(i32),
    Fancy(i32),
    Bush(i32),
    Spruce(IntP),
    Pine(IntP),
    MegaPine(IntP),
    Acacia,
    DarkOak,
    MegaJungle(i32),
    RandomSpread(IntP, i32),
    Cherry { height: IntP, wide_hole: f32, corner_hole: f32, hanging: f32, extension: f32 },
    Poplar { height: IntP, side_hole: f32 },
}

#[derive(Debug)]
struct Foliage {
    radius: IntP,
    offset: IntP,
    kind: FoliageKind,
}

/// `FoliageAttachment`.
#[derive(Clone, Copy, Debug)]
struct Attachment {
    pos: Pos,
    radius_offset: i32,
    height_offset: i32,
    double: bool,
}

impl Attachment {
    fn new(pos: Pos, radius_offset: i32, double: bool) -> Self {
        Self { pos, radius_offset, height_offset: 0, double }
    }
}

#[derive(Debug)]
struct Roots {
    offset_y: IntP,
    provider: Provider,
    above: Option<(Provider, f32)>,
    through: Arc<Vec<bool>>,
    muddy_in: Arc<Vec<bool>>,
    muddy: Provider,
    max_width: i32,
    max_length: i32,
    skew: f32,
}

#[derive(Debug)]
enum Decorator {
    Beehive(f32),
    LeaveVine(f32),
    TrunkVine,
    Cocoa(f32),
    AlterGround(Provider),
    AttachedToLeaves { chance: f32, xz: i32, y: i32, provider: Provider, empty: i32, dirs: Vec<usize> },
    AttachedToLogs { chance: f32, provider: Provider, dirs: Vec<usize> },
    PlaceOnGround { tries: i32, radius: i32, height: i32, provider: Provider },
    CreakingHeart(f32),
    ShelfMushroom(f32),
    PaleMoss { leaves: f32, trunk: f32, ground: f32, patch: Option<Arc<Placed>> },
}

#[derive(Debug)]
pub(super) struct Tree {
    trunk_provider: Provider,
    trunk: Trunk,
    foliage_provider: Provider,
    foliage: Foliage,
    roots: Option<Roots>,
    size: Size,
    min_clipped: Option<i32>,
    decorators: Vec<Decorator>,
    ignore_vines: bool,
    below_trunk: Provider,
    tags: Tags,
}

fn dir_list(v: &Value) -> Result<Vec<usize>, String> {
    v.as_array()
        .ok_or("directions")?
        .iter()
        .map(|d| {
            let d = d.as_str().unwrap_or("");
            DIRECTION_NAMES
                .iter()
                .position(|n| *n == d)
                .ok_or_else(|| format!("direction {d}"))
        })
        .collect()
}

impl Tree {
    pub(super) fn parse(
        v: &Value,
        pal: &mut Palette,
        provider: &mut ProviderFn,
        placed: &mut PlacedFn,
    ) -> Result<Self, String> {
        let int = |v: &Value, k: &str, d: i64| v[k].as_i64().unwrap_or(d) as i32;
        let flt = |v: &Value, k: &str| v[k].as_f64().unwrap_or(0.0) as f32;
        let t = &v["trunk_placer"];
        let kind = match t["type"].as_str().unwrap_or("") {
            "minecraft:straight_trunk_placer" => TrunkKind::Straight,
            "minecraft:fancy_trunk_placer" => TrunkKind::Fancy,
            "minecraft:forking_trunk_placer" => TrunkKind::Forking,
            "minecraft:dark_oak_trunk_placer" => TrunkKind::DarkOak,
            "minecraft:giant_trunk_placer" => TrunkKind::Giant,
            "minecraft:mega_jungle_trunk_placer" => TrunkKind::MegaJungle,
            "minecraft:bending_trunk_placer" => TrunkKind::Bending {
                min_leaves: int(t, "min_height_for_leaves", 1),
                bend: IntP::parse(&t["bend_length"])?,
            },
            "minecraft:upwards_branching_trunk_placer" => TrunkKind::Upwards {
                steps: IntP::parse(&t["extra_branch_steps"])?,
                chance: flt(t, "place_branch_per_log_probability"),
                length: IntP::parse(&t["extra_branch_length"])?,
                through: block_set(&t["can_grow_through"], pal)?,
            },
            "minecraft:cherry_trunk_placer" => {
                let s = &t["branch_start_offset_from_top"];
                TrunkKind::Cherry {
                    count: IntP::parse(&t["branch_count"])?,
                    horizontal: IntP::parse(&t["branch_horizontal_length"])?,
                    start: (int(s, "min_inclusive", 0), int(s, "max_inclusive", 0)),
                    end: IntP::parse(&t["branch_end_offset_from_top"])?,
                }
            }
            "minecraft:poplar_trunk_placer" => TrunkKind::Poplar {
                above: IntP::parse(&t["trunk_height_above_branches"])?,
                branches: IntP::parse(&t["branch_amount"])?,
            },
            other => return Err(format!("trunk placer {other} is not supported")),
        };
        let trunk = Trunk {
            base: int(t, "base_height", 0),
            a: int(t, "height_rand_a", 0),
            b: int(t, "height_rand_b", 0),
            kind,
        };
        let f = &v["foliage_placer"];
        let fkind = match f["type"].as_str().unwrap_or("") {
            "minecraft:blob_foliage_placer" => FoliageKind::Blob(int(f, "height", 0)),
            "minecraft:fancy_foliage_placer" => FoliageKind::Fancy(int(f, "height", 0)),
            "minecraft:bush_foliage_placer" => FoliageKind::Bush(int(f, "height", 0)),
            "minecraft:spruce_foliage_placer" => FoliageKind::Spruce(IntP::parse(&f["trunk_height"])?),
            "minecraft:pine_foliage_placer" => FoliageKind::Pine(IntP::parse(&f["height"])?),
            "minecraft:mega_pine_foliage_placer" => FoliageKind::MegaPine(IntP::parse(&f["crown_height"])?),
            "minecraft:acacia_foliage_placer" => FoliageKind::Acacia,
            "minecraft:dark_oak_foliage_placer" => FoliageKind::DarkOak,
            "minecraft:jungle_foliage_placer" => FoliageKind::MegaJungle(int(f, "height", 0)),
            "minecraft:random_spread_foliage_placer" => FoliageKind::RandomSpread(
                IntP::parse(&f["foliage_height"])?,
                int(f, "leaf_placement_attempts", 0),
            ),
            "minecraft:cherry_foliage_placer" => FoliageKind::Cherry {
                height: IntP::parse(&f["height"])?,
                wide_hole: flt(f, "wide_bottom_layer_hole_chance"),
                corner_hole: flt(f, "corner_hole_chance"),
                hanging: flt(f, "hanging_leaves_chance"),
                extension: flt(f, "hanging_leaves_extension_chance"),
            },
            "minecraft:poplar_foliage_placer" => FoliageKind::Poplar {
                height: IntP::parse(&f["height"])?,
                side_hole: flt(f, "side_hole_chance"),
            },
            other => return Err(format!("foliage placer {other} is not supported")),
        };
        let foliage = Foliage {
            radius: IntP::parse(&f["radius"])?,
            offset: IntP::parse(&f["offset"])?,
            kind: fkind,
        };
        let s = &v["minimum_size"];
        let size = match s["type"].as_str().unwrap_or("") {
            "minecraft:two_layers_feature_size" => Size::Two {
                limit: int(s, "limit", 1),
                lower: int(s, "lower_size", 0),
                upper: int(s, "upper_size", 1),
            },
            "minecraft:three_layers_feature_size" => Size::Three {
                limit: int(s, "limit", 1),
                upper_limit: int(s, "upper_limit", 1),
                lower: int(s, "lower_size", 0),
                middle: int(s, "middle_size", 1),
                upper: int(s, "upper_size", 1),
            },
            other => return Err(format!("feature size {other} is not supported")),
        };
        let roots = match &v["root_placer"] {
            Value::Null => None,
            r => {
                let m = &r["mangrove_root_placement"];
                let above = match &r["above_root_placement"] {
                    Value::Null => None,
                    a => Some((
                        provider(&a["above_root_provider"], pal)?,
                        flt(a, "above_root_placement_chance"),
                    )),
                };
                Some(Roots {
                    offset_y: IntP::parse(&r["trunk_offset_y"])?,
                    provider: provider(&r["root_provider"], pal)?,
                    above,
                    through: block_set(&m["can_grow_through"], pal)?,
                    muddy_in: block_set(&m["muddy_roots_in"], pal)?,
                    muddy: provider(&m["muddy_roots_provider"], pal)?,
                    max_width: int(m, "max_root_width", 0),
                    max_length: int(m, "max_root_length", 0),
                    skew: flt(m, "random_skew_chance"),
                })
            }
        };
        let mut decorators = Vec::new();
        for d in v["decorators"].as_array().into_iter().flatten() {
            decorators.push(parse_decorator(d, pal, provider, placed)?);
        }
        Ok(Self {
            trunk_provider: provider(&v["trunk_provider"], pal)?,
            foliage_provider: provider(&v["foliage_provider"], pal)?,
            below_trunk: provider(&v["below_trunk_provider"], pal)?,
            min_clipped: s["min_clipped_height"].as_i64().map(|x| x as i32),
            ignore_vines: v["ignore_vines"].as_bool().unwrap_or(false),
            trunk,
            foliage,
            roots,
            size,
            decorators,
            tags: Tags::load(pal)?,
        })
    }
}

type ProviderFn<'a> = dyn FnMut(&Value, &mut Palette) -> Result<Provider, String> + 'a;
type PlacedFn<'a> = dyn FnMut(&Value, &mut Palette) -> Result<Arc<Placed>, String> + 'a;

fn parse_decorator(
    d: &Value,
    pal: &mut Palette,
    provider: &mut ProviderFn,
    placed: &mut PlacedFn,
) -> Result<Decorator, String> {
    let int = |v: &Value, k: &str, d: i64| v[k].as_i64().unwrap_or(d) as i32;
    let flt = |v: &Value, k: &str| v[k].as_f64().unwrap_or(0.0) as f32;
    Ok(match d["type"].as_str().unwrap_or("") {
        "minecraft:beehive" => Decorator::Beehive(flt(d, "probability")),
        "minecraft:leave_vine" => Decorator::LeaveVine(flt(d, "probability")),
        "minecraft:trunk_vine" => Decorator::TrunkVine,
        "minecraft:cocoa" => Decorator::Cocoa(flt(d, "probability")),
        "minecraft:alter_ground" => Decorator::AlterGround(provider(&d["provider"], pal)?),
        "minecraft:attached_to_leaves" => Decorator::AttachedToLeaves {
            chance: flt(d, "probability"),
            xz: int(d, "exclusion_radius_xz", 0),
            y: int(d, "exclusion_radius_y", 0),
            provider: provider(&d["block_provider"], pal)?,
            empty: int(d, "required_empty_blocks", 0),
            dirs: dir_list(&d["directions"])?,
        },
        "minecraft:attached_to_logs" => Decorator::AttachedToLogs {
            chance: flt(d, "probability"),
            provider: provider(&d["block_provider"], pal)?,
            dirs: dir_list(&d["directions"])?,
        },
        "minecraft:place_on_ground" => Decorator::PlaceOnGround {
            tries: int(d, "tries", 128),
            radius: int(d, "radius", 2),
            height: int(d, "height", 1),
            provider: provider(&d["block_state_provider"], pal)?,
        },
        "minecraft:creaking_heart" => Decorator::CreakingHeart(flt(d, "probability")),
        "minecraft:shelf_mushroom" => Decorator::ShelfMushroom(flt(d, "probability")),
        "minecraft:pale_moss" => Decorator::PaleMoss {
            leaves: flt(d, "leaves_probability"),
            trunk: flt(d, "trunk_probability"),
            ground: flt(d, "ground_probability"),
            patch: placed(&Value::String("minecraft:pale_moss_patch".into()), pal).ok(),
        },
        other => return Err(format!("tree decorator {other} is not supported")),
    })
}

/// Мир и множества одного дерева.
struct Ctx<'a> {
    features: &'a Features,
    level: &'a mut dyn Level,
    pal: &'a Palette,
    r: &'a mut Worldgen,
    roots: JavaSet,
    trunks: JavaSet,
    foliage: JavaSet,
    decorations: JavaSet,
}

impl Ctx<'_> {
    fn get(&self, p: Pos) -> u32 {
        self.level.get(p[0], p[1], p[2])
    }

    fn put(&mut self, p: Pos, state: u32) {
        if self.level.can_write(p[0], p[2]) {
            self.level.set(p[0], p[1], p[2], state);
        }
    }

    fn is_air(&self, p: Pos) -> bool {
        self.pal.is_air(self.get(p))
    }

    fn has(&self, mask: &[bool], p: Pos) -> bool {
        Palette::flag(mask, self.get(p))
    }

    /// `getState`: провайдер или блок на месте.
    fn state(&mut self, p: &Provider, at: Pos) -> u32 {
        match p.get(&*self.level, self.pal, self.r, at) {
            Some(s) => s,
            None => self.get(at),
        }
    }

    fn optional(&mut self, p: &Provider, at: Pos) -> Option<u32> {
        p.get(&*self.level, self.pal, self.r, at)
    }
}

impl Tree {
    /// `TreeFeature.validTreePos`.
    fn valid(&self, c: &Ctx, p: Pos) -> bool {
        c.is_air(p) || c.has(&self.tags.replaceable, p)
    }

    /// `validTreePos` ствола: у ветвистого ещё `can_grow_through`.
    fn trunk_valid(&self, c: &Ctx, p: Pos) -> bool {
        self.valid(c, p)
            || matches!(&self.trunk.kind, TrunkKind::Upwards { through, .. } if c.has(through, p))
    }

    fn is_free(&self, c: &Ctx, p: Pos) -> bool {
        self.trunk_valid(c, p) || c.has(&self.tags.logs, p)
    }

    fn set_trunk(&self, c: &mut Ctx, p: Pos, s: u32) {
        c.trunks.insert(p);
        c.put(p, s);
    }

    fn place_log_with(&self, c: &mut Ctx, p: Pos, axis: Option<&str>) -> bool {
        if !self.trunk_valid(c, p) {
            return false;
        }
        let mut s = c.state(&self.trunk_provider, p);
        if let Some(a) = axis {
            s = c.pal.with_prop(s, "axis", a).unwrap_or(s);
        }
        self.set_trunk(c, p, s);
        true
    }

    fn place_log(&self, c: &mut Ctx, p: Pos) -> bool {
        self.place_log_with(c, p, None)
    }

    fn below_trunk(&self, c: &mut Ctx, p: Pos) {
        if let Some(s) = c.optional(&self.below_trunk, p) {
            self.set_trunk(c, p, s);
        }
    }

    /// `TreeFeature.place`: поставлено ли дерево.
    pub(super) fn place(&self, features: &Features, level: &mut dyn Level, r: &mut Worldgen, origin: Pos) -> bool {
        let mut c = Ctx {
            features,
            level,
            pal: &features.palette,
            r,
            roots: JavaSet::new(),
            trunks: JavaSet::new(),
            foliage: JavaSet::new(),
            decorations: JavaSet::new(),
        };
        if !self.do_place(&mut c, origin) || (c.trunks.is_empty() && c.foliage.is_empty()) {
            return false;
        }
        for d in &self.decorators {
            self.tags.decorate(&mut c, d);
        }
        self.update_leaves(&mut c);
        true
    }

    fn do_place(&self, c: &mut Ctx, origin: Pos) -> bool {
        let t = &self.trunk;
        let h = t.base + c.r.next_int_bound(t.a + 1) + c.r.next_int_bound(t.b + 1);
        let fh = self.foliage_height(c, h);
        let trunk_h = h - fh;
        let radius = self.foliage.radius.sample(c.r)
            + if matches!(self.foliage.kind, FoliageKind::Pine(_)) {
                c.r.next_int_bound((trunk_h + 1).max(1))
            } else {
                0
            };
        let trunk_origin = match &self.roots {
            Some(roots) => up(origin, roots.offset_y.sample(c.r)),
            None => origin,
        };
        let min_y = origin[1].min(trunk_origin[1]);
        let max_y = origin[1].max(trunk_origin[1]) + h + 1;
        let (lo, top) = (c.level.min_y(), c.level.min_y() + c.level.height() - 1);
        if min_y < lo + 1 || max_y > top + 1 {
            return false;
        }
        let clipped = self.max_free_height(c, h, trunk_origin);
        if clipped < h && !self.min_clipped.is_some_and(|m| clipped >= m) {
            return false;
        }
        if let Some(roots) = &self.roots
            && !self.place_roots(c, roots, origin, trunk_origin)
        {
            return false;
        }
        let attachments = self.place_trunk(c, clipped, trunk_origin);
        for a in attachments {
            let offset = self.foliage.offset.sample(c.r);
            self.create_foliage(c, clipped, a, fh, radius, offset);
        }
        true
    }

    fn max_free_height(&self, c: &Ctx, h: i32, at: Pos) -> i32 {
        for y in 0..=h + 1 {
            let r = self.size.at(h, y);
            for x in -r..=r {
                for z in -r..=r {
                    let p = [at[0] + x, at[1] + y, at[2] + z];
                    if !self.is_free(c, p) || (!self.ignore_vines && c.get(p) == self.tags.vine) {
                        return y - 2;
                    }
                }
            }
        }
        h
    }

    fn foliage_height(&self, c: &mut Ctx, h: i32) -> i32 {
        match &self.foliage.kind {
            FoliageKind::Blob(n) | FoliageKind::Fancy(n) | FoliageKind::Bush(n) | FoliageKind::MegaJungle(n) => *n,
            FoliageKind::Spruce(trunk) => (h - trunk.sample(c.r)).max(4),
            FoliageKind::Pine(p) | FoliageKind::MegaPine(p) | FoliageKind::RandomSpread(p, _) => p.sample(c.r),
            FoliageKind::Cherry { height, .. } | FoliageKind::Poplar { height, .. } => height.sample(c.r),
            FoliageKind::Acacia => 0,
            FoliageKind::DarkOak => 4,
        }
    }

    // ---------------------------------------------------------------- корни

    fn root_ok(&self, c: &Ctx, roots: &Roots, p: Pos) -> bool {
        self.valid(c, p) || c.has(&roots.through, p)
    }

    fn place_roots(&self, c: &mut Ctx, roots: &Roots, origin: Pos, trunk: Pos) -> bool {
        let mut p = origin;
        while p[1] < trunk[1] {
            if !self.root_ok(c, roots, p) {
                return false;
            }
            p = up(p, 1);
        }
        let mut list = vec![up(trunk, -1)];
        for d in HORIZONTAL {
            let start = step(trunk, d, 1);
            let mut found = Vec::new();
            if !self.simulate_roots(c, roots, start, d, trunk, &mut found, 0) {
                return false;
            }
            list.extend(found);
            list.push(step(trunk, d, 1));
        }
        for p in list {
            self.place_root(c, roots, p);
        }
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn simulate_roots(
        &self,
        c: &mut Ctx,
        roots: &Roots,
        pos: Pos,
        dir: Pos,
        origin: Pos,
        out: &mut Vec<Pos>,
        layer: i32,
    ) -> bool {
        if layer == roots.max_length || out.len() as i32 > roots.max_length {
            return false;
        }
        let below = up(pos, -1);
        let next = step(pos, dir, 1);
        let width = manhattan(pos, origin);
        let list = if width > roots.max_width - 3 && width <= roots.max_width {
            if c.r.next_float() < roots.skew { vec![below, up(next, -1)] } else { vec![below] }
        } else if width > roots.max_width || c.r.next_float() < roots.skew {
            vec![below]
        } else if c.r.next_bool() {
            vec![next]
        } else {
            vec![below]
        };
        for p in list {
            if self.root_ok(c, roots, p) {
                out.push(p);
                if !self.simulate_roots(c, roots, p, dir, origin, out, layer + 1) {
                    return false;
                }
            }
        }
        true
    }

    fn waterlogged(&self, c: &Ctx, p: Pos, s: u32) -> u32 {
        let wet = c.pal.fluid(c.get(p)).is_water();
        c.pal.with_prop(s, "waterlogged", if wet { "true" } else { "false" }).unwrap_or(s)
    }

    fn place_root(&self, c: &mut Ctx, roots: &Roots, p: Pos) {
        if c.has(&roots.muddy_in, p) {
            let s = c.state(&roots.muddy, p);
            let s = self.waterlogged(c, p, s);
            c.roots.insert(p);
            c.put(p, s);
            return;
        }
        if !self.root_ok(c, roots, p) {
            return;
        }
        let s = c.state(&roots.provider, p);
        let s = self.waterlogged(c, p, s);
        c.roots.insert(p);
        c.put(p, s);
        if let Some((provider, chance)) = &roots.above {
            let above = up(p, 1);
            if c.r.next_float() < *chance && c.is_air(above) {
                let s = c.state(provider, above);
                let s = self.waterlogged(c, above, s);
                c.roots.insert(above);
                c.put(above, s);
            }
        }
    }

    // ---------------------------------------------------------------- стволы

    fn place_trunk(&self, c: &mut Ctx, h: i32, o: Pos) -> Vec<Attachment> {
        match &self.trunk.kind {
            TrunkKind::Straight => {
                self.below_trunk(c, up(o, -1));
                for y in 0..h {
                    self.place_log(c, up(o, y));
                }
                vec![Attachment::new(up(o, h), 0, false)]
            }
            TrunkKind::Fancy => self.fancy_trunk(c, h, o),
            TrunkKind::Forking => {
                self.below_trunk(c, up(o, -1));
                let mut out = Vec::new();
                let lean = c.r.next_int_bound(4) as usize;
                let lean_h = h - c.r.next_int_bound(4) - 1;
                let mut lean_steps = 3 - c.r.next_int_bound(3);
                let (mut tx, mut tz) = (o[0], o[2]);
                let mut ey = None;
                for yo in 0..h {
                    let yy = o[1] + yo;
                    if yo >= lean_h && lean_steps > 0 {
                        tx += HORIZONTAL[lean][0];
                        tz += HORIZONTAL[lean][2];
                        lean_steps -= 1;
                    }
                    if self.place_log(c, [tx, yy, tz]) {
                        ey = Some(yy + 1);
                    }
                }
                if let Some(y) = ey {
                    out.push(Attachment::new([tx, y, tz], 1, false));
                }
                let (mut tx, mut tz) = (o[0], o[2]);
                let branch = c.r.next_int_bound(4) as usize;
                if branch != lean {
                    let mut yo = lean_h - c.r.next_int_bound(2) - 1;
                    let mut steps = 1 + c.r.next_int_bound(3);
                    let mut ey = None;
                    while yo < h && steps > 0 {
                        if yo >= 1 {
                            let yy = o[1] + yo;
                            tx += HORIZONTAL[branch][0];
                            tz += HORIZONTAL[branch][2];
                            if self.place_log(c, [tx, yy, tz]) {
                                ey = Some(yy + 1);
                            }
                        }
                        yo += 1;
                        steps -= 1;
                    }
                    if let Some(y) = ey {
                        out.push(Attachment::new([tx, y, tz], 0, false));
                    }
                }
                out
            }
            TrunkKind::DarkOak => {
                let below = up(o, -1);
                for d in [[0, 0, 0], [1, 0, 0], [0, 0, 1], [1, 0, 1]] {
                    self.below_trunk(c, add(below, d));
                }
                let mut out = Vec::new();
                let lean = c.r.next_int_bound(4) as usize;
                let lean_h = h - c.r.next_int_bound(4);
                let mut lean_steps = 2 - c.r.next_int_bound(3);
                let (x, y, z) = (o[0], o[1], o[2]);
                let (mut tx, mut tz) = (x, z);
                let ey = y + h - 1;
                for dy in 0..h {
                    if dy >= lean_h && lean_steps > 0 {
                        tx += HORIZONTAL[lean][0];
                        tz += HORIZONTAL[lean][2];
                        lean_steps -= 1;
                    }
                    let p = [tx, y + dy, tz];
                    if c.is_air(p) || c.has(&self.tags.leaves, p) {
                        for d in [[0, 0, 0], [1, 0, 0], [0, 0, 1], [1, 0, 1]] {
                            self.place_log(c, add(p, d));
                        }
                    }
                }
                out.push(Attachment::new([tx, ey, tz], 0, true));
                for ox in -1..=2 {
                    for oz in -1..=2 {
                        if (!(0..=1).contains(&ox) || !(0..=1).contains(&oz)) && c.r.next_int_bound(3) <= 0 {
                            let len = c.r.next_int_bound(3) + 2;
                            for by in 0..len {
                                self.place_log(c, [x + ox, ey - by - 1, z + oz]);
                            }
                            out.push(Attachment::new([x + ox, ey, z + oz], 0, false));
                        }
                    }
                }
                out
            }
            TrunkKind::Giant => self.giant_trunk(c, h, o),
            TrunkKind::MegaJungle => {
                let mut out = self.giant_trunk(c, h, o);
                let mut bh = h - 2 - c.r.next_int_bound(4);
                while bh > h / 2 {
                    let angle = c.r.next_float() * std::f32::consts::TAU;
                    let (mut bx, mut bz) = (0, 0);
                    for b in 0..5 {
                        bx = (1.5f32 + cos(angle as f64) * b as f32) as i32;
                        bz = (1.5f32 + crate::carver::sin(angle as f64) * b as f32) as i32;
                        self.place_log(c, [o[0] + bx, o[1] + bh - 3 + b / 2, o[2] + bz]);
                    }
                    out.push(Attachment::new([o[0] + bx, o[1] + bh, o[2] + bz], -2, false));
                    bh -= 2 + c.r.next_int_bound(4);
                }
                out
            }
            TrunkKind::Bending { min_leaves, bend } => {
                let dir = c.r.next_int_bound(4) as usize;
                let log_h = h - 1;
                let mut p = o;
                self.below_trunk(c, up(p, -1));
                let mut out = Vec::new();
                for i in 0..=log_h {
                    if i + 1 >= log_h + c.r.next_int_bound(2) {
                        p = step(p, HORIZONTAL[dir], 1);
                    }
                    if self.valid(c, p) {
                        self.place_log(c, p);
                    }
                    if i >= *min_leaves {
                        out.push(Attachment::new(p, 0, false));
                    }
                    p = up(p, 1);
                }
                let len = bend.sample(c.r);
                for _ in 0..=len {
                    if self.valid(c, p) {
                        self.place_log(c, p);
                    }
                    out.push(Attachment::new(p, 0, false));
                    p = step(p, HORIZONTAL[dir], 1);
                }
                out
            }
            TrunkKind::Upwards { steps, chance, length, .. } => {
                let mut out = Vec::new();
                for hp in 0..h {
                    let cy = o[1] + hp;
                    let log = [o[0], cy, o[2]];
                    if self.place_log(c, log) && hp < h - 1 && c.r.next_float() < *chance {
                        let dir = c.r.next_int_bound(4) as usize;
                        let len = length.sample(c.r);
                        let pos = (len - length.sample(c.r) - 1).max(0);
                        let n = steps.sample(c.r);
                        self.upwards_branch(c, h, &mut out, log, cy, dir, pos, n);
                    }
                    if hp == h - 1 {
                        out.push(Attachment::new([o[0], cy + 1, o[2]], 0, false));
                    }
                }
                out
            }
            TrunkKind::Cherry { count, horizontal, start, end } => {
                self.below_trunk(c, up(o, -1));
                let first = (h - 1 + c.r.next_int_bound(start.1 - start.0 + 1) + start.0).max(0);
                let second_max = start.1 - 1;
                let mut second = (h - 1 + c.r.next_int_bound(second_max - start.0 + 1) + start.0).max(0);
                if second >= first {
                    second += 1;
                }
                let n = count.sample(c.r);
                let middle = n == 3;
                let both = n >= 2;
                let trunk_h = if middle {
                    h
                } else if both {
                    first.max(second) + 1
                } else {
                    first + 1
                };
                for y in 0..trunk_h {
                    self.place_log(c, up(o, y));
                }
                let mut out = Vec::new();
                if middle {
                    out.push(Attachment::new(up(o, trunk_h), 0, false));
                }
                let dir = c.r.next_int_bound(4) as usize;
                out.push(self.cherry_branch(c, h, o, dir, dir, first, first < trunk_h - 1, horizontal, end));
                if both {
                    out.push(self.cherry_branch(c, h, o, dir, (dir + 2) % 4, second, second < trunk_h - 1, horizontal, end));
                }
                out
            }
            TrunkKind::Poplar { above, branches } => {
                self.below_trunk(c, up(o, -1));
                let upto = h - above.sample(c.r);
                for y in 0..h {
                    self.place_log(c, up(o, y));
                    // `Direction.allShuffled` без вертикальных.
                    let mut all = [0usize, 1, 2, 3, 4, 5];
                    for i in (2..=6).rev() {
                        let j = c.r.next_int_bound(i as i32) as usize;
                        all.swap(i - 1, j);
                    }
                    let dirs: Vec<usize> = all.into_iter().filter(|&d| d >= 2).collect();
                    if upto - 1 == y {
                        let n = branches.sample(c.r);
                        for &d in dirs.iter().take(n.max(0) as usize) {
                            let hi = HORIZONTAL_DIR.iter().position(|&x| x == d).unwrap_or(0);
                            let p = step(up(o, y), HORIZONTAL[hi], 1);
                            self.place_log_with(c, p, Some(axis_of(hi)));
                        }
                    }
                }
                vec![Attachment::new(up(o, upto), 0, false)]
            }
        }
    }

    fn giant_trunk(&self, c: &mut Ctx, h: i32, o: Pos) -> Vec<Attachment> {
        let below = up(o, -1);
        for d in [[0, 0, 0], [1, 0, 0], [0, 0, 1], [1, 0, 1]] {
            self.below_trunk(c, add(below, d));
        }
        let free = |c: &mut Ctx, d: Pos| {
            let p = add(o, d);
            if self.is_free(c, p) {
                self.place_log(c, p);
            }
        };
        for hh in 0..h {
            free(c, [0, hh, 0]);
            if hh < h - 1 {
                free(c, [1, hh, 0]);
                free(c, [1, hh, 1]);
                free(c, [0, hh, 1]);
            }
        }
        vec![Attachment::new(up(o, h), 0, true)]
    }

    #[allow(clippy::too_many_arguments)]
    fn upwards_branch(
        &self,
        c: &mut Ctx,
        h: i32,
        out: &mut Vec<Attachment>,
        log: Pos,
        cy: i32,
        dir: usize,
        pos: i32,
        mut steps: i32,
    ) {
        let mut along = cy + pos;
        let (mut lx, mut lz) = (log[0], log[2]);
        let mut i = pos;
        while i < h && steps > 0 {
            if i >= 1 {
                let ph = cy + i;
                lx += HORIZONTAL[dir][0];
                lz += HORIZONTAL[dir][2];
                along = ph;
                if self.place_log(c, [lx, ph, lz]) {
                    along = ph + 1;
                }
                out.push(Attachment::new([lx, ph, lz], 0, false));
            }
            i += 1;
            steps -= 1;
        }
        if along - cy > 1 {
            let f = [lx, along, lz];
            out.push(Attachment::new(f, 0, false));
            out.push(Attachment::new(up(f, -2), 0, false));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn cherry_branch(
        &self,
        c: &mut Ctx,
        h: i32,
        o: Pos,
        tree_dir: usize,
        dir: usize,
        offset: i32,
        continues: bool,
        horizontal: &IntP,
        end: &IntP,
    ) -> Attachment {
        let axis = Some(axis_of(tree_dir));
        let mut log = up(o, offset);
        let end_off = h - 1 + end.sample(c.r);
        let away = continues || end_off < offset;
        let dist = horizontal.sample(c.r) + i32::from(away);
        let target = up(step(o, HORIZONTAL[dir], dist), end_off);
        for _ in 0..if away { 2 } else { 1 } {
            log = step(log, HORIZONTAL[dir], 1);
            self.place_log_with(c, log, axis);
        }
        let vertical: Pos = if target[1] > log[1] { [0, 1, 0] } else { [0, -1, 0] };
        loop {
            let d = manhattan(log, target);
            if d == 0 {
                return Attachment::new(up(target, 1), 0, false);
            }
            let chance = (target[1] - log[1]).abs() as f32 / d as f32;
            let grow_up = c.r.next_float() < chance;
            log = step(log, if grow_up { vertical } else { HORIZONTAL[dir] }, 1);
            self.place_log_with(c, log, if grow_up { None } else { axis });
        }
    }

    fn fancy_trunk(&self, c: &mut Ctx, tree_h: i32, o: Pos) -> Vec<Attachment> {
        let height = tree_h + 2;
        let trunk_h = floor_d(height as f64 * 0.618);
        self.below_trunk(c, up(o, -1));
        let clusters = 1.min(floor_d(1.382 + (height as f64 / 13.0).powi(2)));
        let trunk_top = o[1] + trunk_h;
        let mut rel = height - 5;
        // (точка кроны, основание ветви)
        let mut coords: Vec<(Pos, i32)> = vec![(up(o, rel), trunk_top)];
        while rel >= 0 {
            let shape = fancy_shape(height, rel);
            if shape >= 0.0 {
                for _ in 0..clusters {
                    let radius = shape as f64 * (c.r.next_float() as f64 + 0.328);
                    let angle = (c.r.next_float() * 2.0) as f64 * std::f64::consts::PI;
                    let x = radius * angle.sin() + 0.5;
                    let z = radius * angle.cos() + 0.5;
                    let start = [o[0] + floor_d(x), o[1] + rel - 1, o[2] + floor_d(z)];
                    let end = up(start, 5);
                    if self.limb(c, start, end, false) {
                        let dx = o[0] - start[0];
                        let dz = o[2] - start[2];
                        let bh = start[1] as f64 - ((dx * dx + dz * dz) as f64).sqrt() * 0.381;
                        let top = if bh > trunk_top as f64 { trunk_top } else { bh as i32 };
                        let base = [o[0], top, o[2]];
                        if self.limb(c, base, start, false) {
                            coords.push((start, base[1]));
                        }
                    }
                }
            }
            rel -= 1;
        }
        self.limb(c, o, up(o, trunk_h), true);
        let trim = |local: i32| local as f64 >= height as f64 * 0.2;
        for &(end, base) in &coords {
            let b = [o[0], base, o[2]];
            if b != end && trim(base - o[1]) {
                self.limb(c, b, end, true);
            }
        }
        coords
            .into_iter()
            .filter(|&(_, base)| trim(base - o[1]))
            .map(|(p, _)| Attachment::new(p, 0, false))
            .collect()
    }

    fn limb(&self, c: &mut Ctx, start: Pos, end: Pos, place: bool) -> bool {
        if !place && start == end {
            return true;
        }
        let d = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
        let steps = d[0].abs().max(d[1].abs()).max(d[2].abs());
        let f = |v: i32| if steps == 0 { 0.0 } else { v as f32 / steps as f32 };
        let (dx, dy, dz) = (f(d[0]), f(d[1]), f(d[2]));
        for i in 0..=steps {
            let p = [
                start[0] + floor_f(0.5 + i as f32 * dx),
                start[1] + floor_f(0.5 + i as f32 * dy),
                start[2] + floor_f(0.5 + i as f32 * dz),
            ];
            if place {
                let xd = (p[0] - start[0]).abs();
                let zd = (p[2] - start[2]).abs();
                let m = xd.max(zd);
                let axis = if m == 0 { "y" } else if xd == m { "x" } else { "z" };
                self.place_log_with(c, p, Some(axis));
            } else if !self.is_free(c, p) {
                return false;
            }
        }
        true
    }

    // ---------------------------------------------------------------- кроны

    fn try_leaf(&self, c: &mut Ctx, p: Pos) -> bool {
        let here = c.get(p);
        if c.pal.prop(here, "persistent") == Some("true") || !self.valid(c, p) {
            return false;
        }
        let mut s = c.state(&self.foliage_provider, p);
        if c.pal.prop(s, "waterlogged").is_some() {
            let wet = c.pal.fluid(here) == super::Fluid::Water;
            s = c.pal.with_prop(s, "waterlogged", if wet { "true" } else { "false" }).unwrap_or(s);
        }
        c.foliage.insert(p);
        c.put(p, s);
        true
    }

    /// `shouldSkipLocation` с уже взятыми модулями.
    fn skip(&self, c: &mut Ctx, dx: i32, y: i32, dz: i32, r: i32, double: bool) -> bool {
        match &self.foliage.kind {
            FoliageKind::Blob(_) => dx == r && dz == r && (c.r.next_int_bound(2) == 0 || y == 0),
            FoliageKind::Bush(_) => dx == r && dz == r && c.r.next_int_bound(2) == 0,
            FoliageKind::Fancy(_) => {
                let a = dx as f32 + 0.5;
                let b = dz as f32 + 0.5;
                a * a + b * b > (r * r) as f32
            }
            FoliageKind::Spruce(_) | FoliageKind::Pine(_) => dx == r && dz == r && r > 0,
            FoliageKind::MegaPine(_) | FoliageKind::MegaJungle(_) => dx + dz >= 7 || dx * dx + dz * dz > r * r,
            FoliageKind::Acacia => {
                if y == 0 {
                    (dx > 1 || dz > 1) && dx != 0 && dz != 0
                } else {
                    dx == r && dz == r && r > 0
                }
            }
            FoliageKind::DarkOak => {
                if y == -1 && !double {
                    dx == r && dz == r
                } else {
                    y == 1 && dx + dz > r * 2 - 2
                }
            }
            FoliageKind::RandomSpread(..) | FoliageKind::Poplar { .. } => false,
            FoliageKind::Cherry { wide_hole, corner_hole, .. } => {
                if y == -1 && (dx == r || dz == r) && c.r.next_float() < *wide_hole {
                    return true;
                }
                let corner = dx == r && dz == r;
                if r > 2 {
                    corner || (dx + dz > r * 2 - 2 && c.r.next_float() < *corner_hole)
                } else {
                    corner && c.r.next_float() < *corner_hole
                }
            }
        }
    }

    fn skip_signed(&self, c: &mut Ctx, dx: i32, y: i32, dz: i32, r: i32, double: bool) -> bool {
        if matches!(self.foliage.kind, FoliageKind::DarkOak)
            && y == 0
            && double
            && (dx == -r || dx >= r)
            && (dz == -r || dz >= r)
        {
            return true;
        }
        let (mx, mz) = if double {
            (dx.abs().min((dx - 1).abs()), dz.abs().min((dz - 1).abs()))
        } else {
            (dx.abs(), dz.abs())
        };
        self.skip(c, mx, y, mz, r, double)
    }

    fn leaves_row(&self, c: &mut Ctx, o: Pos, r: i32, y: i32, double: bool) {
        let off = i32::from(double);
        for dx in -r..=r + off {
            for dz in -r..=r + off {
                if !self.skip_signed(c, dx, y, dz, r, double) {
                    self.try_leaf(c, [o[0] + dx, o[1] + y, o[2] + dz]);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn leaves_row_hanging(&self, c: &mut Ctx, o: Pos, r: i32, y: i32, double: bool, chance: f32, ext: f32) {
        self.leaves_row(c, o, r, y, double);
        let off = i32::from(double);
        let log = up(o, -1);
        // `Plane.HORIZONTAL`, к краю — по часовой стрелке.
        for (along, &dir) in HORIZONTAL.iter().enumerate() {
            let to_edge = (along + 1) % 4;
            let positive = HORIZONTAL[to_edge][0] + HORIZONTAL[to_edge][2] > 0;
            let edge = if positive { r + off } else { r };
            let mut p = step(step(up(o, y - 1), HORIZONTAL[to_edge], edge), dir, -r);
            let mut i = -r;
            while i < r + off {
                let above = c.foliage.contains(&up(p, 1));
                if above && self.extension(c, chance, log, p) {
                    self.extension(c, ext, log, up(p, -1));
                }
                i += 1;
                p = step(p, dir, 1);
            }
        }
    }

    fn extension(&self, c: &mut Ctx, chance: f32, log: Pos, p: Pos) -> bool {
        if manhattan(p, log) >= 7 || c.r.next_float() > chance {
            return false;
        }
        self.try_leaf(c, p)
    }

    fn create_foliage(&self, c: &mut Ctx, _h: i32, a: Attachment, fh: i32, radius: i32, offset: i32) {
        let double = a.double;
        let fho = fh + a.height_offset;
        match &self.foliage.kind {
            FoliageKind::Blob(_) => {
                let mut yo = offset;
                while yo >= offset - fho {
                    let r = (radius + a.radius_offset - 1 - yo / 2).max(0);
                    self.leaves_row(c, a.pos, r, yo, double);
                    yo -= 1;
                }
            }
            FoliageKind::Fancy(_) => {
                let mut yo = offset;
                while yo >= offset - fh {
                    let r = radius + i32::from(yo != offset && yo != offset - fh);
                    self.leaves_row(c, a.pos, r, yo, double);
                    yo -= 1;
                }
            }
            FoliageKind::Bush(_) => {
                let mut yo = offset;
                while yo >= offset - fho {
                    let r = radius + a.radius_offset - 1 - yo;
                    self.leaves_row(c, a.pos, r, yo, double);
                    yo -= 1;
                }
            }
            FoliageKind::Spruce(_) => {
                let mut r = c.r.next_int_bound(2);
                let (mut max_r, mut min_r) = (1, 0);
                let mut yo = offset;
                while yo >= -fho {
                    self.leaves_row(c, a.pos, r, yo, double);
                    if r >= max_r {
                        r = min_r;
                        min_r = 1;
                        max_r = (max_r + 1).min(radius + a.radius_offset);
                    } else {
                        r += 1;
                    }
                    yo -= 1;
                }
            }
            FoliageKind::Pine(_) => {
                let mut r = 0;
                let mut yo = offset;
                while yo >= offset - fho {
                    self.leaves_row(c, a.pos, r, yo, double);
                    if r >= 1 && yo == offset - fho + 1 {
                        r -= 1;
                    } else if r < radius + a.radius_offset {
                        r += 1;
                    }
                    yo -= 1;
                }
            }
            FoliageKind::MegaPine(_) => {
                let p = a.pos;
                let mut prev = 0;
                for yy in p[1] - fho + offset..=p[1] + offset {
                    let yo = p[1] - yy;
                    let smooth = radius + a.radius_offset + floor_f(yo as f32 / fho as f32 * 3.5);
                    let jagged = if yo > 0 && smooth == prev && yy & 1 == 0 { smooth + 1 } else { smooth };
                    self.leaves_row(c, [p[0], yy, p[2]], jagged, 0, double);
                    prev = smooth;
                }
            }
            FoliageKind::Acacia => {
                let p = up(a.pos, offset);
                self.leaves_row(c, p, radius + a.radius_offset, -1 - fho, double);
                self.leaves_row(c, p, radius - 1, -fho, double);
                self.leaves_row(c, p, radius + a.radius_offset - 1, 0, double);
            }
            FoliageKind::DarkOak => {
                let p = up(a.pos, offset);
                if double {
                    self.leaves_row(c, p, radius + 2, -1, double);
                    self.leaves_row(c, p, radius + 3, 0, double);
                    self.leaves_row(c, p, radius + 2, 1, double);
                    if c.r.next_bool() {
                        self.leaves_row(c, p, radius, 2, double);
                    }
                } else {
                    self.leaves_row(c, p, radius + 2, -1, double);
                    self.leaves_row(c, p, radius + 1, 0, double);
                }
            }
            FoliageKind::MegaJungle(_) => {
                let lh = if double { fh } else { 1 + c.r.next_int_bound(2) } + a.height_offset;
                let mut yo = offset;
                while yo >= offset - lh {
                    let r = radius + a.radius_offset + 1 - yo;
                    self.leaves_row(c, a.pos, r, yo, double);
                    yo -= 1;
                }
            }
            FoliageKind::RandomSpread(_, attempts) => {
                for _ in 0..*attempts {
                    let dx = c.r.next_int_bound(radius) - c.r.next_int_bound(radius);
                    let dy = c.r.next_int_bound(fh) - c.r.next_int_bound(fh);
                    let dz = c.r.next_int_bound(radius) - c.r.next_int_bound(radius);
                    self.try_leaf(c, add(a.pos, [dx, dy, dz]));
                }
            }
            FoliageKind::Cherry { hanging, extension, .. } => {
                let p = up(a.pos, offset);
                let r = radius + a.radius_offset - 1;
                self.leaves_row(c, p, r - 2, fho - 3, double);
                self.leaves_row(c, p, r - 1, fho - 4, double);
                let mut y = fho - 5;
                while y >= 0 {
                    self.leaves_row(c, p, r, y, double);
                    y -= 1;
                }
                self.leaves_row_hanging(c, p, r, -1, double, *hanging, *extension);
                self.leaves_row_hanging(c, p, r - 1, -2, double, *hanging, *extension);
            }
            FoliageKind::Poplar { side_hole, .. } => self.poplar(c, a, fho, radius, offset, *side_hole),
        }
    }

    fn poplar(&self, c: &mut Ctx, a: Attachment, fh: i32, radius: i32, offset: i32, hole: f32) {
        let double = a.double;
        let p = up(a.pos, offset);
        let r = radius + a.radius_offset - 1;
        let flip = c.r.next_bool();
        let partial = |y: i32| fh - 1 == y || fh - 2 == y;
        let cut = |dx: i32, dz: i32, r: i32, part: bool| {
            let small = if flip {
                (dx > 0 && dz > 0) || (dz < 0 && dx < 0)
            } else {
                (dx > 0 && dz < 0) || (dz > 0 && dx < 0)
            };
            if small {
                r - 1
            } else if part {
                r + 1
            } else {
                r
            }
        };
        let inside = |r: i32, ax: i32, az: i32, cut: i32, extra: i32| ax + az <= r * 2 - (cut + extra);
        let row = |c: &mut Ctx, r: i32, y: i32| {
            let off = i32::from(double);
            for dx in -r..=r + off {
                for dz in -r..=r + off {
                    let part = partial(y);
                    let k = cut(dx, dz, r, part);
                    let (ax, az) = (dx.abs(), dz.abs());
                    let skip = if part && (ax == r || az == r) {
                        true
                    } else {
                        let extra = i32::from(c.r.next_float() <= hole);
                        !inside(r, ax, az, k, extra)
                    };
                    if !skip {
                        self.try_leaf(c, [p[0] + dx, p[1] + y, p[2] + dz]);
                    }
                }
            }
        };
        row(c, r - 2, fh - 1);
        row(c, r - 1, fh - 2);
        row(c, r - 1, fh - 3);
        let mut y = fh - 4;
        while y >= 1 {
            row(c, r, y);
            y -= 1;
        }
        // `replaceLeavesWithLog`.
        let y = fh - 4;
        let off = i32::from(double);
        for dx in -r..=r + off {
            for dz in -r..=r + off {
                let (ax, az) = (dx.abs(), dz.abs());
                if inside(r, ax, az, cut(dx, dz, r, partial(y)), 2)
                    && ((az == 0 && r - ax >= 4) || (ax == 0 && r - az >= 4))
                {
                    let at = [p[0] + dx, p[1] + y, p[2] + dz];
                    let leaf = c.state(&self.foliage_provider, at);
                    if c.get(at) == leaf {
                        let s = c.state(&self.trunk_provider, at);
                        let axis = if az == 0 { "x" } else { "z" };
                        let s = c.pal.with_prop(s, "axis", axis).unwrap_or(s);
                        c.foliage.insert(at);
                        c.put(at, s);
                    }
                }
            }
        }
        row(c, r - 1, 0);
        row(c, (r - 2).clamp(1, 2), -1);
    }

}

impl Tags {
    // ---------------------------------------------------------------- декораторы

    fn set_decoration(&self, c: &mut Ctx, p: Pos, s: u32) {
        c.decorations.insert(p);
        c.put(p, s);
    }

    fn vine(&self, c: &mut Ctx, p: Pos, side: &str) {
        let s = c.pal.with_prop(self.vine, side, "true").unwrap_or(self.vine);
        self.set_decoration(c, p, s);
    }

    fn lowest(logs: &[Pos], roots: &[Pos]) -> Vec<Pos> {
        if roots.is_empty() {
            logs.to_vec()
        } else if !logs.is_empty() && roots[0][1] == logs[0][1] {
            logs.iter().chain(roots).copied().collect()
        } else {
            roots.to_vec()
        }
    }

    fn shuffled(c: &mut Ctx, list: &[Pos]) -> Vec<Pos> {
        let mut v = list.to_vec();
        for i in (2..=v.len()).rev() {
            let j = c.r.next_int_bound(i as i32) as usize;
            v.swap(i - 1, j);
        }
        v
    }

    fn decorate(&self, c: &mut Ctx, d: &Decorator) {
        let logs = c.trunks.sorted_by_y();
        let leaves = c.foliage.sorted_by_y();
        let roots = c.roots.sorted_by_y();
        // Стороны лиан: (сдвиг, свойство).
        const VINES: [(Pos, &str); 4] =
            [([-1, 0, 0], "east"), ([1, 0, 0], "west"), ([0, 0, -1], "south"), ([0, 0, 1], "north")];
        match d {
            Decorator::Beehive(p) => {
                if logs.is_empty() || c.r.next_float() >= *p {
                    return;
                }
                let hive_y = if let Some(l) = leaves.first() {
                    (l[1] - 1).max(logs[0][1] + 1)
                } else {
                    (logs[0][1] + 1 + c.r.next_int_bound(3)).min(logs[logs.len() - 1][1])
                };
                // Восток, юг, запад — без севера, куда смотрел бы леток.
                let mut spots: Vec<Pos> = logs
                    .iter()
                    .filter(|p| p[1] == hive_y)
                    .flat_map(|&p| [1usize, 2, 3].map(|i| step(p, HORIZONTAL[i], 1)))
                    .collect();
                if spots.is_empty() {
                    return;
                }
                for i in (2..=spots.len()).rev() {
                    let j = c.r.next_int_bound(i as i32) as usize;
                    spots.swap(i - 1, j);
                }
                if let Some(&p) = spots.iter().find(|&&p| c.is_air(p) && c.is_air(step(p, HORIZONTAL[2], 1))) {
                    let s = c.pal.with_prop(self.bee_nest, "facing", "south").unwrap_or(self.bee_nest);
                    self.set_decoration(c, p, s);
                    let bees = 2 + c.r.next_int_bound(2);
                    for _ in 0..bees {
                        c.r.next_int_bound(599);
                    }
                }
            }
            Decorator::LeaveVine(p) => {
                for &leaf in &leaves {
                    for (d, side) in VINES {
                        if c.r.next_float() < *p {
                            let at = add(leaf, d);
                            if c.is_air(at) {
                                self.vine(c, at, side);
                                let mut below = up(at, -1);
                                let mut n = 4;
                                while c.is_air(below) && n > 0 {
                                    self.vine(c, below, side);
                                    below = up(below, -1);
                                    n -= 1;
                                }
                            }
                        }
                    }
                }
            }
            Decorator::TrunkVine => {
                for &log in &logs {
                    for (d, side) in VINES {
                        if c.r.next_int_bound(3) > 0 {
                            let at = add(log, d);
                            if c.is_air(at) {
                                self.vine(c, at, side);
                            }
                        }
                    }
                }
            }
            Decorator::Cocoa(p) => {
                if c.r.next_float() >= *p || logs.is_empty() {
                    return;
                }
                let base = logs[0][1];
                for &log in logs.iter().filter(|l| l[1] - base <= 2) {
                    for (i, name) in ["north", "east", "south", "west"].into_iter().enumerate() {
                        if c.r.next_float() <= 0.25 {
                            let at = step(log, HORIZONTAL[(i + 2) % 4], 1);
                            if c.is_air(at) {
                                let age = c.r.next_int_bound(3).to_string();
                                let s = c.pal.with_prop(self.cocoa, "age", &age).unwrap_or(self.cocoa);
                                let s = c.pal.with_prop(s, "facing", name).unwrap_or(s);
                                self.set_decoration(c, at, s);
                            }
                        }
                    }
                }
            }
            Decorator::AlterGround(provider) => {
                let list = Self::lowest(&logs, &roots);
                let Some(first) = list.first() else {
                    return;
                };
                let min_y = first[1];
                for p in list.iter().filter(|p| p[1] == min_y) {
                    for o in [[-1, 0, -1], [2, 0, -1], [-1, 0, 2], [2, 0, 2]] {
                        self.circle(c, provider, add(*p, o));
                    }
                    for _ in 0..5 {
                        let k = c.r.next_int_bound(64);
                        let (xx, zz) = (k % 8, k / 8);
                        if xx == 0 || xx == 7 || zz == 0 || zz == 7 {
                            self.circle(c, provider, add(*p, [-3 + xx, 0, -3 + zz]));
                        }
                    }
                }
            }
            Decorator::AttachedToLeaves { chance, xz, y, provider, empty, dirs } => {
                let mut black = std::collections::HashSet::new();
                for leaf in Self::shuffled(c, &leaves) {
                    let d = dirs[c.r.next_int_bound(dirs.len() as i32) as usize];
                    let at = add(leaf, super::DIRS[d]);
                    if !black.contains(&at)
                        && c.r.next_float() < *chance
                        && (1..=*empty).all(|i| c.is_air(step(leaf, super::DIRS[d], i)))
                    {
                        for bx in -xz..=*xz {
                            for by in -y..=*y {
                                for bz in -xz..=*xz {
                                    black.insert(add(at, [bx, by, bz]));
                                }
                            }
                        }
                        let s = c.state(provider, at);
                        self.set_decoration(c, at, s);
                    }
                }
            }
            Decorator::AttachedToLogs { chance, provider, dirs } => {
                for log in Self::shuffled(c, &logs) {
                    let d = dirs[c.r.next_int_bound(dirs.len() as i32) as usize];
                    let at = add(log, super::DIRS[d]);
                    if c.r.next_float() <= *chance && c.is_air(at) {
                        let s = c.state(provider, at);
                        self.set_decoration(c, at, s);
                    }
                }
            }
            Decorator::PlaceOnGround { tries, radius, height, provider } => {
                let list = Self::lowest(&logs, &roots);
                let Some(&o) = list.first() else {
                    return;
                };
                let (mut x0, mut x1, mut z0, mut z1) = (o[0], o[0], o[2], o[2]);
                for p in list.iter().filter(|p| p[1] == o[1]) {
                    x0 = x0.min(p[0]);
                    x1 = x1.max(p[0]);
                    z0 = z0.min(p[2]);
                    z1 = z1.max(p[2]);
                }
                let (x0, x1, z0, z1) = (x0 - radius, x1 + radius, z0 - radius, z1 + radius);
                let (y0, y1) = (o[1] - height, o[1] + height);
                for _ in 0..*tries {
                    let x = c.r.next_int_bound(x1 - x0 + 1) + x0;
                    let y = c.r.next_int_bound(y1 - y0 + 1) + y0;
                    let z = c.r.next_int_bound(z1 - z0 + 1) + z0;
                    let p = [x, y, z];
                    let above = up(p, 1);
                    let top = c.get(above);
                    let ground = c.get(p);
                    let solid_render = Palette::flag(&c.pal.solid, ground)
                        && c.pal.sturdy.get(ground as usize) == Some(&63)
                        && !Palette::flag(&self.leaves, ground);
                    if (c.pal.is_air(top) || top == self.vine || c.pal.same_block(top, self.vine))
                        && solid_render
                        && c.level.surface(super::Heightmap::MotionBlockingNoLeaves, x, z) <= above[1]
                    {
                        let s = c.state(provider, above);
                        self.set_decoration(c, above, s);
                    }
                }
            }
            Decorator::CreakingHeart(p) => {
                if logs.is_empty() || c.r.next_float() >= *p {
                    return;
                }
                let list = Self::shuffled(c, &logs);
                let found = list.into_iter().find(|&l| super::DIRS.iter().all(|&d| c.has(&self.logs, add(l, d))));
                if let Some(at) = found {
                    let s = self.creaking_heart;
                    let s = c.pal.with_prop(s, "creaking_heart_state", "dormant").unwrap_or(s);
                    let s = c.pal.with_prop(s, "natural", "true").unwrap_or(s);
                    self.set_decoration(c, at, s);
                }
            }
            Decorator::ShelfMushroom(p) => self.shelf_mushroom(c, *p, &logs),
            Decorator::PaleMoss { leaves: lp, trunk, ground, patch } => {
                let list = Self::shuffled(c, &logs);
                let Some(origin) = list.iter().copied().min_by_key(|p| p[1]) else {
                    return;
                };
                if c.r.next_float() < *ground
                    && let Some(patch) = patch
                {
                    c.features.place(c.level, patch, None, c.r, up(origin, 1));
                }
                for &l in &logs {
                    if c.r.next_float() < *trunk {
                        let down = up(l, -1);
                        if c.is_air(down) {
                            self.moss_hanger(c, down);
                        }
                    }
                }
                for &l in &leaves {
                    if c.r.next_float() < *lp {
                        let down = up(l, -1);
                        if c.is_air(down) {
                            self.moss_hanger(c, down);
                        }
                    }
                }
            }
        }
    }

    fn moss_hanger(&self, c: &mut Ctx, mut p: Pos) {
        let m = self.hanging_moss;
        while c.is_air(up(p, -1)) && c.r.next_float() >= 0.5 {
            let s = c.pal.with_prop(m, "tip", "false").unwrap_or(m);
            self.set_decoration(c, p, s);
            p = up(p, -1);
        }
        let s = c.pal.with_prop(m, "tip", "true").unwrap_or(m);
        self.set_decoration(c, p, s);
    }

    fn circle(&self, c: &mut Ctx, provider: &Provider, p: Pos) {
        for xx in -2..=2i32 {
            for zz in -2..=2i32 {
                if xx.abs() != 2 || zz.abs() != 2 {
                    let at = add(p, [xx, 0, zz]);
                    for dy in (-3..=2).rev() {
                        let cur = up(at, dy);
                        if let Some(s) = c.optional(provider, cur) {
                            self.set_decoration(c, cur, s);
                            break;
                        }
                        if !c.is_air(cur) && dy < 0 {
                            break;
                        }
                    }
                }
            }
        }
    }

    fn shelf_mushroom(&self, c: &mut Ctx, p: f32, logs: &[Pos]) {
        if c.r.next_float() >= p || logs.is_empty() {
            return;
        }
        let m = self.shelf_mushroom;
        let has = |c: &Ctx, at: Pos| c.pal.same_block(c.get(at), m);
        let replaceable = |c: &Ctx, at: Pos| {
            let water = |q: Pos| c.pal.fluid(c.get(q)) == super::Fluid::Water && c.pal.same_block(c.get(q), c.pal.defaults["minecraft:water"]);
            Palette::flag(&c.pal.replaceable, c.get(at))
                && !(water(at) || HORIZONTAL.iter().any(|&d| water(step(at, d, 1))))
        };
        let place = |c: &mut Ctx, at: Pos, facing: usize| {
            let age = c.r.next_int_bound(2).to_string();
            let s = c.pal.with_prop(m, "age", &age).unwrap_or(m);
            let s = c.pal.with_prop(s, "facing", DIRECTION_NAMES[HORIZONTAL_DIR[facing]]).unwrap_or(s);
            self.set_decoration(c, at, s);
        };
        let (first, last) = (logs[0], logs[logs.len() - 1]);
        if first[1] == last[1] {
            let dirs: [usize; 2] = if first[0] != last[0] { [0, 2] } else { [1, 3] };
            for &log in logs {
                for d in dirs {
                    if c.r.next_float() <= 0.25 {
                        let at = step(log, HORIZONTAL[d], 1);
                        if replaceable(c, at)
                            && !HORIZONTAL.iter().any(|&h| has(c, step(at, h, 1)))
                            && !HORIZONTAL.iter().any(|&h| has(c, step(log, h, 1)))
                        {
                            place(c, at, d);
                        }
                    }
                }
            }
        } else {
            let a = c.r.next_int_bound(4) as usize;
            let dirs = [a, (a + 1) % 4];
            let base = first[1];
            for &log in logs {
                let dy = log[1] - base;
                if !(1..=4).contains(&dy) {
                    continue;
                }
                for d in dirs {
                    if c.r.next_float() <= 0.25 {
                        let at = step(log, HORIZONTAL[d], 1);
                        if replaceable(c, at) && !has(c, up(at, -1)) {
                            place(c, at, d);
                            break;
                        }
                    }
                }
            }
        }
    }

}

impl Tree {
    // ---------------------------------------------------------------- листва

    /// `TreeFeature.updateLeaves`: расстояние листвы до ствола в пределах
    /// рамки всех поставленных блоков.
    fn update_leaves(&self, c: &mut Ctx) {
        let all: Vec<Pos> = c
            .roots
            .iter()
            .chain(c.trunks.iter())
            .chain(c.foliage.iter())
            .chain(c.decorations.iter())
            .copied()
            .collect();
        let Some(&first) = all.first() else {
            return;
        };
        let (mut lo, mut hi) = (first, first);
        for p in &all {
            for i in 0..3 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        }
        let span = [hi[0] - lo[0] + 1, hi[1] - lo[1] + 1, hi[2] - lo[2] + 1];
        let inside = |p: &Pos| (0..3).all(|i| lo[i] <= p[i] && p[i] <= hi[i]);
        let index = |p: &Pos| (((p[0] - lo[0]) * span[1] + p[1] - lo[1]) * span[2] + p[2] - lo[2]) as usize;
        let mut shape = vec![false; (span[0] * span[1] * span[2]) as usize];
        for p in c.decorations.iter().chain(c.roots.iter()) {
            if inside(p) {
                shape[index(p)] = true;
            }
        }
        let mut check: Vec<JavaSet> = (0..7).map(|_| JavaSet::new()).collect();
        for p in c.trunks.iter() {
            check[0].insert(*p);
        }
        let mut smallest = 0usize;
        loop {
            while smallest < 7 && !check[smallest].is_empty() {
                let Some(p) = check[smallest].pop_first() else {
                    break;
                };
                if !inside(&p) {
                    continue;
                }
                if smallest != 0 {
                    let s = c.get(p);
                    let d = smallest.to_string();
                    if let Some(n) = c.pal.with_prop(s, "distance", &d) {
                        c.put(p, n);
                    }
                }
                shape[index(&p)] = true;
                for d in super::DIRS {
                    let n = add(p, d);
                    if !inside(&n) || shape[index(&n)] {
                        continue;
                    }
                    let s = c.get(n);
                    let dist = if Palette::flag(&self.tags.no_decay, s) {
                        Some(0)
                    } else {
                        c.pal.prop(s, "distance").and_then(|v| v.parse::<usize>().ok())
                    };
                    if let Some(dist) = dist {
                        let nd = dist.min(smallest + 1);
                        if nd < 7 {
                            check[nd].insert(n);
                            smallest = smallest.min(nd);
                        }
                    }
                }
            }
            if smallest >= 7 {
                return;
            }
            smallest += 1;
        }
    }
}

/// `FancyTrunkPlacer.treeShape`.
fn fancy_shape(height: i32, y: i32) -> f32 {
    if (y as f32) < height as f32 * 0.3 {
        return -1.0;
    }
    let radius = height as f32 / 2.0;
    let adjacent = radius - y as f32;
    let mut distance = (radius * radius - adjacent * adjacent).sqrt();
    if adjacent == 0.0 {
        distance = radius;
    } else if adjacent.abs() >= radius {
        return 0.0;
    }
    distance * 0.5
}

/// `FallenTreeFeature`: пень и лежащий рядом ствол.
#[derive(Debug)]
pub(super) struct Fallen {
    trunk: Provider,
    length: IntP,
    stump: Vec<Decorator>,
    log: Vec<Decorator>,
    tags: Tags,
}

impl Fallen {
    pub(super) fn parse(
        v: &Value,
        pal: &mut Palette,
        provider: &mut ProviderFn,
        placed: &mut PlacedFn,
    ) -> Result<Self, String> {
        let mut list = |k: &str, pal: &mut Palette| -> Result<Vec<Decorator>, String> {
            v[k].as_array().into_iter().flatten().map(|d| parse_decorator(d, pal, provider, placed)).collect()
        };
        let stump = list("stump_decorators", pal)?;
        let log = list("log_decorators", pal)?;
        Ok(Self {
            trunk: provider(&v["trunk_provider"], pal)?,
            length: IntP::parse(&v["log_length"])?,
            stump,
            log,
            tags: Tags::load(pal)?,
        })
    }

    fn valid(&self, c: &Ctx, p: Pos) -> bool {
        c.is_air(p) || c.has(&self.tags.replaceable, p)
    }

    fn over_solid(c: &Ctx, p: Pos) -> bool {
        c.pal.sturdy(c.get(up(p, -1)), 1)
    }

    fn log(&self, c: &mut Ctx, p: Pos, axis: Option<&str>) {
        let mut s = c.state(&self.trunk, p);
        if let Some(a) = axis {
            s = c.pal.with_prop(s, "axis", a).unwrap_or(s);
        }
        c.put(p, s);
    }

    fn decorate(&self, c: &mut Ctx, logs: JavaSet, list: &[Decorator]) {
        if list.is_empty() {
            return;
        }
        c.trunks = logs;
        for d in list {
            self.tags.decorate(c, d);
        }
    }

    pub(super) fn place(&self, features: &Features, level: &mut dyn Level, r: &mut Worldgen, origin: Pos) {
        let mut c = Ctx {
            features,
            level,
            pal: &features.palette,
            r,
            roots: JavaSet::new(),
            trunks: JavaSet::new(),
            foliage: JavaSet::new(),
            decorations: JavaSet::new(),
        };
        self.log(&mut c, origin, None);
        let mut stump = JavaSet::new();
        stump.insert(origin);
        self.decorate(&mut c, stump, &self.stump);
        let dir = c.r.next_int_bound(4) as usize;
        let len = self.length.sample(c.r) - 2;
        let d = HORIZONTAL[dir];
        let mut start = up(step(origin, d, 2 + c.r.next_int_bound(2)), 1);
        for _ in 0..6 {
            if self.valid(&c, start) && Self::over_solid(&c, start) {
                break;
            }
            start = up(start, -1);
        }
        let mut gap = 0;
        for i in 0..len {
            let p = step(start, d, i);
            if !self.valid(&c, p) {
                return;
            }
            if !Self::over_solid(&c, p) {
                gap += 1;
                if gap > 2 {
                    return;
                }
            } else {
                gap = 0;
            }
        }
        let mut logs = JavaSet::new();
        for i in 0..len {
            let p = step(start, d, i);
            self.log(&mut c, p, Some(axis_of(dir)));
            logs.insert(p);
        }
        self.decorate(&mut c, logs, &self.log);
    }
}
