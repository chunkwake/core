//! Океанский монумент: `OceanMonumentStructure` и `OceanMonumentPieces`.
//! Страж и древний страж не пишутся. Броски комнат остаются.

use super::structure::Columns;
use super::template::Bounds;
use super::{Bits, Legacy, Level, Palette, Worldgen};
use std::collections::HashSet;

const D: usize = 0;
const U: usize = 1;
const N: usize = 2;
const S: usize = 3;
const W: usize = 4;
const E: usize = 5;
const OPP: [usize; 6] = [1, 0, 3, 2, 5, 4];
const STEP: [[i32; 3]; 6] = [
    [0, -1, 0],
    [0, 1, 0],
    [0, 0, -1],
    [0, 0, 1],
    [-1, 0, 0],
    [1, 0, 0],
];

/// Проёмы комнаты после закрытия коридоров.
#[derive(Clone, Copy)]
struct Snap {
    open: [bool; 6],
    /// `connections[UP] == null`: потолка сверху нет.
    up_null: bool,
    /// Комната не на нижнем этаже (`index / 25 > 0`).
    upper: bool,
}

#[derive(Clone)]
enum Body {
    Shell,
    Entry(Snap),
    Core,
    Wing(i32),
    Penthouse,
    Simple {
        design: i32,
        room: Snap,
    },
    Top(Snap),
    Dyz {
        south: Snap,
        north: Snap,
        south_up: Snap,
        north_up: Snap,
    },
    Dxy {
        west: Snap,
        east: Snap,
        west_up: Snap,
        east_up: Snap,
    },
    Dz {
        south: Snap,
        north: Snap,
    },
    Dx {
        west: Snap,
        east: Snap,
    },
    Dy {
        low: Snap,
        high: Snap,
    },
}

#[derive(Clone)]
pub struct Piece {
    min: [i32; 3],
    max: [i32; 3],
    /// `Plane.HORIZONTAL`: 0 север, 1 восток, 2 юг, 3 запад.
    facing: u8,
    body: Body,
}

impl Piece {
    pub fn bounds(&self) -> ([i32; 3], [i32; 3]) {
        (self.min, self.max)
    }

    pub fn place(&self, pal: &Palette, level: &mut dyn Level, r: &mut Worldgen, window: Bounds) {
        let mut draw = Draw::new(pal, level, r, window, self.facing, self.min, self.max);
        match &self.body {
            Body::Shell => shell(&mut draw),
            Body::Entry(room) => entry(&mut draw, *room),
            Body::Core => core(&mut draw),
            Body::Wing(design) => wing_room(&mut draw, *design),
            Body::Penthouse => penthouse(&mut draw),
            Body::Simple { design, room } => simple(&mut draw, *design, *room),
            Body::Top(room) => simple_top(&mut draw, *room),
            Body::Dyz {
                south,
                north,
                south_up,
                north_up,
            } => double_yz(&mut draw, *south, *north, *south_up, *north_up),
            Body::Dxy {
                west,
                east,
                west_up,
                east_up,
            } => double_xy(&mut draw, *west, *east, *west_up, *east_up),
            Body::Dz { south, north } => double_z(&mut draw, *south, *north),
            Body::Dx { west, east } => double_x(&mut draw, *west, *east),
            Body::Dy { low, high } => double_y(&mut draw, *low, *high),
        }
    }
}

struct Room {
    index: i32,
    conn: [Option<usize>; 6],
    open: [bool; 6],
    claimed: bool,
    source: bool,
    scan: i32,
}

impl Room {
    fn new(index: i32) -> Self {
        Self {
            index,
            conn: [None; 6],
            open: [false; 6],
            claimed: false,
            source: false,
            scan: 0,
        }
    }
}

/// Биом середины чанка — глубокий океан, а куб радиуса 29 вокруг уровня моря
/// целиком в `#required_ocean_monument_surrounding`.
pub fn can_start(
    sx: i32,
    sz: i32,
    columns: &dyn Columns,
    deep: &HashSet<u32>,
    around: &HashSet<u32>,
) -> Option<()> {
    if !surrounding_ok(sx, sz, columns, around) {
        return None;
    }
    let x = sx * 16 + 8;
    let z = sz * 16 + 8;
    let y = columns.ocean_floor(x, z) - 1;
    let id = columns.biome_at(x, y, z)?;
    deep.contains(&id).then_some(())
}

fn surrounding_ok(sx: i32, sz: i32, columns: &dyn Columns, around: &HashSet<u32>) -> bool {
    let x = sx * 16 + 9;
    let z = sz * 16 + 9;
    let y = columns.sea_level();
    let x0 = (x - 29) >> 2;
    let x1 = (x + 29) >> 2;
    let y0 = (y - 29) >> 2;
    let y1 = (y + 29) >> 2;
    let z0 = (z - 29) >> 2;
    let z1 = (z + 29) >> 2;
    for qz in z0..=z1 {
        for qx in x0..=x1 {
            for qy in y0..=y1 {
                let Some(id) = columns.biome_at(qx << 2, qy << 2, qz << 2) else {
                    return false;
                };
                if !around.contains(&id) {
                    return false;
                }
            }
        }
    }
    true
}

/// Куски здания. Поворот — первый `nextInt(4)` после `setLargeFeatureSeed`.
pub fn build(rng: &mut Legacy, sx: i32, sz: i32) -> Vec<Piece> {
    let facing = rng.next_int_bound(4) as u8;
    let west = sx * 16 - 29;
    let north = sz * 16 - 29;
    let shell_min = [west, 39, north];
    let shell_max = [west + 57, 61, north + 57];
    let mut rooms = Vec::new();
    let mut grid = [None; 75];
    for y in [0, 1] {
        for x in 0..5 {
            for z in 0..4 {
                push_room(&mut rooms, &mut grid, x, y, z);
            }
        }
    }
    for x in 1..4 {
        for z in 0..2 {
            push_room(&mut rooms, &mut grid, x, 2, z);
        }
    }
    for x in 0..5 {
        for z in 0..5 {
            for y in 0..3 {
                let pos = room_index(x, y, z);
                let Some(slot) = grid[pos as usize] else {
                    continue;
                };
                for dir in 0..6 {
                    let nx = x + STEP[dir][0];
                    let ny = y + STEP[dir][1];
                    let nz = z + STEP[dir][2];
                    if !(0..5).contains(&nx) || !(0..5).contains(&nz) || !(0..3).contains(&ny) {
                        continue;
                    }
                    let npos = room_index(nx, ny, nz);
                    let Some(nslot) = grid[npos as usize] else {
                        continue;
                    };
                    if nz == z {
                        link(&mut rooms, slot, dir, nslot);
                    } else {
                        link(&mut rooms, slot, OPP[dir], nslot);
                    }
                }
            }
        }
    }
    let roof = rooms.len();
    rooms.push(Room::new(1003));
    let left_wing = rooms.len();
    rooms.push(Room::new(1001));
    let right_wing = rooms.len();
    rooms.push(Room::new(1002));
    let top = grid[room_index(2, 2, 0) as usize].unwrap();
    let left_conn = grid[room_index(0, 1, 0) as usize].unwrap();
    let right_conn = grid[room_index(4, 1, 0) as usize].unwrap();
    link(&mut rooms, top, U, roof);
    link(&mut rooms, left_conn, S, left_wing);
    link(&mut rooms, right_conn, S, right_wing);
    rooms[roof].claimed = true;
    rooms[left_wing].claimed = true;
    rooms[right_wing].claimed = true;
    let source = grid[room_index(2, 0, 0) as usize].unwrap();
    rooms[source].source = true;
    let core = grid[room_index(rng.next_int_bound(4), 0, 2) as usize].unwrap();
    rooms[core].claimed = true;
    let east = rooms[core].conn[E].unwrap();
    let north_room = rooms[core].conn[N].unwrap();
    rooms[east].claimed = true;
    rooms[north_room].claimed = true;
    let east_north = rooms[east].conn[N].unwrap();
    rooms[east_north].claimed = true;
    let up = rooms[core].conn[U].unwrap();
    rooms[up].claimed = true;
    let east_up = rooms[east].conn[U].unwrap();
    rooms[east_up].claimed = true;
    let north_up = rooms[north_room].conn[U].unwrap();
    rooms[north_up].claimed = true;
    let east_north_up = rooms[east_north].conn[U].unwrap();
    rooms[east_north_up].claimed = true;
    let mut order = Vec::new();
    for slot in grid.into_iter().flatten() {
        open_from_connections(&mut rooms[slot]);
        order.push(slot);
    }
    open_from_connections(&mut rooms[roof]);
    shuffle(&mut order, rng);
    let mut scan = 1i32;
    for &slot in &order {
        let mut closed = 0;
        let mut attempt = 0;
        while closed < 2 && attempt < 5 {
            attempt += 1;
            let f = rng.next_int_bound(6) as usize;
            if !rooms[slot].open[f] {
                continue;
            }
            let other = rooms[slot].conn[f].unwrap();
            let of = OPP[f];
            rooms[slot].open[f] = false;
            rooms[other].open[of] = false;
            let first = scan;
            scan += 1;
            let ok = find_source(&mut rooms, slot, first) && {
                let second = scan;
                scan += 1;
                find_source(&mut rooms, other, second)
            };
            if ok {
                closed += 1;
            } else {
                rooms[slot].open[f] = true;
                rooms[other].open[of] = true;
            }
        }
    }
    order.push(roof);
    order.push(left_wing);
    order.push(right_wing);
    rooms[source].claimed = true;
    let mut pieces = vec![Piece {
        min: shell_min,
        max: shell_max,
        facing,
        body: Body::Shell,
    }];
    pieces.push(room_piece(
        facing,
        &rooms,
        source,
        1,
        1,
        1,
        Body::Entry(snap(&rooms, source)),
    ));
    pieces.push(room_piece(facing, &rooms, core, 2, 2, 2, Body::Core));
    for slot in order {
        if rooms[slot].claimed || rooms[slot].index >= 75 {
            continue;
        }
        let (at, rw, rh, rd, body) = fit(&mut rooms, slot, rng);
        pieces.push(room_piece(facing, &rooms, at, rw, rh, rd, body));
    }
    let off = world_at(facing, shell_min, shell_max, 9, 0, 22);
    for piece in pieces.iter_mut().skip(1) {
        for (axis, o) in off.iter().enumerate() {
            piece.min[axis] += o;
            piece.max[axis] += o;
        }
    }
    let wing_roll = rng.next_int();
    let left_box = corners(
        world_at(facing, shell_min, shell_max, 1, 1, 1),
        world_at(facing, shell_min, shell_max, 23, 8, 21),
    );
    let right_box = corners(
        world_at(facing, shell_min, shell_max, 34, 1, 1),
        world_at(facing, shell_min, shell_max, 56, 8, 21),
    );
    let penthouse_box = corners(
        world_at(facing, shell_min, shell_max, 22, 13, 22),
        world_at(facing, shell_min, shell_max, 35, 17, 35),
    );
    pieces.push(boxed(facing, left_box, Body::Wing(wing_roll & 1)));
    pieces.push(boxed(
        facing,
        right_box,
        Body::Wing(wing_roll.wrapping_add(1) & 1),
    ));
    pieces.push(boxed(facing, penthouse_box, Body::Penthouse));
    pieces
}

