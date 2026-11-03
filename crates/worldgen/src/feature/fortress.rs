//! Крепость Незера: дерево `NetherFortressPieces` на `Legacy`, затем
//! `moveInsideHeights(48, 70)`. Спаунер ифрита — блок без моба, как у
//! остальных структур. Лава входа ставится без тика.

use super::stronghold::{
    Draw, forward, left_of, make_box, orient_box, overlaps, right_of, with_prop,
};
use super::template::Bounds;
use super::{Bits, Legacy, Level, Palette, Worldgen};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PKind {
    BridgeStraight,
    BridgeCrossing,
    RoomCrossing,
    StairsRoom,
    MonsterThrone,
    CastleEntrance,
    SmallCorridor,
    SmallCrossing,
    RightTurn,
    LeftTurn,
    CorridorStairs,
    TBalcony,
    StalkRoom,
}

/// Данные `postProcess`. Сид торца — `selfSeed` из конструктора.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Body {
    Kind(PKind),
    EndFiller { seed: i32 },
    LeftTurn { chest: bool },
    RightTurn { chest: bool },
}

#[derive(Clone, Debug)]
pub(crate) struct Piece {
    pub facing: u8,
    pub min: [i32; 3],
    pub max: [i32; 3],
    pub body: Body,
}

struct Weight {
    kind: PKind,
    weight: i32,
    max: i32,
    in_row: bool,
    count: i32,
}

impl Weight {
    fn valid(&self) -> bool {
        self.max == 0 || self.count < self.max
    }
}

fn weights() -> Vec<Weight> {
    let w = |kind, weight, max, in_row| Weight {
        kind,
        weight,
        max,
        in_row,
        count: 0,
    };
    vec![
        w(PKind::BridgeStraight, 30, 0, true),
        w(PKind::BridgeCrossing, 10, 4, false),
        w(PKind::RoomCrossing, 10, 4, false),
        w(PKind::StairsRoom, 10, 3, false),
        w(PKind::MonsterThrone, 5, 2, false),
        w(PKind::CastleEntrance, 5, 1, false),
        w(PKind::SmallCorridor, 25, 0, true),
        w(PKind::SmallCrossing, 15, 5, false),
        w(PKind::RightTurn, 5, 10, false),
        w(PKind::LeftTurn, 5, 10, false),
        w(PKind::CorridorStairs, 10, 3, true),
        w(PKind::TBalcony, 7, 2, false),
        w(PKind::StalkRoom, 5, 2, false),
    ]
}

struct Raw {
    facing: u8,
    min: [i32; 3],
    max: [i32; 3],
    depth: i32,
    body: Body,
}

struct Grow {
    rng: Legacy,
    pieces: Vec<Raw>,
    pending: Vec<usize>,
    weights: Vec<Weight>,
    bridge: Vec<usize>,
    castle: Vec<usize>,
    previous: Option<usize>,
    start_x: i32,
    start_z: i32,
}

/// `NetherFortressStructure.generatePieces`.
pub(crate) fn build(seed: i64, cx: i32, cz: i32) -> Vec<Piece> {
    let mut rng = Legacy::new(0);
    rng.set_large_feature_seed(seed, cx, cz);
    let facing = rng.next_int_bound(4) as u8;
    let (min, max) = make_box(cx * 16 + 2, 64, cz * 16 + 2, facing, 19, 10, 19);
    let mut tree = Grow {
        rng,
        pieces: vec![Raw {
            facing,
            min,
            max,
            depth: 0,
            body: Body::Kind(PKind::BridgeCrossing),
        }],
        pending: Vec::new(),
        weights: weights(),
        bridge: (0..6).collect(),
        castle: (6..13).collect(),
        previous: None,
        start_x: min[0],
        start_z: min[2],
    };
    tree.add_children(0);
    while !tree.pending.is_empty() {
        let pos = tree.rng.next_int_bound(tree.pending.len() as i32) as usize;
        let index = tree.pending.remove(pos);
        tree.add_children(index);
    }
    tree.move_inside(48, 70);
    tree.pieces
        .into_iter()
        .map(|p| Piece {
            facing: p.facing,
            min: p.min,
            max: p.max,
            body: p.body,
        })
        .collect()
}

