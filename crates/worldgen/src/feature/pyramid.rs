//! Пустынная пирамида и храм в джунглях: `SinglePieceStructure` и
//! `ScatteredFeaturePiece`. Сундуки, раздатчики и `suspicious_sand` остаются
//! состояниями блоков: лут и археология не пишутся.

use super::template::{self, Bounds, Rot};
use super::{Fluid, Heightmap, Level, Palette};
use crate::random::{Bits, Legacy, Worldgen, Xoroshiro, pos_seed};
use std::sync::Mutex;

/// Горизонталь `Direction.Plane.HORIZONTAL`: север, восток, юг, запад.
const FACES: [&str; 4] = ["north", "east", "south", "west"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Which {
    Desert,
    Jungle,
    Swamp,
}

/// Кусок с живой высотой: первый чанк, который его пишет, сдвигает коробку,
/// остальные видят уже сдвинутую.
pub(super) struct Piece {
    which: Which,
    /// Индекс в [`FACES`].
    facing: u8,
    width: i32,
    depth: i32,
    min: Mutex<[i32; 3]>,
    max: Mutex<[i32; 3]>,
    /// `-1`, пока высота не заморожена. Дальше — значение `heightPosition`.
    height_position: Mutex<i32>,
}

/// Старт `findGenerationPoint`: биом столбца и поверхности середины чанка,
/// четыре угла следа по `WORLD_SURFACE_WG` не ниже моря.
pub(super) fn can_start(
    which: Which,
    sx: i32,
    sz: i32,
    columns: &dyn super::structure::Columns,
    biomes: &std::collections::HashSet<u32>,
) -> Option<()> {
    if which == Which::Swamp {
        return surface_biome(sx, sz, columns, biomes);
    }
    let (foot_x, foot_z) = match which {
        Which::Desert => (21, 21),
        Which::Jungle => (12, 15),
        Which::Swamp => unreachable!(),
    };
    let mid_x = sx * 16 + 8;
    let mid_z = sz * 16 + 8;
    if !columns.column_has(mid_x, mid_z, biomes) {
        return None;
    }
    let origin_x = sx * 16;
    let origin_z = sz * 16;
    let corners = [
        (origin_x, origin_z),
        (origin_x, origin_z + foot_z),
        (origin_x + foot_x, origin_z),
        (origin_x + foot_x, origin_z + foot_z),
    ];
    let heights: Vec<i32> = corners
        .into_iter()
        .map(|(x, z)| columns.base_height(x, z) - 1)
        .collect();
    let lowest = heights.iter().copied().min()?;
    let surface = columns.base_height(mid_x, mid_z) - 1;
    let biome = columns.biome_at(mid_x, surface, mid_z);
    if lowest < columns.sea_level() {
        return None;
    }
    let biome = biome?;
    biomes.contains(&biome).then_some(())
}

/// Хижина: биом занятой `WORLD_SURFACE_WG` середины чанка. Углы и море не смотрит.
fn surface_biome(
    sx: i32,
    sz: i32,
    columns: &dyn super::structure::Columns,
    biomes: &std::collections::HashSet<u32>,
) -> Option<()> {
    let mid_x = sx * 16 + 8;
    let mid_z = sz * 16 + 8;
    let surface = columns.base_height(mid_x, mid_z) - 1;
    let biome = columns.biome_at(mid_x, surface, mid_z)?;
    biomes.contains(&biome).then_some(())
}

impl Piece {
    /// `makeBoundingBox` от угла чанка на y = 64. Поворот — ось Z или X.
    pub(super) fn new(which: Which, facing: u8, sx: i32, sz: i32) -> Self {
        let (width, height, depth) = match which {
            Which::Desert => (21, 15, 21),
            Which::Jungle => (12, 10, 15),
            Which::Swamp => (7, 7, 9),
        };
        let (x, y, z) = (sx * 16, 64, sz * 16);
        let (min, max) = if facing == 0 || facing == 2 {
            ([x, y, z], [x + width - 1, y + height - 1, z + depth - 1])
        } else {
            ([x, y, z], [x + depth - 1, y + height - 1, z + width - 1])
        };
        Self {
            which,
            facing,
            width,
            depth,
            min: Mutex::new(min),
            max: Mutex::new(max),
            height_position: Mutex::new(-1),
        }
    }

    pub(super) fn bounds(&self) -> ([i32; 3], [i32; 3]) {
        (*self.min.lock().unwrap(), *self.max.lock().unwrap())
    }

    pub(super) fn place(
        &self,
        pal: &Palette,
        level: &mut dyn Level,
        decor: &mut Worldgen,
        chunk: Bounds,
    ) {
        let ready = match self.which {
            Which::Desert => {
                let offset = -decor.next_int_bound(3);
                self.update_lowest(level, pal, offset)
            }
            Which::Jungle | Which::Swamp => self.update_average(level, pal, chunk, 0),
        };
        if !ready {
            return;
        }
        let (min, max) = self.bounds();
        let (mirror, rot) = orientation(self.facing);
        let mut draw = Draw {
            pal,
            level,
            decor,
            region: region_random(pal.world_seed, chunk.min[0] >> 4, chunk.min[2] >> 4),
            chunk,
            min,
            max,
            facing: self.facing,
            mirror,
            rot,
            width: self.width,
            depth: self.depth,
            sands: Vec::new(),
            roof: [0, 0, 0],
            seed: pal.world_seed,
        };
        match self.which {
            Which::Desert => draw.desert(),
            Which::Jungle => draw.jungle(),
            Which::Swamp => draw.swamp(),
        }
    }

    /// `updateHeightPositionToLowestGroundHeight`: минимум живой карты
    /// `MOTION_BLOCKING_NO_LEAVES` по всей коробке, затем сдвиг.
    /// Стволы деревьев пропускаются: их ставит шаг растительности и соседние
    /// чанки уже после выбора высоты в ванили.
    fn update_lowest(&self, level: &dyn Level, pal: &Palette, offset: i32) -> bool {
        let mut height = self.height_position.lock().unwrap();
        let mut min = self.min.lock().unwrap();
        let mut max = self.max.lock().unwrap();
        if *height >= 0 {
            return true;
        }
        let mut lowest = i32::MAX;
        let mut found = false;
        for z in min[2]..=max[2] {
            for x in min[0]..=max[0] {
                lowest = lowest.min(terrain_top(level, pal, x, z));
                found = true;
            }
        }
        if !found {
            return false;
        }
        *height = lowest;
        let dy = lowest - min[1] + offset;
        min[1] += dy;
        max[1] += dy;
        true
    }

    /// `updateAverageGroundHeight`: целое среднее тех же высот, но только
    /// столбцы внутри коробки текущего чанка. Точка проверки — y = 64.
    fn update_average(&self, level: &dyn Level, pal: &Palette, chunk: Bounds, offset: i32) -> bool {
        let mut height = self.height_position.lock().unwrap();
        let mut min = self.min.lock().unwrap();
        let mut max = self.max.lock().unwrap();
        if *height >= 0 {
            return true;
        }
        let mut total = 0i32;
        let mut count = 0i32;
        for z in min[2]..=max[2] {
            for x in min[0]..=max[0] {
                if !chunk.contains([x, 64, z]) {
                    continue;
                }
                total += terrain_top(level, pal, x, z);
                count += 1;
            }
        }
        if count == 0 {
            return false;
        }
        let average = total / count;
        *height = average;
        let dy = average - min[1] + offset;
        min[1] += dy;
        max[1] += dy;
        true
    }
}

/// `setOrientation`: юг и запад зеркалятся слева направо, запад и восток
/// крутятся на 90° по часовой.
fn orientation(facing: u8) -> (bool, Rot) {
    match facing {
        2 => (true, Rot::None),
        3 => (true, Rot::Cw90),
        1 => (false, Rot::Cw90),
        _ => (false, Rot::None),
    }
}

/// `RandomState.getOrCreateRandomFactory("minecraft:worldgen_region_random")`
/// на минимальном блоке центрального чанка региона. Это `level.getRandom()`.
fn region_random(seed: i64, cx: i32, cz: i32) -> crate::random::Rng {
    let world = Xoroshiro::from_seed(seed).fork_positional();
    world
        .from_hash_of("minecraft:worldgen_region_random")
        .fork_positional()
        .at(cx * 16, 0, cz * 16)
}

/// Первый свободный y карты `MOTION_BLOCKING_NO_LEAVES`. Ствол, уже
/// записанный соседним чанком, пропускается: ваниль выбирает высоту до него.
fn terrain_top(level: &dyn Level, pal: &Palette, x: i32, z: i32) -> i32 {
    let logs = pal.tags.get("minecraft:logs");
    let floor = level.min_y();
    let mut y = level.surface(Heightmap::MotionBlockingNoLeaves, x, z);
    while y - 1 > floor {
        let state = level.get(x, y - 1, z);
        let log = logs.is_some_and(|tag| tag.get(state as usize) == Some(&true));
        if !log && pal.hits(Heightmap::MotionBlockingNoLeaves, state) {
            return y;
        }
        y -= 1;
    }
    y
}

/// `RandomSource.createThreadLocalInstance(seed).forkPositional().at(pos)`.
/// Это `SingleThreadedRandomSource`: линейный генератор, затем
/// `LegacyPositionalRandomFactory`.
fn thread_local_at(seed: i64, x: i32, y: i32, z: i32) -> Legacy {
    let mut base = Legacy::new(seed);
    let factory = base.next_long();
    Legacy::new(pos_seed(x, y, z) ^ factory)
}

struct Draw<'a> {
    pal: &'a Palette,
    level: &'a mut dyn Level,
    decor: &'a mut Worldgen,
    region: crate::random::Rng,
    chunk: Bounds,
    min: [i32; 3],
    max: [i32; 3],
    facing: u8,
    mirror: bool,
    rot: Rot,
    width: i32,
    depth: i32,
    sands: Vec<[i32; 3]>,
    roof: [i32; 3],
    seed: i64,
}

