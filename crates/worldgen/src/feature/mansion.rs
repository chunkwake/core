//! Лесной особняк: сетка-лабиринт `MansionGrid`, размещение кусков
//! `MansionPiecePlacer` и постаменты под ним (`afterPlace`).
//!
//! Особняк целиком из шаблонов `.nbt`, часть кусков ставится с зеркалом.
//! Сундуки остаются блоками, лут не пишется; мобы из маркеров не создаются.

use super::template::{self, Bounds, Rot, Template};
use super::{Bits, Legacy, Level, Palette};
use crate::templates::Mirror;
use Dir::{East, North, South, West};
use std::collections::HashMap;
use std::sync::Arc;

/// Сторона света сетки. Индексы совпадают с порядком `Direction` в ванили.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Dir {
    North,
    East,
    South,
    West,
}

impl Dir {
    fn step(self) -> [i32; 2] {
        match self {
            North => [0, -1],
            East => [1, 0],
            South => [0, 1],
            West => [-1, 0],
        }
    }

    fn opposite(self) -> Self {
        match self {
            North => South,
            East => West,
            South => North,
            West => East,
        }
    }

    fn clockwise(self) -> Self {
        match self {
            North => East,
            East => South,
            South => West,
            West => North,
        }
    }

    fn counter_clockwise(self) -> Self {
        match self {
            North => West,
            West => South,
            South => East,
            East => North,
        }
    }

    /// `Direction.from2DDataValue`: `nextInt(4)` даёт юг, запад, север, восток.
    fn from_2d(value: i32) -> Self {
        match value & 3 {
            0 => South,
            1 => West,
            2 => North,
            _ => East,
        }
    }
    /// `Rotation.rotate(Direction)`.
    fn rotated(self, rot: Rot) -> Self {
        match rot {
            Rot::None => self,
            Rot::Cw90 => self.clockwise(),
            Rot::Cw180 => self.opposite(),
            Rot::Ccw90 => self.counter_clockwise(),
        }
    }
}

/// `SimpleGrid`: значение за границей — `outside`.
#[derive(Clone)]
struct Grid {
    cells: Vec<i32>,
    width: i32,
    height: i32,
    outside: i32,
}

impl Grid {
    fn new(width: i32, height: i32, outside: i32) -> Self {
        Self {
            cells: vec![0; (width * height) as usize],
            width,
            height,
            outside,
        }
    }

    fn get(&self, x: i32, y: i32) -> i32 {
        if x < 0 || x >= self.width || y < 0 || y >= self.height {
            return self.outside;
        }
        self.cells[(y * self.width + x) as usize]
    }

    fn set(&mut self, x: i32, y: i32, value: i32) {
        if x >= 0 && x < self.width && y >= 0 && y < self.height {
            self.cells[(y * self.width + x) as usize] = value;
        }
    }

    fn fill(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, value: i32) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.set(x, y, value);
            }
        }
    }

    fn set_if(&mut self, x: i32, y: i32, want: i32, value: i32) {
        if self.get(x, y) == want {
            self.set(x, y, value);
        }
    }

    fn edges_to(&self, x: i32, y: i32, want: i32) -> bool {
        self.get(x - 1, y) == want
            || self.get(x + 1, y) == want
            || self.get(x, y + 1) == want
            || self.get(x, y - 1) == want
    }

    fn is_house(&self, x: i32, y: i32) -> bool {
        matches!(self.get(x, y), 1..=4)
    }
}

const CLEAR: i32 = 0;
const CORRIDOR: i32 = 1;
const ROOM: i32 = 2;
const START_ROOM: i32 = 3;
const BLOCKED: i32 = 5;
const ROOM_1X1: i32 = 65536;
const ROOM_1X2: i32 = 131072;
const ROOM_2X2: i32 = 262144;
const ROOM_ORIGIN_FLAG: i32 = 0x100000;
const ROOM_DOOR_FLAG: i32 = 0x200000;
const ROOM_STAIRS_FLAG: i32 = 0x400000;
const ROOM_CORRIDOR_FLAG: i32 = 0x800000;
const ROOM_TYPE_MASK: i32 = 0xF0000;
const ROOM_ID_MASK: i32 = 65535;

const GRID_SIZE: i32 = 11;
const ENTRANCE_X: i32 = 7;
const ENTRANCE_Y: i32 = 4;
const START_OFFSET: i32 = 7;

struct MansionGrid {
    base: Grid,
    third: Grid,
    rooms: [Grid; 3],
}

impl MansionGrid {
    fn new(rng: &mut Legacy) -> Self {
        let mut base = Grid::new(GRID_SIZE, GRID_SIZE, BLOCKED);
        base.fill(
            ENTRANCE_X,
            ENTRANCE_Y,
            ENTRANCE_X + 1,
            ENTRANCE_Y + 1,
            START_ROOM,
        );
        base.fill(
            ENTRANCE_X - 1,
            ENTRANCE_Y,
            ENTRANCE_X - 1,
            ENTRANCE_Y + 1,
            ROOM,
        );
        base.fill(
            ENTRANCE_X + 2,
            ENTRANCE_Y - 2,
            ENTRANCE_X + 3,
            ENTRANCE_Y + 3,
            BLOCKED,
        );
        base.fill(
            ENTRANCE_X + 1,
            ENTRANCE_Y - 2,
            ENTRANCE_X + 1,
            ENTRANCE_Y - 1,
            CORRIDOR,
        );
        base.fill(
            ENTRANCE_X + 1,
            ENTRANCE_Y + 2,
            ENTRANCE_X + 1,
            ENTRANCE_Y + 3,
            CORRIDOR,
        );
        base.set(ENTRANCE_X - 1, ENTRANCE_Y - 1, CORRIDOR);
        base.set(ENTRANCE_X - 1, ENTRANCE_Y + 2, CORRIDOR);
        base.fill(0, 0, 11, 1, BLOCKED);
        base.fill(0, 9, 11, 11, BLOCKED);
        corridor(&mut base, rng, ENTRANCE_X, ENTRANCE_Y - 2, West, 6);
        corridor(&mut base, rng, ENTRANCE_X, ENTRANCE_Y + 3, West, 6);
        corridor(&mut base, rng, ENTRANCE_X - 2, ENTRANCE_Y - 1, West, 3);
        corridor(&mut base, rng, ENTRANCE_X - 2, ENTRANCE_Y + 2, West, 3);
        while clean_edges(&mut base) {}

        let mut rooms = [
            Grid::new(GRID_SIZE, GRID_SIZE, BLOCKED),
            Grid::new(GRID_SIZE, GRID_SIZE, BLOCKED),
            Grid::new(GRID_SIZE, GRID_SIZE, BLOCKED),
        ];
        identify_rooms(&base, &mut rooms[0], rng);
        identify_rooms(&base, &mut rooms[1], rng);
        rooms[0].fill(
            ENTRANCE_X + 1,
            ENTRANCE_Y,
            ENTRANCE_X + 1,
            ENTRANCE_Y + 1,
            ROOM_CORRIDOR_FLAG,
        );
        rooms[1].fill(
            ENTRANCE_X + 1,
            ENTRANCE_Y,
            ENTRANCE_X + 1,
            ENTRANCE_Y + 1,
            ROOM_CORRIDOR_FLAG,
        );

        let mut third = Grid::new(base.width, base.height, BLOCKED);
        // setupThirdFloor смотрит во второй этаж и пишет флаг в третий,
        // затем identifyRooms доразмечает loorRooms[2].
        let mut third_rooms = Grid::new(third.width, third.height, BLOCKED);
        setup_third_floor(&base, &mut third, &mut rooms[1], &mut third_rooms, rng);
        identify_rooms(&third, &mut third_rooms, rng);
        rooms[2] = third_rooms;
        Self { base, third, rooms }
    }