impl Grow {
    fn add_children(&mut self, index: usize) {
        let p = &self.pieces[index];
        let (facing, min, max, depth) = (p.facing, p.min, p.max, p.depth);
        let kind = match p.body {
            Body::Kind(kind) => kind,
            Body::LeftTurn { .. } => PKind::LeftTurn,
            Body::RightTurn { .. } => PKind::RightTurn,
            Body::EndFiller { .. } => return,
        };
        match kind {
            PKind::BridgeCrossing => {
                self.child(depth, forward(facing, min, max, 8, 3), false);
                self.child(depth, left_of(facing, min, max, 3, 8), false);
                self.child(depth, right_of(facing, min, max, 3, 8), false);
            }
            PKind::BridgeStraight => self.child(depth, forward(facing, min, max, 1, 3), false),
            PKind::CorridorStairs => self.child(depth, forward(facing, min, max, 1, 0), true),
            PKind::TBalcony => {
                let z_off = if facing == 3 || facing == 0 { 5 } else { 1 };
                let castle = self.rng.next_int_bound(8) > 0;
                self.child(depth, left_of(facing, min, max, 0, z_off), castle);
                let castle = self.rng.next_int_bound(8) > 0;
                self.child(depth, right_of(facing, min, max, 0, z_off), castle);
            }
            PKind::CastleEntrance => self.child(depth, forward(facing, min, max, 5, 3), true),
            PKind::SmallCrossing => {
                self.child(depth, forward(facing, min, max, 1, 0), true);
                self.child(depth, left_of(facing, min, max, 0, 1), true);
                self.child(depth, right_of(facing, min, max, 0, 1), true);
            }
            PKind::LeftTurn => self.child(depth, left_of(facing, min, max, 0, 1), true),
            PKind::SmallCorridor => self.child(depth, forward(facing, min, max, 1, 0), true),
            PKind::RightTurn => self.child(depth, right_of(facing, min, max, 0, 1), true),
            PKind::StalkRoom => {
                self.child(depth, forward(facing, min, max, 5, 3), true);
                self.child(depth, forward(facing, min, max, 5, 11), true);
            }
            PKind::RoomCrossing => {
                self.child(depth, forward(facing, min, max, 2, 0), false);
                self.child(depth, left_of(facing, min, max, 0, 2), false);
                self.child(depth, right_of(facing, min, max, 0, 2), false);
            }
            PKind::StairsRoom => self.child(depth, right_of(facing, min, max, 6, 2), false),
            PKind::MonsterThrone => {}
        }
    }

    /// `generateAndAddPiece`. За радиусом 112 торец строится и выбрасывается,
    /// но его `nextInt` уже съеден.
    fn child(&mut self, parent_depth: i32, step: ([i32; 3], u8), castle: bool) {
        let (foot, facing) = step;
        if (foot[0] - self.start_x).abs() > 112 || (foot[2] - self.start_z).abs() > 112 {
            let _ = self.end_filler(foot, facing, parent_depth);
            return;
        }
        if let Some(raw) = self.generate(castle, foot, facing, parent_depth + 1) {
            self.pieces.push(raw);
            self.pending.push(self.pieces.len() - 1);
        }
    }

    fn generate(&mut self, castle: bool, foot: [i32; 3], facing: u8, depth: i32) -> Option<Raw> {
        let list = if castle {
            self.castle.clone()
        } else {
            self.bridge.clone()
        };
        let mut any = false;
        let mut total = 0;
        for &i in &list {
            let w = &self.weights[i];
            if w.max > 0 && w.count < w.max {
                any = true;
            }
            total += w.weight;
        }
        let total = if any { total } else { -1 };
        let go = total > 0 && depth <= 30;
        let mut attempts = 0;
        while attempts < 5 && go {
            attempts += 1;
            let mut pick = self.rng.next_int_bound(total);
            for &i in &list {
                pick -= self.weights[i].weight;
                if pick < 0 {
                    let w = &self.weights[i];
                    if !w.valid() || self.previous == Some(i) && !w.in_row {
                        break;
                    }
                    let kind = w.kind;
                    if let Some(raw) = self.make(kind, foot, facing, depth) {
                        self.weights[i].count += 1;
                        self.previous = Some(i);
                        if !self.weights[i].valid() {
                            let target = if castle {
                                &mut self.castle
                            } else {
                                &mut self.bridge
                            };
                            target.retain(|&j| j != i);
                        }
                        return Some(raw);
                    }
                }
            }
        }
        self.end_filler(foot, facing, depth)
    }

