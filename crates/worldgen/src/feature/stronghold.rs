//! Крепость: кольца `concentric_rings` и дерево `StrongholdPieces`.
//! Сундук — пустое состояние, `nextLong` лута съедается. Серебрянка — блок
//! спаунера, моб не пишется. Глаз рамки — свойство блока.

use super::structure::Columns;
use super::template::Bounds;
use super::{Bits, Legacy, Level, Palette, Worldgen};
use std::collections::HashSet;

const OPENING: u8 = 0;
const WOOD: u8 = 1;
const GRATES: u8 = 2;
const IRON: u8 = 3;

/// Начальная точка кольца и сид её биомного форка (`LegacyRandomSource.fork`).
#[derive(Clone, Copy)]
pub(crate) struct RingSpot {
    pub initial: (i32, i32),
    pub fork: i64,
}

/// `ChunkGeneratorStructureState.generateRingPositions` без поиска биома.
/// `Math.round` — `floor(x + 0.5)`, не Rust `round`.
pub(crate) fn plan_rings(seed: i64, distance: i32, mut spread: i32, count: i32) -> Vec<RingSpot> {
    let mut rng = Legacy::new(seed);
    let mut angle = rng.next_double() * std::f64::consts::TAU;
    let mut position = 0i32;
    let mut circle = 0i32;
    let mut out = Vec::with_capacity(count.max(0) as usize);
    let mut i = 0i32;
    while i < count {
        let dist = (4 * distance + distance * circle * 6) as f64
            + (rng.next_double() - 0.5) * (distance as f64 * 2.5);
        let initial = (
            java_round(angle.cos() * dist),
            java_round(angle.sin() * dist),
        );
        let fork = rng.next_long();
        out.push(RingSpot { initial, fork });
        angle += std::f64::consts::TAU / spread as f64;
        if position + 1 == spread {
            position = 0;
            circle += 1;
            spread += 2 * spread / (circle + 1);
            spread = spread.min(count - i);
            angle += rng.next_double() * std::f64::consts::TAU;
        } else {
            position += 1;
        }
        i += 1;
    }
    out
}

/// `BiomeSource.findBiomeHorizontal` с `findClosest = false`: весь квадрат
/// радиуса `112 >> 2` квартов. Нет попадания — чанк остаётся начальным.
pub(crate) fn adjust_chunk(
    initial: (i32, i32),
    fork: i64,
    preferred: &HashSet<u32>,
    columns: &dyn Columns,
) -> (i32, i32) {
    let mut rng = Legacy::new(fork);
    let origin_x = initial.0 * 16 + 8;
    let origin_z = initial.1 * 16 + 8;
    let center_x = origin_x >> 2;
    let center_z = origin_z >> 2;
    let radius = 112 >> 2;
    let mut found = 0i32;
    let mut chosen: Option<(i32, i32)> = None;
    for z in -radius..=radius {
        for x in -radius..=radius {
            let qx = center_x + x;
            let qz = center_z + z;
            let Some(biome) = columns.biome_at(qx << 2, 0, qz << 2) else {
                continue;
            };
            if !preferred.contains(&biome) {
                continue;
            }
            if chosen.is_none() || rng.next_int_bound(found + 1) == 0 {
                chosen = Some((qx << 2, qz << 2));
            }
            found += 1;
        }
    }
    match chosen {
        Some((x, z)) => (x >> 4, z >> 4),
        None => initial,
    }
}

fn java_round(v: f64) -> i32 {
    (v + 0.5).floor() as i32
}

#[derive(Clone, Copy, Debug)]
enum PKind {
    Straight,
    Prison,
    Left,
    Right,
    Room,
    StraightStairs,
    Stairs,
    Five,
    Chest,
    Library,
    Portal,
}

#[derive(Clone, Copy, Debug)]
struct Weight {
    kind: PKind,
    weight: i32,
    max: i32,
    count: i32,
    /// `doPlace`: кусок годится только при `depth > gate`. Ноль — без порога.
    gate: i32,
}

impl Weight {
    fn table() -> [Self; 11] {
        fn w(kind: PKind, weight: i32, max: i32, gate: i32) -> Weight {
            Weight {
                kind,
                weight,
                max,
                count: 0,
                gate,
            }
        }
        [
            w(PKind::Straight, 40, 0, 0),
            w(PKind::Prison, 5, 5, 0),
            w(PKind::Left, 20, 0, 0),
            w(PKind::Right, 20, 0, 0),
            w(PKind::Room, 10, 6, 0),
            w(PKind::StraightStairs, 5, 5, 0),
            w(PKind::Stairs, 5, 5, 0),
            w(PKind::Five, 5, 4, 0),
            w(PKind::Chest, 5, 4, 0),
            w(PKind::Library, 10, 2, 4),
            w(PKind::Portal, 20, 1, 5),
        ]
    }

    fn do_place(&self, depth: i32) -> bool {
        (self.max == 0 || self.count < self.max) && depth > self.gate
    }
}