    fn is_room_id(&self, x: i32, y: i32, floor: usize, room_id: i32) -> bool {
        self.rooms[floor].get(x, y) & ROOM_ID_MASK == room_id
    }

    fn room_direction(
        &self,
        _grid: &Grid,
        x: i32,
        y: i32,
        floor: usize,
        room_id: i32,
    ) -> Option<Dir> {
        for dir in [North, East, South, West] {
            let [dx, dz] = dir.step();
            if self.is_room_id(x + dx, y + dz, floor, room_id) {
                return Some(dir);
            }
        }
        None
    }
}

/// `MansionGrid.recursiveCorridor`.
fn corridor(grid: &mut Grid, rng: &mut Legacy, x: i32, y: i32, heading: Dir, depth: i32) {
    if depth <= 0 {
        return;
    }
    let [hx, hz] = heading.step();
    grid.set(x, y, CORRIDOR);
    grid.set_if(x + hx, y + hz, CLEAR, CORRIDOR);
    for _ in 0..8 {
        let next = Dir::from_2d(rng.next_int_bound(4));
        // Восток отбрасывается с шансом пополам: так сетка меньше вытянута.
        if next == heading.opposite() || (next == East && rng.next_bool()) {
            continue;
        }
        let [nx, nz] = next.step();
        if grid.get(x + hx + nx, y + hz + nz) != CLEAR
            || grid.get(x + hx + nx * 2, y + hz + nz * 2) != CLEAR
        {
            continue;
        }
        corridor(grid, rng, x + hx + nx, y + hz + nz, next, depth - 1);
        break;
    }
    let cw = heading.clockwise().step();
    let ccw = heading.counter_clockwise().step();
    grid.set_if(x + cw[0], y + cw[1], CLEAR, ROOM);
    grid.set_if(x + ccw[0], y + ccw[1], CLEAR, ROOM);
    grid.set_if(x + hx + cw[0], y + hz + cw[1], CLEAR, ROOM);
    grid.set_if(x + hx + ccw[0], y + hz + ccw[1], CLEAR, ROOM);
    grid.set_if(x + hx * 2, y + hz * 2, CLEAR, ROOM);
    grid.set_if(x + cw[0] * 2, y + cw[1] * 2, CLEAR, ROOM);
    grid.set_if(x + ccw[0] * 2, y + ccw[1] * 2, CLEAR, ROOM);
}

fn clean_edges(grid: &mut Grid) -> bool {
    let mut touched = false;
    for y in 0..grid.height {
        for x in 0..grid.width {
            if grid.get(x, y) != CLEAR {
                continue;
            }
            let mut direct = 0;
            direct += i32::from(grid.is_house(x + 1, y));
            direct += i32::from(grid.is_house(x - 1, y));
            direct += i32::from(grid.is_house(x, y + 1));
            direct += i32::from(grid.is_house(x, y - 1));
            if direct >= 3 {
                grid.set(x, y, ROOM);
                touched = true;
                continue;
            }
            if direct != 2 {
                continue;
            }
            let mut diagonal = 0;
            diagonal += i32::from(grid.is_house(x + 1, y + 1));
            diagonal += i32::from(grid.is_house(x - 1, y + 1));
            diagonal += i32::from(grid.is_house(x + 1, y - 1));
            diagonal += i32::from(grid.is_house(x - 1, y - 1));
            if diagonal > 1 {
                continue;
            }
            grid.set(x, y, ROOM);
            touched = true;
        }
    }
    touched
}

fn setup_third_floor(
    base: &Grid,
    third: &mut Grid,
    rooms: &mut Grid,
    third_rooms: &mut Grid,
    rng: &mut Legacy,
) {
    let mut candidates = Vec::new();
    for y in 0..third.height {
        for x in 0..third.width {
            let data = rooms.get(x, y);
            if data & ROOM_TYPE_MASK == ROOM_1X2 && data & ROOM_DOOR_FLAG == ROOM_DOOR_FLAG {
                candidates.push((x, y));
            }
        }
    }
    if candidates.is_empty() {
        third.fill(0, 0, third.width, third.height, BLOCKED);
        return;
    }
    let (rx, ry) = candidates[rng.next_int_bound(candidates.len() as i32) as usize];
    let data = rooms.get(rx, ry);
    rooms.set(rx, ry, data | ROOM_STAIRS_FLAG);
    // get1x2RoomDirection(this.baseGrid, roomPos.x, roomPos.y, 1, roomId):
    // сосед ищется в loorRooms[1], а не в aseGrid.
    let dir = room_direction_of(base, rooms, rx, ry, data & ROOM_ID_MASK);
    let [dx, dz] = dir.step();
    let (ex, ey) = (rx + dx, ry + dz);
    for y in 0..third.height {
        for x in 0..third.width {
            if !base.is_house(x, y) {
                third.set(x, y, BLOCKED);
                continue;
            }
            if x == rx && y == ry {
                third.set(x, y, START_ROOM);
                continue;
            }
            if x == ex && y == ey {
                third.set(x, y, START_ROOM);
                // 	his.floorRooms[2].set(x, y, 0x800000) — флаг этажа, не второй.
                third_rooms.set(x, y, ROOM_CORRIDOR_FLAG);
            }
        }
    }
    let mut options = Vec::new();
    for dir in [North, East, South, West] {
        let [sx, sz] = dir.step();
        if third.get(ex + sx, ey + sz) == CLEAR {
            options.push(dir);
        }
    }
    if options.is_empty() {
        third.fill(0, 0, third.width, third.height, BLOCKED);
        rooms.set(rx, ry, data);
        return;
    }
    let dir = options[rng.next_int_bound(options.len() as i32) as usize];
    let [sx, sz] = dir.step();
    corridor(third, rng, ex + sx, ey + sz, dir, 4);
    while clean_edges(third) {}
}

fn room_direction_of(_base: &Grid, rooms: &Grid, x: i32, y: i32, room_id: i32) -> Dir {
    for dir in [North, East, South, West] {
        let [dx, dz] = dir.step();
        if rooms.get(x + dx, y + dz) & ROOM_ID_MASK == room_id {
            return dir;
        }
    }
    // `get1x2RoomDirection` не возвращает `null`: у комнаты 1x2 сосед есть.
    South
}

