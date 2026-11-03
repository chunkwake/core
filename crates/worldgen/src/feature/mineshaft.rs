//! Заброшенные шахты: `MineshaftStructure` и `MineshaftPieces`.
//! Вагонетка и пещерный паук не пишутся, их броски генератора остаются.

use super::structure::Columns;
use super::template::{Bounds, Rot, rotate_state};
use super::{Bits, Fluid, Heightmap, Legacy, Level, Palette, Worldgen};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Dir {
    North,
    South,
    West,
    East,
}

#[derive(Clone)]
pub struct Cfg {
    pub mesa: bool,
    pub blocking: Arc<HashSet<u32>>,
}

enum Body {
    Room,
    Cross {
        dir: Dir,
        two: bool,
    },
    Stairs,
    Corridor {
        rails: bool,
        spider: bool,
        sections: i32,
    },
}

struct Draft {
    min: [i32; 3],
    max: [i32; 3],
    facing: Option<Dir>,
    depth: i32,
    body: Body,
}

struct Tree {
    pieces: Vec<Draft>,
    entrances: Vec<[i32; 6]>,
    room_x: i32,
    room_z: i32,
}

enum PlaceKind {
    Room {
        entrances: Vec<[i32; 6]>,
    },
    Cross {
        two: bool,
    },
    Stairs,
    Corridor {
        rails: bool,
        spider: bool,
        sections: i32,
    },
}

pub struct Piece {
    min: [i32; 3],
    max: [i32; 3],
    facing: Option<Dir>,
    kind: PlaceKind,
    mesa: bool,
    blocking: Arc<HashSet<u32>>,
    spider: Mutex<bool>,
}

struct Mats {
    wood: u32,
    planks: u32,
    fence: u32,
    fence_west: u32,
    fence_east: u32,
    cave_air: u32,
    cobweb: u32,
    rail_ns: u32,
    rail_ew: u32,
    torch_south: u32,
    torch_north: u32,
    chain: u32,
    spawner: u32,
    wood_id: u32,
    plank_id: u32,
    fence_id: u32,
    chain_id: u32,
    lichen_id: u32,
    sea_id: u32,
    tall_sea_id: u32,
    falling: HashSet<u32>,
    wood_fences: HashSet<u32>,
}

pub fn build(
    cfg: &Cfg,
    sx: i32,
    sz: i32,
    rng: &mut Legacy,
    columns: &dyn Columns,
    biomes: &HashSet<u32>,
) -> Option<Vec<Piece>> {
    rng.next_double();
    let west = sx * 16 + 2;
    let north = sz * 16 + 2;
    let room = Draft {
        min: [west, 50, north],
        max: [
            west + 7 + rng.next_int_bound(6),
            54 + rng.next_int_bound(6),
            north + 7 + rng.next_int_bound(6),
        ],
        facing: None,
        depth: 0,
        body: Body::Room,
    };
    let mut tree = Tree {
        room_x: room.min[0],
        room_z: room.min[2],
        pieces: vec![room],
        entrances: Vec::new(),
    };
    for side in [Dir::North, Dir::South, Dir::West, Dir::East] {
        open_side(&mut tree, rng, side);
    }
    let (umin, umax) = union(&tree.pieces);
    let dy = if cfg.mesa {
        let center = [
            umin[0] + (umax[0] - umin[0] + 1) / 2,
            umin[1] + (umax[1] - umin[1] + 1) / 2,
            umin[2] + (umax[2] - umin[2] + 1) / 2,
        ];
        let surface = columns.base_height(center[0], center[2]);
        let sea = columns.sea_level();
        let target = if surface <= sea {
            sea
        } else {
            rng.between_inclusive(sea, surface)
        };
        target - center[1]
    } else {
        let sea = columns.sea_level();
        let limit = sea - 10;
        let y_span = umax[1] - umin[1] + 1;
        let mut y1 = y_span + columns.min_y() + 1;
        if y1 < limit {
            y1 += rng.next_int_bound(limit - y1);
        }
        y1 - umax[1]
    };
    for piece in &mut tree.pieces {
        piece.min[1] += dy;
        piece.max[1] += dy;
    }
    for entrance in &mut tree.entrances {
        entrance[1] += dy;
        entrance[4] += dy;
    }
    let biome = columns.biome_at(sx * 16 + 8, 50 + dy, sz * 16)?;
    if !biomes.contains(&biome) {
        return None;
    }
    let mut entrances = tree.entrances;
    let mut out = Vec::with_capacity(tree.pieces.len());
    for (i, draft) in tree.pieces.into_iter().enumerate() {
        let kind = match draft.body {
            Body::Room => PlaceKind::Room {
                entrances: if i == 0 {
                    std::mem::take(&mut entrances)
                } else {
                    Vec::new()
                },
            },
            Body::Cross { two, .. } => PlaceKind::Cross { two },
            Body::Stairs => PlaceKind::Stairs,
            Body::Corridor {
                rails,
                spider,
                sections,
                ..
            } => PlaceKind::Corridor {
                rails,
                spider,
                sections,
            },
        };
        out.push(Piece {
            min: draft.min,
            max: draft.max,
            facing: draft.facing,
            kind,
            mesa: cfg.mesa,
            blocking: Arc::clone(&cfg.blocking),
            spider: Mutex::new(false),
        });
    }
    Some(out)
}

impl Piece {
    pub fn bounds(&self) -> ([i32; 3], [i32; 3]) {
        (self.min, self.max)
    }

    pub fn place(&self, pal: &Palette, level: &mut dyn Level, r: &mut Worldgen, window: Bounds) {
        if self.invalid(pal, level, window) {
            return;
        }
        let mats = Mats::of(pal, self.mesa);
        match &self.kind {
            PlaceKind::Room { entrances } => self.room(pal, level, window, &mats, entrances),
            PlaceKind::Cross { two } => self.cross(pal, level, window, &mats, *two),
            PlaceKind::Stairs => self.stairs(pal, level, window, &mats),
            PlaceKind::Corridor {
                rails,
                spider,
                sections,
            } => self.corridor(pal, level, r, window, &mats, *rails, *spider, *sections),
        }
    }

    fn invalid(&self, pal: &Palette, level: &dyn Level, window: Bounds) -> bool {
        let x0 = (self.min[0] - 1).max(window.min[0]);
        let y0 = (self.min[1] - 1).max(window.min[1]);
        let z0 = (self.min[2] - 1).max(window.min[2]);
        let x1 = (self.max[0] + 1).min(window.max[0]);
        let y1 = (self.max[1] + 1).min(window.max[1]);
        let z1 = (self.max[2] + 1).min(window.max[2]);
        if x0 > x1 || y0 > y1 || z0 > z1 {
            return false;
        }
        let center = [(x0 + x1) / 2, (y0 + y1) / 2, (z0 + z1) / 2];
        if self
            .blocking
            .contains(&level.biome(center[0], center[1], center[2]))
        {
            return true;
        }
        for x in x0..=x1 {
            for z in z0..=z1 {
                if liquid(pal, level.get(x, y0, z)) || liquid(pal, level.get(x, y1, z)) {
                    return true;
                }
            }
        }
        for x in x0..=x1 {
            for y in y0..=y1 {
                if liquid(pal, level.get(x, y, z0)) || liquid(pal, level.get(x, y, z1)) {
                    return true;
                }
            }
        }
        for z in z0..=z1 {
            for y in y0..=y1 {
                if liquid(pal, level.get(x0, y, z)) || liquid(pal, level.get(x1, y, z)) {
                    return true;
                }
            }
        }
        false
    }