/// Данные куска, которые `postProcess` читает. `source` у лестницы старта
/// заставляет следующий кусок быть пятипутьем и в блоки не попадает.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Body {
    Straight {
        left: bool,
        right: bool,
        door: u8,
    },
    Prison {
        door: u8,
    },
    /// `left` — левый поворот. Правый зеркалит проём.
    Turn {
        left: bool,
        door: u8,
    },
    Room {
        door: u8,
        ty: i32,
    },
    StraightStairs {
        door: u8,
    },
    Stairs {
        door: u8,
    },
    Five {
        door: u8,
        left_low: bool,
        left_high: bool,
        right_low: bool,
        right_high: bool,
    },
    Chest {
        door: u8,
    },
    Library {
        door: u8,
        tall: bool,
    },
    Portal,
    Filler {
        steps: i32,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct Piece {
    pub facing: u8,
    pub min: [i32; 3],
    pub max: [i32; 3],
    pub body: Body,
}

impl Piece {
    pub(crate) fn bounds(&self) -> ([i32; 3], [i32; 3]) {
        (self.min, self.max)
    }

    pub(crate) fn place(
        &self,
        pal: &Palette,
        level: &mut dyn Level,
        rng: &mut Worldgen,
        window: Bounds,
    ) {
        let mut draw = Draw::new(pal, level, rng, window, self.facing, self.min, self.max);
        match self.body {
            Body::Straight { left, right, door } => draw.straight(door, left, right),
            Body::Prison { door } => draw.prison(door),
            Body::Turn { left, door } => draw.turn(left, door),
            Body::Room { door, ty } => draw.room(door, ty),
            Body::StraightStairs { door } => draw.straight_stairs(door),
            Body::Stairs { door } => draw.stairs(door),
            Body::Five {
                door,
                left_low,
                left_high,
                right_low,
                right_high,
            } => draw.five(door, left_low, left_high, right_low, right_high),
            Body::Chest { door } => draw.chest_corridor(door),
            Body::Library { door, tall } => draw.library(door, tall),
            Body::Portal => draw.portal(),
            Body::Filler { steps } => draw.filler(steps),
        }
    }
}

struct Raw {
    facing: u8,
    min: [i32; 3],
    max: [i32; 3],
    depth: i32,
    body: Body,
}

struct Grow {
    /// `GenerationContext` держит `WorldgenRandom` поверх `LegacyRandomSource`.
    /// `next` уходит в LCG, не в старшие биты Xoroshiro.
    rng: Legacy,
    pieces: Vec<Raw>,
    pending: Vec<usize>,
    weights: [Weight; 11],
    active: Vec<usize>,
    previous: Option<usize>,
    imposed: Option<PKind>,
    portal: bool,
    start_x: i32,
    start_z: i32,
    total: i32,
}

/// Дерево кусков до комнаты портала, затем `moveBelowSeaLevel` на том же
/// `Legacy`. Повтор сида `seed + tries`, пока портал не собран.
pub(crate) fn build(seed: i64, cx: i32, cz: i32, sea: i32, min_y: i32) -> Vec<Piece> {
    for tries in 0..128 {
        let mut tree = Grow::fresh(seed + tries as i64, cx, cz);
        tree.begin(cx, cz);
        tree.add_children(0);
        while !tree.pending.is_empty() {
            let pos = tree.rng.next_int_bound(tree.pending.len() as i32) as usize;
            let index = tree.pending.remove(pos);
            tree.add_children(index);
        }
        tree.shift(sea, min_y);
        if tree.portal {
            return tree
                .pieces
                .into_iter()
                .map(|p| Piece {
                    facing: p.facing,
                    min: p.min,
                    max: p.max,
                    body: p.body,
                })
                .collect();
        }
    }
    Vec::new()
}

impl Grow {
    fn fresh(seed: i64, cx: i32, cz: i32) -> Self {
        let mut rng = Legacy::new(0);
        rng.set_large_feature_seed(seed, cx, cz);
        Self {
            rng,
            pieces: Vec::new(),
            pending: Vec::new(),
            weights: Weight::table(),
            active: (0..11).collect(),
            previous: None,
            imposed: None,
            portal: false,
            start_x: 0,
            start_z: 0,
            total: 0,
        }
    }

    fn begin(&mut self, cx: i32, cz: i32) {
        let facing = self.rng.next_int_bound(4) as u8;
        let (min, max) = make_box(cx * 16 + 2, 64, cz * 16 + 2, facing, 5, 11, 5);
        self.start_x = min[0];
        self.start_z = min[2];
        self.pieces.push(Raw {
            facing,
            min,
            max,
            depth: 0,
            body: Body::Stairs { door: OPENING },
        });
    }

    fn add_children(&mut self, index: usize) {
        let facing = self.pieces[index].facing;
        let min = self.pieces[index].min;
        let max = self.pieces[index].max;
        let depth = self.pieces[index].depth;
        match self.pieces[index].body {
            Body::Stairs { .. } if index == 0 => {
                self.imposed = Some(PKind::Five);
                self.spawn(depth, forward(facing, min, max, 1, 1));
            }
            Body::Straight { left, right, .. } => {
                self.spawn(depth, forward(facing, min, max, 1, 1));
                if left {
                    self.spawn(depth, left_of(facing, min, max, 1, 2));
                }
                if right {
                    self.spawn(depth, right_of(facing, min, max, 1, 2));
                }
            }
            Body::Prison { .. }
            | Body::StraightStairs { .. }
            | Body::Stairs { .. }
            | Body::Chest { .. } => {
                self.spawn(depth, forward(facing, min, max, 1, 1));
            }
            Body::Turn { left, .. } => {
                let north_or_east = facing == 0 || facing == 1;
                let go_left = left == north_or_east;
                if go_left {
                    self.spawn(depth, left_of(facing, min, max, 1, 1));
                } else {
                    self.spawn(depth, right_of(facing, min, max, 1, 1));
                }
            }
            Body::Room { .. } => {
                self.spawn(depth, forward(facing, min, max, 4, 1));
                self.spawn(depth, left_of(facing, min, max, 1, 4));
                self.spawn(depth, right_of(facing, min, max, 1, 4));
            }
            Body::Five {
                left_low,
                left_high,
                right_low,
                right_high,
                ..
            } => {
                let mut a = 3;
                let mut b = 5;
                if facing == 3 || facing == 0 {
                    a = 8 - a;
                    b = 8 - b;
                }
                self.spawn(depth, forward(facing, min, max, 5, 1));
                if left_low {
                    self.spawn(depth, left_of(facing, min, max, a, 1));
                }
                if left_high {
                    self.spawn(depth, left_of(facing, min, max, b, 7));
                }
                if right_low {
                    self.spawn(depth, right_of(facing, min, max, a, 1));
                }
                if right_high {
                    self.spawn(depth, right_of(facing, min, max, b, 7));
                }
            }
            Body::Portal => self.portal = true,
            Body::Library { .. } | Body::Filler { .. } => {}
        }
    }

    fn spawn_at(&mut self, parent_depth: i32, foot: [i32; 3], facing: u8) {
        if parent_depth > 50 {
            return;
        }
        if (foot[0] - self.start_x).abs() > 112 || (foot[2] - self.start_z).abs() > 112 {
            return;
        }
        let depth = parent_depth + 1;
        if let Some(piece) = self.from_door(foot, facing, depth) {
            let index = self.pieces.len();
            self.pieces.push(piece);
            self.pending.push(index);
        }
    }

    fn from_door(&mut self, foot: [i32; 3], facing: u8, depth: i32) -> Option<Raw> {
        if !self.refresh() {
            return None;
        }
        if let Some(kind) = self.imposed.take() {
            if let Some(piece) = self.make(kind, foot, facing, depth) {
                return Some(piece);
            }
        }
        let mut attempt = 0;
        while attempt < 5 {
            attempt += 1;
            let mut sel = self.rng.next_int_bound(self.total);
            let active = self.active.clone();
            let mut skip_attempt = false;
            for idx in active {
                sel -= self.weights[idx].weight;
                if sel >= 0 {
                    continue;
                }
                if !self.weights[idx].do_place(depth) || self.previous == Some(idx) {
                    skip_attempt = true;
                    break;
                }
                let kind = self.weights[idx].kind;
                if let Some(piece) = self.make(kind, foot, facing, depth) {
                    self.weights[idx].count += 1;
                    self.previous = Some(idx);
                    if self.weights[idx].max > 0 && self.weights[idx].count >= self.weights[idx].max
                    {
                        self.active.retain(|&i| i != idx);
                    }
                    return Some(piece);
                }
            }
            if skip_attempt {
                continue;
            }
        }
        let (min, max) = self.filler_box(foot, facing)?;
        if min[1] > 1 {
            let steps = if facing == 0 || facing == 2 {
                max[2] - min[2] + 1
            } else {
                max[0] - min[0] + 1
            };
            return Some(self.boxed(facing, min, max, depth, Body::Filler { steps }));
        }
        None
    }

    fn refresh(&mut self) -> bool {
        let mut any = false;
        let mut total = 0;
        for &i in &self.active {
            let w = &self.weights[i];
            if w.max > 0 && w.count < w.max {
                any = true;
            }
            total += w.weight;
        }
        self.total = total;
        any
    }

    fn filler_box(&self, foot: [i32; 3], facing: u8) -> Option<([i32; 3], [i32; 3])> {
        let probe = orient_box(foot, [-1, -1, 0], 5, 5, 4, facing);
        let hit = self
            .pieces
            .iter()
            .find(|p| overlaps(p.min, p.max, probe.0, probe.1))?;
        if hit.min[1] != probe.0[1] {
            return None;
        }
        let (hit_min, hit_max) = (hit.min, hit.max);
        for depth in (1..=2).rev() {
            let box_ = orient_box(foot, [-1, -1, 0], 5, 5, depth, facing);
            if overlaps(hit_min, hit_max, box_.0, box_.1) {
                continue;
            }
            return Some(orient_box(foot, [-1, -1, 0], 5, 5, depth + 1, facing));
        }
        None
    }

    fn make(&mut self, kind: PKind, foot: [i32; 3], facing: u8, depth: i32) -> Option<Raw> {
        if matches!(kind, PKind::Library) {
            return self.make_library(foot, facing, depth);
        }
        let (off, w, h, d) = match kind {
            PKind::Straight | PKind::Chest => ([-1, -1, 0], 5, 5, 7),
            PKind::Prison => ([-1, -1, 0], 9, 5, 11),
            PKind::Left | PKind::Right => ([-1, -1, 0], 5, 5, 5),
            PKind::Room => ([-4, -1, 0], 11, 7, 11),
            PKind::StraightStairs => ([-1, -7, 0], 5, 11, 8),
            PKind::Stairs => ([-1, -7, 0], 5, 11, 5),
            PKind::Five => ([-4, -3, 0], 10, 9, 11),
            PKind::Portal => ([-4, -1, 0], 11, 8, 16),
            PKind::Library => unreachable!(),
        };
        let (min, max) = orient_box(foot, off, w, h, d, facing);
        if min[1] <= 10 || self.hits(min, max) {
            return None;
        }
        if matches!(kind, PKind::Portal) {
            return Some(self.boxed(facing, min, max, depth, Body::Portal));
        }
        let door = self.roll_door();
        let body = match kind {
            PKind::Straight => Body::Straight {
                left: self.rng.next_int_bound(2) == 0,
                right: self.rng.next_int_bound(2) == 0,
                door,
            },
            PKind::Prison => Body::Prison { door },
            PKind::Left => Body::Turn { left: true, door },
            PKind::Right => Body::Turn { left: false, door },
            PKind::Room => Body::Room {
                door,
                ty: self.rng.next_int_bound(5),
            },
            PKind::StraightStairs => Body::StraightStairs { door },
            PKind::Stairs => Body::Stairs { door },
            PKind::Five => Body::Five {
                door,
                left_low: self.rng.next_bool(),
                left_high: self.rng.next_bool(),
                right_low: self.rng.next_bool(),
                right_high: self.rng.next_int_bound(3) > 0,
            },
            PKind::Chest => Body::Chest { door },
            PKind::Library | PKind::Portal => unreachable!(),
        };
        Some(self.boxed(facing, min, max, depth, body))
    }

    fn make_library(&mut self, foot: [i32; 3], facing: u8, depth: i32) -> Option<Raw> {
        let tall = orient_box(foot, [-4, -1, 0], 14, 11, 15, facing);
        let (min, max) = if tall.0[1] > 10 && !self.hits(tall.0, tall.1) {
            tall
        } else {
            let short = orient_box(foot, [-4, -1, 0], 14, 6, 15, facing);
            if short.0[1] > 10 && !self.hits(short.0, short.1) {
                short
            } else {
                return None;
            }
        };
        let door = self.roll_door();
        let tall = max[1] - min[1] + 1 > 6;
        Some(self.boxed(facing, min, max, depth, Body::Library { door, tall }))
    }

    fn boxed(&self, facing: u8, min: [i32; 3], max: [i32; 3], depth: i32, body: Body) -> Raw {
        Raw {
            facing,
            min,
            max,
            depth,
            body,
        }
    }

    fn roll_door(&mut self) -> u8 {
        match self.rng.next_int_bound(5) {
            2 => WOOD,
            3 => GRATES,
            4 => IRON,
            _ => OPENING,
        }
    }

    fn hits(&self, min: [i32; 3], max: [i32; 3]) -> bool {
        self.pieces.iter().any(|p| overlaps(p.min, p.max, min, max))
    }

    fn shift(&mut self, sea: i32, min_y: i32) {
        let mut lo = i32::MAX;
        let mut hi = i32::MIN;
        for p in &self.pieces {
            lo = lo.min(p.min[1]);
            hi = hi.max(p.max[1]);
        }
        let span = hi - lo + 1;
        let mut y1 = span + min_y + 1;
        let limit = sea - 10;
        if y1 < limit {
            y1 += self.rng.next_int_bound(limit - y1);
        }
        let dy = y1 - hi;
        for p in &mut self.pieces {
            p.min[1] += dy;
            p.max[1] += dy;
        }
    }
}

fn forward(facing: u8, min: [i32; 3], max: [i32; 3], x_off: i32, y_off: i32) -> ([i32; 3], u8) {
    let y = min[1] + y_off;
    match facing {
        0 => ([min[0] + x_off, y, min[2] - 1], 0),
        2 => ([min[0] + x_off, y, max[2] + 1], 2),
        3 => ([min[0] - 1, y, min[2] + x_off], 3),
        _ => ([max[0] + 1, y, min[2] + x_off], 1),
    }
}

fn left_of(facing: u8, min: [i32; 3], _max: [i32; 3], y_off: i32, z_off: i32) -> ([i32; 3], u8) {
    let y = min[1] + y_off;
    match facing {
        0 | 2 => ([min[0] - 1, y, min[2] + z_off], 3),
        _ => ([min[0] + z_off, y, min[2] - 1], 0),
    }
}

fn right_of(facing: u8, min: [i32; 3], max: [i32; 3], y_off: i32, z_off: i32) -> ([i32; 3], u8) {
    let y = min[1] + y_off;
    match facing {
        0 | 2 => ([max[0] + 1, y, min[2] + z_off], 1),
        _ => ([min[0] + z_off, y, max[2] + 1], 2),
    }
}

impl Grow {
    fn spawn(&mut self, parent_depth: i32, step: ([i32; 3], u8)) {
        self.spawn_at(parent_depth, step.0, step.1);
    }
}

fn make_box(x: i32, y: i32, z: i32, facing: u8, w: i32, h: i32, d: i32) -> ([i32; 3], [i32; 3]) {
    if facing == 0 || facing == 2 {
        ([x, y, z], [x + w - 1, y + h - 1, z + d - 1])
    } else {
        ([x, y, z], [x + d - 1, y + h - 1, z + w - 1])
    }
}

/// `BoundingBox.orientBox`. Юг — ветка `default`.
fn orient_box(
    foot: [i32; 3],
    off: [i32; 3],
    w: i32,
    h: i32,
    d: i32,
    facing: u8,
) -> ([i32; 3], [i32; 3]) {
    let (x, y, z) = (foot[0], foot[1], foot[2]);
    let (ox, oy, oz) = (off[0], off[1], off[2]);
    match facing {
        0 => (
            [x + ox, y + oy, z - d + 1 + oz],
            [x + w - 1 + ox, y + h - 1 + oy, z + oz],
        ),
        3 => (
            [x - d + 1 + oz, y + oy, z + ox],
            [x + oz, y + h - 1 + oy, z + w - 1 + ox],
        ),
        1 => (
            [x + oz, y + oy, z + ox],
            [x + d - 1 + oz, y + h - 1 + oy, z + w - 1 + ox],
        ),
        _ => (
            [x + ox, y + oy, z + oz],
            [x + w - 1 + ox, y + h - 1 + oy, z + d - 1 + oz],
        ),
    }
}

/// `StructurePiece.SHAPE_CHECK_BLOCKS`: после установки ванила помечает эти
/// клетки на пост-обработку чанка (`updateFromNeighbourShapes`).
fn shape_check_ids(pal: &Palette) -> HashSet<u32> {
    let names = [
        "minecraft:nether_brick_fence",
        "minecraft:torch",
        "minecraft:wall_torch",
        "minecraft:oak_fence",
        "minecraft:spruce_fence",
        "minecraft:dark_oak_fence",
        "minecraft:pale_oak_fence",
        "minecraft:acacia_fence",
        "minecraft:birch_fence",
        "minecraft:jungle_fence",
        "minecraft:ladder",
        "minecraft:iron_bars",
    ];
    names
        .iter()
        .filter_map(|name| pal.blocks.get(*name))
        .map(|(id, _)| *id)
        .collect()
}

fn overlaps(a0: [i32; 3], a1: [i32; 3], b0: [i32; 3], b1: [i32; 3]) -> bool {
    a1[0] >= b0[0]
        && a0[0] <= b1[0]
        && a1[1] >= b0[1]
        && a0[1] <= b1[1]
        && a1[2] >= b0[2]
        && a0[2] <= b1[2]
}

fn opposite(dir: &str) -> &str {
    match dir {
        "north" => "south",
        "south" => "north",
        "east" => "west",
        "west" => "east",
        other => other,
    }
}

fn clockwise(dir: &str) -> &str {
    match dir {
        "north" => "east",
        "east" => "south",
        "south" => "west",
        "west" => "north",
        other => other,
    }
}

/// `placeBlock`: сначала зеркало куска, потом поворот. Север — тождество.
fn orient(pal: &Palette, state: u32, facing: u8) -> u32 {
    let mirror = facing == 2 || facing == 3;
    let turn = facing == 1 || facing == 3;
    let mut state = state;
    if let (Some(n), Some(e), Some(s), Some(w)) = (
        pal.prop(state, "north").map(str::to_owned),
        pal.prop(state, "east").map(str::to_owned),
        pal.prop(state, "south").map(str::to_owned),
        pal.prop(state, "west").map(str::to_owned),
    ) {
        let (n, s) = if mirror { (s, n) } else { (n, s) };
        let (n, e, s, w) = if turn { (w, n, e, s) } else { (n, e, s, w) };
        for (key, value) in [("north", n), ("east", e), ("south", s), ("west", w)] {
            if let Some(next) = pal.with_prop(state, key, &value) {
                state = next;
            }
        }
    }
    if let Some(dir0) = pal.prop(state, "facing").map(str::to_owned) {
        let mut dir = dir0.clone();
        if mirror {
            if dir == "north" || dir == "south" {
                dir = opposite(&dir).to_owned();
            }
            if let Some(hinge) = pal.prop(state, "hinge") {
                let flipped = if hinge == "left" { "right" } else { "left" };
                if let Some(next) = pal.with_prop(state, "hinge", flipped) {
                    state = next;
                }
            }
        }
        if turn {
            dir = clockwise(&dir).to_owned();
        }
        if let Some(next) = pal.with_prop(state, "facing", &dir) {
            state = next;
        }
    }
    state
}

fn with_prop(pal: &Palette, state: u32, key: &str, value: &str) -> u32 {
    pal.with_prop(state, key, value).unwrap_or(state)
}

struct Draw<'a> {
    pal: &'a Palette,
    level: &'a mut dyn Level,
    rng: &'a mut Worldgen,
    window: Bounds,
    facing: u8,
    min: [i32; 3],
    max: [i32; 3],
    bricks: u32,
    cracked: u32,
    mossy: u32,
    infested: u32,
    cave: u32,
    slab_smooth: u32,
    slab_brick: u32,
    cobble: u32,
    planks: u32,
    books: u32,
    web: u32,
    water: u32,
    lava: u32,
    chest: u32,
    spawner: u32,
    portal: u32,
    torch: u32,
    /// Блоки `SHAPE_CHECK_BLOCKS`: их форму ванила пересчитывает
    /// пост-обработкой чанка.
    shapes: HashSet<u32>,
}