fn identify_rooms(from: &Grid, rooms: &mut Grid, rng: &mut Legacy) {
    let mut positions = Vec::new();
    for y in 0..from.height {
        for x in 0..from.width {
            if from.get(x, y) == ROOM {
                positions.push((x, y));
            }
        }
    }
    template::shuffle(&mut positions, rng);
    let mut room_id = 10;
    for (x, y) in positions {
        if rooms.get(x, y) != 0 {
            continue;
        }
        let (mut x0, mut x1, mut y0, mut y1) = (x, x, y, y);
        let mut kind = ROOM_1X1;
        if rooms.get(x + 1, y) == 0
            && rooms.get(x, y + 1) == 0
            && rooms.get(x + 1, y + 1) == 0
            && from.get(x + 1, y) == ROOM
            && from.get(x, y + 1) == ROOM
            && from.get(x + 1, y + 1) == ROOM
        {
            x1 += 1;
            y1 += 1;
            kind = ROOM_2X2;
        } else if rooms.get(x - 1, y) == 0
            && rooms.get(x, y + 1) == 0
            && rooms.get(x - 1, y + 1) == 0
            && from.get(x - 1, y) == ROOM
            && from.get(x, y + 1) == ROOM
            && from.get(x - 1, y + 1) == ROOM
        {
            x0 -= 1;
            y1 += 1;
            kind = ROOM_2X2;
        } else if rooms.get(x - 1, y) == 0
            && rooms.get(x, y - 1) == 0
            && rooms.get(x - 1, y - 1) == 0
            && from.get(x - 1, y) == ROOM
            && from.get(x, y - 1) == ROOM
            && from.get(x - 1, y - 1) == ROOM
        {
            x0 -= 1;
            y0 -= 1;
            kind = ROOM_2X2;
        } else if rooms.get(x + 1, y) == 0 && from.get(x + 1, y) == ROOM {
            x1 += 1;
            kind = ROOM_1X2;
        } else if rooms.get(x, y + 1) == 0 && from.get(x, y + 1) == ROOM {
            y1 += 1;
            kind = ROOM_1X2;
        } else if rooms.get(x - 1, y) == 0 && from.get(x - 1, y) == ROOM {
            x0 -= 1;
            kind = ROOM_1X2;
        } else if rooms.get(x, y - 1) == 0 && from.get(x, y - 1) == ROOM {
            y0 -= 1;
            kind = ROOM_1X2;
        }
        let (mut door_x, mut door_y);
        door_x = if rng.next_bool() { x0 } else { x1 };
        door_y = if rng.next_bool() { y0 } else { y1 };
        let mut door_flag = ROOM_DOOR_FLAG;
        if !from.edges_to(door_x, door_y, CORRIDOR) {
            door_x = if door_x == x0 { x1 } else { x0 };
            door_y = if door_y == y0 { y1 } else { y0 };
            if !from.edges_to(door_x, door_y, CORRIDOR) {
                door_y = if door_y == y0 { y1 } else { y0 };
                if !from.edges_to(door_x, door_y, CORRIDOR) {
                    door_x = if door_x == x0 { x1 } else { x0 };
                    door_y = if door_y == y0 { y1 } else { y0 };
                    if !from.edges_to(door_x, door_y, CORRIDOR) {
                        door_flag = 0;
                        door_x = x0;
                        door_y = y0;
                    }
                }
            }
        }
        for ry in y0..=y1 {
            for rx in x0..=x1 {
                if rx == door_x && ry == door_y {
                    rooms.set(rx, ry, ROOM_ORIGIN_FLAG | door_flag | kind | room_id);
                } else {
                    rooms.set(rx, ry, kind | room_id);
                }
            }
        }
        room_id += 1;
    }
}

/// Один кусок особняка: шаблон, мировое начало, зеркало и поворот.
#[derive(Clone, Debug)]
pub(crate) struct Piece {
    /// Имя шаблона: нужно сверке с NBT старта в тесте.
    #[cfg_attr(not(test), allow(dead_code))]
    pub name: String,
    pub template: Arc<Template>,
    pub origin: [i32; 3],
    pub mirror: Mirror,
    pub rot: Rot,
    pub min: [i32; 3],
    pub max: [i32; 3],
}

impl Piece {
    pub(crate) fn bounds(&self) -> ([i32; 3], [i32; 3]) {
        (self.min, self.max)
    }

    /// Маркеры: `Chest*` дают сундук, остальные — мобы, которых не пишем.
    pub(crate) fn place(
        &self,
        pal: &Palette,
        level: &mut dyn Level,
        r: &mut super::Worldgen,
        window: Bounds,
    ) {
        let markers = template::place_mirrored(
            pal,
            level,
            &self.template,
            self.origin,
            self.mirror,
            self.rot,
            window,
            r,
            false,
        );
        for (pos, meta) in markers {
            let Some(meta) = meta else { continue };
            if !meta.starts_with("Chest") || !window.contains(pos) {
                continue;
            }
            if !level.can_write(pos[0], pos[2]) {
                continue;
            }
            let facing = match meta.as_str() {
                "ChestWest" => Some(West),
                "ChestEast" => Some(East),
                "ChestSouth" => Some(South),
                "ChestNorth" => Some(North),
                _ => None,
            };
            let mut state = pal.defaults["minecraft:chest"];
            if let Some(dir) = facing {
                let dir = dir.rotated(self.rot);
                let name = match dir {
                    North => "north",
                    East => "east",
                    South => "south",
                    West => "west",
                };
                if let Some(next) = pal.with_prop(state, "facing", name) {
                    state = next;
                }
            }
            level.set(pos[0], pos[1], pos[2], state);
            // `createChest(..., BuiltInLootTables.WOODLAND_MANSION)`: свой
            // `nextLong` на маркер, помимо съеденного за шаблонный NBT.
            let seed = r.next_long();
            template::set_loot(level, pos, "minecraft:chests/woodland_mansion", seed);
        }
    }
}

/// Куски особняка: порядок и содержимое как у `MansionPiecePlacer`.
pub(crate) fn build(
    templates: &HashMap<String, Arc<Template>>,
    sx: i32,
    sz: i32,
    start_y: i32,
    rot: Rot,
    rng: &mut Legacy,
) -> Vec<Piece> {
    let grid = MansionGrid::new(rng);
    let mut placer = Placer {
        templates,
        rng,
        start_x: ENTRANCE_X + 1,
        start_y: ENTRANCE_Y + 1,
        out: Vec::new(),
    };
    placer.create(
        [sx * 16 + START_OFFSET, start_y, sz * 16 + START_OFFSET],
        rot,
        &grid,
    );
    placer.out
}

struct Placer<'a> {
    templates: &'a HashMap<String, Arc<Template>>,
    rng: &'a mut Legacy,
    start_x: i32,
    start_y: i32,
    out: Vec<Piece>,
}

/// Куда ставить следующий кусок стены: позиция и текущий поворот.
#[derive(Clone, Copy)]
struct Data {
    position: [i32; 3],
    rot: Rot,
}