    fn room(
        &self,
        pal: &Palette,
        level: &mut dyn Level,
        window: Bounds,
        mats: &Mats,
        entrances: &[[i32; 6]],
    ) {
        let top = (self.min[1] + 3).min(self.max[1]);
        self.fill(
            pal,
            level,
            window,
            mats,
            self.min[0],
            self.min[1] + 1,
            self.min[2],
            self.max[0],
            top,
            self.max[2],
            mats.cave_air,
            mats.cave_air,
        );
        for e in entrances {
            self.fill(
                pal,
                level,
                window,
                mats,
                e[0],
                e[4] - 2,
                e[2],
                e[3],
                e[4],
                e[5],
                mats.cave_air,
                mats.cave_air,
            );
        }
        self.sphere(
            pal,
            level,
            window,
            mats,
            self.min[0],
            self.min[1] + 4,
            self.min[2],
            self.max[0],
            self.max[1],
            self.max[2],
        );
    }

    fn cross(&self, pal: &Palette, level: &mut dyn Level, window: Bounds, mats: &Mats, two: bool) {
        let air = mats.cave_air;
        if two {
            self.fill(
                pal,
                level,
                window,
                mats,
                self.min[0] + 1,
                self.min[1],
                self.min[2],
                self.max[0] - 1,
                self.min[1] + 2,
                self.max[2],
                air,
                air,
            );
            self.fill(
                pal,
                level,
                window,
                mats,
                self.min[0],
                self.min[1],
                self.min[2] + 1,
                self.max[0],
                self.min[1] + 2,
                self.max[2] - 1,
                air,
                air,
            );
            self.fill(
                pal,
                level,
                window,
                mats,
                self.min[0] + 1,
                self.max[1] - 2,
                self.min[2],
                self.max[0] - 1,
                self.max[1],
                self.max[2],
                air,
                air,
            );
            self.fill(
                pal,
                level,
                window,
                mats,
                self.min[0],
                self.max[1] - 2,
                self.min[2] + 1,
                self.max[0],
                self.max[1],
                self.max[2] - 1,
                air,
                air,
            );
            self.fill(
                pal,
                level,
                window,
                mats,
                self.min[0] + 1,
                self.min[1] + 3,
                self.min[2] + 1,
                self.max[0] - 1,
                self.min[1] + 3,
                self.max[2] - 1,
                air,
                air,
            );
        } else {
            self.fill(
                pal,
                level,
                window,
                mats,
                self.min[0] + 1,
                self.min[1],
                self.min[2],
                self.max[0] - 1,
                self.max[1],
                self.max[2],
                air,
                air,
            );
            self.fill(
                pal,
                level,
                window,
                mats,
                self.min[0],
                self.min[1],
                self.min[2] + 1,
                self.max[0],
                self.max[1],
                self.max[2] - 1,
                air,
                air,
            );
        }
        for (x, z) in [
            (self.min[0] + 1, self.min[2] + 1),
            (self.min[0] + 1, self.max[2] - 1),
            (self.max[0] - 1, self.min[2] + 1),
            (self.max[0] - 1, self.max[2] - 1),
        ] {
            if !self.air_at(pal, level, window, x, self.max[1] + 1, z) {
                self.fill(
                    pal,
                    level,
                    window,
                    mats,
                    x,
                    self.min[1],
                    z,
                    x,
                    self.max[1],
                    z,
                    mats.planks,
                    air,
                );
            }
        }
        let y = self.min[1] - 1;
        for x in self.min[0]..=self.max[0] {
            for z in self.min[2]..=self.max[2] {
                self.plank_floor(pal, level, window, mats, x, y, z);
            }
        }
    }