impl Draw<'_> {
    fn block(&self, name: &str) -> u32 {
        self.pal.defaults.get(name).copied().unwrap_or(0)
    }

    fn with(&self, state: u32, key: &str, value: &str) -> u32 {
        self.pal.with_prop(state, key, value).unwrap_or(state)
    }

    fn world(&self, x: i32, y: i32, z: i32) -> [i32; 3] {
        let wx = match self.facing {
            0 | 2 => self.min[0] + x,
            3 => self.max[0] - z,
            _ => self.min[0] + z,
        };
        let wz = match self.facing {
            0 => self.max[2] - z,
            2 => self.min[2] + z,
            _ => self.min[2] + x,
        };
        [wx, self.min[1] + y, wz]
    }

    fn put(&mut self, state: u32, x: i32, y: i32, z: i32) {
        let p = self.world(x, y, z);
        if !self.chunk.contains(p) || !self.level.can_write(p[0], p[2]) {
            return;
        }
        let state = apply_orient(self.pal, state, self.mirror, self.rot);
        self.level.set(p[0], p[1], p[2], state);
    }

    fn raw(&mut self, state: u32, p: [i32; 3]) {
        if !self.chunk.contains(p) || !self.level.can_write(p[0], p[2]) {
            return;
        }
        self.level.set(p[0], p[1], p[2], state);
    }

    fn seen_air(&self, x: i32, y: i32, z: i32) -> bool {
        let p = self.world(x, y, z);
        if !self.chunk.contains(p) {
            return true;
        }
        self.pal.is_air(self.level.get(p[0], p[1], p[2]))
    }

    #[allow(clippy::too_many_arguments)]
    fn fill(
        &mut self,
        x0: i32,
        y0: i32,
        z0: i32,
        x1: i32,
        y1: i32,
        z1: i32,
        edge: u32,
        inside: u32,
        skip_air: bool,
    ) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                for z in z0..=z1 {
                    if skip_air && self.seen_air(x, y, z) {
                        continue;
                    }
                    let boundary = y == y0 || y == y1 || x == x0 || x == x1 || z == z0 || z == z1;
                    self.put(if boundary { edge } else { inside }, x, y, z);
                }
            }
        }
    }

    fn air(&mut self, x0: i32, y0: i32, z0: i32, x1: i32, y1: i32, z1: i32) {
        let air = self.block("minecraft:air");
        self.fill(x0, y0, z0, x1, y1, z1, air, air, false);
    }

    fn moss(&mut self, x0: i32, y0: i32, z0: i32, x1: i32, y1: i32, z1: i32) {
        let cobble = self.block("minecraft:cobblestone");
        let moss = self.block("minecraft:mossy_cobblestone");
        for y in y0..=y1 {
            for x in x0..=x1 {
                for z in z0..=z1 {
                    let state = if self.decor.next_float() < 0.4 {
                        cobble
                    } else {
                        moss
                    };
                    self.put(state, x, y, z);
                }
            }
        }
    }

    fn column_down(&mut self, state: u32, x: i32, y: i32, z: i32) {
        let mut p = self.world(x, y, z);
        if !self.chunk.contains(p) {
            return;
        }
        let floor = self.level.min_y() + 1;
        while p[1] > floor && replaceable(self.pal, self.level.get(p[0], p[1], p[2])) {
            if self.level.can_write(p[0], p[2]) {
                self.level.set(p[0], p[1], p[2], state);
            }
            p[1] -= 1;
        }
    }

    fn chest(&mut self, x: i32, y: i32, z: i32, table: &str) -> bool {
        let p = self.world(x, y, z);
        let chest = self.block("minecraft:chest");
        if !self.chunk.contains(p) || self.pal.same_block(self.level.get(p[0], p[1], p[2]), chest) {
            return false;
        }
        let state = reorient(self.pal, self.level, p, chest);
        if self.level.can_write(p[0], p[2]) {
            self.level.set(p[0], p[1], p[2], state);
        }
        // `createChest`: `setLootTable(table, random.nextLong())`.
        let seed = self.decor.next_long();
        if self.level.can_write(p[0], p[2]) {
            super::template::set_loot(self.level, p, table, seed);
        }
        true
    }

    fn dispenser(&mut self, x: i32, y: i32, z: i32, facing: &str, table: &str) -> bool {
        let p = self.world(x, y, z);
        let base = self.block("minecraft:dispenser");
        if !self.chunk.contains(p) || self.pal.same_block(self.level.get(p[0], p[1], p[2]), base) {
            return false;
        }
        self.put(self.with(base, "facing", facing), x, y, z);
        // `createDispenser`: та же пара `LootTable` + `LootTableSeed`.
        let seed = self.decor.next_long();
        if self.chunk.contains(p) && self.level.can_write(p[0], p[2]) {
            let mut nbt = chunkwake_world::nbt::Compound::new();
            nbt.insert(
                "LootTable".into(),
                chunkwake_world::nbt::Tag::String(table.to_owned()),
            );
            nbt.insert(
                "LootTableSeed".into(),
                chunkwake_world::nbt::Tag::Long(seed),
            );
            self.level
                .set_block_entity(p[0], p[1], p[2], "minecraft:dispenser", nbt);
        }
        true
    }

    fn record_sand(&mut self, x: i32, y: i32, z: i32) {
        self.sands.push(self.world(x, y, z));
    }

    fn record_box(&mut self, x0: i32, y0: i32, z0: i32, x1: i32, y1: i32, z1: i32) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                for z in z0..=z1 {
                    self.record_sand(x, y, z);
                }
            }
        }
    }

    fn roof_piece(&mut self, x: i32, y: i32, z: i32) {
        let state = if self.region.next_float() < 0.33 {
            self.block("minecraft:sandstone")
        } else {
            self.block("minecraft:sand")
        };
        self.put(state, x, y, z);
    }

    fn collapsed_roof(&mut self, x0: i32, y: i32, z0: i32, x1: i32, z1: i32) {
        for x in x0..=x1 {
            for z in z0..=z1 {
                self.roof_piece(x, y, z);
            }
        }
        let corner = self.world(x0, y, z0);
        let mut rng = thread_local_at(self.seed, corner[0], corner[1], corner[2]);
        let rx = rng.next_int_bound(x1 - x0 + 1) + x0;
        let rz = rng.next_int_bound(z1 - z0 + 1) + z0;
        self.roof = self.world(rx, y, rz);
    }

    /// `DesertPyramidStructure.afterPlace`: крыша всегда `suspicious_sand`,
    /// из списка — от 5 до 7 подозрительных, остальные песок. Лут кисти не пишется.
    fn suspicious(&mut self) {
        let suspicious = self.block("minecraft:suspicious_sand");
        let sand = self.block("minecraft:sand");
        self.raw(suspicious, self.roof);
        self.sands
            .sort_by(|a, b| a[1].cmp(&b[1]).then(a[2].cmp(&b[2])).then(a[0].cmp(&b[0])));
        self.sands.dedup();
        let center = [
            (self.min[0] + self.max[0]) / 2,
            (self.min[1] + self.max[1]) / 2,
            (self.min[2] + self.max[2]) / 2,
        ];
        let mut rng = thread_local_at(self.seed, center[0], center[1], center[2]);
        let mut i = self.sands.len();
        while i > 1 {
            let j = rng.next_int_bound(i as i32) as usize;
            i -= 1;
            self.sands.swap(i, j);
        }
        let mut left = (rng.next_int_bound(3) + 5) as usize;
        left = left.min(self.sands.len());
        for p in self.sands.clone() {
            if left > 0 {
                left -= 1;
                self.raw(suspicious, p);
            } else {
                self.raw(sand, p);
            }
        }
    }

    fn desert(&mut self) {
        let sand = self.block("minecraft:sandstone");
        let cut = self.block("minecraft:cut_sandstone");
        let chisel = self.block("minecraft:chiseled_sandstone");
        let air = self.block("minecraft:air");
        let orange = self.block("minecraft:orange_terracotta");
        let blue = self.block("minecraft:blue_terracotta");
        let stairs = self.block("minecraft:sandstone_stairs");
        let north = self.with(stairs, "facing", "north");
        let south = self.with(stairs, "facing", "south");
        let east = self.with(stairs, "facing", "east");
        let west = self.with(stairs, "facing", "west");
        let w = self.width;
        let d = self.depth;
        self.fill(0, -4, 0, w - 1, 0, d - 1, sand, sand, false);
        for pos in 1..=9 {
            self.fill(
                pos,
                pos,
                pos,
                w - 1 - pos,
                pos,
                d - 1 - pos,
                sand,
                sand,
                false,
            );
            self.fill(
                pos + 1,
                pos,
                pos + 1,
                w - 2 - pos,
                pos,
                d - 2 - pos,
                air,
                air,
                false,
            );
        }
        for x in 0..w {
            for z in 0..d {
                self.column_down(sand, x, -5, z);
            }
        }
        self.fill(0, 0, 0, 4, 9, 4, sand, air, false);
        self.fill(1, 10, 1, 3, 10, 3, sand, sand, false);
        self.put(north, 2, 10, 0);
        self.put(south, 2, 10, 4);
        self.put(east, 0, 10, 2);
        self.put(west, 4, 10, 2);
        self.fill(w - 5, 0, 0, w - 1, 9, 4, sand, air, false);
        self.fill(w - 4, 10, 1, w - 2, 10, 3, sand, sand, false);
        self.put(north, w - 3, 10, 0);
        self.put(south, w - 3, 10, 4);
        self.put(east, w - 5, 10, 2);
        self.put(west, w - 1, 10, 2);
        self.fill(8, 0, 0, 12, 4, 4, sand, air, false);
        self.fill(9, 1, 0, 11, 3, 4, air, air, false);
        for (x, y) in [(9, 1), (9, 2), (9, 3), (10, 3), (11, 3), (11, 2), (11, 1)] {
            self.put(cut, x, y, 1);
        }
        self.fill(4, 1, 1, 8, 3, 3, sand, air, false);
        self.fill(4, 1, 2, 8, 2, 2, air, air, false);
        self.fill(12, 1, 1, 16, 3, 3, sand, air, false);
        self.fill(12, 1, 2, 16, 2, 2, air, air, false);
        self.fill(5, 4, 5, w - 6, 4, d - 6, sand, sand, false);
        self.fill(9, 4, 9, 11, 4, 11, air, air, false);
        self.fill(8, 1, 8, 8, 3, 8, cut, cut, false);
        self.fill(12, 1, 8, 12, 3, 8, cut, cut, false);
        self.fill(8, 1, 12, 8, 3, 12, cut, cut, false);
        self.fill(12, 1, 12, 12, 3, 12, cut, cut, false);
        self.fill(1, 1, 5, 4, 4, 11, sand, sand, false);
        self.fill(w - 5, 1, 5, w - 2, 4, 11, sand, sand, false);
        self.fill(6, 7, 9, 6, 7, 11, sand, sand, false);
        self.fill(w - 7, 7, 9, w - 7, 7, 11, sand, sand, false);
        self.fill(5, 5, 9, 5, 7, 11, cut, cut, false);
        self.fill(w - 6, 5, 9, w - 6, 7, 11, cut, cut, false);
        self.put(air, 5, 5, 10);
        self.put(air, 5, 6, 10);
        self.put(air, 6, 6, 10);
        self.put(air, w - 6, 5, 10);
        self.put(air, w - 6, 6, 10);
        self.put(air, w - 7, 6, 10);
        self.fill(2, 4, 4, 2, 6, 4, air, air, false);
        self.fill(w - 3, 4, 4, w - 3, 6, 4, air, air, false);
        self.put(north, 2, 4, 5);
        self.put(north, 2, 3, 4);
        self.put(north, w - 3, 4, 5);
        self.put(north, w - 3, 3, 4);
        self.fill(1, 1, 3, 2, 2, 3, sand, sand, false);
        self.fill(w - 3, 1, 3, w - 2, 2, 3, sand, sand, false);
        self.put(sand, 1, 1, 2);
        self.put(sand, w - 2, 1, 2);
        let slab = self.block("minecraft:sandstone_slab");
        self.put(slab, 1, 2, 2);
        self.put(slab, w - 2, 2, 2);
        self.put(west, 2, 1, 2);
        self.put(east, w - 3, 1, 2);
        self.fill(4, 3, 5, 4, 3, 17, sand, sand, false);
        self.fill(w - 5, 3, 5, w - 5, 3, 17, sand, sand, false);
        self.fill(3, 1, 5, 4, 2, 16, air, air, false);
        self.fill(w - 6, 1, 5, w - 5, 2, 16, air, air, false);
        let mut z = 5;
        while z <= 17 {
            self.put(cut, 4, 1, z);
            self.put(chisel, 4, 2, z);
            self.put(cut, w - 5, 1, z);
            self.put(chisel, w - 5, 2, z);
            z += 2;
        }
        for (x, z) in [
            (10, 7),
            (10, 8),
            (9, 9),
            (11, 9),
            (8, 10),
            (12, 10),
            (7, 10),
            (13, 10),
            (9, 11),
            (11, 11),
            (10, 12),
            (10, 13),
        ] {
            self.put(orange, x, 0, z);
        }
        self.put(blue, 10, 0, 10);
        let mut x = 0;
        while x < w {
            self.put(cut, x, 2, 1);
            self.put(orange, x, 2, 2);
            self.put(cut, x, 2, 3);
            self.put(cut, x, 3, 1);
            self.put(orange, x, 3, 2);
            self.put(cut, x, 3, 3);
            self.put(orange, x, 4, 1);
            self.put(chisel, x, 4, 2);
            self.put(orange, x, 4, 3);
            self.put(cut, x, 5, 1);
            self.put(orange, x, 5, 2);
            self.put(cut, x, 5, 3);
            self.put(orange, x, 6, 1);
            self.put(chisel, x, 6, 2);
            self.put(orange, x, 6, 3);
            self.put(orange, x, 7, 1);
            self.put(orange, x, 7, 2);
            self.put(orange, x, 7, 3);
            self.put(cut, x, 8, 1);
            self.put(cut, x, 8, 2);
            self.put(cut, x, 8, 3);
            x += w - 1;
        }
        x = 2;
        while x <= w - 3 {
            self.put(cut, x - 1, 2, 0);
            self.put(orange, x, 2, 0);
            self.put(cut, x + 1, 2, 0);
            self.put(cut, x - 1, 3, 0);
            self.put(orange, x, 3, 0);
            self.put(cut, x + 1, 3, 0);
            self.put(orange, x - 1, 4, 0);
            self.put(chisel, x, 4, 0);
            self.put(orange, x + 1, 4, 0);
            self.put(cut, x - 1, 5, 0);
            self.put(orange, x, 5, 0);
            self.put(cut, x + 1, 5, 0);
            self.put(orange, x - 1, 6, 0);
            self.put(chisel, x, 6, 0);
            self.put(orange, x + 1, 6, 0);
            self.put(orange, x - 1, 7, 0);
            self.put(orange, x, 7, 0);
            self.put(orange, x + 1, 7, 0);
            self.put(cut, x - 1, 8, 0);
            self.put(cut, x, 8, 0);
            self.put(cut, x + 1, 8, 0);
            x += w - 3 - 2;
        }
        self.fill(8, 4, 0, 12, 6, 0, cut, cut, false);
        self.put(air, 8, 6, 0);
        self.put(air, 12, 6, 0);
        self.put(orange, 9, 5, 0);
        self.put(chisel, 10, 5, 0);
        self.put(orange, 11, 5, 0);
        self.fill(8, -14, 8, 12, -11, 12, cut, cut, false);
        self.fill(8, -10, 8, 12, -10, 12, chisel, chisel, false);
        self.fill(8, -9, 8, 12, -9, 12, cut, cut, false);
        self.fill(8, -8, 8, 12, -1, 12, sand, sand, false);
        self.fill(9, -11, 9, 11, -1, 11, air, air, false);
        self.put(self.block("minecraft:stone_pressure_plate"), 10, -11, 10);
        self.fill(
            9,
            -13,
            9,
            11,
            -13,
            11,
            self.block("minecraft:tnt"),
            air,
            false,
        );
        self.put(air, 8, -11, 10);
        self.put(air, 8, -10, 10);
        self.put(chisel, 7, -10, 10);
        self.put(cut, 7, -11, 10);
        self.put(air, 12, -11, 10);
        self.put(air, 12, -10, 10);
        self.put(chisel, 13, -10, 10);
        self.put(cut, 13, -11, 10);
        self.put(air, 10, -11, 8);
        self.put(air, 10, -10, 8);
        self.put(chisel, 10, -10, 7);
        self.put(cut, 10, -11, 7);
        self.put(air, 10, -11, 12);
        self.put(air, 10, -10, 12);
        self.put(chisel, 10, -10, 13);
        self.put(cut, 10, -11, 13);
        for (sx, sz) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
            self.chest(
                10 + sx * 2,
                -11,
                10 + sz * 2,
                "minecraft:chests/desert_pyramid",
            );
        }
        self.cellar();
        self.suspicious();
    }

    fn cellar(&mut self) {
        let (x, y, z) = (16, -4, 13);
        let stairs = self.block("minecraft:sandstone_stairs");
        let spun = template::rotate_state(self.pal, stairs, Rot::Ccw90);
        self.put(spun, 13, -1, 17);
        self.put(spun, 14, -2, 17);
        self.put(spun, 15, -3, 17);
        let sand = self.block("minecraft:sand");
        let stone = self.block("minecraft:sandstone");
        let variant = self.region.next_long() & 1 != 0;
        self.put(sand, x - 4, y + 4, z + 4);
        self.put(sand, x - 3, y + 4, z + 4);
        self.put(sand, x - 2, y + 4, z + 4);
        self.put(sand, x - 1, y + 4, z + 4);
        self.put(sand, x, y + 4, z + 4);
        self.put(sand, x - 2, y + 3, z + 4);
        self.put(if variant { sand } else { stone }, x - 1, y + 3, z + 4);
        self.put(if variant { stone } else { sand }, x, y + 3, z + 4);
        self.put(sand, x - 1, y + 2, z + 4);
        self.put(stone, x, y + 2, z + 4);
        self.put(sand, x, y + 1, z + 4);
        let cut = self.block("minecraft:cut_sandstone");
        let chisel = self.block("minecraft:chiseled_sandstone");
        self.fill(x - 3, y + 1, z - 3, x - 3, y + 1, z + 2, cut, cut, true);
        self.fill(x + 3, y + 1, z - 3, x + 3, y + 1, z + 2, cut, cut, true);
        self.fill(x - 3, y + 1, z - 3, x + 3, y + 1, z - 2, cut, cut, true);
        self.fill(x - 3, y + 1, z + 3, x + 3, y + 1, z + 3, cut, cut, true);
        self.fill(
            x - 3,
            y + 2,
            z - 3,
            x - 3,
            y + 2,
            z + 2,
            chisel,
            chisel,
            true,
        );
        self.fill(
            x + 3,
            y + 2,
            z - 3,
            x + 3,
            y + 2,
            z + 2,
            chisel,
            chisel,
            true,
        );
        self.fill(
            x - 3,
            y + 2,
            z - 3,
            x + 3,
            y + 2,
            z - 2,
            chisel,
            chisel,
            true,
        );
        self.fill(
            x - 3,
            y + 2,
            z + 3,
            x + 3,
            y + 2,
            z + 3,
            chisel,
            chisel,
            true,
        );
        self.fill(x - 3, -1, z - 3, x - 3, -1, z + 2, cut, cut, true);
        self.fill(x + 3, -1, z - 3, x + 3, -1, z + 2, cut, cut, true);
        self.fill(x - 3, -1, z - 3, x + 3, -1, z - 2, cut, cut, true);
        self.fill(x - 3, -1, z + 3, x + 3, -1, z + 3, cut, cut, true);
        self.record_box(x - 2, y + 1, z - 2, x + 2, y + 3, z + 2);
        self.collapsed_roof(x - 2, y + 4, z - 2, x + 2, z + 2);
        let orange = self.block("minecraft:orange_terracotta");
        let blue = self.block("minecraft:blue_terracotta");
        self.put(blue, x, y, z);
        self.put(orange, x + 1, y, z - 1);
        self.put(orange, x + 1, y, z + 1);
        self.put(orange, x - 1, y, z - 1);
        self.put(orange, x - 1, y, z + 1);
        self.put(orange, x + 2, y, z);
        self.put(orange, x - 2, y, z);
        self.put(orange, x, y, z + 2);
        self.put(orange, x, y, z - 2);
        self.put(orange, x + 3, y, z);
        self.record_sand(x + 3, y + 1, z);
        self.record_sand(x + 3, y + 2, z);
        self.put(cut, x + 4, y + 1, z);
        self.put(chisel, x + 4, y + 2, z);
        self.put(orange, x - 3, y, z);
        self.record_sand(x - 3, y + 1, z);
        self.record_sand(x - 3, y + 2, z);
        self.put(cut, x - 4, y + 1, z);
        self.put(chisel, x - 4, y + 2, z);
        self.put(orange, x, y, z + 3);
        self.record_sand(x, y + 1, z + 3);
        self.record_sand(x, y + 2, z + 3);
        self.put(orange, x, y, z - 3);
        self.record_sand(x, y + 1, z - 3);
        self.record_sand(x, y + 2, z - 3);
        self.put(cut, x, y + 1, z - 4);
        self.put(chisel, x, -2, z - 4);
    }

    fn jungle(&mut self) {
        let w = self.width;
        let d = self.depth;
        self.moss(0, -4, 0, w - 1, 0, d - 1);
        self.moss(2, 1, 2, 9, 2, 2);
        self.moss(2, 1, 12, 9, 2, 12);
        self.moss(2, 1, 3, 2, 2, 11);
        self.moss(9, 1, 3, 9, 2, 11);
        self.moss(1, 3, 1, 10, 6, 1);
        self.moss(1, 3, 13, 10, 6, 13);
        self.moss(1, 3, 2, 1, 6, 12);
        self.moss(10, 3, 2, 10, 6, 12);
        self.moss(2, 3, 2, 9, 3, 12);
        self.moss(2, 6, 2, 9, 6, 12);
        self.moss(3, 7, 3, 8, 7, 11);
        self.moss(4, 8, 4, 7, 8, 10);
        self.air(3, 1, 3, 8, 2, 11);
        self.air(4, 3, 6, 7, 3, 9);
        self.air(2, 4, 2, 9, 5, 12);
        self.air(4, 6, 5, 7, 6, 9);
        self.air(5, 7, 6, 6, 7, 8);
        self.air(5, 1, 2, 6, 2, 2);
        self.air(5, 2, 12, 6, 2, 12);
        self.air(5, 5, 1, 6, 5, 1);
        self.air(5, 5, 13, 6, 5, 13);
        let air = self.block("minecraft:air");
        self.put(air, 1, 5, 5);
        self.put(air, 10, 5, 5);
        self.put(air, 1, 5, 9);
        self.put(air, 10, 5, 9);
        for z in [0, 14] {
            self.moss(2, 4, z, 2, 5, z);
            self.moss(4, 4, z, 4, 5, z);
            self.moss(7, 4, z, 7, 5, z);
            self.moss(9, 4, z, 9, 5, z);
        }
        self.moss(5, 6, 0, 6, 6, 0);
        for x in [0, 11] {
            let mut z = 2;
            while z <= 12 {
                self.moss(x, 4, z, x, 5, z);
                z += 2;
            }
            self.moss(x, 6, 5, x, 6, 5);
            self.moss(x, 6, 9, x, 6, 9);
        }
        self.moss(2, 7, 2, 2, 9, 2);
        self.moss(9, 7, 2, 9, 9, 2);
        self.moss(2, 7, 12, 2, 9, 12);
        self.moss(9, 7, 12, 9, 9, 12);
        self.moss(4, 9, 4, 4, 9, 4);
        self.moss(7, 9, 4, 7, 9, 4);
        self.moss(4, 9, 10, 4, 9, 10);
        self.moss(7, 9, 10, 7, 9, 10);
        self.moss(5, 9, 7, 6, 9, 7);
        let stairs = self.block("minecraft:cobblestone_stairs");
        let north = self.with(stairs, "facing", "north");
        let south = self.with(stairs, "facing", "south");
        let east = self.with(stairs, "facing", "east");
        let west = self.with(stairs, "facing", "west");
        self.put(north, 5, 9, 6);
        self.put(north, 6, 9, 6);
        self.put(south, 5, 9, 8);
        self.put(south, 6, 9, 8);
        self.put(north, 4, 0, 0);
        self.put(north, 5, 0, 0);
        self.put(north, 6, 0, 0);
        self.put(north, 7, 0, 0);
        self.put(north, 4, 1, 8);
        self.put(north, 4, 2, 9);
        self.put(north, 4, 3, 10);
        self.put(north, 7, 1, 8);
        self.put(north, 7, 2, 9);
        self.put(north, 7, 3, 10);
        self.moss(4, 1, 9, 4, 1, 9);
        self.moss(7, 1, 9, 7, 1, 9);
        self.moss(4, 1, 10, 7, 2, 10);
        self.moss(5, 4, 5, 6, 4, 5);
        self.put(east, 4, 4, 5);
        self.put(west, 7, 4, 5);
        for i in 0..4 {
            self.put(south, 5, -i, 6 + i);
            self.put(south, 6, -i, 6 + i);
            self.air(5, -i, 7 + i, 6, -i, 9 + i);
        }
        self.air(1, -3, 12, 10, -1, 13);
        self.air(1, -3, 1, 3, -1, 13);
        self.air(1, -3, 1, 9, -1, 5);
        let mut z = 1;
        while z <= 13 {
            self.moss(1, -3, z, 1, -2, z);
            z += 2;
        }
        z = 2;
        while z <= 12 {
            self.moss(1, -1, z, 3, -1, z);
            z += 2;
        }
        self.moss(2, -2, 1, 5, -2, 1);
        self.moss(7, -2, 1, 9, -2, 1);
        self.moss(6, -3, 1, 6, -3, 1);
        self.moss(6, -1, 1, 6, -1, 1);
        let hook = self.block("minecraft:tripwire_hook");
        let wire = self.block("minecraft:tripwire");
        let redstone = self.block("minecraft:redstone_wire");
        let vine = self.block("minecraft:vine");
        let mossy = self.block("minecraft:mossy_cobblestone");
        self.put(
            self.with(self.with(hook, "facing", "east"), "attached", "true"),
            1,
            -3,
            8,
        );
        self.put(
            self.with(self.with(hook, "facing", "west"), "attached", "true"),
            4,
            -3,
            8,
        );
        let trip_ew = self.with(
            self.with(self.with(wire, "east", "true"), "west", "true"),
            "attached",
            "true",
        );
        self.put(trip_ew, 2, -3, 8);
        self.put(trip_ew, 3, -3, 8);
        let ns = self.with(self.with(redstone, "north", "side"), "south", "side");
        for z in 2..=7 {
            self.put(ns, 5, -3, z);
        }
        self.put(
            self.with(self.with(redstone, "north", "side"), "west", "side"),
            5,
            -3,
            1,
        );
        self.put(
            self.with(self.with(redstone, "east", "side"), "west", "side"),
            4,
            -3,
            1,
        );
        self.put(mossy, 3, -3, 1);
        self.dispenser(
            3,
            -2,
            1,
            "north",
            "minecraft:chests/jungle_temple_dispenser",
        );
        self.put(self.with(vine, "south", "true"), 3, -2, 2);
        self.put(
            self.with(self.with(hook, "facing", "north"), "attached", "true"),
            7,
            -3,
            1,
        );
        self.put(
            self.with(self.with(hook, "facing", "south"), "attached", "true"),
            7,
            -3,
            5,
        );
        let trip_ns = self.with(
            self.with(self.with(wire, "north", "true"), "south", "true"),
            "attached",
            "true",
        );
        self.put(trip_ns, 7, -3, 2);
        self.put(trip_ns, 7, -3, 3);
        self.put(trip_ns, 7, -3, 4);
        self.put(
            self.with(self.with(redstone, "east", "side"), "west", "side"),
            8,
            -3,
            6,
        );
        self.put(
            self.with(self.with(redstone, "west", "side"), "south", "side"),
            9,
            -3,
            6,
        );
        self.put(
            self.with(self.with(redstone, "north", "side"), "south", "up"),
            9,
            -3,
            5,
        );
        self.put(mossy, 9, -3, 4);
        self.put(ns, 9, -2, 4);
        self.dispenser(9, -2, 3, "west", "minecraft:chests/jungle_temple_dispenser");
        self.put(self.with(vine, "east", "true"), 8, -1, 3);
        self.put(self.with(vine, "east", "true"), 8, -2, 3);
        self.chest(8, -3, 3, "minecraft:chests/jungle_temple");
        self.put(mossy, 9, -3, 2);
        self.put(mossy, 8, -3, 1);
        self.put(mossy, 4, -3, 5);
        self.put(mossy, 5, -2, 5);
        self.put(mossy, 5, -1, 5);
        self.put(mossy, 6, -3, 5);
        self.put(mossy, 7, -2, 5);
        self.put(mossy, 7, -1, 5);
        self.put(mossy, 8, -3, 5);
        self.moss(9, -1, 1, 9, -1, 5);
        self.air(8, -3, 8, 10, -1, 10);
        let bricks = self.block("minecraft:chiseled_stone_bricks");
        self.put(bricks, 8, -2, 11);
        self.put(bricks, 9, -2, 11);
        self.put(bricks, 10, -2, 11);
        let lever = self.with(
            self.with(self.block("minecraft:lever"), "facing", "north"),
            "face",
            "wall",
        );
        self.put(lever, 8, -2, 12);
        self.put(lever, 9, -2, 12);
        self.put(lever, 10, -2, 12);
        self.moss(8, -3, 8, 8, -3, 10);
        self.moss(10, -3, 8, 10, -3, 10);
        self.put(mossy, 10, -2, 9);
        self.put(ns, 8, -2, 9);
        self.put(ns, 8, -2, 10);
        self.put(
            self.with(
                self.with(
                    self.with(self.with(redstone, "north", "side"), "south", "side"),
                    "east",
                    "side",
                ),
                "west",
                "side",
            ),
            10,
            -1,
            9,
        );
        let piston = self.block("minecraft:sticky_piston");
        self.put(self.with(piston, "facing", "up"), 9, -2, 8);
        self.put(self.with(piston, "facing", "west"), 10, -2, 8);
        self.put(self.with(piston, "facing", "west"), 10, -1, 8);
        self.put(
            self.with(self.block("minecraft:repeater"), "facing", "north"),
            10,
            -2,
            10,
        );
        self.chest(9, -3, 10, "minecraft:chests/jungle_temple");
    }

    /// `SwampHutPiece.postProcess`. Ведьма и кошка не пишутся.
    fn swamp(&mut self) {
        let planks = self.block("minecraft:spruce_planks");
        let log = self.block("minecraft:oak_log");
        let fence = self.block("minecraft:oak_fence");
        let stairs = self.block("minecraft:spruce_stairs");
        self.fill(1, 1, 1, 5, 1, 7, planks, planks, false);
        self.fill(1, 4, 2, 5, 4, 7, planks, planks, false);
        self.fill(2, 1, 0, 4, 1, 0, planks, planks, false);
        self.fill(2, 2, 2, 3, 3, 2, planks, planks, false);
        self.fill(1, 2, 3, 1, 3, 6, planks, planks, false);
        self.fill(5, 2, 3, 5, 3, 6, planks, planks, false);
        self.fill(2, 2, 7, 4, 3, 7, planks, planks, false);
        self.fill(1, 0, 2, 1, 3, 2, log, log, false);
        self.fill(5, 0, 2, 5, 3, 2, log, log, false);
        self.fill(1, 0, 7, 1, 3, 7, log, log, false);
        self.fill(5, 0, 7, 5, 3, 7, log, log, false);
        self.put(fence, 2, 3, 2);
        self.put(fence, 3, 3, 7);
        let air = self.block("minecraft:air");
        self.put(air, 1, 3, 4);
        self.put(air, 5, 3, 4);
        self.put(air, 5, 3, 5);
        self.put(self.block("minecraft:potted_red_mushroom"), 1, 3, 5);
        self.put(self.block("minecraft:crafting_table"), 3, 2, 6);
        self.put(self.block("minecraft:cauldron"), 4, 2, 6);
        self.put(fence, 1, 2, 1);
        self.put(fence, 5, 2, 1);
        let north = self.with(stairs, "facing", "north");
        let east = self.with(stairs, "facing", "east");
        let west = self.with(stairs, "facing", "west");
        let south = self.with(stairs, "facing", "south");
        self.fill(0, 4, 1, 6, 4, 1, north, north, false);
        self.fill(0, 4, 2, 0, 4, 7, east, east, false);
        self.fill(6, 4, 2, 6, 4, 7, west, west, false);
        self.fill(0, 4, 8, 6, 4, 8, south, south, false);
        self.put(self.with(north, "shape", "outer_right"), 0, 4, 1);
        self.put(self.with(north, "shape", "outer_left"), 6, 4, 1);
        self.put(self.with(south, "shape", "outer_left"), 0, 4, 8);
        self.put(self.with(south, "shape", "outer_right"), 6, 4, 8);
        for z in [2, 7] {
            for x in [1, 5] {
                self.column_down(log, x, -1, z);
            }
        }
        // `SHAPE_CHECK_BLOCKS`: забор связывается с прочными гранями соседей
        // уже после того, как стоят брёвна и доски.
        for (x, y, z) in [(2, 3, 2), (3, 3, 7), (1, 2, 1), (5, 2, 1)] {
            self.connect_fence(x, y, z);
        }
    }

    /// `FenceBlock.connectsTo`: сторона смотрит, прочна ли грань соседа к забору.
    fn connect_fence(&mut self, x: i32, y: i32, z: i32) {
        let p = self.world(x, y, z);
        if !self.chunk.contains(p) || !self.level.can_write(p[0], p[2]) {
            return;
        }
        let mut state = self.level.get(p[0], p[1], p[2]);
        let sides = [
            ("north", [0, 0, -1], 3usize),
            ("south", [0, 0, 1], 2),
            ("west", [-1, 0, 0], 5),
            ("east", [1, 0, 0], 4),
        ];
        for (prop, delta, face) in sides {
            let neighbor = self
                .level
                .get(p[0] + delta[0], p[1] + delta[1], p[2] + delta[2]);
            let value = if self.pal.sturdy(neighbor, face) {
                "true"
            } else {
                "false"
            };
            if let Some(next) = self.pal.with_prop(state, prop, value) {
                state = next;
            }
        }
        self.level.set(p[0], p[1], p[2], state);
    }
}