    fn fits(&self, min: [i32; 3], max: [i32; 3]) -> bool {
        min[1] > 10 && !self.pieces.iter().any(|p| overlaps(p.min, p.max, min, max))
    }

    fn end_filler(&mut self, foot: [i32; 3], facing: u8, depth: i32) -> Option<Raw> {
        let (min, max) = orient_box(foot, [-1, -3, 0], 5, 10, 8, facing);
        if !self.fits(min, max) {
            return None;
        }
        let seed = self.rng.next_int();
        Some(Raw {
            facing,
            min,
            max,
            depth,
            body: Body::EndFiller { seed },
        })
    }

    fn make(&mut self, kind: PKind, foot: [i32; 3], facing: u8, depth: i32) -> Option<Raw> {
        let (off, w, h, d) = match kind {
            PKind::BridgeStraight => ([-1, -3, 0], 5, 10, 19),
            PKind::BridgeCrossing => ([-8, -3, 0], 19, 10, 19),
            PKind::RoomCrossing => ([-2, 0, 0], 7, 9, 7),
            PKind::StairsRoom => ([-2, 0, 0], 7, 11, 7),
            PKind::MonsterThrone => ([-2, 0, 0], 7, 8, 9),
            PKind::CastleEntrance | PKind::StalkRoom => ([-5, -3, 0], 13, 14, 13),
            PKind::SmallCorridor | PKind::SmallCrossing | PKind::RightTurn | PKind::LeftTurn => {
                ([-1, 0, 0], 5, 7, 5)
            }
            PKind::CorridorStairs => ([-1, -7, 0], 5, 14, 10),
            PKind::TBalcony => ([-3, 0, 0], 9, 7, 9),
        };
        let (min, max) = orient_box(foot, off, w, h, d, facing);
        if !self.fits(min, max) {
            return None;
        }
        let body = match kind {
            PKind::LeftTurn => Body::LeftTurn {
                chest: self.rng.next_int_bound(3) == 0,
            },
            PKind::RightTurn => Body::RightTurn {
                chest: self.rng.next_int_bound(3) == 0,
            },
            other => Body::Kind(other),
        };
        Some(Raw {
            facing,
            min,
            max,
            depth,
            body,
        })
    }

    /// `StructurePiecesBuilder.moveInsideHeights`.
    fn move_inside(&mut self, low: i32, high: i32) {
        let lo = self.pieces.iter().map(|p| p.min[1]).min().unwrap_or(0);
        let hi = self.pieces.iter().map(|p| p.max[1]).max().unwrap_or(0);
        let span = high - low + 1 - (hi - lo + 1);
        let y0 = if span > 1 {
            low + self.rng.next_int_bound(span)
        } else {
            low
        };
        let dy = y0 - lo;
        for p in &mut self.pieces {
            p.min[1] += dy;
            p.max[1] += dy;
        }
    }
}

const N: u8 = 1;
const E: u8 = 2;
const S: u8 = 4;
const W: u8 = 8;

struct Paint<'a> {
    d: Draw<'a>,
    bricks: u32,
    air: u32,
}