    fn stairs(&self, pal: &Palette, level: &mut dyn Level, window: Bounds, mats: &Mats) {
        let air = mats.cave_air;
        self.fill(pal, level, window, mats, 0, 5, 0, 2, 7, 1, air, air);
        self.fill(pal, level, window, mats, 0, 0, 7, 2, 2, 8, air, air);
        for i in 0..5 {
            let y0 = 5 - i - if i < 4 { 1 } else { 0 };
            self.fill(
                pal,
                level,
                window,
                mats,
                0,
                y0,
                2 + i,
                2,
                7 - i,
                2 + i,
                air,
                air,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn corridor(
        &self,
        pal: &Palette,
        level: &mut dyn Level,
        r: &mut Worldgen,
        window: Bounds,
        mats: &Mats,
        rails: bool,
        spider: bool,
        sections: i32,
    ) {
        let length = sections * 5 - 1;
        let air = mats.cave_air;
        self.fill(pal, level, window, mats, 0, 0, 0, 2, 1, length, air, air);
        self.maybe_fill(
            pal, level, r, window, mats, 0.8, 0, 2, 0, 2, 2, length, air, air, false,
        );
        if spider {
            self.maybe_fill(
                pal,
                level,
                r,
                window,
                mats,
                0.6,
                0,
                0,
                0,
                2,
                1,
                length,
                mats.cobweb,
                air,
                true,
            );
        }
        for section in 0..sections {
            let z = 2 + section * 5;
            self.support(pal, level, r, window, mats, 0, 0, z, 2, 2);
            for (prob, dz) in [(0.1f32, -1), (0.1, 1), (0.05, -2), (0.05, 2)] {
                self.cobweb(pal, level, r, window, mats, prob, 0, 2, z + dz);
                self.cobweb(pal, level, r, window, mats, prob, 2, 2, z + dz);
            }
            if r.next_int_bound(100) == 0 {
                self.chest(pal, level, r, window, mats, 2, 0, z - 1);
            }
            if r.next_int_bound(100) == 0 {
                self.chest(pal, level, r, window, mats, 0, 0, z + 1);
            }
            if spider {
                let mut placed = self.spider.lock().unwrap();
                if !*placed {
                    let at = z - 1 + r.next_int_bound(3);
                    let pos = self.world(1, 0, at);
                    if window.contains(pos) && self.interior(level, window, 1, 0, at) {
                        *placed = true;
                        drop(placed);
                        self.put(level, pos[0], pos[1], pos[2], mats.spawner);
                    }
                }
            }
        }
        for x in 0..=2 {
            for z in 0..=length {
                self.plank_floor(pal, level, window, mats, x, -1, z);
            }
        }
        self.hang(pal, level, window, mats, 0, -1, 2);
        if sections > 1 {
            self.hang(pal, level, window, mats, 0, -1, length - 2);
        }
        if rails {
            for z in 0..=length {
                let Some(floor) = self.block_at(level, window, 1, -1, z) else {
                    continue;
                };
                if pal.is_air(floor) || !pal.solid.get(floor as usize).copied().unwrap_or(false) {
                    continue;
                }
                let probability = if self.interior(level, window, 1, 0, z) {
                    0.7
                } else {
                    0.9
                };
                self.maybe_block(
                    pal,
                    level,
                    r,
                    window,
                    mats,
                    probability,
                    1,
                    0,
                    z,
                    mats.rail_ns,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn support(
        &self,
        pal: &Palette,
        level: &mut dyn Level,
        r: &mut Worldgen,
        window: Bounds,
        mats: &Mats,
        x0: i32,
        y0: i32,
        z: i32,
        y1: i32,
        x1: i32,
    ) {
        for x in x0..=x1 {
            if self.air_at(pal, level, window, x, y1 + 1, z) {
                return;
            }
        }
        self.fill(
            pal,
            level,
            window,
            mats,
            x0,
            y0,
            z,
            x0,
            y1 - 1,
            z,
            mats.fence_west,
            mats.cave_air,
        );
        self.fill(
            pal,
            level,
            window,
            mats,
            x1,
            y0,
            z,
            x1,
            y1 - 1,
            z,
            mats.fence_east,
            mats.cave_air,
        );
        if r.next_int_bound(4) == 0 {
            self.fill(
                pal,
                level,
                window,
                mats,
                x0,
                y1,
                z,
                x0,
                y1,
                z,
                mats.planks,
                mats.cave_air,
            );
            self.fill(
                pal,
                level,
                window,
                mats,
                x1,
                y1,
                z,
                x1,
                y1,
                z,
                mats.planks,
                mats.cave_air,
            );
        } else {
            self.fill(
                pal,
                level,
                window,
                mats,
                x0,
                y1,
                z,
                x1,
                y1,
                z,
                mats.planks,
                mats.cave_air,
            );
            self.maybe_block(
                pal,
                level,
                r,
                window,
                mats,
                0.05,
                x0 + 1,
                y1,
                z - 1,
                mats.torch_south,
            );
            self.maybe_block(
                pal,
                level,
                r,
                window,
                mats,
                0.05,
                x0 + 1,
                y1,
                z + 1,
                mats.torch_north,
            );
        }
        if y0 < y1 {
            let mut posts = Vec::new();
            for y in y0..=(y1 - 1) {
                posts.push(self.world(x0, y, z));
                posts.push(self.world(x1, y, z));
            }
            connect_fences(pal, level, window, mats, &posts);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn cobweb(
        &self,
        pal: &Palette,
        level: &mut dyn Level,
        r: &mut Worldgen,
        window: Bounds,
        mats: &Mats,
        probability: f32,
        x: i32,
        y: i32,
        z: i32,
    ) {
        if !self.interior(level, window, x, y, z) {
            return;
        }
        if r.next_float() >= probability || !self.sturdy_neighbours(pal, level, window, x, y, z) {
            return;
        }
        self.paint(pal, level, window, mats, x, y, z, mats.cobweb);
    }

    #[allow(clippy::too_many_arguments)]
    fn chest(
        &self,
        pal: &Palette,
        level: &mut dyn Level,
        r: &mut Worldgen,
        window: Bounds,
        mats: &Mats,
        x: i32,
        y: i32,
        z: i32,
    ) {
        let pos = self.world(x, y, z);
        if !window.contains(pos) {
            return;
        }
        let below = level.get(pos[0], pos[1] - 1, pos[2]);
        if !pal.is_air(level.get(pos[0], pos[1], pos[2])) || pal.is_air(below) {
            return;
        }
        let rail = if r.next_boolean() {
            mats.rail_ns
        } else {
            mats.rail_ew
        };
        self.paint(pal, level, window, mats, x, y, z, rail);
        // `createMinecart`: вагонетка с сундуком — сущность, а не блок-сущность.
        // Сущности не пишутся, поэтому здесь только расход семени.
        r.next_raw_long();
    }

    #[allow(clippy::too_many_arguments)]
    fn hang(
        &self,
        pal: &Palette,
        level: &mut dyn Level,
        window: Bounds,
        mats: &Mats,
        x: i32,
        y: i32,
        z: i32,
    ) {
        if self.same(pal, self.block_at(level, window, x, y, z), mats.plank_id) {
            self.pillar(pal, level, window, mats, x, y, z);
        }
        if self.same(
            pal,
            self.block_at(level, window, x + 2, y, z),
            mats.plank_id,
        ) {
            self.pillar(pal, level, window, mats, x + 2, y, z);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn pillar(
        &self,
        pal: &Palette,
        level: &mut dyn Level,
        window: Bounds,
        mats: &Mats,
        x: i32,
        y: i32,
        z: i32,
    ) {
        let pos = self.world(x, y, z);
        if !window.contains(pos) {
            return;
        }
        let world_y = pos[1];
        let min_y = level.min_y();
        let max_y = min_y + level.height();
        let mut distance = 1;
        let mut below = true;
        let mut above = true;
        while below || above {
            if below {
                let by = world_y - distance;
                let state = level.get(pos[0], by, pos[2]);
                let empty = replaceable(pal, mats, state) && !pal.fluid(state).is_lava();
                if !empty && pal.sturdy(state, 1) {
                    self.column(level, pos[0], pos[2], by + 1, world_y, mats.wood);
                    return;
                }
                below = distance <= 20 && empty && by > min_y + 1;
            }
            if above {
                let ay = world_y + distance;
                let state = level.get(pos[0], ay, pos[2]);
                let empty = replaceable(pal, mats, state);
                // `canHangChainBelow`: центр низа, а не целая грань — столб
                // забора опоры над доской тоже держит.
                if !empty
                    && pal.supports_center_below(state)
                    && !mats.falling.contains(&block_id(pal, state))
                {
                    let hook = [pos[0], world_y + 1, pos[2]];
                    self.put(level, hook[0], hook[1], hook[2], mats.fence);
                    self.column(level, pos[0], pos[2], world_y + 2, ay, mats.chain);
                    connect_fences(pal, level, window, mats, &[hook]);
                    return;
                }
                above = distance <= 50 && empty && ay < max_y;
            }
            distance += 1;
        }
    }

    fn column(&self, level: &mut dyn Level, x: i32, z: i32, from: i32, to: i32, state: u32) {
        if from >= to {
            return;
        }
        for y in from..to {
            self.put(level, x, y, z, state);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn plank_floor(
        &self,
        pal: &Palette,
        level: &mut dyn Level,
        window: Bounds,
        mats: &Mats,
        x: i32,
        y: i32,
        z: i32,
    ) {
        if !self.interior(level, window, x, y, z) {
            return;
        }
        let pos = self.world(x, y, z);
        let state = level.get(pos[0], pos[1], pos[2]);
        if !pal.sturdy(state, 1) {
            self.put(level, pos[0], pos[1], pos[2], mats.planks);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn fill(
        &self,
        pal: &Palette,
        level: &mut dyn Level,
        window: Bounds,
        mats: &Mats,
        x0: i32,
        y0: i32,
        z0: i32,
        x1: i32,
        y1: i32,
        z1: i32,
        edge: u32,
        fill: u32,
    ) {
        if x0 > x1 || y0 > y1 || z0 > z1 {
            return;
        }
        for y in y0..=y1 {
            for x in x0..=x1 {
                for z in z0..=z1 {
                    let state = if y == y0 || y == y1 || x == x0 || x == x1 || z == z0 || z == z1 {
                        edge
                    } else {
                        fill
                    };
                    self.paint(pal, level, window, mats, x, y, z, state);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn maybe_fill(
        &self,
        pal: &Palette,
        level: &mut dyn Level,
        r: &mut Worldgen,
        window: Bounds,
        mats: &Mats,
        probability: f32,
        x0: i32,
        y0: i32,
        z0: i32,
        x1: i32,
        y1: i32,
        z1: i32,
        edge: u32,
        fill: u32,
        inside: bool,
    ) {
        if x0 > x1 || y0 > y1 || z0 > z1 {
            return;
        }
        for y in y0..=y1 {
            for x in x0..=x1 {
                for z in z0..=z1 {
                    if r.next_float() > probability
                        || inside && !self.interior(level, window, x, y, z)
                    {
                        continue;
                    }
                    let state = if y == y0 || y == y1 || x == x0 || x == x1 || z == z0 || z == z1 {
                        edge
                    } else {
                        fill
                    };
                    self.paint(pal, level, window, mats, x, y, z, state);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn maybe_block(
        &self,
        pal: &Palette,
        level: &mut dyn Level,
        r: &mut Worldgen,
        window: Bounds,
        mats: &Mats,
        probability: f32,
        x: i32,
        y: i32,
        z: i32,
        state: u32,
    ) {
        if r.next_float() < probability {
            self.paint(pal, level, window, mats, x, y, z, state);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint(
        &self,
        pal: &Palette,
        level: &mut dyn Level,
        window: Bounds,
        mats: &Mats,
        x: i32,
        y: i32,
        z: i32,
        state: u32,
    ) {
        let pos = self.world(x, y, z);
        if !window.contains(pos) || self.kept(pal, mats, level.get(pos[0], pos[1], pos[2])) {
            return;
        }
        self.put(
            level,
            pos[0],
            pos[1],
            pos[2],
            orient(pal, state, self.facing),
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn sphere(
        &self,
        pal: &Palette,
        level: &mut dyn Level,
        window: Bounds,
        mats: &Mats,
        x0: i32,
        y0: i32,
        z0: i32,
        x1: i32,
        y1: i32,
        z1: i32,
    ) {
        if x0 > x1 || y0 > y1 || z0 > z1 {
            return;
        }
        let diag_x = (x1 - x0 + 1) as f32;
        let diag_y = (y1 - y0 + 1) as f32;
        let diag_z = (z1 - z0 + 1) as f32;
        let cx = x0 as f32 + diag_x / 2.0;
        let cz = z0 as f32 + diag_z / 2.0;
        for y in y0..=y1 {
            let ny = (y - y0) as f32 / diag_y;
            for x in x0..=x1 {
                let nx = (x as f32 - cx) / (diag_x * 0.5);
                for z in z0..=z1 {
                    let nz = (z as f32 - cz) / (diag_z * 0.5);
                    if nx * nx + ny * ny + nz * nz <= 1.05 {
                        self.paint(pal, level, window, mats, x, y, z, mats.cave_air);
                    }
                }
            }
        }
    }

    fn world(&self, x: i32, y: i32, z: i32) -> [i32; 3] {
        match self.facing {
            None => [x, y, z],
            Some(Dir::North) => [self.min[0] + x, self.min[1] + y, self.max[2] - z],
            Some(Dir::South) => [self.min[0] + x, self.min[1] + y, self.min[2] + z],
            Some(Dir::West) => [self.max[0] - z, self.min[1] + y, self.min[2] + x],
            Some(Dir::East) => [self.min[0] + z, self.min[1] + y, self.min[2] + x],
        }
    }

    fn block_at(&self, level: &dyn Level, window: Bounds, x: i32, y: i32, z: i32) -> Option<u32> {
        let pos = self.world(x, y, z);
        window
            .contains(pos)
            .then(|| level.get(pos[0], pos[1], pos[2]))
    }

    fn air_at(
        &self,
        pal: &Palette,
        level: &dyn Level,
        window: Bounds,
        x: i32,
        y: i32,
        z: i32,
    ) -> bool {
        match self.block_at(level, window, x, y, z) {
            None => true,
            Some(state) => pal.is_air(state),
        }
    }

    fn interior(&self, level: &dyn Level, window: Bounds, x: i32, y: i32, z: i32) -> bool {
        let above = self.world(x, y + 1, z);
        window.contains(above)
            && above[1] < level.surface(Heightmap::OceanFloorWg, above[0], above[2])
    }

    fn sturdy_neighbours(
        &self,
        pal: &Palette,
        level: &dyn Level,
        window: Bounds,
        x: i32,
        y: i32,
        z: i32,
    ) -> bool {
        const STEP: [[i32; 3]; 6] = [
            [0, -1, 0],
            [0, 1, 0],
            [0, 0, -1],
            [0, 0, 1],
            [-1, 0, 0],
            [1, 0, 0],
        ];
        const FACE: [usize; 6] = [1, 0, 3, 2, 5, 4];
        let origin = self.world(x, y, z);
        let mut sturdy = 0;
        for (step, face) in STEP.iter().zip(FACE) {
            let pos = [
                origin[0] + step[0],
                origin[1] + step[1],
                origin[2] + step[2],
            ];
            if window.contains(pos) && pal.sturdy(level.get(pos[0], pos[1], pos[2]), face) {
                sturdy += 1;
                if sturdy >= 2 {
                    return true;
                }
            }
        }
        false
    }

    fn kept(&self, pal: &Palette, mats: &Mats, state: u32) -> bool {
        let id = block_id(pal, state);
        id == mats.wood_id || id == mats.plank_id || id == mats.fence_id || id == mats.chain_id
    }

    fn same(&self, pal: &Palette, state: Option<u32>, block: u32) -> bool {
        state.is_some_and(|s| block_id(pal, s) == block)
    }

    fn put(&self, level: &mut dyn Level, x: i32, y: i32, z: i32, state: u32) {
        if y < level.min_y() || y >= level.min_y() + level.height() || !level.can_write(x, z) {
            return;
        }
        level.set(x, y, z, state);
    }
}

fn open_side(tree: &mut Tree, rng: &mut Legacy, side: Dir) {
    let room = &tree.pieces[0];
    let span = match side {
        Dir::North | Dir::South => room.max[0] - room.min[0] + 1,
        Dir::West | Dir::East => room.max[2] - room.min[2] + 1,
    };
    let y_span = room.max[1] - room.min[1] + 1;
    let height_space = (y_span - 4).max(1);
    let mut pos = 0;
    loop {
        if pos >= span {
            break;
        }
        pos += rng.next_int_bound(span);
        if pos + 3 > span {
            break;
        }
        let (min, max) = (tree.pieces[0].min, tree.pieces[0].max);
        let foot_y = min[1] + rng.next_int_bound(height_space) + 1;
        let foot = match side {
            Dir::North => [min[0] + pos, foot_y, min[2] - 1],
            Dir::South => [min[0] + pos, foot_y, max[2] + 1],
            Dir::West => [min[0] - 1, foot_y, min[2] + pos],
            Dir::East => [max[0] + 1, foot_y, min[2] + pos],
        };
        let before = tree.pieces.len();
        add_piece(tree, rng, foot, side, 0);
        if tree.pieces.len() > before {
            let child = &tree.pieces[before];
            tree.entrances.push(match side {
                Dir::North => [
                    child.min[0],
                    child.min[1],
                    min[2],
                    child.max[0],
                    child.max[1],
                    min[2] + 1,
                ],
                Dir::South => [
                    child.min[0],
                    child.min[1],
                    max[2] - 1,
                    child.max[0],
                    child.max[1],
                    max[2],
                ],
                Dir::West => [
                    min[0],
                    child.min[1],
                    child.min[2],
                    min[0] + 1,
                    child.max[1],
                    child.max[2],
                ],
                Dir::East => [
                    max[0] - 1,
                    child.min[1],
                    child.min[2],
                    max[0],
                    child.max[1],
                    child.max[2],
                ],
            });
        }
        pos += 4;
    }
}

fn add_piece(tree: &mut Tree, rng: &mut Legacy, foot: [i32; 3], dir: Dir, depth: i32) {
    if depth > 8 {
        return;
    }
    if (foot[0] - tree.room_x).abs() > 80 || (foot[2] - tree.room_z).abs() > 80 {
        return;
    }
    let Some(draft) = random_piece(tree, rng, foot, dir, depth + 1) else {
        return;
    };
    tree.pieces.push(draft);
    let idx = tree.pieces.len() - 1;
    extend(tree, rng, idx);
}

fn random_piece(
    tree: &Tree,
    rng: &mut Legacy,
    foot: [i32; 3],
    dir: Dir,
    depth: i32,
) -> Option<Draft> {
    let roll = rng.next_int_bound(100);
    if roll >= 80 {
        crossing(tree, rng, foot, dir, depth)
    } else if roll >= 70 {
        stairs(tree, foot, dir, depth)
    } else {
        corridor(tree, rng, foot, dir, depth)
    }
}

fn crossing(tree: &Tree, rng: &mut Legacy, foot: [i32; 3], dir: Dir, depth: i32) -> Option<Draft> {
    let y1 = if rng.next_int_bound(4) == 0 { 6 } else { 2 };
    let local = match dir {
        Dir::South => [-1, 0, 0, 3, y1, 4],
        Dir::West => [-4, 0, -1, 0, y1, 3],
        Dir::East => [0, 0, -1, 4, y1, 3],
        Dir::North => [-1, 0, -4, 3, y1, 0],
    };
    let box_ = moved(local, foot);
    if collides(tree, box_) {
        return None;
    }
    Some(Draft {
        min: [box_[0], box_[1], box_[2]],
        max: [box_[3], box_[4], box_[5]],
        facing: None,
        depth,
        body: Body::Cross {
            dir,
            two: box_[4] - box_[1] + 1 > 3,
        },
    })
}

fn stairs(tree: &Tree, foot: [i32; 3], dir: Dir, depth: i32) -> Option<Draft> {
    let local = match dir {
        Dir::South => [0, -5, 0, 2, 2, 8],
        Dir::West => [-8, -5, 0, 0, 2, 2],
        Dir::East => [0, -5, 0, 8, 2, 2],
        Dir::North => [0, -5, -8, 2, 2, 0],
    };
    let box_ = moved(local, foot);
    if collides(tree, box_) {
        return None;
    }
    Some(Draft {
        min: [box_[0], box_[1], box_[2]],
        max: [box_[3], box_[4], box_[5]],
        facing: Some(dir),
        depth,
        body: Body::Stairs,
    })
}

fn corridor(tree: &Tree, rng: &mut Legacy, foot: [i32; 3], dir: Dir, depth: i32) -> Option<Draft> {
    let mut length = rng.next_int_bound(3) + 2;
    let box_ = loop {
        if length <= 0 {
            return None;
        }
        let blocks = length * 5;
        let local = match dir {
            Dir::South => [0, 0, 0, 2, 2, blocks - 1],
            Dir::West => [-(blocks - 1), 0, 0, 0, 2, 2],
            Dir::East => [0, 0, 0, blocks - 1, 2, 2],
            Dir::North => [0, 0, -(blocks - 1), 2, 2, 0],
        };
        let box_ = moved(local, foot);
        if !collides(tree, box_) {
            break box_;
        }
        length -= 1;
    };
    let rails = rng.next_int_bound(3) == 0;
    let spider = !rails && rng.next_int_bound(23) == 0;
    let sections = match dir {
        Dir::North | Dir::South => (box_[5] - box_[2] + 1) / 5,
        Dir::West | Dir::East => (box_[3] - box_[0] + 1) / 5,
    };
    Some(Draft {
        min: [box_[0], box_[1], box_[2]],
        max: [box_[3], box_[4], box_[5]],
        facing: Some(dir),
        depth,
        body: Body::Corridor {
            rails,
            spider,
            sections,
        },
    })
}

enum Arm {
    Cross { dir: Dir, two: bool },
    Stairs { dir: Dir },
    Corridor { dir: Dir },
}

fn extend(tree: &mut Tree, rng: &mut Legacy, idx: usize) {
    let depth = tree.pieces[idx].depth;
    let min = tree.pieces[idx].min;
    let max = tree.pieces[idx].max;
    let facing = tree.pieces[idx].facing;
    let arm = match &tree.pieces[idx].body {
        Body::Room => return,
        Body::Cross { dir, two } => Arm::Cross {
            dir: *dir,
            two: *two,
        },
        Body::Stairs => Arm::Stairs {
            dir: facing.unwrap_or(Dir::North),
        },
        Body::Corridor { .. } => Arm::Corridor {
            dir: facing.unwrap_or(Dir::North),
        },
    };
    match arm {
        Arm::Cross { dir, two } => {
            match dir {
                Dir::South => {
                    add_piece(
                        tree,
                        rng,
                        [min[0] + 1, min[1], max[2] + 1],
                        Dir::South,
                        depth,
                    );
                    add_piece(
                        tree,
                        rng,
                        [min[0] - 1, min[1], min[2] + 1],
                        Dir::West,
                        depth,
                    );
                    add_piece(
                        tree,
                        rng,
                        [max[0] + 1, min[1], min[2] + 1],
                        Dir::East,
                        depth,
                    );
                }
                Dir::West => {
                    add_piece(
                        tree,
                        rng,
                        [min[0] + 1, min[1], min[2] - 1],
                        Dir::North,
                        depth,
                    );
                    add_piece(
                        tree,
                        rng,
                        [min[0] + 1, min[1], max[2] + 1],
                        Dir::South,
                        depth,
                    );
                    add_piece(
                        tree,
                        rng,
                        [min[0] - 1, min[1], min[2] + 1],
                        Dir::West,
                        depth,
                    );
                }
                Dir::East => {
                    add_piece(
                        tree,
                        rng,
                        [min[0] + 1, min[1], min[2] - 1],
                        Dir::North,
                        depth,
                    );
                    add_piece(
                        tree,
                        rng,
                        [min[0] + 1, min[1], max[2] + 1],
                        Dir::South,
                        depth,
                    );
                    add_piece(
                        tree,
                        rng,
                        [max[0] + 1, min[1], min[2] + 1],
                        Dir::East,
                        depth,
                    );
                }
                Dir::North => {
                    add_piece(
                        tree,
                        rng,
                        [min[0] + 1, min[1], min[2] - 1],
                        Dir::North,
                        depth,
                    );
                    add_piece(
                        tree,
                        rng,
                        [min[0] - 1, min[1], min[2] + 1],
                        Dir::West,
                        depth,
                    );
                    add_piece(
                        tree,
                        rng,
                        [max[0] + 1, min[1], min[2] + 1],
                        Dir::East,
                        depth,
                    );
                }
            }
            if two {
                if rng.next_bool() {
                    add_piece(
                        tree,
                        rng,
                        [min[0] + 1, min[1] + 4, min[2] - 1],
                        Dir::North,
                        depth,
                    );
                }
                if rng.next_bool() {
                    add_piece(
                        tree,
                        rng,
                        [min[0] - 1, min[1] + 4, min[2] + 1],
                        Dir::West,
                        depth,
                    );
                }
                if rng.next_bool() {
                    add_piece(
                        tree,
                        rng,
                        [max[0] + 1, min[1] + 4, min[2] + 1],
                        Dir::East,
                        depth,
                    );
                }
                if rng.next_bool() {
                    add_piece(
                        tree,
                        rng,
                        [min[0] + 1, min[1] + 4, max[2] + 1],
                        Dir::South,
                        depth,
                    );
                }
            }
        }
        Arm::Stairs { dir } => {
            let foot = match dir {
                Dir::South => [min[0], min[1], max[2] + 1],
                Dir::West => [min[0] - 1, min[1], min[2]],
                Dir::East => [max[0] + 1, min[1], min[2]],
                Dir::North => [min[0], min[1], min[2] - 1],
            };
            add_piece(tree, rng, foot, dir, depth);
        }
        Arm::Corridor { dir } => {
            let end = rng.next_int_bound(4);
            let y = min[1] - 1 + rng.next_int_bound(3);
            match dir {
                Dir::South => {
                    if end <= 1 {
                        add_piece(tree, rng, [min[0], y, max[2] + 1], dir, depth);
                    } else if end == 2 {
                        add_piece(tree, rng, [min[0] - 1, y, max[2] - 3], Dir::West, depth);
                    } else {
                        add_piece(tree, rng, [max[0] + 1, y, max[2] - 3], Dir::East, depth);
                    }
                }
                Dir::West => {
                    if end <= 1 {
                        add_piece(tree, rng, [min[0] - 1, y, min[2]], dir, depth);
                    } else if end == 2 {
                        add_piece(tree, rng, [min[0], y, min[2] - 1], Dir::North, depth);
                    } else {
                        add_piece(tree, rng, [min[0], y, max[2] + 1], Dir::South, depth);
                    }
                }
                Dir::East => {
                    if end <= 1 {
                        add_piece(tree, rng, [max[0] + 1, y, min[2]], dir, depth);
                    } else if end == 2 {
                        add_piece(tree, rng, [max[0] - 3, y, min[2] - 1], Dir::North, depth);
                    } else {
                        add_piece(tree, rng, [max[0] - 3, y, max[2] + 1], Dir::South, depth);
                    }
                }
                Dir::North => {
                    if end <= 1 {
                        add_piece(tree, rng, [min[0], y, min[2] - 1], dir, depth);
                    } else if end == 2 {
                        add_piece(tree, rng, [min[0] - 1, y, min[2]], Dir::West, depth);
                    } else {
                        add_piece(tree, rng, [max[0] + 1, y, min[2]], Dir::East, depth);
                    }
                }
            }
            if depth >= 8 {
                return;
            }
            if matches!(dir, Dir::North | Dir::South) {
                let mut z = min[2] + 3;
                while z + 3 <= max[2] {
                    let pick = rng.next_int_bound(5);
                    if pick == 0 {
                        add_piece(tree, rng, [min[0] - 1, min[1], z], Dir::West, depth + 1);
                    } else if pick == 1 {
                        add_piece(tree, rng, [max[0] + 1, min[1], z], Dir::East, depth + 1);
                    }
                    z += 5;
                }
            } else {
                let mut x = min[0] + 3;
                while x + 3 <= max[0] {
                    let pick = rng.next_int_bound(5);
                    if pick == 0 {
                        add_piece(tree, rng, [x, min[1], min[2] - 1], Dir::North, depth + 1);
                    } else if pick == 1 {
                        add_piece(tree, rng, [x, min[1], max[2] + 1], Dir::South, depth + 1);
                    }
                    x += 5;
                }
            }
        }
    }
}

fn collides(tree: &Tree, box_: [i32; 6]) -> bool {
    tree.pieces.iter().any(|piece| {
        piece.max[0] >= box_[0]
            && piece.min[0] <= box_[3]
            && piece.max[1] >= box_[1]
            && piece.min[1] <= box_[4]
            && piece.max[2] >= box_[2]
            && piece.min[2] <= box_[5]
    })
}

fn moved(local: [i32; 6], foot: [i32; 3]) -> [i32; 6] {
    [
        local[0] + foot[0],
        local[1] + foot[1],
        local[2] + foot[2],
        local[3] + foot[0],
        local[4] + foot[1],
        local[5] + foot[2],
    ]
}

fn union(pieces: &[Draft]) -> ([i32; 3], [i32; 3]) {
    let mut min = [i32::MAX; 3];
    let mut max = [i32::MIN; 3];
    for piece in pieces {
        for axis in 0..3 {
            min[axis] = min[axis].min(piece.min[axis]);
            max[axis] = max[axis].max(piece.max[axis]);
        }
    }
    (min, max)
}

impl Mats {
    fn of(pal: &Palette, mesa: bool) -> Self {
        let (wood_name, plank_name, fence_name) = if mesa {
            (
                "minecraft:dark_oak_log",
                "minecraft:dark_oak_planks",
                "minecraft:dark_oak_fence",
            )
        } else {
            (
                "minecraft:oak_log",
                "minecraft:oak_planks",
                "minecraft:oak_fence",
            )
        };
        let wood = def(pal, wood_name);
        let planks = def(pal, plank_name);
        let fence = def(pal, fence_name);
        let rail = def(pal, "minecraft:rail");
        let torch = def(pal, "minecraft:wall_torch");
        let chain = def(pal, "minecraft:iron_chain");
        Self {
            wood,
            planks,
            fence,
            fence_west: with(pal, fence, "west", "true"),
            fence_east: with(pal, fence, "east", "true"),
            cave_air: def(pal, "minecraft:cave_air"),
            cobweb: def(pal, "minecraft:cobweb"),
            rail_ns: with(pal, rail, "shape", "north_south"),
            rail_ew: with(pal, rail, "shape", "east_west"),
            torch_south: with(pal, torch, "facing", "south"),
            torch_north: with(pal, torch, "facing", "north"),
            chain,
            spawner: def(pal, "minecraft:spawner"),
            wood_id: block_id(pal, wood),
            plank_id: block_id(pal, planks),
            fence_id: block_id(pal, fence),
            chain_id: block_id(pal, chain),
            lichen_id: block_id(pal, def(pal, "minecraft:glow_lichen")),
            sea_id: block_id(pal, def(pal, "minecraft:seagrass")),
            tall_sea_id: block_id(pal, def(pal, "minecraft:tall_seagrass")),
            falling: falling_ids(pal),
            wood_fences: wood_fence_ids(pal),
        }
    }
}

/// `FenceBlock.updateShape` только у поставленных клеток: сторона от деревянного
/// забора или прочной грани соседа.
fn connect_fences(
    pal: &Palette,
    level: &mut dyn Level,
    window: Bounds,
    mats: &Mats,
    cells: &[[i32; 3]],
) {
    let sides = [
        ("north", [0, 0, -1], 3usize),
        ("south", [0, 0, 1], 2),
        ("west", [-1, 0, 0], 5),
        ("east", [1, 0, 0], 4),
    ];
    for pos in cells {
        if !window.contains(*pos) {
            continue;
        }
        let current = level.get(pos[0], pos[1], pos[2]);
        if !mats.wood_fences.contains(&block_id(pal, current)) {
            continue;
        }
        level.mark_shape(pos[0], pos[1], pos[2]);
        let mut state = current;
        for (prop, delta, face) in sides {
            let neighbor = level.get(pos[0] + delta[0], pos[1] + delta[1], pos[2] + delta[2]);
            let connect =
                mats.wood_fences.contains(&block_id(pal, neighbor)) || pal.sturdy(neighbor, face);
            let value = if connect { "true" } else { "false" };
            if let Some(next) = pal.with_prop(state, prop, value) {
                state = next;
            }
        }
        if state != current {
            level.set(pos[0], pos[1], pos[2], state);
        }
    }
}

/// Пересчёт сторон после декорации соседей. `get` даёт `None`, если чанк
/// соседа уже отдан: эту сторону не трогаем, её посчитали, пока он ещё был
/// в области.
pub(crate) fn fence_updates(
    pal: &Palette,
    posts: &[[i32; 3]],
    get: impl Fn(i32, i32, i32) -> Option<u32>,
) -> Vec<([i32; 3], u32)> {
    let woods = wood_fence_ids(pal);
    let bars = block_ids(pal, "minecraft:iron_bars");
    // `BaseTorchBlock.updateShape`: стоящий факел падает без опоры снизу.
    let floor_torches = block_ids(pal, "minecraft:torch");
    let wall_torches = block_ids(pal, "minecraft:wall_torch");
    let ladders = block_ids(pal, "minecraft:ladder");
    let sides = [
        ("north", [0, 0, -1], 3usize),
        ("south", [0, 0, 1], 2),
        ("west", [-1, 0, 0], 5),
        ("east", [1, 0, 0], 4),
    ];
    // Опора настенного факела и лестницы: клетка `pos.relative(facing
    // .getOpposite())`, прочна её грань, смотрящая в сторону `facing`.
    let support = [
        ("north", [0, 0, 1], 2usize),
        ("south", [0, 0, -1], 3),
        ("west", [1, 0, 0], 4),
        ("east", [-1, 0, 0], 5),
    ];
    let mut out = Vec::new();
    for pos in posts {
        let Some(current) = get(pos[0], pos[1], pos[2]) else {
            continue;
        };
        let id = block_id(pal, current);
        if floor_torches.contains(&id) {
            let below = get(pos[0], pos[1] - 1, pos[2]);
            if below.is_some_and(|b| !pal.sturdy(b, 1)) {
                out.push((*pos, pal.defaults["minecraft:air"]));
            }
            continue;
        }
        if wall_torches.contains(&id) || ladders.contains(&id) {
            // `WallTorchBlock.canSurvive` и `LadderBlock.canSurvive`: опора в
            // клетке `pos.relative(facing.getOpposite())`, и прочна её грань,
            // смотрящая в сторону `facing`.
            let Some(facing) = pal.prop(current, "facing") else {
                continue;
            };
            let found = support
                .iter()
                .find(|(name, _, _)| *name == facing)
                .map(|(_, d, f)| (*d, *f));
            let Some((delta, face)) = found else {
                continue;
            };
            let Some(support) = get(pos[0] + delta[0], pos[1] + delta[1], pos[2] + delta[2]) else {
                continue;
            };
            if !pal.sturdy(support, face) {
                out.push((*pos, pal.defaults["minecraft:air"]));
            }
            continue;
        }
        let is_fence = woods.contains(&id);
        let is_bars = bars.contains(&id);
        if !is_fence && !is_bars {
            continue;
        }
        let mut state = current;
        for (prop, delta, face) in sides {
            let Some(neighbor) = get(pos[0] + delta[0], pos[1] + delta[1], pos[2] + delta[2])
            else {
                continue;
            };
            let nid = block_id(pal, neighbor);
            // `FenceBlock.connectsTo` и `IronBarsBlock.attachsTo`.
            let connect = if is_bars {
                bars.contains(&nid) || pal.sturdy(neighbor, face)
            } else {
                woods.contains(&nid) || pal.sturdy(neighbor, face)
            };
            let value = if connect { "true" } else { "false" };
            if let Some(next) = pal.with_prop(state, prop, value) {
                state = next;
            }
        }
        if state != current {
            out.push((*pos, state));
        }
    }
    out
}

/// `WallBlock.sideUpdate`: стороны получают `none`/`low`/`tall`, пост `up` —
/// `shouldRaisePost`. Формы аппроксимируются полным кубом сверху.
pub(crate) fn wall_updates(
    pal: &Palette,
    posts: &[[i32; 3]],
    get: impl Fn(i32, i32, i32) -> Option<u32>,
) -> Vec<([i32; 3], u32)> {
    let walls = wall_ids(pal);
    let sides = [
        ("north", [0, 0, -1], 3usize),
        ("south", [0, 0, 1], 2),
        ("west", [-1, 0, 0], 5),
        ("east", [1, 0, 0], 4),
    ];
    let mut out = Vec::new();
    for pos in posts {
        let Some(current) = get(pos[0], pos[1], pos[2]) else {
            continue;
        };
        if !walls.contains(&block_id(pal, current)) {
            continue;
        }
        // `aboveShape` — нижняя грань блока сверху. Тестовая грань стены — узкая
        // полоса 2×9 в сторону `dir`, её закрывает полный куб или рука стены
        // сверху в ту же сторону. Пост 8×8 полосу не накрывает.
        let above = get(pos[0], pos[1] + 1, pos[2]);
        let above_id = above.map(|a| block_id(pal, a));
        let above_full = above.is_some_and(|a| pal.is_full_cube(a));
        let mut state = current;
        for (prop, delta, face) in sides {
            let Some(neighbor) = get(pos[0] + delta[0], pos[1] + delta[1], pos[2] + delta[2])
            else {
                continue;
            };
            let connects = walls.contains(&block_id(pal, neighbor)) || pal.sturdy(neighbor, face);
            let arm = above_id.is_some_and(|id| walls.contains(&id))
                && above.is_some_and(|a| pal.prop(a, prop) != Some("none"));
            let value = if !connects {
                "none"
            } else if above_full || arm {
                "tall"
            } else {
                "low"
            };
            if let Some(next) = pal.with_prop(state, prop, value) {
                state = next;
            }
        }
        // `shouldRaisePost`: сосед сверху с постом → пост; угол → пост;
        // высокая пара по одной оси → без поста; иначе навес закрывает.
        let above_post = above.is_some_and(|a| {
            walls.contains(&block_id(pal, a)) && pal.prop(a, "up") == Some("true")
        });
        let side = |prop: &str| pal.prop(state, prop).unwrap_or("none");
        let (n, e, s, w) = (side("north"), side("east"), side("south"), side("west"));
        let corner = (n == "none" && s == "none" && w == "none" && e == "none")
            || ((n == "none") != (s == "none"))
            || ((w == "none") != (e == "none"));
        let high = (n == "tall" && s == "tall") || (e == "tall" && w == "tall");
        let up = if above_post || corner {
            true
        } else if high {
            false
        } else {
            above_full || above_post
        };
        if let Some(next) = pal.with_prop(state, "up", if up { "true" } else { "false" }) {
            state = next;
        }
        if state != current {
            out.push((*pos, state));
        }
    }
    out
}

/// `shouldRaisePost`: пост есть у угла, у стены с высокой парой и под навесом.
fn wall_ids(pal: &Palette) -> HashSet<u32> {
    pal.blocks
        .iter()
        .filter(|(name, _)| name.ends_with("_wall"))
        .map(|(_, (id, _))| *id)
        .collect()
}

fn block_ids(pal: &Palette, name: &str) -> HashSet<u32> {
    pal.blocks
        .get(name)
        .map(|(id, _)| std::iter::once(*id).collect())
        .unwrap_or_default()
}

fn wood_fence_ids(pal: &Palette) -> HashSet<u32> {
    pal.blocks
        .iter()
        .filter(|(name, _)| name.ends_with("_fence") && !name.contains("nether_brick"))
        .map(|(_, (id, _))| *id)
        .collect()
}

fn air_block(pal: &Palette, state: u32) -> bool {
    pal.is_air(state)
}

fn liquid(pal: &Palette, state: u32) -> bool {
    pal.fluid(state) != Fluid::None
}

fn replaceable(pal: &Palette, mats: &Mats, state: u32) -> bool {
    let id = block_id(pal, state);
    air_block(pal, state)
        || liquid(pal, state)
        || id == mats.lichen_id
        || id == mats.sea_id
        || id == mats.tall_sea_id
}

fn falling_ids(pal: &Palette) -> HashSet<u32> {
    pal.blocks
        .iter()
        .filter(|(name, _)| falling_name(name))
        .map(|(_, (id, _))| *id)
        .collect()
}

fn falling_name(name: &str) -> bool {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    matches!(
        name,
        "sand"
            | "red_sand"
            | "gravel"
            | "suspicious_sand"
            | "suspicious_gravel"
            | "dragon_egg"
            | "scaffolding"
            | "anvil"
            | "chipped_anvil"
            | "damaged_anvil"
    ) || name.ends_with("_concrete_powder")
}

fn orient(pal: &Palette, state: u32, facing: Option<Dir>) -> u32 {
    let Some(dir) = facing else {
        return state;
    };
    let (mirror, rot) = match dir {
        Dir::North => (false, Rot::None),
        Dir::South => (true, Rot::None),
        Dir::West => (true, Rot::Cw90),
        Dir::East => (false, Rot::Cw90),
    };
    let state = if mirror {
        mirror_left_right(pal, state)
    } else {
        state
    };
    let state = rotate_state(pal, state, rot);
    rotate_rail(pal, state, rot)
}

fn mirror_left_right(pal: &Palette, state: u32) -> u32 {
    let mut state = state;
    for key in ["facing", "horizontal_facing"] {
        let Some(face) = pal.prop(state, key).map(str::to_owned) else {
            continue;
        };
        let flipped = match face.as_str() {
            "north" => "south",
            "south" => "north",
            _ => continue,
        };
        if let Some(next) = pal.with_prop(state, key, flipped) {
            state = next;
        }
    }
    if let Some(shape) = pal.prop(state, "shape").map(str::to_owned) {
        let flipped = match shape.as_str() {
            "ascending_north" => "ascending_south",
            "ascending_south" => "ascending_north",
            "south_east" => "north_east",
            "south_west" => "north_west",
            "north_west" => "south_west",
            "north_east" => "south_east",
            _ => "",
        };
        if !flipped.is_empty()
            && let Some(next) = pal.with_prop(state, "shape", flipped)
        {
            state = next;
        }
    }
    let north = pal.prop(state, "north").map(str::to_owned);
    let south = pal.prop(state, "south").map(str::to_owned);
    if let (Some(north), Some(south)) = (north, south)
        && pal.prop(state, "east").is_some()
        && pal.prop(state, "west").is_some()
    {
        if let Some(next) = pal.with_prop(state, "north", &south) {
            state = next;
        }
        if let Some(next) = pal.with_prop(state, "south", &north) {
            state = next;
        }
    }
    state
}

fn rotate_rail(pal: &Palette, state: u32, rot: Rot) -> u32 {
    if rot == Rot::None {
        return state;
    }
    let Some(shape) = pal.prop(state, "shape").map(str::to_owned) else {
        return state;
    };
    let turned = match (shape.as_str(), rot) {
        ("north_south", Rot::Cw90 | Rot::Ccw90) => "east_west",
        ("east_west", Rot::Cw90 | Rot::Ccw90) => "north_south",
        ("ascending_north", Rot::Cw90) => "ascending_east",
        ("ascending_east", Rot::Cw90) => "ascending_south",
        ("ascending_south", Rot::Cw90) => "ascending_west",
        ("ascending_west", Rot::Cw90) => "ascending_north",
        ("south_east", Rot::Cw90) => "south_west",
        ("south_west", Rot::Cw90) => "north_west",
        ("north_west", Rot::Cw90) => "north_east",
        ("north_east", Rot::Cw90) => "south_east",
        _ => return state,
    };
    pal.with_prop(state, "shape", turned).unwrap_or(state)
}

fn def(pal: &Palette, name: &str) -> u32 {
    pal.defaults.get(name).copied().unwrap_or(0)
}

fn with(pal: &Palette, state: u32, key: &str, value: &str) -> u32 {
    pal.with_prop(state, key, value).unwrap_or(state)
}

fn block_id(pal: &Palette, state: u32) -> u32 {
    pal.block_of
        .get(state as usize)
        .copied()
        .unwrap_or(u32::MAX)
}