fn apply_orient(pal: &Palette, state: u32, mirror_ns: bool, rot: Rot) -> u32 {
    let state = if mirror_ns {
        mirror_left_right(pal, state)
    } else {
        state
    };
    template::rotate_state(pal, state, rot)
}

/// `Mirror.LEFT_RIGHT`: север и юг меняются местами.
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
    if let Some(face) = pal.prop(state, "facing")
        && matches!(face, "north" | "south")
        && let Some(shape) = pal.prop(state, "shape")
    {
        let swapped = match shape {
            "inner_left" => "inner_right",
            "inner_right" => "inner_left",
            "outer_left" => "outer_right",
            "outer_right" => "outer_left",
            _ => "",
        };
        if !swapped.is_empty()
            && let Some(next) = pal.with_prop(state, "shape", swapped)
        {
            state = next;
        }
    }
    let north = pal.prop(state, "north").map(str::to_owned);
    let south = pal.prop(state, "south").map(str::to_owned);
    if let (Some(north), Some(south)) = (north, south) {
        if let Some(next) = pal.with_prop(state, "north", &south) {
            state = next;
        }
        if let Some(next) = pal.with_prop(state, "south", &north) {
            state = next;
        }
    }
    state
}

fn replaceable(pal: &Palette, state: u32) -> bool {
    pal.is_air(state)
        || pal.fluid(state) != Fluid::None
        || same(pal, state, "minecraft:glow_lichen")
        || same(pal, state, "minecraft:seagrass")
        || same(pal, state, "minecraft:tall_seagrass")
}