impl Paint<'_> {
    #[allow(clippy::too_many_arguments)]
    fn bx(&mut self, x0: i32, y0: i32, z0: i32, x1: i32, y1: i32, z1: i32, state: u32) {
        self.d.blocks(x0, y0, z0, x1, y1, z1, state, state, false);
    }

    fn nb(&mut self, x0: i32, y0: i32, z0: i32, x1: i32, y1: i32, z1: i32) {
        self.bx(x0, y0, z0, x1, y1, z1, self.bricks);
    }

    fn air(&mut self, x0: i32, y0: i32, z0: i32, x1: i32, y1: i32, z1: i32) {
        self.bx(x0, y0, z0, x1, y1, z1, self.air);
    }

    fn col(&mut self, x: i32, z: i32) {
        self.d.column_down(self.bricks, x, -1, z);
    }

    fn cols(&mut self, x1: i32, z1: i32) {
        for x in 0..=x1 {
            for z in 0..=z1 {
                self.col(x, z);
            }
        }
    }

    fn put(&mut self, state: u32, x: i32, y: i32, z: i32) {
        self.d.put(state, x, y, z);
    }

    fn fence(&self, sides: u8) -> u32 {
        let pal = self.d.palette();
        let mut state = pal.defaults["minecraft:nether_brick_fence"];
        for (bit, key) in [(N, "north"), (E, "east"), (S, "south"), (W, "west")] {
            if sides & bit != 0 {
                state = with_prop(pal, state, key, "true");
            }
        }
        state
    }

    fn stairs(&self, facing: &str) -> u32 {
        let pal = self.d.palette();
        with_prop(
            pal,
            pal.defaults["minecraft:nether_brick_stairs"],
            "facing",
            facing,
        )
    }

    fn block(&self, name: &str) -> u32 {
        self.d.palette().defaults[name]
    }
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
        let d = Draw::new(pal, level, rng, window, self.facing, self.min, self.max);
        let mut p = Paint {
            d,
            bricks: pal.defaults["minecraft:nether_bricks"],
            air: pal.defaults["minecraft:air"],
        };
        match self.body {
            Body::EndFiller { seed } => end_filler(&mut p, seed),
            Body::LeftTurn { chest } => left_turn(&mut p, chest),
            Body::RightTurn { chest } => right_turn(&mut p, chest),
            Body::Kind(kind) => match kind {
                PKind::BridgeCrossing => bridge_crossing(&mut p),
                PKind::BridgeStraight => bridge_straight(&mut p),
                PKind::CorridorStairs => corridor_stairs(&mut p),
                PKind::TBalcony => t_balcony(&mut p),
                PKind::CastleEntrance => castle_entrance(&mut p),
                PKind::SmallCrossing => small_crossing(&mut p),
                PKind::SmallCorridor => small_corridor(&mut p),
                PKind::StalkRoom => stalk_room(&mut p),
                PKind::MonsterThrone => monster_throne(&mut p),
                PKind::RoomCrossing => room_crossing(&mut p),
                PKind::StairsRoom => stairs_room(&mut p),
                PKind::LeftTurn => left_turn(&mut p, false),
                PKind::RightTurn => right_turn(&mut p, false),
            },
        }
    }
}

fn bridge_crossing(p: &mut Paint) {
    p.nb(7, 3, 0, 11, 4, 18);
    p.nb(0, 3, 7, 18, 4, 11);
    p.air(8, 5, 0, 10, 7, 18);
    p.air(0, 5, 8, 18, 7, 10);
    p.nb(7, 5, 0, 7, 5, 7);
    p.nb(7, 5, 11, 7, 5, 18);
    p.nb(11, 5, 0, 11, 5, 7);
    p.nb(11, 5, 11, 11, 5, 18);
    p.nb(0, 5, 7, 7, 5, 7);
    p.nb(11, 5, 7, 18, 5, 7);
    p.nb(0, 5, 11, 7, 5, 11);
    p.nb(11, 5, 11, 18, 5, 11);
    p.nb(7, 2, 0, 11, 2, 5);
    p.nb(7, 2, 13, 11, 2, 18);
    p.nb(7, 0, 0, 11, 1, 3);
    p.nb(7, 0, 15, 11, 1, 18);
    for x in 7..=11 {
        for z in 0..=2 {
            p.col(x, z);
            p.col(x, 18 - z);
        }
    }
    p.nb(0, 2, 7, 5, 2, 11);
    p.nb(13, 2, 7, 18, 2, 11);
    p.nb(0, 0, 7, 3, 1, 11);
    p.nb(15, 0, 7, 18, 1, 11);
    for x in 0..=2 {
        for z in 7..=11 {
            p.col(x, z);
            p.col(18 - x, z);
        }
    }
}

/// `BridgeEndFiller`: свой `SingleThreadedRandomSource(selfSeed)` — тот же LCG.
fn end_filler(p: &mut Paint, seed: i32) {
    let mut r = Legacy::new(seed as i64);
    for x in 0..=4 {
        for y in 3..=4 {
            let z = r.next_int_bound(8);
            p.nb(x, y, 0, x, y, z);
        }
    }
    let z = r.next_int_bound(8);
    p.nb(0, 5, 0, 0, 5, z);
    let z = r.next_int_bound(8);
    p.nb(4, 5, 0, 4, 5, z);
    for x in 0..=4 {
        let z = r.next_int_bound(5);
        p.nb(x, 2, 0, x, 2, z);
    }
    for x in 0..=4 {
        for y in 0..=1 {
            let z = r.next_int_bound(3);
            p.nb(x, y, 0, x, y, z);
        }
    }
}