impl<'a> Draw<'a> {
    fn new(
        pal: &'a Palette,
        level: &'a mut dyn Level,
        rng: &'a mut Worldgen,
        window: Bounds,
        facing: u8,
        min: [i32; 3],
        max: [i32; 3],
    ) -> Self {
        let state = |name: &str| pal.defaults[name];
        let slab = state("minecraft:smooth_stone_slab");
        Self {
            pal,
            level,
            rng,
            window,
            facing,
            min,
            max,
            bricks: state("minecraft:stone_bricks"),
            cracked: state("minecraft:cracked_stone_bricks"),
            mossy: state("minecraft:mossy_stone_bricks"),
            infested: state("minecraft:infested_stone_bricks"),
            cave: state("minecraft:cave_air"),
            slab_smooth: slab,
            slab_brick: state("minecraft:stone_brick_slab"),
            cobble: state("minecraft:cobblestone"),
            planks: state("minecraft:oak_planks"),
            books: state("minecraft:bookshelf"),
            web: state("minecraft:cobweb"),
            water: state("minecraft:water"),
            lava: state("minecraft:lava"),
            chest: state("minecraft:chest"),
            spawner: state("minecraft:spawner"),
            portal: state("minecraft:end_portal"),
            torch: state("minecraft:torch"),
            shapes: shape_check_ids(pal),
        }
    }