fn room_index(x: i32, y: i32, z: i32) -> i32 {
    y * 25 + z * 5 + x
}

fn push_room(rooms: &mut Vec<Room>, grid: &mut [Option<usize>; 75], x: i32, y: i32, z: i32) {
    let index = room_index(x, y, z);
    let slot = rooms.len();
    rooms.push(Room::new(index));
    grid[index as usize] = Some(slot);
}

fn link(rooms: &mut [Room], a: usize, dir: usize, b: usize) {
    rooms[a].conn[dir] = Some(b);
    rooms[b].conn[OPP[dir]] = Some(a);
}

fn open_from_connections(room: &mut Room) {
    for i in 0..6 {
        room.open[i] = room.conn[i].is_some();
    }
}

fn shuffle(order: &mut [usize], rng: &mut Legacy) {
    let mut i = order.len();
    while i > 1 {
        let swap_to = rng.next_int_bound(i as i32) as usize;
        i -= 1;
        order.swap(i, swap_to);
    }
}

fn find_source(rooms: &mut [Room], slot: usize, scan: i32) -> bool {
    if rooms[slot].source {
        return true;
    }
    rooms[slot].scan = scan;
    for i in 0..6 {
        let Some(next) = rooms[slot].conn[i] else {
            continue;
        };
        if !rooms[slot].open[i] || rooms[next].scan == scan {
            continue;
        }
        if find_source(rooms, next, scan) {
            return true;
        }
    }
    false
}

fn snap(rooms: &[Room], slot: usize) -> Snap {
    Snap {
        open: rooms[slot].open,
        up_null: rooms[slot].conn[U].is_none(),
        upper: rooms[slot].index / 25 > 0,
    }
}

fn fit(rooms: &mut [Room], slot: usize, rng: &mut Legacy) -> (usize, i32, i32, i32, Body) {
    if fits_xy(rooms, slot) {
        return (slot, 2, 2, 1, claim_xy(rooms, slot));
    }
    if fits_yz(rooms, slot) {
        return (slot, 1, 2, 2, claim_yz(rooms, slot));
    }
    if fits_z(rooms, slot) {
        let (at, body) = claim_z(rooms, slot);
        return (at, 1, 1, 2, body);
    }
    if fits_x(rooms, slot) {
        return (slot, 2, 1, 1, claim_x(rooms, slot));
    }
    if fits_y(rooms, slot) {
        return (slot, 1, 2, 1, claim_y(rooms, slot));
    }
    if fits_top(rooms, slot) {
        rooms[slot].claimed = true;
        return (slot, 1, 1, 1, Body::Top(snap(rooms, slot)));
    }
    rooms[slot].claimed = true;
    (
        slot,
        1,
        1,
        1,
        Body::Simple {
            design: rng.next_int_bound(3),
            room: snap(rooms, slot),
        },
    )
}

fn fits_xy(rooms: &[Room], slot: usize) -> bool {
    let Some(east) = rooms[slot].conn[E] else {
        return false;
    };
    let Some(up) = rooms[slot].conn[U] else {
        return false;
    };
    if !(rooms[slot].open[E] && !rooms[east].claimed && rooms[slot].open[U] && !rooms[up].claimed) {
        return false;
    }
    let Some(east_up) = rooms[east].conn[U] else {
        return false;
    };
    rooms[east].open[U] && !rooms[east_up].claimed
}

fn claim_xy(rooms: &mut [Room], slot: usize) -> Body {
    let east = rooms[slot].conn[E].unwrap();
    let up = rooms[slot].conn[U].unwrap();
    let east_up = rooms[east].conn[U].unwrap();
    rooms[slot].claimed = true;
    rooms[east].claimed = true;
    rooms[up].claimed = true;
    rooms[east_up].claimed = true;
    Body::Dxy {
        west: snap(rooms, slot),
        east: snap(rooms, east),
        west_up: snap(rooms, up),
        east_up: snap(rooms, east_up),
    }
}

fn fits_yz(rooms: &[Room], slot: usize) -> bool {
    let Some(north) = rooms[slot].conn[N] else {
        return false;
    };
    let Some(up) = rooms[slot].conn[U] else {
        return false;
    };
    if !(rooms[slot].open[N] && !rooms[north].claimed && rooms[slot].open[U] && !rooms[up].claimed)
    {
        return false;
    }
    let Some(north_up) = rooms[north].conn[U] else {
        return false;
    };
    rooms[north].open[U] && !rooms[north_up].claimed
}

fn claim_yz(rooms: &mut [Room], slot: usize) -> Body {
    let north = rooms[slot].conn[N].unwrap();
    let up = rooms[slot].conn[U].unwrap();
    let north_up = rooms[north].conn[U].unwrap();
    rooms[slot].claimed = true;
    rooms[north].claimed = true;
    rooms[up].claimed = true;
    rooms[north_up].claimed = true;
    Body::Dyz {
        south: snap(rooms, slot),
        north: snap(rooms, north),
        south_up: snap(rooms, up),
        north_up: snap(rooms, north_up),
    }
}

fn fits_z(rooms: &[Room], slot: usize) -> bool {
    rooms[slot].conn[N].is_some_and(|north| rooms[slot].open[N] && !rooms[north].claimed)
}

fn claim_z(rooms: &mut [Room], slot: usize) -> (usize, Body) {
    let north_ok =
        rooms[slot].conn[N].is_some_and(|north| rooms[slot].open[N] && !rooms[north].claimed);
    let source = if north_ok {
        slot
    } else {
        rooms[slot].conn[S].unwrap()
    };
    let north = rooms[source].conn[N].unwrap();
    rooms[source].claimed = true;
    rooms[north].claimed = true;
    (
        source,
        Body::Dz {
            south: snap(rooms, source),
            north: snap(rooms, north),
        },
    )
}

fn fits_x(rooms: &[Room], slot: usize) -> bool {
    rooms[slot].conn[E].is_some_and(|east| rooms[slot].open[E] && !rooms[east].claimed)
}

fn claim_x(rooms: &mut [Room], slot: usize) -> Body {
    let east = rooms[slot].conn[E].unwrap();
    rooms[slot].claimed = true;
    rooms[east].claimed = true;
    Body::Dx {
        west: snap(rooms, slot),
        east: snap(rooms, east),
    }
}

fn fits_y(rooms: &[Room], slot: usize) -> bool {
    rooms[slot].conn[U].is_some_and(|up| rooms[slot].open[U] && !rooms[up].claimed)
}

fn claim_y(rooms: &mut [Room], slot: usize) -> Body {
    let up = rooms[slot].conn[U].unwrap();
    rooms[slot].claimed = true;
    rooms[up].claimed = true;
    Body::Dy {
        low: snap(rooms, slot),
        high: snap(rooms, up),
    }
}

fn fits_top(rooms: &[Room], slot: usize) -> bool {
    !rooms[slot].open[W]
        && !rooms[slot].open[E]
        && !rooms[slot].open[N]
        && !rooms[slot].open[S]
        && !rooms[slot].open[U]
}

fn room_piece(
    facing: u8,
    rooms: &[Room],
    slot: usize,
    rw: i32,
    rh: i32,
    rd: i32,
    body: Body,
) -> Piece {
    let (min, max) = piece_box(facing, rooms[slot].index, rw, rh, rd);
    Piece {
        min,
        max,
        facing,
        body,
    }
}

fn boxed(facing: u8, (min, max): ([i32; 3], [i32; 3]), body: Body) -> Piece {
    Piece {
        min,
        max,
        facing,
        body,
    }
}