fn bridge_straight(p: &mut Paint) {
    p.nb(0, 3, 0, 4, 4, 18);
    p.air(1, 5, 0, 3, 7, 18);
    p.nb(0, 5, 0, 0, 5, 18);
    p.nb(4, 5, 0, 4, 5, 18);
    p.nb(0, 2, 0, 4, 2, 5);
    p.nb(0, 2, 13, 4, 2, 18);
    p.nb(0, 0, 0, 4, 1, 3);
    p.nb(0, 0, 15, 4, 1, 18);
    for x in 0..=4 {
        for z in 0..=2 {
            p.col(x, z);
            p.col(x, 18 - z);
        }
    }
    let nse = p.fence(N | S | E);
    let nsw = p.fence(N | S | W);
    p.bx(0, 1, 1, 0, 4, 1, nse);
    p.bx(0, 3, 4, 0, 4, 4, nse);
    p.bx(0, 3, 14, 0, 4, 14, nse);
    p.bx(0, 1, 17, 0, 4, 17, nse);
    p.bx(4, 1, 1, 4, 4, 1, nsw);
    p.bx(4, 3, 4, 4, 4, 4, nsw);
    p.bx(4, 3, 14, 4, 4, 14, nsw);
    p.bx(4, 1, 17, 4, 4, 17, nsw);
}

fn corridor_stairs(p: &mut Paint) {
    let stairs = p.stairs("south");
    let ns = p.fence(N | S);
    for step in 0..=9 {
        let floor = (7 - step).max(1);
        let roof = (floor + 5).max(14 - step).min(13);
        p.nb(0, 0, step, 4, floor, step);
        p.air(1, floor + 1, step, 3, roof - 1, step);
        if step <= 6 {
            for x in 1..=3 {
                p.put(stairs, x, floor + 1, step);
            }
        }
        p.nb(0, roof, step, 4, roof, step);
        p.nb(0, floor + 1, step, 0, roof - 1, step);
        p.nb(4, floor + 1, step, 4, roof - 1, step);
        if step & 1 == 0 {
            p.bx(0, floor + 2, step, 0, floor + 3, step, ns);
            p.bx(4, floor + 2, step, 4, floor + 3, step, ns);
        }
        for x in 0..=4 {
            p.col(x, step);
        }
    }
}

fn t_balcony(p: &mut Paint) {
    let ns = p.fence(N | S);
    let we = p.fence(W | E);
    p.nb(0, 0, 0, 8, 1, 8);
    p.air(0, 2, 0, 8, 5, 8);
    p.nb(0, 6, 0, 8, 6, 5);
    p.nb(0, 2, 0, 2, 5, 0);
    p.nb(6, 2, 0, 8, 5, 0);
    p.bx(1, 3, 0, 1, 4, 0, we);
    p.bx(7, 3, 0, 7, 4, 0, we);
    p.nb(0, 2, 4, 8, 2, 8);
    p.air(1, 1, 4, 2, 2, 4);
    p.air(6, 1, 4, 7, 2, 4);
    p.bx(1, 3, 8, 7, 3, 8, we);
    let es = p.fence(E | S);
    p.put(es, 0, 3, 8);
    let ws = p.fence(W | S);
    p.put(ws, 8, 3, 8);
    p.bx(0, 3, 6, 0, 3, 7, ns);
    p.bx(8, 3, 6, 8, 3, 7, ns);
    p.nb(0, 3, 4, 0, 5, 5);
    p.nb(8, 3, 4, 8, 5, 5);
    p.nb(1, 3, 5, 2, 5, 5);
    p.nb(6, 3, 5, 7, 5, 5);
    p.bx(1, 4, 5, 1, 5, 5, we);
    p.bx(7, 4, 5, 7, 5, 5, we);
    for z in 0..=5 {
        for x in 0..=8 {
            p.col(x, z);
        }
    }
}