    fn world(&self, x: i32, y: i32, z: i32) -> [i32; 3] {
        let (wx, wz) = match self.facing {
            0 => (self.min[0] + x, self.max[2] - z),
            2 => (self.min[0] + x, self.min[2] + z),
            3 => (self.max[0] - z, self.min[2] + x),
            _ => (self.min[0] + z, self.min[2] + x),
        };
        [wx, self.min[1] + y, wz]
    }

    fn air_cell(&self, x: i32, y: i32, z: i32) -> bool {
        let p = self.world(x, y, z);
        if !self.window.contains(p) {
            return true;
        }
        self.pal.is_air(self.level.get(p[0], p[1], p[2]))
    }

    fn put(&mut self, state: u32, x: i32, y: i32, z: i32) {
        let p = self.world(x, y, z);
        if !self.window.contains(p) {
            return;
        }
        let state = orient(self.pal, state, self.facing);
        // `StructurePiece.placeBlock`: решётка, факел и лестница попадают в
        // `SHAPE_CHECK_BLOCKS` и получают пост-обработку чанка.
        let id = self.pal.block_of.get(state as usize).copied();
        if id.is_some_and(|id| self.shapes.contains(&id)) {
            self.level.mark_shape(p[0], p[1], p[2]);
        }
        self.level.set(p[0], p[1], p[2], state);
    }