fn piece_box(facing: u8, index: i32, rw: i32, rh: i32, rd: i32) -> ([i32; 3], [i32; 3]) {
    let room_x = index % 5;
    let room_z = index / 5 % 5;
    let room_y = index / 25;
    let (width, height, depth) = (rw * 8, rh * 4, rd * 8);
    let (mut min, mut max) = if facing == 0 || facing == 2 {
        ([0, 0, 0], [width - 1, height - 1, depth - 1])
    } else {
        ([0, 0, 0], [depth - 1, height - 1, width - 1])
    };
    let shift = match facing {
        0 => [room_x * 8, room_y * 4, -(room_z + rd) * 8 + 1],
        2 => [room_x * 8, room_y * 4, room_z * 8],
        3 => [-(room_z + rd) * 8 + 1, room_y * 4, room_x * 8],
        _ => [room_z * 8, room_y * 4, room_x * 8],
    };
    for axis in 0..3 {
        min[axis] += shift[axis];
        max[axis] += shift[axis];
    }
    (min, max)
}

fn world_at(facing: u8, min: [i32; 3], max: [i32; 3], x: i32, y: i32, z: i32) -> [i32; 3] {
    let wy = min[1] + y;
    match facing {
        0 => [min[0] + x, wy, max[2] - z],
        2 => [min[0] + x, wy, min[2] + z],
        3 => [max[0] - z, wy, min[2] + x],
        _ => [min[0] + z, wy, min[2] + x],
    }
}

fn corners(a: [i32; 3], b: [i32; 3]) -> ([i32; 3], [i32; 3]) {
    let mut min = [0; 3];
    let mut max = [0; 3];
    for axis in 0..3 {
        min[axis] = a[axis].min(b[axis]);
        max[axis] = a[axis].max(b[axis]);
    }
    (min, max)
}