/// Общие стены входа и комнаты с адским наростом.
fn castle_shell(p: &mut Paint) {
    p.nb(0, 3, 0, 12, 4, 12);
    p.air(0, 5, 0, 12, 13, 12);
    p.nb(0, 5, 0, 1, 12, 12);
    p.nb(11, 5, 0, 12, 12, 12);
    p.nb(2, 5, 11, 4, 12, 12);
    p.nb(8, 5, 11, 10, 12, 12);
    p.nb(5, 9, 11, 7, 12, 12);
    p.nb(2, 5, 0, 4, 12, 1);
    p.nb(8, 5, 0, 10, 12, 1);
    p.nb(5, 9, 0, 7, 12, 1);
    p.nb(2, 11, 2, 10, 12, 10);
}

/// Зубцы и окна-решётки.
fn castle_battlements(p: &mut Paint) {
    let we = p.fence(W | E);
    let ns = p.fence(N | S);
    let bricks = p.bricks;
    for i in (1..=11).step_by(2) {
        p.bx(i, 10, 0, i, 11, 0, we);
        p.bx(i, 10, 12, i, 11, 12, we);
        p.bx(0, 10, i, 0, 11, i, ns);
        p.bx(12, 10, i, 12, 11, i, ns);
        p.put(bricks, i, 13, 0);
        p.put(bricks, i, 13, 12);
        p.put(bricks, 0, 13, i);
        p.put(bricks, 12, 13, i);
        if i != 11 {
            p.put(we, i + 1, 13, 0);
            p.put(we, i + 1, 13, 12);
            p.put(ns, 0, 13, i + 1);
            p.put(ns, 12, 13, i + 1);
        }
    }
    let corner = p.fence(N | E);
    p.put(corner, 0, 13, 0);
    let corner = p.fence(S | E);
    p.put(corner, 0, 13, 12);
    let corner = p.fence(S | W);
    p.put(corner, 12, 13, 12);
    let corner = p.fence(N | W);
    p.put(corner, 12, 13, 0);
    let nsw = p.fence(N | S | W);
    let nse = p.fence(N | S | E);
    for z in (3..=9).step_by(2) {
        p.bx(1, 7, z, 1, 8, z, nsw);
        p.bx(11, 7, z, 11, 8, z, nse);
    }
}

fn castle_base(p: &mut Paint) {
    p.nb(4, 2, 0, 8, 2, 12);
    p.nb(0, 2, 4, 12, 2, 8);
    p.nb(4, 0, 0, 8, 1, 3);
    p.nb(4, 0, 9, 8, 1, 12);
    p.nb(0, 0, 4, 3, 1, 8);
    p.nb(9, 0, 4, 12, 1, 8);
    for x in 4..=8 {
        for z in 0..=2 {
            p.col(x, z);
            p.col(x, 12 - z);
        }
    }
    for x in 0..=2 {
        for z in 4..=8 {
            p.col(x, z);
            p.col(12 - x, z);
        }
    }
}

fn castle_entrance(p: &mut Paint) {
    castle_shell(p);
    let fence = p.fence(0);
    p.bx(5, 8, 0, 7, 8, 0, fence);
    castle_battlements(p);
    castle_base(p);
    p.nb(5, 5, 5, 7, 5, 7);
    p.air(6, 1, 6, 6, 4, 6);
    let bricks = p.bricks;
    p.put(bricks, 6, 0, 6);
    let lava = p.block("minecraft:lava");
    p.put(lava, 6, 5, 6);
}

fn stalk_room(p: &mut Paint) {
    castle_shell(p);
    castle_battlements(p);
    let stairs = p.stairs("north");
    for ix in 0..=6 {
        let z = ix + 4;
        for x in 5..=7 {
            p.put(stairs, x, 5 + ix, z);
        }
        if (5..=8).contains(&z) {
            p.nb(5, 5, z, 7, ix + 4, z);
        } else if (9..=10).contains(&z) {
            p.nb(5, 8, z, 7, ix + 4, z);
        }
        if ix >= 1 {
            p.air(5, 6 + ix, z, 7, 9 + ix, z);
        }
    }
    for x in 5..=7 {
        p.put(stairs, x, 12, 11);
    }
    let nse = p.fence(N | S | E);
    let nsw = p.fence(N | S | W);
    p.bx(5, 6, 7, 5, 7, 7, nse);
    p.bx(7, 6, 7, 7, 7, 7, nsw);
    p.air(5, 13, 12, 7, 13, 12);
    p.nb(2, 5, 2, 3, 5, 3);
    p.nb(2, 5, 9, 3, 5, 10);
    p.nb(2, 5, 4, 2, 5, 8);
    p.nb(9, 5, 2, 10, 5, 3);
    p.nb(9, 5, 9, 10, 5, 10);
    p.nb(10, 5, 4, 10, 5, 8);
    let east = p.stairs("east");
    let west = p.stairs("west");
    for z in [2, 3, 9, 10] {
        p.put(west, 4, 5, z);
    }
    for z in [2, 3, 9, 10] {
        p.put(east, 8, 5, z);
    }
    let soul = p.block("minecraft:soul_sand");
    let wart = p.block("minecraft:nether_wart");
    p.bx(3, 4, 4, 4, 4, 8, soul);
    p.bx(8, 4, 4, 9, 4, 8, soul);
    p.bx(3, 5, 4, 4, 5, 8, wart);
    p.bx(8, 5, 4, 9, 5, 8, wart);
    castle_base(p);
}