impl Placer<'_> {
    fn add(&mut self, name: &str, origin: [i32; 3], rot: Rot, mirror: Mirror) {
        let Some(template) = self.templates.get(name) else {
            return;
        };
        let (min, max) = template::mirrored_bounds(origin, mirror, rot, template.size);
        self.out.push(Piece {
            name: name.to_owned(),
            template: Arc::clone(template),
            origin,
            mirror,
            rot,
            min,
            max,
        });
    }

    fn create(&mut self, origin: [i32; 3], rot: Rot, grid: &MansionGrid) {
        let mut data = Data {
            position: origin,
            rot,
        };
        self.entrance(&mut data);
        let mut second = Data {
            position: above(data.position, 8),
            rot: data.rot,
        };
        let base = &grid.base;
        let third = &grid.third;
        self.traverse_outer(
            &mut data,
            base,
            South,
            self.start_x,
            self.start_y,
            self.start_x,
            ENTRANCE_Y,
            "wall_flat",
        );
        self.traverse_outer(
            &mut second,
            base,
            South,
            self.start_x,
            self.start_y,
            self.start_x,
            ENTRANCE_Y,
            "wall_window",
        );
        let mut third_data = Data {
            position: above(data.position, 19),
            rot: data.rot,
        };
        let mut done = false;
        for y in 0..third.height {
            if done {
                break;
            }
            for x in (0..third.width).rev() {
                if !third.is_house(x, y) {
                    continue;
                }
                let step = 8 + (y - self.start_y) * 8;
                third_data.position = step_dir(third_data.position, third_data.rot, South, step);
                let step_x = (x - self.start_x) * 8;
                third_data.position = step_dir(third_data.position, third_data.rot, East, step_x);
                self.traverse_wall(&mut third_data, "wall_window");
                self.traverse_outer(&mut third_data, third, South, x, y, x, y, "wall_window");
                done = true;
                break;
            }
        }
        let start_x = self.start_x;
        let start_y = self.start_y;
        self.roof(above(origin, 16), rot, base, Some(third), start_x, start_y);
        self.roof(above(origin, 27), rot, third, None, start_x, start_y);
        self.rooms(origin, rot, grid, start_x, start_y);
    }

    fn entrance(&mut self, data: &mut Data) {
        let west = step_dir(data.position, data.rot, West, 9);
        self.add("entrance", west, data.rot, Mirror::None);
        data.position = step_dir(data.position, data.rot, South, 16);
    }

    fn traverse_wall(&mut self, data: &mut Data, wall: &str) {
        let pos = step_dir(data.position, data.rot, East, 7);
        self.add(wall, pos, data.rot, Mirror::None);
        data.position = step_dir(data.position, data.rot, South, 8);
    }

    fn traverse_turn(&mut self, data: &mut Data) {
        data.position = step_dir(data.position, data.rot, South, -1);
        self.add("wall_corner", data.position, data.rot, Mirror::None);
        data.position = step_dir(data.position, data.rot, South, -7);
        data.position = step_dir(data.position, data.rot, West, -6);
        data.rot = rotate(data.rot, Rot::Cw90);
    }

    fn traverse_inner_turn(&mut self, data: &mut Data) {
        data.position = step_dir(data.position, data.rot, South, 6);
        data.position = step_dir(data.position, data.rot, East, 8);
        data.rot = rotate(data.rot, Rot::Ccw90);
    }

    #[allow(clippy::too_many_arguments)]
    fn traverse_outer(
        &mut self,
        data: &mut Data,
        grid: &Grid,
        mut dir: Dir,
        mut x: i32,
        mut y: i32,
        end_x: i32,
        end_y: i32,
        wall: &str,
    ) {
        let start = dir;
        // `do { … } while (x != endX || y != endY || start != dir)`: `continue`
        // уходит на проверку условия, поэтому она стоит в начале витка.
        let mut first = true;
        loop {
            if !first && x == end_x && y == end_y && start == dir {
                break;
            }
            first = false;
            let [dx, dz] = dir.step();
            if !grid.is_house(x + dx, y + dz) {
                self.traverse_turn(data);
                dir = dir.clockwise();
                if x == end_x && y == end_y && start == dir {
                    continue;
                }
                self.traverse_wall(data, wall);
                continue;
            }
            let ccw = dir.counter_clockwise().step();
            if grid.is_house(x + dx + ccw[0], y + dz + ccw[1]) {
                self.traverse_inner_turn(data);
                x += dx;
                y += dz;
                dir = dir.counter_clockwise();
                continue;
            }
            x += dx;
            y += dz;
            if x == end_x && y == end_y && start == dir {
                continue;
            }
            self.traverse_wall(data, wall);
        }
    }
}

const ROTS: [Rot; 4] = [Rot::None, Rot::Cw90, Rot::Cw180, Rot::Ccw90];

fn rotate(rot: Rot, add: Rot) -> Rot {
    // `Rotation.getRotated`: складываем четверти оборота по модулю 4.
    let index = |r: Rot| match r {
        Rot::None => 0,
        Rot::Cw90 => 1,
        Rot::Cw180 => 2,
        Rot::Ccw90 => 3,
    };
    ROTS[(index(rot) + index(add)) % 4]
}

fn above(pos: [i32; 3], dy: i32) -> [i32; 3] {
    [pos[0], pos[1] + dy, pos[2]]
}

/// `BlockPos.relative(rotation.rotate(dir), steps)`.
fn step_dir(pos: [i32; 3], rot: Rot, dir: Dir, steps: i32) -> [i32; 3] {
    let d = dir.rotated(rot);
    let [dx, dz] = d.step();
    [pos[0] + dx * steps, pos[1], pos[2] + dz * steps]
}