fn same(pal: &Palette, state: u32, name: &str) -> bool {
    pal.defaults
        .get(name)
        .is_some_and(|block| pal.same_block(state, *block))
}

/// `StructurePiece.reorient` для сундука без заданной стороны.
fn reorient(pal: &Palette, level: &dyn Level, p: [i32; 3], chest: u32) -> u32 {
    let at = |d: usize| {
        let (x, z) = match d {
            0 => (0, -1),
            1 => (1, 0),
            2 => (0, 1),
            _ => (-1, 0),
        };
        level.get(p[0] + x, p[1], p[2] + z)
    };
    let solid = |d: usize| {
        let s = at(d);
        Palette::flag(&pal.solid, s) && pal.sturdy.get(s as usize) == Some(&63)
    };
    let mut neighbor = None;
    for d in 0..4 {
        if pal.same_block(at(d), chest) {
            return chest;
        }
        if solid(d) {
            if neighbor.is_some() {
                neighbor = None;
                break;
            }
            neighbor = Some(d);
        }
    }
    let facing = match neighbor {
        Some(d) => (d + 2) % 4,
        None => {
            let mut d = 0;
            if solid(d) {
                d = 2;
            }
            if solid(d) {
                d = (d + 1) % 4;
            }
            if solid(d) {
                d = (d + 2) % 4;
            }
            d
        }
    };
    pal.with_prop(chest, "facing", FACES[facing])
        .unwrap_or(chest)
}