    fn stone(&mut self, x0: i32, y0: i32, z0: i32, x1: i32, y1: i32, z1: i32, skip_air: bool) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                for z in z0..=z1 {
                    if skip_air && self.air_cell(x, y, z) {
                        continue;
                    }
                    let edge = y == y0 || y == y1 || x == x0 || x == x1 || z == z0 || z == z1;
                    // `SmoothStoneSelector.next` бросает `nextFloat` только на краю;
                    // внутренняя клетка сразу пещерный воздух.
                    let state = if !edge {
                        self.cave
                    } else {
                        let roll = self.rng.next_float();
                        if roll < 0.2 {
                            self.cracked
                        } else if roll < 0.5 {
                            self.mossy
                        } else if roll < 0.55 {
                            self.infested
                        } else {
                            self.bricks
                        }
                    };
                    self.put(state, x, y, z);
                }
            }
        }
    }

    fn blocks(
        &mut self,
        x0: i32,
        y0: i32,
        z0: i32,
        x1: i32,
        y1: i32,
        z1: i32,
        edge: u32,
        fill: u32,
        skip_air: bool,
    ) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                for z in z0..=z1 {
                    if skip_air && self.air_cell(x, y, z) {
                        continue;
                    }
                    let shell = y == y0 || y == y1 || x == x0 || x == x1 || z == z0 || z == z1;
                    self.put(if shell { edge } else { fill }, x, y, z);                }
            }
        }
    }

    fn maybe(
        &mut self,
        prob: f32,
        x0: i32,
        y0: i32,
        z0: i32,
        x1: i32,
        y1: i32,
        z1: i32,
        state: u32,
    ) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                for z in z0..=z1 {
                    if self.rng.next_float() > prob {
                        continue;
                    }
                    self.put(state, x, y, z);
                }
            }
        }
    }

    fn maybe_one(&mut self, prob: f32, x: i32, y: i32, z: i32, state: u32) {
        if self.rng.next_float() < prob {
            self.put(state, x, y, z);
        }
    }

    fn stair(&self, name: &str, facing: &str) -> u32 {
        with_prop(self.pal, self.pal.defaults[name], "facing", facing)
    }

    fn door(&self, name: &str, facing: &str, upper: bool) -> u32 {
        let state = with_prop(
            self.pal,
            self.pal.defaults[name],
            "half",
            if upper { "upper" } else { "lower" },
        );
        with_prop(self.pal, state, "facing", facing)
    }

    fn bars(&self, n: bool, e: bool, s: bool, w: bool) -> u32 {
        let mut state = self.pal.defaults["minecraft:iron_bars"];
        for (key, on) in [("north", n), ("east", e), ("south", s), ("west", w)] {
            if on {
                state = with_prop(self.pal, state, key, "true");
            }
        }
        state
    }

    fn fence(&self, n: bool, e: bool, s: bool, w: bool) -> u32 {
        let mut state = self.pal.defaults["minecraft:oak_fence"];
        for (key, on) in [("north", n), ("east", e), ("south", s), ("west", w)] {
            if on {
                state = with_prop(self.pal, state, key, "true");
            }
        }
        state
    }

    fn wall_torch(&self, facing: &str) -> u32 {
        with_prop(
            self.pal,
            self.pal.defaults["minecraft:wall_torch"],
            "facing",
            facing,
        )
    }

    fn ladder(&self, facing: &str) -> u32 {
        with_prop(
            self.pal,
            self.pal.defaults["minecraft:ladder"],
            "facing",
            facing,
        )
    }

    fn button(&self, facing: &str) -> u32 {
        with_prop(
            self.pal,
            self.pal.defaults["minecraft:stone_button"],
            "facing",
            facing,
        )
    }

    fn frame(&self, facing: &str, eye: bool) -> u32 {
        let state = with_prop(
            self.pal,
            self.pal.defaults["minecraft:end_portal_frame"],
            "facing",
            facing,
        );
        with_prop(self.pal, state, "eye", if eye { "true" } else { "false" })
    }

    fn slab_double(&self) -> u32 {
        with_prop(self.pal, self.slab_smooth, "type", "double")
    }

    fn small_door(&mut self, door: u8, x: i32, y: i32, z: i32) {
        match door {
            OPENING => self.blocks(x, y, z, x + 2, y + 2, z, self.cave, self.cave, false),
            WOOD => {
                self.put(self.bricks, x, y, z);
                self.put(self.bricks, x, y + 1, z);
                self.put(self.bricks, x, y + 2, z);
                self.put(self.bricks, x + 1, y + 2, z);
                self.put(self.bricks, x + 2, y + 2, z);
                self.put(self.bricks, x + 2, y + 1, z);
                self.put(self.bricks, x + 2, y, z);
                self.put(self.door("minecraft:oak_door", "north", false), x + 1, y, z);
                self.put(
                    self.door("minecraft:oak_door", "north", true),
                    x + 1,
                    y + 1,
                    z,
                );
            }
            GRATES => {
                self.put(self.cave, x + 1, y, z);
                self.put(self.cave, x + 1, y + 1, z);
                let west = self.bars(false, false, false, true);
                let both = self.bars(false, true, false, true);
                let east = self.bars(false, true, false, false);
                self.put(west, x, y, z);
                self.put(west, x, y + 1, z);
                self.put(both, x, y + 2, z);
                self.put(both, x + 1, y + 2, z);
                self.put(both, x + 2, y + 2, z);
                self.put(east, x + 2, y + 1, z);
                self.put(east, x + 2, y, z);
            }
            _ => {
                self.put(self.bricks, x, y, z);
                self.put(self.bricks, x, y + 1, z);
                self.put(self.bricks, x, y + 2, z);
                self.put(self.bricks, x + 1, y + 2, z);
                self.put(self.bricks, x + 2, y + 2, z);
                self.put(self.bricks, x + 2, y + 1, z);
                self.put(self.bricks, x + 2, y, z);
                self.put(
                    self.door("minecraft:iron_door", "north", false),
                    x + 1,
                    y,
                    z,
                );
                self.put(
                    self.door("minecraft:iron_door", "north", true),
                    x + 1,
                    y + 1,
                    z,
                );
                self.put(self.button("north"), x + 2, y + 1, z + 1);
                self.put(self.button("south"), x + 2, y + 1, z - 1);
            }
        }
    }

    fn chest_at(&mut self, x: i32, y: i32, z: i32) {
        let p = self.world(x, y, z);
        if !self.window.contains(p) {
            return;
        }
        let existing = self.level.get(p[0], p[1], p[2]);
        if self.pal.same_block(existing, self.chest) {
            return;
        }
        let state = self.reorient_chest(p);
        self.level.set(p[0], p[1], p[2], state);
        let _loot = self.rng.next_long();
    }

    /// `StructurePiece.reorient`: сторона сундука в мировых координатах.
    fn reorient_chest(&self, pos: [i32; 3]) -> u32 {
        const H: [(&str, i32, i32); 4] = [
            ("north", 0, -1),
            ("east", 1, 0),
            ("south", 0, 1),
            ("west", -1, 0),
        ];
        let mut solid: Option<&str> = None;
        for (name, dx, dz) in H {
            let st = self.level.get(pos[0] + dx, pos[1], pos[2] + dz);
            if self.pal.same_block(st, self.chest) {
                return self.chest;
            }
            if !self.pal.is_full_cube(st) {
                continue;
            }
            if solid.is_none() {
                solid = Some(name);
            } else {
                solid = None;
                break;
            }
        }
        if let Some(dir) = solid {
            return with_prop(self.pal, self.chest, "facing", opposite(dir));
        }
        let mut lock = "north";
        let mut front = step(lock);
        if self
            .pal
            .is_full_cube(self.level.get(pos[0] + front.0, pos[1], pos[2] + front.1))
        {
            lock = opposite(lock);
            front = step(lock);
        }
        if self
            .pal
            .is_full_cube(self.level.get(pos[0] + front.0, pos[1], pos[2] + front.1))
        {
            lock = clockwise(lock);
            front = step(lock);
        }
        if self
            .pal
            .is_full_cube(self.level.get(pos[0] + front.0, pos[1], pos[2] + front.1))
        {
            lock = opposite(lock);
        }
        with_prop(self.pal, self.chest, "facing", lock)
    }

    fn spawner_at(&mut self, x: i32, y: i32, z: i32) {
        let p = self.world(x, y, z);
        if !self.window.contains(p) {
            return;
        }
        self.level.set(p[0], p[1], p[2], self.spawner);
        let _roll = self.rng.next_int_bound(1);
    }

    fn straight(&mut self, door: u8, left: bool, right: bool) {
        self.stone(0, 0, 0, 4, 4, 6, true);
        self.small_door(door, 1, 1, 0);
        self.small_door(OPENING, 1, 1, 6);
        let east = self.wall_torch("east");
        let west = self.wall_torch("west");
        self.maybe_one(0.1, 1, 2, 1, east);
        self.maybe_one(0.1, 3, 2, 1, west);
        self.maybe_one(0.1, 1, 2, 5, east);
        self.maybe_one(0.1, 3, 2, 5, west);
        if left {
            self.blocks(0, 1, 2, 0, 3, 4, self.cave, self.cave, false);
        }
        if right {
            self.blocks(4, 1, 2, 4, 3, 4, self.cave, self.cave, false);
        }
    }

    fn prison(&mut self, door: u8) {
        self.stone(0, 0, 0, 8, 4, 10, true);
        self.small_door(door, 1, 1, 0);
        self.blocks(1, 1, 10, 3, 3, 10, self.cave, self.cave, false);
        self.stone(4, 1, 1, 4, 3, 1, false);
        self.stone(4, 1, 3, 4, 3, 3, false);
        self.stone(4, 1, 7, 4, 3, 7, false);
        self.stone(4, 1, 9, 4, 3, 9, false);
        let ns = self.bars(true, false, true, false);
        let nse = self.bars(true, true, true, false);
        let we = self.bars(false, true, false, true);
        for y in 1..=3 {
            self.put(ns, 4, y, 4);
            self.put(nse, 4, y, 5);
            self.put(ns, 4, y, 6);
            self.put(we, 5, y, 5);
            self.put(we, 6, y, 5);
            self.put(we, 7, y, 5);
        }
        self.put(ns, 4, 3, 2);
        self.put(ns, 4, 3, 8);
        let lower = self.door("minecraft:iron_door", "west", false);
        let upper = self.door("minecraft:iron_door", "west", true);
        self.put(lower, 4, 1, 2);
        self.put(upper, 4, 2, 2);
        self.put(lower, 4, 1, 8);
        self.put(upper, 4, 2, 8);
    }

    fn turn(&mut self, left: bool, door: u8) {
        self.stone(0, 0, 0, 4, 4, 4, true);
        self.small_door(door, 1, 1, 0);
        let north_or_east = self.facing == 0 || self.facing == 1;
        let carve_left = left == north_or_east;
        if carve_left {
            self.blocks(0, 1, 1, 0, 3, 3, self.cave, self.cave, false);
        } else {
            self.blocks(4, 1, 1, 4, 3, 3, self.cave, self.cave, false);
        }
    }

    fn room(&mut self, door: u8, ty: i32) {
        self.stone(0, 0, 0, 10, 6, 10, true);
        self.small_door(door, 4, 1, 0);
        self.blocks(4, 1, 10, 6, 3, 10, self.cave, self.cave, false);
        self.blocks(0, 1, 4, 0, 3, 6, self.cave, self.cave, false);
        self.blocks(10, 1, 4, 10, 3, 6, self.cave, self.cave, false);
        match ty {
            0 => {
                self.put(self.bricks, 5, 1, 5);
                self.put(self.bricks, 5, 2, 5);
                self.put(self.bricks, 5, 3, 5);
                self.put(self.wall_torch("west"), 4, 3, 5);
                self.put(self.wall_torch("east"), 6, 3, 5);
                self.put(self.wall_torch("south"), 5, 3, 4);
                self.put(self.wall_torch("north"), 5, 3, 6);
                for (x, z) in [
                    (4, 4),
                    (4, 5),
                    (4, 6),
                    (6, 4),
                    (6, 5),
                    (6, 6),
                    (5, 4),
                    (5, 6),
                ] {
                    self.put(self.slab_smooth, x, 1, z);
                }
            }
            1 => {
                for i in 0..5 {
                    self.put(self.bricks, 3, 1, 3 + i);
                    self.put(self.bricks, 7, 1, 3 + i);
                    self.put(self.bricks, 3 + i, 1, 3);
                    self.put(self.bricks, 3 + i, 1, 7);
                }
                self.put(self.bricks, 5, 1, 5);
                self.put(self.bricks, 5, 2, 5);
                self.put(self.bricks, 5, 3, 5);
                self.put(self.water, 5, 4, 5);
            }
            2 => {
                for z in 1..=9 {
                    self.put(self.cobble, 1, 3, z);
                    self.put(self.cobble, 9, 3, z);
                }
                for x in 1..=9 {
                    self.put(self.cobble, x, 3, 1);
                    self.put(self.cobble, x, 3, 9);
                }
                for (x, y, z) in [
                    (5, 1, 4),
                    (5, 1, 6),
                    (5, 3, 4),
                    (5, 3, 6),
                    (4, 1, 5),
                    (6, 1, 5),
                    (4, 3, 5),
                    (6, 3, 5),
                ] {
                    self.put(self.cobble, x, y, z);
                }
                for y in 1..=3 {
                    self.put(self.cobble, 4, y, 4);
                    self.put(self.cobble, 6, y, 4);
                    self.put(self.cobble, 4, y, 6);
                    self.put(self.cobble, 6, y, 6);
                }
                self.put(self.pal.defaults["minecraft:wall_torch"], 5, 3, 5);
                for z in 2..=8 {
                    self.put(self.planks, 2, 3, z);
                    self.put(self.planks, 3, 3, z);
                    if z <= 3 || z >= 7 {
                        self.put(self.planks, 4, 3, z);
                        self.put(self.planks, 5, 3, z);
                        self.put(self.planks, 6, 3, z);
                    }
                    self.put(self.planks, 7, 3, z);
                    self.put(self.planks, 8, 3, z);
                }
                let ladder = self.ladder("west");
                self.put(ladder, 9, 1, 3);
                self.put(ladder, 9, 2, 3);
                self.put(ladder, 9, 3, 3);
                self.chest_at(3, 4, 8);
            }
            _ => {}
        }
    }

    fn straight_stairs(&mut self, door: u8) {
        self.stone(0, 0, 0, 4, 10, 7, true);
        self.small_door(door, 1, 7, 0);
        self.small_door(OPENING, 1, 1, 7);
        let stairs = self.stair("minecraft:cobblestone_stairs", "south");
        for i in 0..6 {
            let i = i;
            self.put(stairs, 1, 6 - i, 1 + i);
            self.put(stairs, 2, 6 - i, 1 + i);
            self.put(stairs, 3, 6 - i, 1 + i);
            if i >= 5 {
                continue;
            }
            self.put(self.bricks, 1, 5 - i, 1 + i);
            self.put(self.bricks, 2, 5 - i, 1 + i);
            self.put(self.bricks, 3, 5 - i, 1 + i);
        }
    }

    fn stairs(&mut self, door: u8) {
        self.stone(0, 0, 0, 4, 10, 4, true);
        self.small_door(door, 1, 7, 0);
        self.small_door(OPENING, 1, 1, 4);
        let cells = [
            (2, 6, 1, true),
            (1, 5, 1, true),
            (1, 6, 1, false),
            (1, 5, 2, true),
            (1, 4, 3, true),
            (1, 5, 3, false),
            (2, 4, 3, true),
            (3, 3, 3, true),
            (3, 4, 3, false),
            (3, 3, 2, true),
            (3, 2, 1, true),
            (3, 3, 1, false),
            (2, 2, 1, true),
            (1, 1, 1, true),
            (1, 2, 1, false),
            (1, 1, 2, true),
            (1, 1, 3, false),
        ];
        for (x, y, z, brick) in cells {
            self.put(if brick { self.bricks } else { self.slab_smooth }, x, y, z);
        }
    }

    fn five(
        &mut self,
        door: u8,
        left_low: bool,
        left_high: bool,
        right_low: bool,
        right_high: bool,
    ) {
        self.stone(0, 0, 0, 9, 8, 10, true);
        self.small_door(door, 4, 3, 0);
        if left_low {
            self.blocks(0, 3, 1, 0, 5, 3, self.cave, self.cave, false);
        }
        if right_low {
            self.blocks(9, 3, 1, 9, 5, 3, self.cave, self.cave, false);
        }
        if left_high {
            self.blocks(0, 5, 7, 0, 7, 9, self.cave, self.cave, false);
        }
        if right_high {
            self.blocks(9, 5, 7, 9, 7, 9, self.cave, self.cave, false);
        }
        self.blocks(5, 1, 10, 7, 3, 10, self.cave, self.cave, false);
        self.stone(1, 2, 1, 8, 2, 6, false);
        self.stone(4, 1, 5, 4, 4, 9, false);
        self.stone(8, 1, 5, 8, 4, 9, false);
        self.stone(1, 4, 7, 3, 4, 9, false);
        self.stone(1, 3, 5, 3, 3, 6, false);
        self.blocks(1, 3, 4, 3, 3, 4, self.slab_smooth, self.slab_smooth, false);
        self.blocks(1, 4, 6, 3, 4, 6, self.slab_smooth, self.slab_smooth, false);
        self.stone(5, 1, 7, 7, 1, 8, false);
        self.blocks(5, 1, 9, 7, 1, 9, self.slab_smooth, self.slab_smooth, false);
        self.blocks(5, 2, 7, 7, 2, 7, self.slab_smooth, self.slab_smooth, false);
        self.blocks(4, 5, 7, 4, 5, 9, self.slab_smooth, self.slab_smooth, false);
        self.blocks(8, 5, 7, 8, 5, 9, self.slab_smooth, self.slab_smooth, false);
        let double = self.slab_double();
        self.blocks(5, 5, 7, 7, 5, 9, double, double, false);
        self.put(self.wall_torch("south"), 6, 5, 6);
    }

    fn chest_corridor(&mut self, door: u8) {
        self.stone(0, 0, 0, 4, 4, 6, true);
        self.small_door(door, 1, 1, 0);
        self.small_door(OPENING, 1, 1, 6);
        self.blocks(3, 1, 2, 3, 1, 4, self.bricks, self.bricks, false);
        self.put(self.slab_brick, 3, 1, 1);
        self.put(self.slab_brick, 3, 1, 5);
        self.put(self.slab_brick, 3, 2, 2);
        self.put(self.slab_brick, 3, 2, 4);
        for z in 2..=4 {
            self.put(self.slab_brick, 2, 1, z);
        }
        self.chest_at(3, 2, 3);
    }

    fn library(&mut self, door: u8, tall: bool) {
        let height = if tall { 11 } else { 6 };
        self.stone(0, 0, 0, 13, height - 1, 14, true);
        self.small_door(door, 4, 1, 0);
        self.maybe(0.07, 2, 1, 1, 11, 4, 13, self.web);
        let east = self.wall_torch("east");
        let west = self.wall_torch("west");
        for d in 1..=13 {
            let plank = (d - 1) % 4 == 0;
            let block = if plank { self.planks } else { self.books };
            self.blocks(1, 1, d, 1, 4, d, block, block, false);
            self.blocks(12, 1, d, 12, 4, d, block, block, false);
            if plank {
                self.put(east, 2, 3, d);
                self.put(west, 11, 3, d);
            }
            if !tall {
                continue;
            }
            self.blocks(1, 6, d, 1, 9, d, block, block, false);
            self.blocks(12, 6, d, 12, 9, d, block, block, false);
        }
        for d in (3..12).step_by(2) {
            self.blocks(3, 1, d, 4, 3, d, self.books, self.books, false);
            self.blocks(6, 1, d, 7, 3, d, self.books, self.books, false);
            self.blocks(9, 1, d, 10, 3, d, self.books, self.books, false);
        }
        if tall {
            self.blocks(1, 5, 1, 3, 5, 13, self.planks, self.planks, false);
            self.blocks(10, 5, 1, 12, 5, 13, self.planks, self.planks, false);
            self.blocks(4, 5, 1, 9, 5, 2, self.planks, self.planks, false);
            self.blocks(4, 5, 12, 9, 5, 13, self.planks, self.planks, false);
            self.put(self.planks, 9, 5, 11);
            self.put(self.planks, 8, 5, 11);
            self.put(self.planks, 9, 5, 10);
            let ns = self.fence(true, false, true, false);
            let we = self.fence(false, true, false, true);
            self.blocks(3, 6, 3, 3, 6, 11, ns, ns, false);
            self.blocks(10, 6, 3, 10, 6, 9, ns, ns, false);
            self.blocks(4, 6, 2, 9, 6, 2, we, we, false);
            self.blocks(4, 6, 12, 7, 6, 12, we, we, false);
            self.put(self.fence(true, true, false, false), 3, 6, 2);
            self.put(self.fence(false, true, true, false), 3, 6, 12);
            self.put(self.fence(true, false, false, true), 10, 6, 2);
            for i in 0..=2 {
                self.put(self.fence(false, false, true, true), 8 + i, 6, 12 - i);
                if i == 2 {
                    continue;
                }
                self.put(self.fence(true, true, false, false), 8 + i, 6, 11 - i);
            }
            let ladder = self.ladder("south");
            for y in 1..=7 {
                self.put(ladder, 10, y, 13);
            }
            let east_f = self.fence(false, true, false, false);
            let west_f = self.fence(false, false, false, true);
            let all = self.fence(true, true, true, true);
            self.put(east_f, 6, 9, 7);
            self.put(west_f, 7, 9, 7);
            self.put(east_f, 6, 8, 7);
            self.put(west_f, 7, 8, 7);
            self.put(all, 6, 7, 7);
            self.put(all, 7, 7, 7);
            self.put(east_f, 5, 7, 7);
            self.put(west_f, 8, 7, 7);
            self.put(self.fence(true, true, false, false), 6, 7, 6);
            self.put(self.fence(false, true, true, false), 6, 7, 8);
            self.put(self.fence(true, false, false, true), 7, 7, 6);
            self.put(self.fence(false, false, true, true), 7, 7, 8);
            for (x, z) in [(5, 7), (8, 7), (6, 6), (6, 8), (7, 6), (7, 8)] {
                self.put(self.torch, x, 8, z);
            }
        }
        self.chest_at(3, 3, 5);
        if tall {
            self.put(self.cave, 12, 9, 1);
            self.chest_at(12, 8, 1);
        }
    }

    fn portal(&mut self) {
        self.stone(0, 0, 0, 10, 7, 15, false);
        self.small_door(GRATES, 4, 1, 0);
        self.stone(1, 6, 1, 1, 6, 14, false);
        self.stone(9, 6, 1, 9, 6, 14, false);
        self.stone(2, 6, 1, 8, 6, 2, false);
        self.stone(2, 6, 14, 8, 6, 14, false);
        self.stone(1, 1, 1, 2, 1, 4, false);
        self.stone(8, 1, 1, 9, 1, 4, false);
        self.blocks(1, 1, 1, 1, 1, 3, self.lava, self.lava, false);
        self.blocks(9, 1, 1, 9, 1, 3, self.lava, self.lava, false);
        self.stone(3, 1, 8, 7, 1, 12, false);
        self.blocks(4, 1, 9, 6, 1, 11, self.lava, self.lava, false);
        let ns = self.bars(true, false, true, false);
        let we = self.bars(false, true, false, true);
        let mut z = 3;
        while z < 14 {
            self.blocks(0, 3, z, 0, 4, z, ns, ns, false);
            self.blocks(10, 3, z, 10, 4, z, ns, ns, false);
            z += 2;
        }
        let mut x = 2;
        while x < 9 {
            self.blocks(x, 3, 15, x, 4, 15, we, we, false);
            x += 2;
        }
        let stair = self.stair("minecraft:stone_brick_stairs", "north");
        self.stone(4, 1, 5, 6, 1, 7, false);
        self.stone(4, 2, 6, 6, 2, 7, false);
        self.stone(4, 3, 7, 6, 3, 7, false);
        for x in 4..=6 {
            self.put(stair, x, 1, 4);
            self.put(stair, x, 2, 5);
            self.put(stair, x, 3, 6);
        }
        let mut eyes = [false; 12];
        let mut all = true;
        for eye in &mut eyes {
            *eye = self.rng.next_float() > 0.9;
            all &= *eye;
        }
        self.put(self.frame("north", eyes[0]), 4, 3, 8);
        self.put(self.frame("north", eyes[1]), 5, 3, 8);
        self.put(self.frame("north", eyes[2]), 6, 3, 8);
        self.put(self.frame("south", eyes[3]), 4, 3, 12);
        self.put(self.frame("south", eyes[4]), 5, 3, 12);
        self.put(self.frame("south", eyes[5]), 6, 3, 12);
        self.put(self.frame("east", eyes[6]), 3, 3, 9);
        self.put(self.frame("east", eyes[7]), 3, 3, 10);
        self.put(self.frame("east", eyes[8]), 3, 3, 11);
        self.put(self.frame("west", eyes[9]), 7, 3, 9);
        self.put(self.frame("west", eyes[10]), 7, 3, 10);
        self.put(self.frame("west", eyes[11]), 7, 3, 11);
        if all {
            for z in 9..=11 {
                for x in 4..=6 {
                    self.put(self.portal, x, 3, z);
                }
            }
        }
        self.spawner_at(5, 3, 6);
    }

    fn filler(&mut self, steps: i32) {
        for i in 0..steps {
            for x in 0..=4 {
                self.put(self.bricks, x, 0, i);
                self.put(self.bricks, x, 4, i);
            }
            for y in 1..=3 {
                self.put(self.bricks, 0, y, i);
                self.put(self.cave, 1, y, i);
                self.put(self.cave, 2, y, i);
                self.put(self.cave, 3, y, i);
                self.put(self.bricks, 4, y, i);
            }
        }
    }
}