impl Placer<'_> {
    /// `createRoof`: плиты крыши, торцы и углы поверх стен.
    #[allow(clippy::too_many_arguments)]
    fn roof(
        &mut self,
        origin: [i32; 3],
        rot: Rot,
        grid: &Grid,
        above_grid: Option<&Grid>,
        start_x: i32,
        start_y: i32,
    ) {
        let at = |x: i32, y: i32| {
            let mut p = step_dir(origin, rot, South, 8 + (y - start_y) * 8);
            p = step_dir(p, rot, East, (x - start_x) * 8);
            p
        };
        for y in 0..grid.height {
            for x in 0..grid.width {
                let position = at(x, y);
                let is_above = above_grid.is_some_and(|g| g.is_house(x, y));
                if !grid.is_house(x, y) || is_above {
                    continue;
                }
                self.add("roof", above(position, 3), rot, Mirror::None);
                if !grid.is_house(x + 1, y) {
                    let p2 = step_dir(position, rot, East, 6);
                    self.add("roof_front", p2, rot, Mirror::None);
                }
                if !grid.is_house(x - 1, y) {
                    let p2 = step_dir(position, rot, East, 0);
                    let p2 = step_dir(p2, rot, South, 7);
                    self.add("roof_front", p2, rotate(rot, Rot::Cw180), Mirror::None);
                }
                if !grid.is_house(x, y - 1) {
                    let p2 = step_dir(position, rot, West, 1);
                    self.add("roof_front", p2, rotate(rot, Rot::Ccw90), Mirror::None);
                }
                if grid.is_house(x, y + 1) {
                    continue;
                }
                let p2 = step_dir(position, rot, East, 6);
                let p2 = step_dir(p2, rot, South, 6);
                self.add("roof_front", p2, rotate(rot, Rot::Cw90), Mirror::None);
            }
        }
        if let Some(above_grid) = above_grid {
            for y in 0..grid.height {
                for x in 0..grid.width {
                    let position = at(x, y);
                    let is_above = above_grid.is_house(x, y);
                    if !grid.is_house(x, y) || !is_above {
                        continue;
                    }
                    if !grid.is_house(x + 1, y) {
                        let p2 = step_dir(position, rot, East, 7);
                        self.add("small_wall", p2, rot, Mirror::None);
                    }
                    if !grid.is_house(x - 1, y) {
                        let p2 = step_dir(position, rot, West, 1);
                        let p2 = step_dir(p2, rot, South, 6);
                        self.add("small_wall", p2, rotate(rot, Rot::Cw180), Mirror::None);
                    }
                    if !grid.is_house(x, y - 1) {
                        let p2 = step_dir(position, rot, West, 0);
                        let p2 = step_dir(p2, rot, North, 1);
                        self.add("small_wall", p2, rotate(rot, Rot::Ccw90), Mirror::None);
                    }
                    if !grid.is_house(x, y + 1) {
                        let p2 = step_dir(position, rot, East, 6);
                        let p2 = step_dir(p2, rot, South, 7);
                        self.add("small_wall", p2, rotate(rot, Rot::Cw90), Mirror::None);
                    }
                    if !grid.is_house(x + 1, y) {
                        if !grid.is_house(x, y - 1) {
                            let p2 = step_dir(position, rot, East, 7);
                            let p2 = step_dir(p2, rot, North, 2);
                            self.add("small_wall_corner", p2, rot, Mirror::None);
                        }
                        if !grid.is_house(x, y + 1) {
                            let p2 = step_dir(position, rot, East, 8);
                            let p2 = step_dir(p2, rot, South, 7);
                            self.add(
                                "small_wall_corner",
                                p2,
                                rotate(rot, Rot::Cw90),
                                Mirror::None,
                            );
                        }
                    }
                    if grid.is_house(x - 1, y) {
                        continue;
                    }
                    if !grid.is_house(x, y - 1) {
                        let p2 = step_dir(position, rot, West, 2);
                        let p2 = step_dir(p2, rot, North, 1);
                        self.add(
                            "small_wall_corner",
                            p2,
                            rotate(rot, Rot::Ccw90),
                            Mirror::None,
                        );
                    }
                    if grid.is_house(x, y + 1) {
                        continue;
                    }
                    let p2 = step_dir(position, rot, West, 1);
                    let p2 = step_dir(p2, rot, South, 8);
                    self.add(
                        "small_wall_corner",
                        p2,
                        rotate(rot, Rot::Cw180),
                        Mirror::None,
                    );
                }
            }
        }
        for y in 0..grid.height {
            for x in 0..grid.width {
                let position = at(x, y);
                let is_above = above_grid.is_some_and(|g| g.is_house(x, y));
                if !grid.is_house(x, y) || is_above {
                    continue;
                }
                if !grid.is_house(x + 1, y) {
                    let p2 = step_dir(position, rot, East, 6);
                    if !grid.is_house(x, y + 1) {
                        let p3 = step_dir(p2, rot, South, 6);
                        self.add("roof_corner", p3, rot, Mirror::None);
                    } else if grid.is_house(x + 1, y + 1) {
                        let p3 = step_dir(p2, rot, South, 5);
                        self.add("roof_inner_corner", p3, rot, Mirror::None);
                    }
                    if !grid.is_house(x, y - 1) {
                        self.add("roof_corner", p2, rotate(rot, Rot::Ccw90), Mirror::None);
                    } else if grid.is_house(x + 1, y - 1) {
                        let p3 = step_dir(position, rot, East, 9);
                        let p3 = step_dir(p3, rot, North, 2);
                        self.add(
                            "roof_inner_corner",
                            p3,
                            rotate(rot, Rot::Cw90),
                            Mirror::None,
                        );
                    }
                }
                if grid.is_house(x - 1, y) {
                    continue;
                }
                let p2 = step_dir(position, rot, East, 0);
                let p2 = step_dir(p2, rot, South, 0);
                if !grid.is_house(x, y + 1) {
                    let p3 = step_dir(p2, rot, South, 6);
                    self.add("roof_corner", p3, rotate(rot, Rot::Cw90), Mirror::None);
                } else if grid.is_house(x - 1, y + 1) {
                    let p3 = step_dir(p2, rot, South, 8);
                    let p3 = step_dir(p3, rot, West, 3);
                    self.add(
                        "roof_inner_corner",
                        p3,
                        rotate(rot, Rot::Ccw90),
                        Mirror::None,
                    );
                }
                if !grid.is_house(x, y - 1) {
                    self.add("roof_corner", p2, rotate(rot, Rot::Cw180), Mirror::None);
                    continue;
                }
                if !grid.is_house(x - 1, y - 1) {
                    continue;
                }
                let p3 = step_dir(p2, rot, South, 1);
                self.add(
                    "roof_inner_corner",
                    p3,
                    rotate(rot, Rot::Cw180),
                    Mirror::None,
                );
            }
        }
    }

    /// Комнаты, коридоры, ковры и внутренние стены всех трёх этажей.
    fn rooms(
        &mut self,
        origin: [i32; 3],
        rot: Rot,
        grid: &MansionGrid,
        start_x: i32,
        start_y: i32,
    ) {
        for floor in 0..3 {
            let floor_origin = above(origin, 8 * floor + if floor == 2 { 3 } else { 0 });
            let rooms = &grid.rooms[floor as usize];
            let plan = if floor == 2 { &grid.third } else { &grid.base };
            let south_piece = if floor == 0 {
                "carpet_south_1"
            } else {
                "carpet_south_2"
            };
            let west_piece = if floor == 0 {
                "carpet_west_1"
            } else {
                "carpet_west_2"
            };
            for y in 0..plan.height {
                for x in 0..plan.width {
                    if plan.get(x, y) != CORRIDOR {
                        continue;
                    }
                    let mut pos = step_dir(floor_origin, rot, South, 8 + (y - start_y) * 8);
                    pos = step_dir(pos, rot, East, (x - start_x) * 8);
                    self.add("corridor_floor", pos, rot, Mirror::None);
                    if plan.get(x, y - 1) == CORRIDOR
                        || rooms.get(x, y - 1) & ROOM_CORRIDOR_FLAG == ROOM_CORRIDOR_FLAG
                    {
                        let p = above(step_dir(pos, rot, East, 1), 1);
                        self.add("carpet_north", p, rot, Mirror::None);
                    }
                    if plan.get(x + 1, y) == CORRIDOR
                        || rooms.get(x + 1, y) & ROOM_CORRIDOR_FLAG == ROOM_CORRIDOR_FLAG
                    {
                        let p = step_dir(pos, rot, South, 1);
                        let p = above(step_dir(p, rot, East, 5), 1);
                        self.add("carpet_east", p, rot, Mirror::None);
                    }
                    if plan.get(x, y + 1) == CORRIDOR
                        || rooms.get(x, y + 1) & ROOM_CORRIDOR_FLAG == ROOM_CORRIDOR_FLAG
                    {
                        let p = step_dir(pos, rot, South, 5);
                        let p = step_dir(p, rot, West, 1);
                        self.add(south_piece, p, rot, Mirror::None);
                    }
                    if plan.get(x - 1, y) == CORRIDOR
                        || rooms.get(x - 1, y) & ROOM_CORRIDOR_FLAG == ROOM_CORRIDOR_FLAG
                    {
                        let p = step_dir(pos, rot, West, 1);
                        let p = step_dir(p, rot, North, 1);
                        self.add(west_piece, p, rot, Mirror::None);
                    }
                }
            }
            let wall_piece = if floor == 0 {
                "indoors_wall_1"
            } else {
                "indoors_wall_2"
            };
            let door_piece = if floor == 0 {
                "indoors_door_1"
            } else {
                "indoors_door_2"
            };
            for y in 0..plan.height {
                for x in 0..plan.width {
                    let is_start_room = floor == 2 && plan.get(x, y) == START_ROOM;
                    if plan.get(x, y) != ROOM && !is_start_room {
                        continue;
                    }
                    let data = rooms.get(x, y);
                    let kind = data & ROOM_TYPE_MASK;
                    let room_id = data & ROOM_ID_MASK;
                    let is_start_room =
                        is_start_room && data & ROOM_CORRIDOR_FLAG == ROOM_CORRIDOR_FLAG;
                    let mut door_dirs = Vec::new();
                    if data & ROOM_DOOR_FLAG == ROOM_DOOR_FLAG {
                        for dir in [North, East, South, West] {
                            let [dx, dz] = dir.step();
                            if plan.get(x + dx, y + dz) == CORRIDOR {
                                door_dirs.push(dir);
                            }
                        }
                    }
                    let door_dir = if door_dirs.is_empty() {
                        if data & ROOM_ORIGIN_FLAG == ROOM_ORIGIN_FLAG {
                            // `Direction.UP`: дверь сверху, ставится как проём.
                            Some(None)
                        } else {
                            None
                        }
                    } else {
                        Some(Some(
                            door_dirs[self.rng.next_int_bound(door_dirs.len() as i32) as usize],
                        ))
                    };
                    let mut room_pos = step_dir(floor_origin, rot, South, 8 + (y - start_y) * 8);
                    room_pos = step_dir(room_pos, rot, East, -1 + (x - start_x) * 8);
                    if plan.is_house(x - 1, y)
                        && !grid.is_room_id(x - 1, y, floor as usize, room_id)
                    {
                        let piece = if door_dir == Some(Some(West)) {
                            door_piece
                        } else {
                            wall_piece
                        };
                        self.add(piece, room_pos, rot, Mirror::None);
                    }
                    if plan.get(x + 1, y) == CORRIDOR && !is_start_room {
                        let pos = step_dir(room_pos, rot, East, 8);
                        let piece = if door_dir == Some(Some(East)) {
                            door_piece
                        } else {
                            wall_piece
                        };
                        self.add(piece, pos, rot, Mirror::None);
                    }
                    if plan.is_house(x, y + 1)
                        && !grid.is_room_id(x, y + 1, floor as usize, room_id)
                    {
                        let mut pos = step_dir(room_pos, rot, South, 7);
                        pos = step_dir(pos, rot, East, 7);
                        let piece = if door_dir == Some(Some(South)) {
                            door_piece
                        } else {
                            wall_piece
                        };
                        self.add(piece, pos, rotate(rot, Rot::Cw90), Mirror::None);
                    }
                    if plan.get(x, y - 1) == CORRIDOR && !is_start_room {
                        let mut pos = step_dir(room_pos, rot, North, 1);
                        pos = step_dir(pos, rot, East, 7);
                        let piece = if door_dir == Some(Some(North)) {
                            door_piece
                        } else {
                            wall_piece
                        };
                        self.add(piece, pos, rotate(rot, Rot::Cw90), Mirror::None);
                    }
                    // DoorKind: стена, дверь в комнату или проём сверху
                    // (Direction.UP), который у 1x2 даёт потайную комнату.
                    let door = match door_dir {
                        Some(d) => d,
                        None => continue,
                    };
                    match kind {
                        ROOM_1X1 => self.room_1x1(room_pos, rot, door, floor as usize),
                        ROOM_1X2 => {
                            let room_dir = grid
                                .room_direction(plan, x, y, floor as usize, room_id)
                                .unwrap_or(South);
                            let stairs = data & ROOM_STAIRS_FLAG == ROOM_STAIRS_FLAG;
                            self.room_1x2(room_pos, rot, room_dir, door, floor as usize, stairs);
                        }
                        ROOM_2X2 => {
                            let Some(door) = door else {
                                // Дверь сверху — потайная комната 2x2.
                                self.room_2x2_secret(room_pos, rot);
                                continue;
                            };
                            let mut room_dir = door.clockwise();
                            let [dx, dz] = room_dir.step();
                            if !grid.is_room_id(x + dx, y + dz, floor as usize, room_id) {
                                room_dir = room_dir.opposite();
                            }
                            self.room_2x2(room_pos, rot, room_dir, door, floor as usize);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn one_x_one(&mut self, floor: usize, secret: bool) -> String {
        let n = if secret {
            self.rng.next_int_bound(4) + 1
        } else {
            // Оба этажа ванилы берут один и тот же диапазон.
            let _ = floor;
            self.rng.next_int_bound(5) + 1
        };
        let suffix = if secret {
            format!("as{n}")
        } else if floor == 0 {
            format!("a{n}")
        } else {
            format!("b{n}")
        };
        format!("1x1_{suffix}")
    }

    fn room_1x1(&mut self, room_pos: [i32; 3], rot: Rot, door: Option<Dir>, floor: usize) {
        let mut piece_rot = Rot::None;
        let name = match door {
            Some(East) => self.one_x_one(floor, false),
            Some(North) => {
                piece_rot = rotate(piece_rot, Rot::Ccw90);
                self.one_x_one(floor, false)
            }
            Some(West) => {
                piece_rot = rotate(piece_rot, Rot::Cw180);
                self.one_x_one(floor, false)
            }
            Some(South) => {
                piece_rot = rotate(piece_rot, Rot::Cw90);
                self.one_x_one(floor, false)
            }
            None => self.one_x_one(floor, true),
        };
        // `getZeroPositionWithTransform(new BlockPos(1, 0, 0), NONE, pieceRot, 7, 7)`.
        let zero = template::zero_with_transform([1, 0, 0], Mirror::None, piece_rot, 7, 7);
        let rotated = zero_rotate(zero, rot);
        let pos = [
            room_pos[0] + rotated[0],
            room_pos[1],
            room_pos[2] + rotated[2],
        ];
        self.add(&name, pos, rotate(piece_rot, rot), Mirror::None);
    }

    fn side_entrance(&mut self, floor: usize, stairs: bool) -> String {
        if stairs {
            return "1x2_c_stairs".to_string();
        }
        if floor == 0 {
            format!("1x2_a{}", self.rng.next_int_bound(9) + 1)
        } else {
            format!("1x2_c{}", self.rng.next_int_bound(4) + 1)
        }
    }

    fn front_entrance(&mut self, floor: usize, stairs: bool) -> String {
        if stairs {
            return "1x2_d_stairs".to_string();
        }
        if floor == 0 {
            format!("1x2_b{}", self.rng.next_int_bound(5) + 1)
        } else {
            format!("1x2_d{}", self.rng.next_int_bound(5) + 1)
        }
    }

    fn secret_1x2(&mut self, floor: usize) -> String {
        if floor == 0 {
            format!("1x2_s{}", self.rng.next_int_bound(2) + 1)
        } else {
            "1x2_se1".to_string()
        }
    }

    fn two_x_two(&mut self, floor: usize) -> String {
        if floor == 0 {
            format!("2x2_a{}", self.rng.next_int_bound(4) + 1)
        } else {
            format!("2x2_b{}", self.rng.next_int_bound(5) + 1)
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn room_1x2(
        &mut self,
        room_pos: [i32; 3],
        rot: Rot,
        room_dir: Dir,
        door: Option<Dir>,
        floor: usize,
        stairs: bool,
    ) {
        let east = |p: [i32; 3], n: i32| step_dir(p, rot, East, n);
        let south = |p: [i32; 3], n: i32| step_dir(p, rot, South, n);
        let Some(door) = door else {
            // doorDir == UP: потайная комната 1x2.
            let name = self.secret_1x2(floor);
            let (pos, piece_rot) = match room_dir {
                East => (east(room_pos, 15), rotate(rot, Rot::Cw90)),
                South => (east(room_pos, 1), rot),
                _ => (east(room_pos, 1), rot),
            };
            self.add(&name, pos, piece_rot, Mirror::None);
            return;
        };
        let (pos, piece_rot, mirror, name) = match (door, room_dir) {
            (East, South) => (
                east(room_pos, 1),
                rot,
                Mirror::None,
                self.side_entrance(floor, stairs),
            ),
            (East, North) => (
                south(east(room_pos, 1), 6),
                rot,
                Mirror::LeftRight,
                self.side_entrance(floor, stairs),
            ),
            (West, North) => (
                south(east(room_pos, 7), 6),
                rotate(rot, Rot::Cw180),
                Mirror::None,
                self.side_entrance(floor, stairs),
            ),
            (West, South) => (
                east(room_pos, 7),
                rot,
                Mirror::FrontBack,
                self.side_entrance(floor, stairs),
            ),
            (South, East) => (
                east(room_pos, 1),
                rotate(rot, Rot::Cw90),
                Mirror::LeftRight,
                self.side_entrance(floor, stairs),
            ),
            (South, West) => (
                east(room_pos, 7),
                rotate(rot, Rot::Cw90),
                Mirror::None,
                self.side_entrance(floor, stairs),
            ),
            (North, West) => (
                south(east(room_pos, 7), 6),
                rotate(rot, Rot::Cw90),
                Mirror::FrontBack,
                self.side_entrance(floor, stairs),
            ),
            (North, East) => (
                south(east(room_pos, 1), 6),
                rotate(rot, Rot::Ccw90),
                Mirror::None,
                self.side_entrance(floor, stairs),
            ),
            (South, North) => (
                step_dir(east(room_pos, 1), rot, North, 8),
                rot,
                Mirror::None,
                self.front_entrance(floor, stairs),
            ),
            (North, South) => (
                south(east(room_pos, 7), 14),
                rotate(rot, Rot::Cw180),
                Mirror::None,
                self.front_entrance(floor, stairs),
            ),
            (West, East) => (
                east(room_pos, 15),
                rotate(rot, Rot::Cw90),
                Mirror::None,
                self.front_entrance(floor, stairs),
            ),
            (East, West) => (
                south(step_dir(room_pos, rot, West, 7), 6),
                rotate(rot, Rot::Ccw90),
                Mirror::None,
                self.front_entrance(floor, stairs),
            ),
            // Секретные комнаты: `doorDir == UP` приходит сюда как `None`, но
            // для 1x2 потайной проём задаётся направлением комнаты.
            (_, _) => {
                let name = self.secret_1x2(floor);
                match room_dir {
                    East => (
                        east(room_pos, 15),
                        rotate(rot, Rot::Cw90),
                        Mirror::None,
                        name,
                    ),
                    _ => (east(room_pos, 1), rot, Mirror::None, name),
                }
            }
        };
        self.add(&name, pos, piece_rot, mirror);
    }

    fn room_2x2(&mut self, room_pos: [i32; 3], rot: Rot, room_dir: Dir, door: Dir, floor: usize) {
        let (mut east_off, mut south_off) = (0, 0);
        let mut piece_rot = rot;
        let mut mirror = Mirror::None;
        match (door, room_dir) {
            (East, South) => east_off = -7,
            (East, North) => {
                east_off = -7;
                south_off = 6;
                mirror = Mirror::LeftRight;
            }
            (North, East) => {
                east_off = 1;
                south_off = 14;
                piece_rot = rotate(rot, Rot::Ccw90);
            }
            (North, West) => {
                east_off = 7;
                south_off = 14;
                piece_rot = rotate(rot, Rot::Ccw90);
                mirror = Mirror::LeftRight;
            }
            (South, West) => {
                east_off = 7;
                south_off = -8;
                piece_rot = rotate(rot, Rot::Cw90);
            }
            (South, East) => {
                east_off = 1;
                south_off = -8;
                piece_rot = rotate(rot, Rot::Cw90);
                mirror = Mirror::LeftRight;
            }
            (West, North) => {
                east_off = 15;
                south_off = 6;
                piece_rot = rotate(rot, Rot::Cw180);
            }
            (West, South) => {
                east_off = 15;
                mirror = Mirror::FrontBack;
            }
            _ => {}
        }
        let mut pos = step_dir(room_pos, rot, East, east_off);
        pos = step_dir(pos, rot, South, south_off);
        let name = self.two_x_two(floor);
        self.add(&name, pos, piece_rot, mirror);
    }

    fn room_2x2_secret(&mut self, room_pos: [i32; 3], rot: Rot) {
        let pos = step_dir(room_pos, rot, East, 1);
        self.add("2x2_s1", pos, rot, Mirror::None);
    }
}

/// `WoodlandMansionStructure.afterPlace`: под низом коробки ставится булыжник,
/// пока не упрётся в непустой блок или жидкость.
pub(crate) fn after_place(
    pal: &Palette,
    level: &mut dyn Level,
    pieces: &[([i32; 3], [i32; 3])],
    window: Bounds,
    cobblestone: u32,
) {
    if pieces.is_empty() {
        return;
    }
    let mut bmin = [i32::MAX; 3];
    let mut bmax = [i32::MIN; 3];
    for (min, max) in pieces {
        for i in 0..3 {
            bmin[i] = bmin[i].min(min[i]);
            bmax[i] = bmax[i].max(max[i]);
        }
    }
    let y_start = bmin[1];
    let min_y = level.min_y();
    let inside = |p: [i32; 3]| {
        pieces
            .iter()
            .any(|(min, max)| (0..3).all(|i| min[i] <= p[i] && p[i] <= max[i]))
    };
    for x in window.min[0]..=window.max[0] {
        'next: for z in window.min[2]..=window.max[2] {
            let top = [x, y_start, z];
            if pal.is_air(level.get(x, y_start, z)) || !inside(top) {
                continue;
            }
            let mut y = y_start - 1;
            while y > min_y {
                let state = level.get(x, y, z);
                if !pal.is_air(state) && !pal.is_liquid(state) {
                    continue 'next;
                }
                level.set(x, y, z, cobblestone);
                y -= 1;
            }
        }
    }
}

/// `BlockPos.rotate`: четверть оборота вокруг нуля.
fn zero_rotate(pos: [i32; 3], rot: Rot) -> [i32; 3] {
    match rot {
        Rot::None => pos,
        Rot::Cw90 => [-pos[2], pos[1], pos[0]],
        Rot::Cw180 => [-pos[0], pos[1], -pos[2]],
        Rot::Ccw90 => [pos[2], pos[1], -pos[0]],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn templates(data: &std::path::Path, palette: &Palette) -> HashMap<String, Arc<Template>> {
        let dir = data.join("data/minecraft/structure/woodland_mansion");
        let mut out = HashMap::new();
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let id = format!("minecraft:woodland_mansion/{stem}");
            out.insert(
                stem.to_owned(),
                Arc::new(template::load_template(data, palette, &id).unwrap()),
            );
        }
        out
    }

    #[test]
    fn mansion_seed_12345_pieces() {
        let Some(data) = std::env::var_os("CHUNKWAKE_DATA").map(std::path::PathBuf::from) else {
            return;
        };
        let blocks = chunkwake_data::world::Blocks::load(&data).unwrap();
        let state = |n: &str| blocks.default_state(n).ok().map(|s| s as u32);
        let palette = Palette::new(&data, &blocks.states(), &state);
        let templates = templates(&data, &palette);
        assert!(templates.len() > 50, "{}", templates.len());
        let mut rng = Legacy::new(0);
        rng.set_large_feature_seed(12345, 291, 441);
        let rot = ROTS[rng.next_int_bound(4) as usize];
        let pieces = build(&templates, 291, 441, 70, rot, &mut rng);
        // Коробка особняка на сиде 12345 из NBT старта: 487 кусков.
        assert_eq!(pieces.len(), 487);
        assert!(!pieces.is_empty());
    }
}

#[cfg(test)]
mod against_vanilla {
    use super::*;

    fn templates(data: &std::path::Path, palette: &Palette) -> HashMap<String, Arc<Template>> {
        let dir = data.join("data/minecraft/structure/woodland_mansion");
        let mut out = HashMap::new();
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let id = format!("minecraft:woodland_mansion/{stem}");
            out.insert(
                stem.to_owned(),
                Arc::new(template::load_template(data, palette, &id).unwrap()),
            );
        }
        out
    }

    /// Куски сравниваются с NBT старта из `server.jar` (сид 12345, чанк 291 441).
    #[test]
    fn mansion_pieces_match_vanilla() {
        let Some(data) = std::env::var_os("CHUNKWAKE_DATA").map(std::path::PathBuf::from) else {
            return;
        };
        let Some(region) =
            std::env::var_os("CHUNKWAKE_MANSION_REGION").map(std::path::PathBuf::from)
        else {
            return;
        };
        let blocks = chunkwake_data::world::Blocks::load(&data).unwrap();
        let state = |n: &str| blocks.default_state(n).ok().map(|s| s as u32);
        let palette = Palette::new(&data, &blocks.states(), &state);
        let templates = templates(&data, &palette);
        let store = chunkwake_world::anvil::RegionStore::new(region);
        let chunks = store.load_region(9, 13).unwrap();
        let (_, root) = chunks.iter().find(|(p, _)| *p == (291, 441)).unwrap();
        let children = root
            .get("structures")
            .and_then(|t| t.as_compound())
            .and_then(|c| c.get("starts"))
            .and_then(|t| t.as_compound())
            .and_then(|c| c.get("minecraft:mansion"))
            .and_then(|t| t.as_compound())
            .and_then(|c| c.get("Children"))
            .and_then(|t| t.as_list())
            .unwrap();
        let mut rng = Legacy::new(0);
        rng.set_large_feature_seed(12345, 291, 441);
        let rot = ROTS[rng.next_int_bound(4) as usize];
        let ours = build(&templates, 291, 441, 70, rot, &mut rng);
        if ours.len() != children.len() {
            println!("count {} vs {}", ours.len(), children.len());
        }
        assert_eq!(ours.len(), children.len(), "число кусков");
        let mut vset: Vec<(String, [i32; 6])> = children
            .iter()
            .map(|c| {
                let c = c.as_compound().unwrap();
                let bb = c.get("BB").and_then(|t| t.as_int_array()).unwrap_or(&[]);
                let mut b = [0; 6];
                b.copy_from_slice(&bb[..6]);
                (
                    c.get("Template")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_owned(),
                    b,
                )
            })
            .collect();
        let mut oset: Vec<(String, [i32; 6])> = ours
            .iter()
            .map(|p| {
                (
                    p.name.clone(),
                    [p.min[0], p.min[1], p.min[2], p.max[0], p.max[1], p.max[2]],
                )
            })
            .collect();
        vset.sort();
        oset.sort();
        assert_eq!(vset.len(), oset.len());
        if vset != oset {
            use std::collections::BTreeMap;
            let count = |set: &Vec<(String, [i32; 6])>| {
                let mut m: BTreeMap<String, i32> = BTreeMap::new();
                for (n, _) in set {
                    *m.entry(n.clone()).or_default() += 1;
                }
                m
            };
            for (n, b) in &vset {
                if !oset.iter().any(|(n2, b2)| n2 == n && b2 == b) {
                    println!("only vanilla {n} {b:?}");
                }
            }
            for (n, b) in &oset {
                if !vset.iter().any(|(n2, b2)| n2 == n && b2 == b) {
                    println!("only ours {n} {b:?}");
                }
            }
            let vc = count(&vset);
            let oc = count(&oset);
            for (name, n) in &vc {
                let ours_n = oc.get(name).copied().unwrap_or(0);
                if *n != ours_n {
                    println!("{name}: vanilla {n} ours {ours_n}");
                }
            }
            for (name, n) in &oc {
                if !vc.contains_key(name) {
                    println!("{name}: vanilla 0 ours {n}");
                }
            }
            panic!("множество кусков разошлось");
        }
    }
}