struct Draw<'a> {
    pal: &'a Palette,
    level: &'a mut dyn Level,
    rng: &'a mut Worldgen,
    window: Bounds,
    facing: u8,
    min: [i32; 3],
    max: [i32; 3],
    gray: u32,
    light: u32,
    black: u32,
    lamp: u32,
    water: u32,
    air: u32,
    sponge: u32,
    gold: u32,
    ice: u32,
    packed: u32,
    blue: u32,
    water_block: u32,
    lichen: u32,
    sea: u32,
    tall: u32,
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
        let id = |name: &str| block_id(pal, state(name));
        Self {
            pal,
            level,
            rng,
            window,
            facing,
            min,
            max,
            gray: state("minecraft:prismarine"),
            light: state("minecraft:prismarine_bricks"),
            black: state("minecraft:dark_prismarine"),
            lamp: state("minecraft:sea_lantern"),
            water: state("minecraft:water"),
            air: state("minecraft:air"),
            sponge: state("minecraft:wet_sponge"),
            gold: state("minecraft:gold_block"),
            ice: id("minecraft:ice"),
            packed: id("minecraft:packed_ice"),
            blue: id("minecraft:blue_ice"),
            water_block: id("minecraft:water"),
            lichen: id("minecraft:glow_lichen"),
            sea: id("minecraft:seagrass"),
            tall: id("minecraft:tall_seagrass"),
        }
    }

    fn world(&self, x: i32, y: i32, z: i32) -> [i32; 3] {
        world_at(self.facing, self.min, self.max, x, y, z)
    }

    fn put(&mut self, x: i32, y: i32, z: i32, state: u32) {
        let pos = self.world(x, y, z);
        if self.window.contains(pos) {
            self.level.set(pos[0], pos[1], pos[2], state);
        }
    }

    fn get(&self, x: i32, y: i32, z: i32) -> u32 {
        let pos = self.world(x, y, z);
        if !self.window.contains(pos) {
            return self.air;
        }
        self.level.get(pos[0], pos[1], pos[2])
    }

    #[allow(clippy::too_many_arguments)]
    fn cuboid(&mut self, x0: i32, y0: i32, z0: i32, x1: i32, y1: i32, z1: i32, state: u32) {
        if x0 > x1 || y0 > y1 || z0 > z1 {
            return;
        }
        for y in y0..=y1 {
            for x in x0..=x1 {
                for z in z0..=z1 {
                    self.put(x, y, z, state);
                }
            }
        }
    }

    fn water(&mut self, x0: i32, y0: i32, z0: i32, x1: i32, y1: i32, z1: i32) {
        if x0 > x1 || y0 > y1 || z0 > z1 {
            return;
        }
        let sea = self.level.sea_level();
        let (air, water) = (self.air, self.water);
        for y in y0..=y1 {
            for x in x0..=x1 {
                for z in z0..=z1 {
                    let state = self.get(x, y, z);
                    let id = block_id(self.pal, state);
                    if id == self.ice
                        || id == self.packed
                        || id == self.blue
                        || id == self.water_block
                    {
                        continue;
                    }
                    let place = if self.min[1] + y >= sea { air } else { water };
                    self.put(x, y, z, place);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn fill_only(&mut self, x0: i32, y0: i32, z0: i32, x1: i32, y1: i32, z1: i32, state: u32) {
        if x0 > x1 || y0 > y1 || z0 > z1 {
            return;
        }
        let water = self.water;
        for y in y0..=y1 {
            for x in x0..=x1 {
                for z in z0..=z1 {
                    if self.get(x, y, z) == water {
                        self.put(x, y, z, state);
                    }
                }
            }
        }
    }

    fn column_down(&mut self, state: u32, x: i32, y: i32, z: i32) {
        let mut pos = self.world(x, y, z);
        if !self.window.contains(pos) {
            return;
        }
        let floor = self.level.min_y() + 1;
        while pos[1] > floor && self.replaceable(self.level.get(pos[0], pos[1], pos[2])) {
            self.level.set(pos[0], pos[1], pos[2], state);
            pos[1] -= 1;
        }
    }

    fn replaceable(&self, state: u32) -> bool {
        if self.pal.is_air(state) || self.pal.is_liquid(state) {
            return true;
        }
        let id = block_id(self.pal, state);
        id == self.lichen || id == self.sea || id == self.tall
    }

    fn floor(&mut self, x_off: i32, z_off: i32, down: bool) {
        let (gray, light) = (self.gray, self.light);
        if down {
            self.cuboid(x_off, 0, z_off, x_off + 2, 0, z_off + 7, gray);
            self.cuboid(x_off + 5, 0, z_off, x_off + 7, 0, z_off + 7, gray);
            self.cuboid(x_off + 3, 0, z_off, x_off + 4, 0, z_off + 2, gray);
            self.cuboid(x_off + 3, 0, z_off + 5, x_off + 4, 0, z_off + 7, gray);
            self.cuboid(x_off + 3, 0, z_off + 2, x_off + 4, 0, z_off + 2, light);
            self.cuboid(x_off + 3, 0, z_off + 5, x_off + 4, 0, z_off + 5, light);
            self.cuboid(x_off + 2, 0, z_off + 3, x_off + 2, 0, z_off + 4, light);
            self.cuboid(x_off + 5, 0, z_off + 3, x_off + 5, 0, z_off + 4, light);
        } else {
            self.cuboid(x_off, 0, z_off, x_off + 7, 0, z_off + 7, gray);
        }
    }
}

fn block_id(pal: &Palette, state: u32) -> u32 {
    pal.block_of
        .get(state as usize)
        .copied()
        .unwrap_or(u32::MAX)
}

fn shell(d: &mut Draw) {
    let (gray, light, black, lamp) = (d.gray, d.light, d.black, d.lamp);
    let water_height = d.level.sea_level().max(64) - d.min[1];
    d.water(0, 0, 0, 58, water_height, 58);
    wing(d, false, 0, gray, light);
    wing(d, true, 33, gray, light);
    arches(d, gray, light, lamp);
    entrance_wall(d, gray, light, black);
    roof(d, gray, light, lamp);
    lower_wall(d, gray, light);
    middle_wall(d, gray, light);
    upper_wall(d, gray, light);
    for pillar_x in 0..7 {
        let mut pillar_z = 0;
        while pillar_z < 7 {
            if pillar_z == 0 && pillar_x == 3 {
                pillar_z = 6;
            }
            let bx = pillar_x * 9;
            let bz = pillar_z * 9;
            for w in 0..4 {
                for depth in 0..4 {
                    d.put(bx + w, 0, bz + depth, light);
                    d.column_down(light, bx + w, -1, bz + depth);
                }
            }
            if pillar_x == 0 || pillar_x == 6 {
                pillar_z += 1;
            } else {
                pillar_z += 6;
            }
        }
    }
    for i in 0..5 {
        d.water(-1 - i, i * 2, -1 - i, -1 - i, 23, 58 + i);
        d.water(58 + i, i * 2, -1 - i, 58 + i, 23, 58 + i);
        d.water(-i, i * 2, -1 - i, 57 + i, 23, -1 - i);
        d.water(-i, i * 2, 58 + i, 57 + i, 23, 58 + i);
    }
}

fn wing(d: &mut Draw, flipped: bool, xoff: i32, gray: u32, light: u32) {
    d.cuboid(xoff, 0, 0, xoff + 24, 0, 20, gray);
    d.water(xoff, 1, 0, xoff + 24, 10, 20);
    for i in 0..4 {
        d.cuboid(xoff + i, i + 1, i, xoff + i, i + 1, 20, light);
        d.cuboid(xoff + i + 7, i + 5, i + 7, xoff + i + 7, i + 5, 20, light);
        d.cuboid(xoff + 17 - i, i + 5, i + 7, xoff + 17 - i, i + 5, 20, light);
        d.cuboid(xoff + 24 - i, i + 1, i, xoff + 24 - i, i + 1, 20, light);
        d.cuboid(xoff + i + 1, i + 1, i, xoff + 23 - i, i + 1, i, light);
        d.cuboid(
            xoff + i + 8,
            i + 5,
            i + 7,
            xoff + 16 - i,
            i + 5,
            i + 7,
            light,
        );
    }
    d.cuboid(xoff + 4, 4, 4, xoff + 6, 4, 20, gray);
    d.cuboid(xoff + 7, 4, 4, xoff + 17, 4, 6, gray);
    d.cuboid(xoff + 18, 4, 4, xoff + 20, 4, 20, gray);
    d.cuboid(xoff + 11, 8, 11, xoff + 13, 8, 20, gray);
    d.put(xoff + 12, 9, 12, light);
    d.put(xoff + 12, 9, 15, light);
    d.put(xoff + 12, 9, 18, light);
    let left = xoff + if flipped { 19 } else { 5 };
    let right = xoff + if flipped { 5 } else { 19 };
    let mut z = 20;
    while z >= 5 {
        d.put(left, 5, z, light);
        z -= 3;
    }
    z = 19;
    while z >= 7 {
        d.put(right, 5, z, light);
        z -= 3;
    }
    for i in 0..4 {
        let pos = if flipped {
            xoff + 24 - (17 - i * 3)
        } else {
            xoff + 17 - i * 3
        };
        d.put(pos, 5, 5, light);
    }
    d.put(right, 5, 5, light);
    d.cuboid(xoff + 11, 1, 12, xoff + 13, 7, 12, gray);
    d.cuboid(xoff + 12, 1, 11, xoff + 12, 7, 13, gray);
}

fn arches(d: &mut Draw, gray: u32, light: u32, lamp: u32) {
    d.water(25, 0, 0, 32, 8, 20);
    for i in 0..4 {
        let z = 5 + i * 4;
        d.cuboid(24, 2, z, 24, 4, z, light);
        d.cuboid(22, 4, z, 23, 4, z, light);
        d.put(25, 5, z, light);
        d.put(26, 6, z, light);
        d.put(26, 5, z, lamp);
        d.cuboid(33, 2, z, 33, 4, z, light);
        d.cuboid(34, 4, z, 35, 4, z, light);
        d.put(32, 5, z, light);
        d.put(31, 6, z, light);
        d.put(31, 5, z, lamp);
        d.cuboid(27, 6, z, 30, 6, z, gray);
    }
}

fn entrance_wall(d: &mut Draw, gray: u32, light: u32, black: u32) {
    d.cuboid(15, 0, 21, 42, 0, 21, gray);
    d.water(26, 1, 21, 31, 3, 21);
    d.cuboid(21, 12, 21, 36, 12, 21, gray);
    d.cuboid(17, 11, 21, 40, 11, 21, gray);
    d.cuboid(16, 10, 21, 41, 10, 21, gray);
    d.cuboid(15, 7, 21, 42, 9, 21, gray);
    d.cuboid(16, 6, 21, 41, 6, 21, gray);
    d.cuboid(17, 5, 21, 40, 5, 21, gray);
    d.cuboid(21, 4, 21, 36, 4, 21, gray);
    d.cuboid(22, 3, 21, 26, 3, 21, gray);
    d.cuboid(31, 3, 21, 35, 3, 21, gray);
    d.cuboid(23, 2, 21, 25, 2, 21, gray);
    d.cuboid(32, 2, 21, 34, 2, 21, gray);
    d.cuboid(28, 4, 20, 29, 4, 21, light);
    d.put(27, 3, 21, light);
    d.put(30, 3, 21, light);
    d.put(26, 2, 21, light);
    d.put(31, 2, 21, light);
    d.put(25, 1, 21, light);
    d.put(32, 1, 21, light);
    for i in 0..7 {
        d.put(28 - i, 6 + i, 21, black);
        d.put(29 + i, 6 + i, 21, black);
    }
    for i in 0..4 {
        d.put(28 - i, 9 + i, 21, black);
        d.put(29 + i, 9 + i, 21, black);
    }
    d.put(28, 12, 21, black);
    d.put(29, 12, 21, black);
    for i in 0..3 {
        d.put(22 - i * 2, 8, 21, black);
        d.put(22 - i * 2, 9, 21, black);
        d.put(35 + i * 2, 8, 21, black);
        d.put(35 + i * 2, 9, 21, black);
    }
    d.water(15, 13, 21, 42, 15, 21);
    d.water(15, 1, 21, 15, 6, 21);
    d.water(16, 1, 21, 16, 5, 21);
    d.water(17, 1, 21, 20, 4, 21);
    d.water(21, 1, 21, 21, 3, 21);
    d.water(22, 1, 21, 22, 2, 21);
    d.water(23, 1, 21, 24, 1, 21);
    d.water(42, 1, 21, 42, 6, 21);
    d.water(41, 1, 21, 41, 5, 21);
    d.water(37, 1, 21, 40, 4, 21);
    d.water(36, 1, 21, 36, 3, 21);
    d.water(33, 1, 21, 34, 1, 21);
    d.water(35, 1, 21, 35, 2, 21);
}

fn roof(d: &mut Draw, gray: u32, light: u32, lamp: u32) {
    d.cuboid(21, 0, 22, 36, 0, 36, gray);
    d.water(21, 1, 22, 36, 23, 36);
    for i in 0..4 {
        d.cuboid(21 + i, 13 + i, 21 + i, 36 - i, 13 + i, 21 + i, light);
        d.cuboid(21 + i, 13 + i, 36 - i, 36 - i, 13 + i, 36 - i, light);
        d.cuboid(21 + i, 13 + i, 22 + i, 21 + i, 13 + i, 35 - i, light);
        d.cuboid(36 - i, 13 + i, 22 + i, 36 - i, 13 + i, 35 - i, light);
    }
    d.cuboid(25, 16, 25, 32, 16, 32, gray);
    d.cuboid(25, 17, 25, 25, 19, 25, light);
    d.cuboid(32, 17, 25, 32, 19, 25, light);
    d.cuboid(25, 17, 32, 25, 19, 32, light);
    d.cuboid(32, 17, 32, 32, 19, 32, light);
    d.put(26, 20, 26, light);
    d.put(27, 21, 27, light);
    d.put(27, 20, 27, lamp);
    d.put(26, 20, 31, light);
    d.put(27, 21, 30, light);
    d.put(27, 20, 30, lamp);
    d.put(31, 20, 31, light);
    d.put(30, 21, 30, light);
    d.put(30, 20, 30, lamp);
    d.put(31, 20, 26, light);
    d.put(30, 21, 27, light);
    d.put(30, 20, 27, lamp);
    d.cuboid(28, 21, 27, 29, 21, 27, gray);
    d.cuboid(27, 21, 28, 27, 21, 29, gray);
    d.cuboid(28, 21, 30, 29, 21, 30, gray);
    d.cuboid(30, 21, 28, 30, 21, 29, gray);
}

fn lower_wall(d: &mut Draw, gray: u32, light: u32) {
    d.cuboid(0, 0, 21, 6, 0, 57, gray);
    d.water(0, 1, 21, 6, 7, 57);
    d.cuboid(4, 4, 21, 6, 4, 53, gray);
    for i in 0..4 {
        d.cuboid(i, i + 1, 21, i, i + 1, 57 - i, light);
    }
    let mut z = 23;
    while z < 53 {
        d.put(5, 5, z, light);
        z += 3;
    }
    d.put(5, 5, 52, light);
    for i in 0..4 {
        d.cuboid(i, i + 1, 21, i, i + 1, 57 - i, light);
    }
    d.cuboid(4, 1, 52, 6, 3, 52, gray);
    d.cuboid(5, 1, 51, 5, 3, 53, gray);
    d.cuboid(51, 0, 21, 57, 0, 57, gray);
    d.water(51, 1, 21, 57, 7, 57);
    d.cuboid(51, 4, 21, 53, 4, 53, gray);
    for i in 0..4 {
        d.cuboid(57 - i, i + 1, 21, 57 - i, i + 1, 57 - i, light);
    }
    z = 23;
    while z < 53 {
        d.put(52, 5, z, light);
        z += 3;
    }
    d.put(52, 5, 52, light);
    d.cuboid(51, 1, 52, 53, 3, 52, gray);
    d.cuboid(52, 1, 51, 52, 3, 53, gray);
    d.cuboid(7, 0, 51, 50, 0, 57, gray);
    d.water(7, 1, 51, 50, 10, 57);
    for i in 0..4 {
        d.cuboid(i + 1, i + 1, 57 - i, 56 - i, i + 1, 57 - i, light);
    }
}

fn middle_wall(d: &mut Draw, gray: u32, light: u32) {
    d.cuboid(7, 0, 21, 13, 0, 50, gray);
    d.water(7, 1, 21, 13, 10, 50);
    d.cuboid(11, 8, 21, 13, 8, 53, gray);
    for i in 0..4 {
        d.cuboid(i + 7, i + 5, 21, i + 7, i + 5, 54, light);
    }
    let mut z = 21;
    while z <= 45 {
        d.put(12, 9, z, light);
        z += 3;
    }
    d.cuboid(44, 0, 21, 50, 0, 50, gray);
    d.water(44, 1, 21, 50, 10, 50);
    d.cuboid(44, 8, 21, 46, 8, 53, gray);
    for i in 0..4 {
        d.cuboid(50 - i, i + 5, 21, 50 - i, i + 5, 54, light);
    }
    z = 21;
    while z <= 45 {
        d.put(45, 9, z, light);
        z += 3;
    }
    d.cuboid(14, 0, 44, 43, 0, 50, gray);
    d.water(14, 1, 44, 43, 10, 50);
    let mut x = 12;
    while x <= 45 {
        d.put(x, 9, 45, light);
        d.put(x, 9, 52, light);
        if matches!(x, 12 | 18 | 24 | 33 | 39 | 45) {
            d.put(x, 9, 47, light);
            d.put(x, 9, 50, light);
            d.put(x, 10, 45, light);
            d.put(x, 10, 46, light);
            d.put(x, 10, 51, light);
            d.put(x, 10, 52, light);
            d.put(x, 11, 47, light);
            d.put(x, 11, 50, light);
            d.put(x, 12, 48, light);
            d.put(x, 12, 49, light);
        }
        x += 3;
    }
    for i in 0..3 {
        d.cuboid(8 + i, 5 + i, 54, 49 - i, 5 + i, 54, gray);
    }
    d.cuboid(11, 8, 54, 46, 8, 54, light);
    d.cuboid(14, 8, 44, 43, 8, 53, gray);
}

fn upper_wall(d: &mut Draw, gray: u32, light: u32) {
    d.cuboid(14, 0, 21, 20, 0, 43, gray);
    d.water(14, 1, 22, 20, 14, 43);
    d.cuboid(18, 12, 22, 20, 12, 39, gray);
    d.cuboid(18, 12, 21, 20, 12, 21, light);
    for i in 0..4 {
        d.cuboid(i + 14, i + 9, 21, i + 14, i + 9, 43 - i, light);
    }
    let mut z = 23;
    while z <= 39 {
        d.put(19, 13, z, light);
        z += 3;
    }
    d.cuboid(37, 0, 21, 43, 0, 43, gray);
    d.water(37, 1, 22, 43, 14, 43);
    d.cuboid(37, 12, 22, 39, 12, 39, gray);
    d.cuboid(37, 12, 21, 39, 12, 21, light);
    for i in 0..4 {
        d.cuboid(43 - i, i + 9, 21, 43 - i, i + 9, 43 - i, light);
    }
    z = 23;
    while z <= 39 {
        d.put(38, 13, z, light);
        z += 3;
    }
    d.cuboid(21, 0, 37, 36, 0, 43, gray);
    d.water(21, 1, 37, 36, 14, 43);
    d.cuboid(21, 12, 37, 36, 12, 39, gray);
    for i in 0..4 {
        d.cuboid(15 + i, i + 9, 43 - i, 42 - i, i + 9, 43 - i, light);
    }
    let mut x = 21;
    while x <= 36 {
        d.put(x, 13, 38, light);
        x += 3;
    }
}

fn entry(d: &mut Draw, room: Snap) {
    let light = d.light;
    d.cuboid(0, 3, 0, 2, 3, 7, light);
    d.cuboid(5, 3, 0, 7, 3, 7, light);
    d.cuboid(0, 2, 0, 1, 2, 7, light);
    d.cuboid(6, 2, 0, 7, 2, 7, light);
    d.cuboid(0, 1, 0, 0, 1, 7, light);
    d.cuboid(7, 1, 0, 7, 1, 7, light);
    d.cuboid(0, 1, 7, 7, 3, 7, light);
    d.cuboid(1, 1, 0, 2, 3, 0, light);
    d.cuboid(5, 1, 0, 6, 3, 0, light);
    if room.open[N] {
        d.water(3, 1, 7, 4, 2, 7);
    }
    if room.open[W] {
        d.water(0, 1, 3, 1, 2, 4);
    }
    if room.open[E] {
        d.water(6, 1, 3, 7, 2, 4);
    }
}

fn core(d: &mut Draw) {
    let (gray, light, black, lamp, gold) = (d.gray, d.light, d.black, d.lamp, d.gold);
    d.fill_only(1, 8, 0, 14, 8, 14, gray);
    d.cuboid(0, 7, 0, 0, 7, 15, light);
    d.cuboid(15, 7, 0, 15, 7, 15, light);
    d.cuboid(1, 7, 0, 15, 7, 0, light);
    d.cuboid(1, 7, 15, 14, 7, 15, light);
    for y in 1..=6 {
        let block = if y == 2 || y == 6 { gray } else { light };
        for x in [0, 15] {
            d.cuboid(x, y, 0, x, y, 1, block);
            d.cuboid(x, y, 6, x, y, 9, block);
            d.cuboid(x, y, 14, x, y, 15, block);
        }
        d.cuboid(1, y, 0, 1, y, 0, block);
        d.cuboid(6, y, 0, 9, y, 0, block);
        d.cuboid(14, y, 0, 14, y, 0, block);
        d.cuboid(1, y, 15, 14, y, 15, block);
    }
    d.cuboid(6, 3, 6, 9, 6, 9, black);
    d.cuboid(7, 4, 7, 8, 5, 8, gold);
    for y in [3, 6] {
        for x in [6, 9] {
            d.put(x, y, 6, lamp);
            d.put(x, y, 9, lamp);
        }
    }
    d.cuboid(5, 1, 6, 5, 2, 6, light);
    d.cuboid(5, 1, 9, 5, 2, 9, light);
    d.cuboid(10, 1, 6, 10, 2, 6, light);
    d.cuboid(10, 1, 9, 10, 2, 9, light);
    d.cuboid(6, 1, 5, 6, 2, 5, light);
    d.cuboid(9, 1, 5, 9, 2, 5, light);
    d.cuboid(6, 1, 10, 6, 2, 10, light);
    d.cuboid(9, 1, 10, 9, 2, 10, light);
    d.cuboid(5, 2, 5, 5, 6, 5, light);
    d.cuboid(5, 2, 10, 5, 6, 10, light);
    d.cuboid(10, 2, 5, 10, 6, 5, light);
    d.cuboid(10, 2, 10, 10, 6, 10, light);
    d.cuboid(5, 7, 1, 5, 7, 6, light);
    d.cuboid(10, 7, 1, 10, 7, 6, light);
    d.cuboid(5, 7, 9, 5, 7, 14, light);
    d.cuboid(10, 7, 9, 10, 7, 14, light);
    d.cuboid(1, 7, 5, 6, 7, 5, light);
    d.cuboid(1, 7, 10, 6, 7, 10, light);
    d.cuboid(9, 7, 5, 14, 7, 5, light);
    d.cuboid(9, 7, 10, 14, 7, 10, light);
    d.cuboid(2, 1, 2, 2, 1, 3, light);
    d.cuboid(3, 1, 2, 3, 1, 2, light);
    d.cuboid(13, 1, 2, 13, 1, 3, light);
    d.cuboid(12, 1, 2, 12, 1, 2, light);
    d.cuboid(2, 1, 12, 2, 1, 13, light);
    d.cuboid(3, 1, 13, 3, 1, 13, light);
    d.cuboid(13, 1, 12, 13, 1, 13, light);
    d.cuboid(12, 1, 13, 12, 1, 13, light);
}

fn wing_room(d: &mut Draw, design: i32) {
    let (light, black, lamp) = (d.light, d.black, d.lamp);
    if design == 0 {
        for i in 0..4 {
            d.cuboid(10 - i, 3 - i, 20 - i, 12 + i, 3 - i, 20, light);
        }
        d.cuboid(7, 0, 6, 15, 0, 16, light);
        d.cuboid(6, 0, 6, 6, 3, 20, light);
        d.cuboid(16, 0, 6, 16, 3, 20, light);
        d.cuboid(7, 1, 7, 7, 1, 20, light);
        d.cuboid(15, 1, 7, 15, 1, 20, light);
        d.cuboid(7, 1, 6, 9, 3, 6, light);
        d.cuboid(13, 1, 6, 15, 3, 6, light);
        d.cuboid(8, 1, 7, 9, 1, 7, light);
        d.cuboid(13, 1, 7, 14, 1, 7, light);
        d.cuboid(9, 0, 5, 13, 0, 5, light);
        d.cuboid(10, 0, 7, 12, 0, 7, black);
        d.cuboid(8, 0, 10, 8, 0, 12, black);
        d.cuboid(14, 0, 10, 14, 0, 12, black);
        let mut z = 18;
        while z >= 7 {
            d.put(6, 3, z, lamp);
            d.put(16, 3, z, lamp);
            z -= 3;
        }
        d.put(10, 0, 10, lamp);
        d.put(12, 0, 10, lamp);
        d.put(10, 0, 12, lamp);
        d.put(12, 0, 12, lamp);
        d.put(8, 3, 6, lamp);
        d.put(14, 3, 6, lamp);
        d.put(4, 2, 4, light);
        d.put(4, 1, 4, lamp);
        d.put(4, 0, 4, light);
        d.put(18, 2, 4, light);
        d.put(18, 1, 4, lamp);
        d.put(18, 0, 4, light);
        d.put(4, 2, 18, light);
        d.put(4, 1, 18, lamp);
        d.put(4, 0, 18, light);
        d.put(18, 2, 18, light);
        d.put(18, 1, 18, lamp);
        d.put(18, 0, 18, light);
        d.put(9, 7, 20, light);
        d.put(13, 7, 20, light);
        d.cuboid(6, 0, 21, 7, 4, 21, light);
        d.cuboid(15, 0, 21, 16, 4, 21, light);
    } else if design == 1 {
        d.cuboid(9, 3, 18, 13, 3, 20, light);
        d.cuboid(9, 0, 18, 9, 2, 18, light);
        d.cuboid(13, 0, 18, 13, 2, 18, light);
        for x in [9, 13] {
            d.put(x, 6, 20, light);
            d.put(x, 5, 20, lamp);
            d.put(x, 4, 20, light);
        }
        d.cuboid(7, 3, 7, 15, 3, 14, light);
        for x in [10, 12] {
            d.cuboid(x, 0, 10, x, 6, 10, light);
            d.cuboid(x, 0, 12, x, 6, 12, light);
            d.put(x, 0, 10, lamp);
            d.put(x, 0, 12, lamp);
            d.put(x, 4, 10, lamp);
            d.put(x, 4, 12, lamp);
        }
        for x in [8, 14] {
            d.cuboid(x, 0, 7, x, 2, 7, light);
            d.cuboid(x, 0, 14, x, 2, 14, light);
        }
        d.cuboid(8, 3, 8, 8, 3, 13, black);
        d.cuboid(14, 3, 8, 14, 3, 13, black);
    }
}

fn penthouse(d: &mut Draw) {
    let (gray, light, black, lamp) = (d.gray, d.light, d.black, d.lamp);
    d.cuboid(2, -1, 2, 11, -1, 11, light);
    d.cuboid(0, -1, 0, 1, -1, 11, gray);
    d.cuboid(12, -1, 0, 13, -1, 11, gray);
    d.cuboid(2, -1, 0, 11, -1, 1, gray);
    d.cuboid(2, -1, 12, 11, -1, 13, gray);
    d.cuboid(0, 0, 0, 0, 0, 13, light);
    d.cuboid(13, 0, 0, 13, 0, 13, light);
    d.cuboid(1, 0, 0, 12, 0, 0, light);
    d.cuboid(1, 0, 13, 12, 0, 13, light);
    let mut i = 2;
    while i <= 11 {
        d.put(0, 0, i, lamp);
        d.put(13, 0, i, lamp);
        d.put(i, 0, 0, lamp);
        i += 3;
    }
    d.cuboid(2, 0, 3, 4, 0, 9, light);
    d.cuboid(9, 0, 3, 11, 0, 9, light);
    d.cuboid(4, 0, 9, 9, 0, 11, light);
    d.put(5, 0, 8, light);
    d.put(8, 0, 8, light);
    d.put(10, 0, 10, light);
    d.put(3, 0, 10, light);
    d.cuboid(3, 0, 3, 3, 0, 7, black);
    d.cuboid(10, 0, 3, 10, 0, 7, black);
    d.cuboid(6, 0, 10, 7, 0, 10, black);
    for x in [3, 10] {
        let mut z = 2;
        while z <= 8 {
            d.cuboid(x, 0, z, x, 2, z, light);
            z += 3;
        }
    }
    d.cuboid(5, 0, 10, 5, 2, 10, light);
    d.cuboid(8, 0, 10, 8, 2, 10, light);
    d.cuboid(6, -1, 7, 7, -1, 8, black);
    d.water(6, -1, 3, 7, -1, 4);
}

fn simple(d: &mut Draw, design: i32, room: Snap) {
    let (gray, light, black, lamp) = (d.gray, d.light, d.black, d.lamp);
    if room.upper {
        d.floor(0, 0, room.open[D]);
    }
    if room.up_null {
        d.fill_only(1, 4, 1, 6, 4, 6, gray);
    }
    // `WorldgenRandom.nextBoolean` — это `next(1)`, старший бит `nextLong`.
    let pillar = design != 0
        && d.rng.next_bool()
        && !room.open[D]
        && !room.open[U]
        && room.open.iter().filter(|open| **open).count() > 1;
    if design == 0 {
        d.cuboid(0, 1, 0, 2, 1, 2, light);
        d.cuboid(0, 3, 0, 2, 3, 2, light);
        d.cuboid(0, 2, 0, 0, 2, 2, gray);
        d.cuboid(1, 2, 0, 2, 2, 0, gray);
        d.put(1, 2, 1, lamp);
        d.cuboid(5, 1, 0, 7, 1, 2, light);
        d.cuboid(5, 3, 0, 7, 3, 2, light);
        d.cuboid(7, 2, 0, 7, 2, 2, gray);
        d.cuboid(5, 2, 0, 6, 2, 0, gray);
        d.put(6, 2, 1, lamp);
        d.cuboid(0, 1, 5, 2, 1, 7, light);
        d.cuboid(0, 3, 5, 2, 3, 7, light);
        d.cuboid(0, 2, 5, 0, 2, 7, gray);
        d.cuboid(1, 2, 7, 2, 2, 7, gray);
        d.put(1, 2, 6, lamp);
        d.cuboid(5, 1, 5, 7, 1, 7, light);
        d.cuboid(5, 3, 5, 7, 3, 7, light);
        d.cuboid(7, 2, 5, 7, 2, 7, gray);
        d.cuboid(5, 2, 7, 6, 2, 7, gray);
        d.put(6, 2, 6, lamp);
        if room.open[S] {
            d.cuboid(3, 3, 0, 4, 3, 0, light);
        } else {
            d.cuboid(3, 3, 0, 4, 3, 1, light);
            d.cuboid(3, 2, 0, 4, 2, 0, gray);
            d.cuboid(3, 1, 0, 4, 1, 1, light);
        }
        if room.open[N] {
            d.cuboid(3, 3, 7, 4, 3, 7, light);
        } else {
            d.cuboid(3, 3, 6, 4, 3, 7, light);
            d.cuboid(3, 2, 7, 4, 2, 7, gray);
            d.cuboid(3, 1, 6, 4, 1, 7, light);
        }
        if room.open[W] {
            d.cuboid(0, 3, 3, 0, 3, 4, light);
        } else {
            d.cuboid(0, 3, 3, 1, 3, 4, light);
            d.cuboid(0, 2, 3, 0, 2, 4, gray);
            d.cuboid(0, 1, 3, 1, 1, 4, light);
        }
        if room.open[E] {
            d.cuboid(7, 3, 3, 7, 3, 4, light);
        } else {
            d.cuboid(6, 3, 3, 7, 3, 4, light);
            d.cuboid(7, 2, 3, 7, 2, 4, gray);
            d.cuboid(6, 1, 3, 7, 1, 4, light);
        }
    } else if design == 1 {
        d.cuboid(2, 1, 2, 2, 3, 2, light);
        d.cuboid(2, 1, 5, 2, 3, 5, light);
        d.cuboid(5, 1, 5, 5, 3, 5, light);
        d.cuboid(5, 1, 2, 5, 3, 2, light);
        d.put(2, 2, 2, lamp);
        d.put(2, 2, 5, lamp);
        d.put(5, 2, 5, lamp);
        d.put(5, 2, 2, lamp);
        d.cuboid(0, 1, 0, 1, 3, 0, light);
        d.cuboid(0, 1, 1, 0, 3, 1, light);
        d.cuboid(0, 1, 7, 1, 3, 7, light);
        d.cuboid(0, 1, 6, 0, 3, 6, light);
        d.cuboid(6, 1, 7, 7, 3, 7, light);
        d.cuboid(7, 1, 6, 7, 3, 6, light);
        d.cuboid(6, 1, 0, 7, 3, 0, light);
        d.cuboid(7, 1, 1, 7, 3, 1, light);
        d.put(1, 2, 0, gray);
        d.put(0, 2, 1, gray);
        d.put(1, 2, 7, gray);
        d.put(0, 2, 6, gray);
        d.put(6, 2, 7, gray);
        d.put(7, 2, 6, gray);
        d.put(6, 2, 0, gray);
        d.put(7, 2, 1, gray);
        if !room.open[S] {
            d.cuboid(1, 3, 0, 6, 3, 0, light);
            d.cuboid(1, 2, 0, 6, 2, 0, gray);
            d.cuboid(1, 1, 0, 6, 1, 0, light);
        }
        if !room.open[N] {
            d.cuboid(1, 3, 7, 6, 3, 7, light);
            d.cuboid(1, 2, 7, 6, 2, 7, gray);
            d.cuboid(1, 1, 7, 6, 1, 7, light);
        }
        if !room.open[W] {
            d.cuboid(0, 3, 1, 0, 3, 6, light);
            d.cuboid(0, 2, 1, 0, 2, 6, gray);
            d.cuboid(0, 1, 1, 0, 1, 6, light);
        }
        if !room.open[E] {
            d.cuboid(7, 3, 1, 7, 3, 6, light);
            d.cuboid(7, 2, 1, 7, 2, 6, gray);
            d.cuboid(7, 1, 1, 7, 1, 6, light);
        }
    } else if design == 2 {
        d.cuboid(0, 1, 0, 0, 1, 7, light);
        d.cuboid(7, 1, 0, 7, 1, 7, light);
        d.cuboid(1, 1, 0, 6, 1, 0, light);
        d.cuboid(1, 1, 7, 6, 1, 7, light);
        d.cuboid(0, 2, 0, 0, 2, 7, black);
        d.cuboid(7, 2, 0, 7, 2, 7, black);
        d.cuboid(1, 2, 0, 6, 2, 0, black);
        d.cuboid(1, 2, 7, 6, 2, 7, black);
        d.cuboid(0, 3, 0, 0, 3, 7, light);
        d.cuboid(7, 3, 0, 7, 3, 7, light);
        d.cuboid(1, 3, 0, 6, 3, 0, light);
        d.cuboid(1, 3, 7, 6, 3, 7, light);
        d.cuboid(0, 1, 3, 0, 2, 4, black);
        d.cuboid(7, 1, 3, 7, 2, 4, black);
        d.cuboid(3, 1, 0, 4, 2, 0, black);
        d.cuboid(3, 1, 7, 4, 2, 7, black);
        if room.open[S] {
            d.water(3, 1, 0, 4, 2, 0);
        }
        if room.open[N] {
            d.water(3, 1, 7, 4, 2, 7);
        }
        if room.open[W] {
            d.water(0, 1, 3, 0, 2, 4);
        }
        if room.open[E] {
            d.water(7, 1, 3, 7, 2, 4);
        }
    }
    if pillar {
        d.cuboid(3, 1, 3, 4, 1, 4, light);
        d.cuboid(3, 2, 3, 4, 2, 4, gray);
        d.cuboid(3, 3, 3, 4, 3, 4, light);
    }
}

fn simple_top(d: &mut Draw, room: Snap) {
    let (gray, light, black, sponge) = (d.gray, d.light, d.black, d.sponge);
    if room.upper {
        d.floor(0, 0, room.open[D]);
    }
    if room.up_null {
        d.fill_only(1, 4, 1, 6, 4, 6, gray);
    }
    for x in 1..=6 {
        for z in 1..=6 {
            if d.rng.next_int_bound(3) == 0 {
                continue;
            }
            let y0 = 2 + if d.rng.next_int_bound(4) == 0 { 0 } else { 1 };
            d.cuboid(x, y0, z, x, 3, z, sponge);
        }
    }
    d.cuboid(0, 1, 0, 0, 1, 7, light);
    d.cuboid(7, 1, 0, 7, 1, 7, light);
    d.cuboid(1, 1, 0, 6, 1, 0, light);
    d.cuboid(1, 1, 7, 6, 1, 7, light);
    d.cuboid(0, 2, 0, 0, 2, 7, black);
    d.cuboid(7, 2, 0, 7, 2, 7, black);
    d.cuboid(1, 2, 0, 6, 2, 0, black);
    d.cuboid(1, 2, 7, 6, 2, 7, black);
    d.cuboid(0, 3, 0, 0, 3, 7, light);
    d.cuboid(7, 3, 0, 7, 3, 7, light);
    d.cuboid(1, 3, 0, 6, 3, 0, light);
    d.cuboid(1, 3, 7, 6, 3, 7, light);
    d.cuboid(0, 1, 3, 0, 2, 4, black);
    d.cuboid(7, 1, 3, 7, 2, 4, black);
    d.cuboid(3, 1, 0, 4, 2, 0, black);
    d.cuboid(3, 1, 7, 4, 2, 7, black);
    if room.open[S] {
        d.water(3, 1, 0, 4, 2, 0);
    }
}

fn double_yz(d: &mut Draw, south: Snap, north: Snap, south_up: Snap, north_up: Snap) {
    let (gray, light, black, lamp) = (d.gray, d.light, d.black, d.lamp);
    if south.upper {
        d.floor(0, 8, north.open[D]);
        d.floor(0, 0, south.open[D]);
    }
    if south_up.up_null {
        d.fill_only(1, 8, 1, 6, 8, 7, gray);
    }
    if north_up.up_null {
        d.fill_only(1, 8, 8, 6, 8, 14, gray);
    }
    for y in 1..=7 {
        let block = if y == 2 || y == 6 { gray } else { light };
        d.cuboid(0, y, 0, 0, y, 15, block);
        d.cuboid(7, y, 0, 7, y, 15, block);
        d.cuboid(1, y, 0, 6, y, 0, block);
        d.cuboid(1, y, 15, 6, y, 15, block);
    }
    for y in 1..=7 {
        let block = if y == 2 || y == 6 { lamp } else { black };
        d.cuboid(3, y, 7, 4, y, 8, block);
    }
    if south.open[S] {
        d.water(3, 1, 0, 4, 2, 0);
    }
    if south.open[E] {
        d.water(7, 1, 3, 7, 2, 4);
    }
    if south.open[W] {
        d.water(0, 1, 3, 0, 2, 4);
    }
    if north.open[N] {
        d.water(3, 1, 15, 4, 2, 15);
    }
    if north.open[W] {
        d.water(0, 1, 11, 0, 2, 12);
    }
    if north.open[E] {
        d.water(7, 1, 11, 7, 2, 12);
    }
    if south_up.open[S] {
        d.water(3, 5, 0, 4, 6, 0);
    }
    if south_up.open[E] {
        d.water(7, 5, 3, 7, 6, 4);
        d.cuboid(5, 4, 2, 6, 4, 5, light);
        d.cuboid(6, 1, 2, 6, 3, 2, light);
        d.cuboid(6, 1, 5, 6, 3, 5, light);
    }
    if south_up.open[W] {
        d.water(0, 5, 3, 0, 6, 4);
        d.cuboid(1, 4, 2, 2, 4, 5, light);
        d.cuboid(1, 1, 2, 1, 3, 2, light);
        d.cuboid(1, 1, 5, 1, 3, 5, light);
    }
    if north_up.open[N] {
        d.water(3, 5, 15, 4, 6, 15);
    }
    if north_up.open[W] {
        d.water(0, 5, 11, 0, 6, 12);
        d.cuboid(1, 4, 10, 2, 4, 13, light);
        d.cuboid(1, 1, 10, 1, 3, 10, light);
        d.cuboid(1, 1, 13, 1, 3, 13, light);
    }
    if north_up.open[E] {
        d.water(7, 5, 11, 7, 6, 12);
        d.cuboid(5, 4, 10, 6, 4, 13, light);
        d.cuboid(6, 1, 10, 6, 3, 10, light);
        d.cuboid(6, 1, 13, 6, 3, 13, light);
    }
}

fn double_xy(d: &mut Draw, west: Snap, east: Snap, west_up: Snap, east_up: Snap) {
    let (gray, light, lamp) = (d.gray, d.light, d.lamp);
    if west.upper {
        d.floor(8, 0, east.open[D]);
        d.floor(0, 0, west.open[D]);
    }
    if west_up.up_null {
        d.fill_only(1, 8, 1, 7, 8, 6, gray);
    }
    if east_up.up_null {
        d.fill_only(8, 8, 1, 14, 8, 6, gray);
    }
    for y in 1..=7 {
        let block = if y == 2 || y == 6 { gray } else { light };
        d.cuboid(0, y, 0, 0, y, 7, block);
        d.cuboid(15, y, 0, 15, y, 7, block);
        d.cuboid(1, y, 0, 15, y, 0, block);
        d.cuboid(1, y, 7, 14, y, 7, block);
    }
    d.cuboid(2, 1, 3, 2, 7, 4, light);
    d.cuboid(3, 1, 2, 4, 7, 2, light);
    d.cuboid(3, 1, 5, 4, 7, 5, light);
    d.cuboid(13, 1, 3, 13, 7, 4, light);
    d.cuboid(11, 1, 2, 12, 7, 2, light);
    d.cuboid(11, 1, 5, 12, 7, 5, light);
    d.cuboid(5, 1, 3, 5, 3, 4, light);
    d.cuboid(10, 1, 3, 10, 3, 4, light);
    d.cuboid(5, 7, 2, 10, 7, 5, light);
    d.cuboid(5, 5, 2, 5, 7, 2, light);
    d.cuboid(10, 5, 2, 10, 7, 2, light);
    d.cuboid(5, 5, 5, 5, 7, 5, light);
    d.cuboid(10, 5, 5, 10, 7, 5, light);
    d.put(6, 6, 2, light);
    d.put(9, 6, 2, light);
    d.put(6, 6, 5, light);
    d.put(9, 6, 5, light);
    d.cuboid(5, 4, 3, 6, 4, 4, light);
    d.cuboid(9, 4, 3, 10, 4, 4, light);
    d.put(5, 4, 2, lamp);
    d.put(5, 4, 5, lamp);
    d.put(10, 4, 2, lamp);
    d.put(10, 4, 5, lamp);
    if west.open[S] {
        d.water(3, 1, 0, 4, 2, 0);
    }
    if west.open[N] {
        d.water(3, 1, 7, 4, 2, 7);
    }
    if west.open[W] {
        d.water(0, 1, 3, 0, 2, 4);
    }
    if east.open[S] {
        d.water(11, 1, 0, 12, 2, 0);
    }
    if east.open[N] {
        d.water(11, 1, 7, 12, 2, 7);
    }
    if east.open[E] {
        d.water(15, 1, 3, 15, 2, 4);
    }
    if west_up.open[S] {
        d.water(3, 5, 0, 4, 6, 0);
    }
    if west_up.open[N] {
        d.water(3, 5, 7, 4, 6, 7);
    }
    if west_up.open[W] {
        d.water(0, 5, 3, 0, 6, 4);
    }
    if east_up.open[S] {
        d.water(11, 5, 0, 12, 6, 0);
    }
    if east_up.open[N] {
        d.water(11, 5, 7, 12, 6, 7);
    }
    if east_up.open[E] {
        d.water(15, 5, 3, 15, 6, 4);
    }
}

fn double_z(d: &mut Draw, south: Snap, north: Snap) {
    let (gray, light, lamp) = (d.gray, d.light, d.lamp);
    if south.upper {
        d.floor(0, 8, north.open[D]);
        d.floor(0, 0, south.open[D]);
    }
    if south.up_null {
        d.fill_only(1, 4, 1, 6, 4, 7, gray);
    }
    if north.up_null {
        d.fill_only(1, 4, 8, 6, 4, 14, gray);
    }
    d.cuboid(0, 3, 0, 0, 3, 15, light);
    d.cuboid(7, 3, 0, 7, 3, 15, light);
    d.cuboid(1, 3, 0, 7, 3, 0, light);
    d.cuboid(1, 3, 15, 6, 3, 15, light);
    d.cuboid(0, 2, 0, 0, 2, 15, gray);
    d.cuboid(7, 2, 0, 7, 2, 15, gray);
    d.cuboid(1, 2, 0, 7, 2, 0, gray);
    d.cuboid(1, 2, 15, 6, 2, 15, gray);
    d.cuboid(0, 1, 0, 0, 1, 15, light);
    d.cuboid(7, 1, 0, 7, 1, 15, light);
    d.cuboid(1, 1, 0, 7, 1, 0, light);
    d.cuboid(1, 1, 15, 6, 1, 15, light);
    d.cuboid(1, 1, 1, 1, 1, 2, light);
    d.cuboid(6, 1, 1, 6, 1, 2, light);
    d.cuboid(1, 3, 1, 1, 3, 2, light);
    d.cuboid(6, 3, 1, 6, 3, 2, light);
    d.cuboid(1, 1, 13, 1, 1, 14, light);
    d.cuboid(6, 1, 13, 6, 1, 14, light);
    d.cuboid(1, 3, 13, 1, 3, 14, light);
    d.cuboid(6, 3, 13, 6, 3, 14, light);
    d.cuboid(2, 1, 6, 2, 3, 6, light);
    d.cuboid(5, 1, 6, 5, 3, 6, light);
    d.cuboid(2, 1, 9, 2, 3, 9, light);
    d.cuboid(5, 1, 9, 5, 3, 9, light);
    d.cuboid(3, 2, 6, 4, 2, 6, light);
    d.cuboid(3, 2, 9, 4, 2, 9, light);
    d.cuboid(2, 2, 7, 2, 2, 8, light);
    d.cuboid(5, 2, 7, 5, 2, 8, light);
    d.put(2, 2, 5, lamp);
    d.put(5, 2, 5, lamp);
    d.put(2, 2, 10, lamp);
    d.put(5, 2, 10, lamp);
    d.put(2, 3, 5, light);
    d.put(5, 3, 5, light);
    d.put(2, 3, 10, light);
    d.put(5, 3, 10, light);
    if south.open[S] {
        d.water(3, 1, 0, 4, 2, 0);
    }
    if south.open[E] {
        d.water(7, 1, 3, 7, 2, 4);
    }
    if south.open[W] {
        d.water(0, 1, 3, 0, 2, 4);
    }
    if north.open[N] {
        d.water(3, 1, 15, 4, 2, 15);
    }
    if north.open[W] {
        d.water(0, 1, 11, 0, 2, 12);
    }
    if north.open[E] {
        d.water(7, 1, 11, 7, 2, 12);
    }
}

fn double_x(d: &mut Draw, west: Snap, east: Snap) {
    let (gray, light, lamp) = (d.gray, d.light, d.lamp);
    if west.upper {
        d.floor(8, 0, east.open[D]);
        d.floor(0, 0, west.open[D]);
    }
    if west.up_null {
        d.fill_only(1, 4, 1, 7, 4, 6, gray);
    }
    if east.up_null {
        d.fill_only(8, 4, 1, 14, 4, 6, gray);
    }
    d.cuboid(0, 3, 0, 0, 3, 7, light);
    d.cuboid(15, 3, 0, 15, 3, 7, light);
    d.cuboid(1, 3, 0, 15, 3, 0, light);
    d.cuboid(1, 3, 7, 14, 3, 7, light);
    d.cuboid(0, 2, 0, 0, 2, 7, gray);
    d.cuboid(15, 2, 0, 15, 2, 7, gray);
    d.cuboid(1, 2, 0, 15, 2, 0, gray);
    d.cuboid(1, 2, 7, 14, 2, 7, gray);
    d.cuboid(0, 1, 0, 0, 1, 7, light);
    d.cuboid(15, 1, 0, 15, 1, 7, light);
    d.cuboid(1, 1, 0, 15, 1, 0, light);
    d.cuboid(1, 1, 7, 14, 1, 7, light);
    d.cuboid(5, 1, 0, 10, 1, 4, light);
    d.cuboid(6, 2, 0, 9, 2, 3, gray);
    d.cuboid(5, 3, 0, 10, 3, 4, light);
    d.put(6, 2, 3, lamp);
    d.put(9, 2, 3, lamp);
    if west.open[S] {
        d.water(3, 1, 0, 4, 2, 0);
    }
    if west.open[N] {
        d.water(3, 1, 7, 4, 2, 7);
    }
    if west.open[W] {
        d.water(0, 1, 3, 0, 2, 4);
    }
    if east.open[S] {
        d.water(11, 1, 0, 12, 2, 0);
    }
    if east.open[N] {
        d.water(11, 1, 7, 12, 2, 7);
    }
    if east.open[E] {
        d.water(15, 1, 3, 15, 2, 4);
    }
}

fn double_y(d: &mut Draw, low: Snap, high: Snap) {
    let (gray, light) = (d.gray, d.light);
    if low.upper {
        d.floor(0, 0, low.open[D]);
    }
    if high.up_null {
        d.fill_only(1, 8, 1, 6, 8, 6, gray);
    }
    d.cuboid(0, 4, 0, 0, 4, 7, light);
    d.cuboid(7, 4, 0, 7, 4, 7, light);
    d.cuboid(1, 4, 0, 6, 4, 0, light);
    d.cuboid(1, 4, 7, 6, 4, 7, light);
    d.cuboid(2, 4, 1, 2, 4, 2, light);
    d.cuboid(1, 4, 2, 1, 4, 2, light);
    d.cuboid(5, 4, 1, 5, 4, 2, light);
    d.cuboid(6, 4, 2, 6, 4, 2, light);
    d.cuboid(2, 4, 5, 2, 4, 6, light);
    d.cuboid(1, 4, 5, 1, 4, 5, light);
    d.cuboid(5, 4, 5, 5, 4, 6, light);
    d.cuboid(6, 4, 5, 6, 4, 5, light);
    for (y, room) in [(1, low), (5, high)] {
        let mut z = 0;
        if room.open[S] {
            d.cuboid(2, y, z, 2, y + 2, z, light);
            d.cuboid(5, y, z, 5, y + 2, z, light);
            d.cuboid(3, y + 2, z, 4, y + 2, z, light);
        } else {
            d.cuboid(0, y, z, 7, y + 2, z, light);
            d.cuboid(0, y + 1, z, 7, y + 1, z, gray);
        }
        z = 7;
        if room.open[N] {
            d.cuboid(2, y, z, 2, y + 2, z, light);
            d.cuboid(5, y, z, 5, y + 2, z, light);
            d.cuboid(3, y + 2, z, 4, y + 2, z, light);
        } else {
            d.cuboid(0, y, z, 7, y + 2, z, light);
            d.cuboid(0, y + 1, z, 7, y + 1, z, gray);
        }
        let mut x = 0;
        if room.open[W] {
            d.cuboid(x, y, 2, x, y + 2, 2, light);
            d.cuboid(x, y, 5, x, y + 2, 5, light);
            d.cuboid(x, y + 2, 3, x, y + 2, 4, light);
        } else {
            d.cuboid(x, y, 0, x, y + 2, 7, light);
            d.cuboid(x, y + 1, 0, x, y + 1, 7, gray);
        }
        x = 7;
        if room.open[E] {
            d.cuboid(x, y, 2, x, y + 2, 2, light);
            d.cuboid(x, y, 5, x, y + 2, 5, light);
            d.cuboid(x, y + 2, 3, x, y + 2, 4, light);
        } else {
            d.cuboid(x, y, 0, x, y + 2, 7, light);
            d.cuboid(x, y + 1, 0, x, y + 1, 7, gray);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Bits;
    use super::*;

    #[test]
    fn wings_of_one_roll_differ() {
        let mut rng = Legacy::new(0);
        rng.set_large_feature_seed(12345, 0, 0);
        let pieces = build(&mut rng, 0, 0);
        let n = pieces.len();
        assert!(n > 4, "{n}");
        match (
            &pieces[n - 3].body,
            &pieces[n - 2].body,
            &pieces[n - 1].body,
        ) {
            (Body::Wing(a), Body::Wing(b), Body::Penthouse) => assert_eq!(*a ^ *b, 1),
            _ => panic!("последние куски — два крыла и пентхаус"),
        }
        assert!(matches!(pieces[0].body, Body::Shell));
        for piece in &pieces {
            assert!(piece.min[0] <= piece.max[0]);
            assert!(piece.min[1] <= piece.max[1]);
            assert!(piece.min[2] <= piece.max[2]);
        }
    }

    /// Сид 12345: чанк старта монумента. Без `CHUNKWAKE_DATA` пропускается.
    #[test]
    fn seed_12345_monument_starts() {
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
            .unwrap();
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
            .locate_structure("minecraft:monument", [0, 64, 0])
            .expect("монумент");
        eprintln!("monument {pos:?}");
        assert_eq!(pos[0].rem_euclid(16), 8);
        assert_eq!(pos[2].rem_euclid(16), 8);
    }
}