fn small_crossing(p: &mut Paint) {
    p.nb(0, 0, 0, 4, 1, 4);
    p.air(0, 2, 0, 4, 5, 4);
    p.nb(0, 2, 0, 0, 5, 0);
    p.nb(4, 2, 0, 4, 5, 0);
    p.nb(0, 2, 4, 0, 5, 4);
    p.nb(4, 2, 4, 4, 5, 4);
    p.nb(0, 6, 0, 4, 6, 4);
    p.cols(4, 4);
}

fn left_turn(p: &mut Paint, chest: bool) {
    p.nb(0, 0, 0, 4, 1, 4);
    p.air(0, 2, 0, 4, 5, 4);
    let we = p.fence(W | E);
    let ns = p.fence(N | S);
    p.nb(4, 2, 0, 4, 5, 4);
    p.bx(4, 3, 1, 4, 4, 1, ns);
    p.bx(4, 3, 3, 4, 4, 3, ns);
    p.nb(0, 2, 0, 0, 5, 0);
    p.nb(0, 2, 4, 3, 5, 4);
    p.bx(1, 3, 4, 1, 4, 4, we);
    p.bx(3, 3, 4, 3, 4, 4, we);
    if chest {
        p.d.chest_at(3, 2, 3, "minecraft:chests/nether_bridge");
    }
    p.nb(0, 6, 0, 4, 6, 4);
    p.cols(4, 4);
}

fn small_corridor(p: &mut Paint) {
    p.nb(0, 0, 0, 4, 1, 4);
    p.air(0, 2, 0, 4, 5, 4);
    let ns = p.fence(N | S);
    p.nb(0, 2, 0, 0, 5, 4);
    p.nb(4, 2, 0, 4, 5, 4);
    p.bx(0, 3, 1, 0, 4, 1, ns);
    p.bx(0, 3, 3, 0, 4, 3, ns);
    p.bx(4, 3, 1, 4, 4, 1, ns);
    p.bx(4, 3, 3, 4, 4, 3, ns);
    p.nb(0, 6, 0, 4, 6, 4);
    p.cols(4, 4);
}

fn right_turn(p: &mut Paint, chest: bool) {
    p.nb(0, 0, 0, 4, 1, 4);
    p.air(0, 2, 0, 4, 5, 4);
    let we = p.fence(W | E);
    let ns = p.fence(N | S);
    p.nb(0, 2, 0, 0, 5, 4);
    p.bx(0, 3, 1, 0, 4, 1, ns);
    p.bx(0, 3, 3, 0, 4, 3, ns);
    p.nb(4, 2, 0, 4, 5, 0);
    p.nb(1, 2, 4, 4, 5, 4);
    p.bx(1, 3, 4, 1, 4, 4, we);
    p.bx(3, 3, 4, 3, 4, 4, we);
    if chest {
        p.d.chest_at(1, 2, 3, "minecraft:chests/nether_bridge");
    }
    p.nb(0, 6, 0, 4, 6, 4);
    p.cols(4, 4);
}