fn step(dir: &str) -> (i32, i32) {
    match dir {
        "north" => (0, -1),
        "east" => (1, 0),
        "south" => (0, 1),
        "west" => (-1, 0),
        _ => (0, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::{Body, build, plan_rings};

    #[test]
    fn stronghold_seed_12345_span() {
        let pieces = build(12345, -35, -103, 63, -64);
        let mut min = [i32::MAX, i32::MAX, i32::MAX];
        let mut max = [i32::MIN, i32::MIN, i32::MIN];
        for p in &pieces {
            for i in 0..3 {
                min[i] = min[i].min(p.min[i]);
                max[i] = max[i].max(p.max[i]);
            }
        }
        eprintln!(
            "span {} pieces x {}..{} y {}..{} z {}..{}",
            pieces.len(),
            min[0],
            max[0],
            min[1],
            max[1],
            min[2],
            max[2]
        );
        for (i, p) in pieces.iter().enumerate() {
            if matches!(p.body, Body::Portal | Body::Stairs { .. }) {
                eprintln!(
                    "  {i} {:?} face {} {}..{} {}..{} {}..{}",
                    std::mem::discriminant(&p.body),
                    p.facing,
                    p.min[0],
                    p.max[0],
                    p.min[1],
                    p.max[1],
                    p.min[2],
                    p.max[2]
                );
            }
        }
        assert!(pieces.iter().any(|p| matches!(p.body, Body::Portal)));
    }

    #[test]
    fn stronghold_seed_12345_against_saved_pieces() {
        let path = std::path::Path::new(
            r"E:\paper_rust\target\stronghold-12345\world\dimensions\minecraft\overworld\region",
        );
        if !path.is_dir() {
            return;
        }
        let store = chunkwake_world::anvil::RegionStore::new(path);
        let chunks = store.load_region(-2, -4).expect("region");
        let (_, root) = chunks
            .iter()
            .find(|(pos, _)| *pos == (-35, -103))
            .expect("start chunk");
        let children = root
            .get("structures")
            .and_then(|t| t.as_compound())
            .and_then(|c| c.get("starts"))
            .and_then(|t| t.as_compound())
            .and_then(|c| c.get("minecraft:stronghold"))
            .and_then(|t| t.as_compound())
            .and_then(|c| c.get("Children"))
            .and_then(|t| t.as_list())
            .expect("children");
        let ours = build(12345, -35, -103, 63, -64);
        let n = children.len().min(ours.len());
        let mut bad = None;
        for i in 0..n {
            let c = children[i].as_compound().unwrap();
            let id = c.get("id").and_then(|t| t.as_str()).unwrap_or("?");
            let bb = c.get("BB").and_then(|t| t.as_int_array()).unwrap_or(&[]);
            let p = &ours[i];
            let kind_ok =
                id == match body_kind(&p.body) {
                    "straight" => "minecraft:shs",
                    "prison" => "minecraft:shph",
                    "left" => "minecraft:shlt",
                    "right" => "minecraft:shrt",
                    "room" => "minecraft:shrc",
                    "straight_stairs" => "minecraft:shssd",
                    "stairs" => "minecraft:shsd",
                    "five" => "minecraft:sh5c",
                    "chest" => "minecraft:shcc",
                    "library" => "minecraft:shli",
                    "portal" => "minecraft:shpr",
                    "filler" => "minecraft:shfc",
                    _ => "",
                } || (body_kind(&p.body) == "stairs" && id == "minecraft:shstart");
            let box_ok = bb.len() == 6
                && bb[0] == p.min[0]
                && bb[1] == p.min[1]
                && bb[2] == p.min[2]
                && bb[3] == p.max[0]
                && bb[4] == p.max[1]
                && bb[5] == p.max[2];
            if !kind_ok || !box_ok {
                bad = Some(i);
                eprintln!("mismatch {i} van {id} {bb:?} ours {}", body_kind(&p.body));
                break;
            }
        }
        if bad.is_none() && children.len() != ours.len() {
            eprintln!("length {} vs {}", children.len(), ours.len());
        }
        assert!(bad.is_none(), "first mismatch {}", bad.unwrap());
        assert_eq!(children.len(), ours.len());
    }

    fn body_kind(body: &Body) -> &'static str {
        match body {
            Body::Straight { .. } => "straight",
            Body::Prison { .. } => "prison",
            Body::Turn { left: true, .. } => "left",
            Body::Turn { left: false, .. } => "right",
            Body::Room { .. } => "room",
            Body::StraightStairs { .. } => "straight_stairs",
            Body::Stairs { .. } => "stairs",
            Body::Five { .. } => "five",
            Body::Chest { .. } => "chest",
            Body::Library { .. } => "library",
            Body::Portal => "portal",
            Body::Filler { .. } => "filler",
        }
    }

    #[test]
    fn stronghold_portal_room() {
        let pieces = build(12345, 0, 0, 63, -64);
        assert!(
            pieces.iter().any(|p| matches!(p.body, Body::Portal)),
            "кусков {}",
            pieces.len()
        );
        assert!(pieces.len() > 8, "{}", pieces.len());
    }

    #[test]
    fn stronghold_seed_12345_starts() {
        let Some(data) = std::env::var_os("CHUNKWAKE_DATA").map(std::path::PathBuf::from) else {
            return;
        };
        let registries = chunkwake_data::registries::RegistryData::load(&data).unwrap();
        let biome_ids: std::collections::HashMap<String, u32> = registries
            .synchronized
            .iter()
            .find(|r| r.name == "worldgen/biome")
            .map(|r| {
                r.entries
                    .iter()
                    .enumerate()
                    .map(|(i, n)| (n.clone(), i as u32))
                    .collect()
            })
            .unwrap_or_default();
        let blocks = chunkwake_data::world::Blocks::load(&data).unwrap();
        let states = blocks.states();
        let mut noise = crate::Generator::load_with_version(&data, 12345, None).unwrap();
        noise.bind_biomes(biome_ids.clone());
        let state = |n: &str| blocks.default_state(n).ok().map(|s| s as u32);
        let world = crate::pipeline::Overworld::new(std::sync::Arc::new(noise), &state, &biome_ids)
            .unwrap()
            .with_features(&data, &states, &state, &biome_ids)
            .unwrap();
        let pos = world
            .locate_structure("minecraft:stronghold", [0, 64, 0])
            .expect("крепость");
        eprintln!(
            "stronghold {pos:?} chunk {}, {}",
            pos[0].div_euclid(16),
            pos[2].div_euclid(16)
        );
        assert_eq!(pos[0].rem_euclid(16), 8);
        assert_eq!(pos[2].rem_euclid(16), 8);
    }

    #[test]
    fn stronghold_rings_stable() {
        let a = plan_rings(12345, 32, 3, 128);
        let b = plan_rings(12345, 32, 3, 128);
        assert_eq!(a.len(), 128);
        assert_eq!(
            a.iter().map(|s| s.initial).collect::<Vec<_>>(),
            b.iter().map(|s| s.initial).collect::<Vec<_>>()
        );
        let (x, z) = a[0].initial;
        let dist = ((x * x + z * z) as f64).sqrt();
        assert!(dist > 80.0 && dist < 200.0, "{x} {z}");
    }
}