fn monster_throne(p: &mut Paint) {
    p.air(0, 2, 0, 6, 7, 7);
    p.nb(1, 0, 0, 5, 1, 7);
    p.nb(1, 2, 1, 5, 2, 7);
    p.nb(1, 3, 2, 5, 3, 7);
    p.nb(1, 4, 3, 5, 4, 7);
    p.nb(1, 2, 0, 1, 4, 2);
    p.nb(5, 2, 0, 5, 4, 2);
    p.nb(1, 5, 2, 1, 5, 3);
    p.nb(5, 5, 2, 5, 5, 3);
    p.nb(0, 5, 3, 0, 5, 8);
    p.nb(6, 5, 3, 6, 5, 8);
    p.nb(1, 5, 8, 5, 5, 8);
    let we = p.fence(W | E);
    let ns = p.fence(N | S);
    let w = p.fence(W);
    let e = p.fence(E);
    p.put(w, 1, 6, 3);
    p.put(e, 5, 6, 3);
    let en = p.fence(E | N);
    p.put(en, 0, 6, 3);
    let wn = p.fence(W | N);
    p.put(wn, 6, 6, 3);
    p.bx(0, 6, 4, 0, 6, 7, ns);
    p.bx(6, 6, 4, 6, 6, 7, ns);
    let es = p.fence(E | S);
    p.put(es, 0, 6, 8);
    let ws = p.fence(W | S);
    p.put(ws, 6, 6, 8);
    p.bx(1, 6, 8, 5, 6, 8, we);
    p.put(e, 1, 7, 8);
    p.bx(2, 7, 8, 4, 7, 8, we);
    p.put(w, 5, 7, 8);
    p.put(e, 2, 8, 8);
    p.put(we, 3, 8, 8);
    p.put(w, 4, 8, 8);
    p.d.spawner_at(3, 5, 5);
    p.cols(6, 6);
}

fn room_crossing(p: &mut Paint) {
    p.nb(0, 0, 0, 6, 1, 6);
    p.air(0, 2, 0, 6, 7, 6);
    p.nb(0, 2, 0, 1, 6, 0);
    p.nb(0, 2, 6, 1, 6, 6);
    p.nb(5, 2, 0, 6, 6, 0);
    p.nb(5, 2, 6, 6, 6, 6);
    p.nb(0, 2, 0, 0, 6, 1);
    p.nb(0, 2, 5, 0, 6, 6);
    p.nb(6, 2, 0, 6, 6, 1);
    p.nb(6, 2, 5, 6, 6, 6);
    let we = p.fence(W | E);
    let ns = p.fence(N | S);
    p.nb(2, 6, 0, 4, 6, 0);
    p.bx(2, 5, 0, 4, 5, 0, we);
    p.nb(2, 6, 6, 4, 6, 6);
    p.bx(2, 5, 6, 4, 5, 6, we);
    p.nb(0, 6, 2, 0, 6, 4);
    p.bx(0, 5, 2, 0, 5, 4, ns);
    p.nb(6, 6, 2, 6, 6, 4);
    p.bx(6, 5, 2, 6, 5, 4, ns);
    p.cols(6, 6);
}

fn stairs_room(p: &mut Paint) {
    p.nb(0, 0, 0, 6, 1, 6);
    p.air(0, 2, 0, 6, 10, 6);
    p.nb(0, 2, 0, 1, 8, 0);
    p.nb(5, 2, 0, 6, 8, 0);
    p.nb(0, 2, 1, 0, 8, 6);
    p.nb(6, 2, 1, 6, 8, 6);
    p.nb(1, 2, 6, 5, 8, 6);
    let we = p.fence(W | E);
    let ns = p.fence(N | S);
    p.bx(0, 3, 2, 0, 5, 4, ns);
    p.bx(6, 3, 2, 6, 5, 2, ns);
    p.bx(6, 3, 4, 6, 5, 4, ns);
    let bricks = p.bricks;
    p.put(bricks, 5, 2, 5);
    p.nb(4, 2, 5, 4, 3, 5);
    p.nb(3, 2, 5, 3, 4, 5);
    p.nb(2, 2, 5, 2, 5, 5);
    p.nb(1, 2, 5, 1, 6, 5);
    p.nb(1, 7, 1, 5, 7, 4);
    p.air(6, 8, 2, 6, 8, 4);
    p.nb(2, 6, 0, 4, 8, 0);
    p.bx(2, 5, 0, 4, 5, 0, we);
    p.cols(6, 6);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fortress_tree_is_stable() {
        let pieces = build(12345, 10, 10);
        assert!(pieces.len() > 1);
        let lo = pieces.iter().map(|p| p.min[1]).min().unwrap();
        assert!((48..=70).contains(&lo));
        let again = build(12345, 10, 10);
        assert_eq!(pieces.len(), again.len());
    }
}
