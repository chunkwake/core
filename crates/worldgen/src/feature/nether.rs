//! Фичи Нижнего мира: столбы базальта, дельты, пятна, светокамень и лианы,
//! огромные грибы.

use super::{DIRECTION_NAMES, DIRS, Features, Level, Palette, Placed, Predicate, Provider, block_set};
use crate::carver::IntP;
use crate::random::{Bits, Worldgen};
use serde_json::Value;
use std::sync::Arc;

type Pos = [i32; 3];

fn get(level: &dyn Level, [x, y, z]: Pos) -> u32 {
    level.get(x, y, z)
}

/// `WorldGenRegion.setBlock`: вне радиуса записи блок не ставится.
fn put(level: &mut dyn Level, [x, y, z]: Pos, state: u32) {
    if level.can_write(x, z) {
        level.set(x, y, z, state);
    }
}

fn add(a: Pos, b: Pos) -> Pos {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn dist_manhattan(a: Pos, b: Pos) -> i32 {
    (a[0] - b[0]).abs() + (a[1] - b[1]).abs() + (a[2] - b[2]).abs()
}

fn outside_height(level: &dyn Level, y: i32) -> bool {
    y < level.min_y() || y >= level.min_y() + level.height()
}

/// `Mth.nextInt(random, min, max)`.
fn next_int(r: &mut Worldgen, min: i32, max: i32) -> i32 {
    if min >= max {
        min
    } else {
        r.next_int_bound(max - min + 1) + min
    }
}

/// `BlockPos.manhattanOrdered`: слои по манхэттенскому расстоянию, внутри
/// слоя `x`, затем `y`, а `z` — парами `+z`, `-z`.
pub(crate) fn manhattan_ordered(origin: Pos, reach: Pos, max_depth: i32) -> Vec<Pos> {
    let mut out = Vec::new();
    for depth in 0..=max_depth {
        let max_x = reach[0].min(depth);
        for x in -max_x..=max_x {
            let max_y = reach[1].min(depth - x.abs());
            for y in -max_y..=max_y {
                let z = depth - x.abs() - y.abs();
                if z <= reach[2] {
                    out.push([origin[0] + x, origin[1] + y, origin[2] + z]);
                    if z != 0 {
                        out.push([origin[0] + x, origin[1] + y, origin[2] - z]);
                    }
                }
            }
        }
    }
    out
}

/// `BlockPos.withinBoxByManhattanDistance`.
fn within_box_by_manhattan(origin: Pos, rx: i32, ry: i32, rz: i32) -> Vec<Pos> {
    manhattan_ordered(origin, [rx, ry, rz], rx + ry + rz)
}

fn direction(v: &Value) -> Result<Pos, String> {
    let name = v.as_str().unwrap_or("up");
    DIRECTION_NAMES
        .iter()
        .position(|n| *n == name)
        .map(|i| DIRS[i])
        .ok_or_else(|| format!("direction {name}"))
}

/// `single_block_pillar`: столб, пока клетка заменяема и бросок удачен.
#[derive(Debug)]
pub(crate) struct Pillar {
    block: Provider,
    can_replace: Predicate,
    direction: Pos,
    chance: f32,
    cap: Option<Arc<Placed>>,
}

impl Pillar {
    pub(crate) fn parse(
        v: &Value,
        palette: &mut Palette,
        block: Provider,
        cap: Option<Arc<Placed>>,
    ) -> Result<Self, String> {
        Ok(Self {
            block,
            can_replace: match v.get("can_replace") {
                Some(p) if !p.is_null() => Predicate::parse(p, palette)?,
                _ => Predicate::True,
            },
            direction: direction(&v["direction"])?,
            chance: v["chance_to_continue"].as_f64().unwrap_or(1.0) as f32,
            cap,
        })
    }

    pub(crate) fn place(&self, f: &Features, level: &mut dyn Level, r: &mut Worldgen, origin: Pos) {
        let pal = &f.palette;
        let mut pos = origin;
        while self.can_replace.test(&*level, pal, pos)
            && r.next_float() < self.chance
            && !outside_height(&*level, pos[1])
        {
            if let Some(s) = self.block.get(&*level, pal, r, pos) {
                put(level, pos, s);
            }
            pos = add(pos, self.direction);
        }
        pos = [
            pos[0] - self.direction[0],
            pos[1] - self.direction[1],
            pos[2] - self.direction[2],
        ];
        if let Some(cap) = &self.cap {
            f.place(level, cap, None, r, pos);
        }
    }
}

/// `delta_feature`: лужи магмы или лавы с каймой базальта.
#[derive(Debug)]
pub(crate) struct Delta {
    contents: u32,
    rim: u32,
    size: IntP,
    rim_size: IntP,
    cannot_replace: Arc<Vec<bool>>,
}

impl Delta {
    pub(crate) fn parse(v: &Value, palette: &mut Palette) -> Result<Self, String> {
        Ok(Self {
            contents: palette.state(&v["contents"])?,
            rim: palette.state(&v["rim"])?,
            size: IntP::parse(&v["size"])?,
            rim_size: IntP::parse(&v["rim_size"])?,
            cannot_replace: block_set(
                &serde_json::json!([
                    "minecraft:bedrock",
                    "minecraft:nether_bricks",
                    "minecraft:nether_brick_fence",
                    "minecraft:nether_brick_stairs",
                    "minecraft:nether_wart",
                    "minecraft:chest",
                    "minecraft:spawner"
                ]),
                palette,
            )?,
        })
    }

    pub(crate) fn place(&self, f: &Features, level: &mut dyn Level, r: &mut Worldgen, origin: Pos) {
        let spawn_rim = r.next_double() < 0.9;
        let rim_x = if spawn_rim { self.rim_size.sample(r) } else { 0 };
        let rim_z = if spawn_rim { self.rim_size.sample(r) } else { 0 };
        let has_rim = spawn_rim && rim_x != 0 && rim_z != 0;
        let radius_x = self.size.sample(r);
        let radius_z = self.size.sample(r);
        let limit = radius_x.max(radius_z);
        for pos in within_box_by_manhattan(origin, radius_x, 0, radius_z) {
            if dist_manhattan(pos, origin) > limit {
                break;
            }
            if self.is_clear(f, &*level, pos) {
                if has_rim {
                    put(level, pos, self.rim);
                }
                let off = add(pos, [rim_x, 0, rim_z]);
                if self.is_clear(f, &*level, off) {
                    put(level, off, self.contents);
                }
            }
        }
    }

    fn is_clear(&self, f: &Features, level: &dyn Level, pos: Pos) -> bool {
        let pal = &f.palette;
        let state = get(level, pos);
        if pal.same_block(state, self.contents) || Palette::flag(&self.cannot_replace, state) {
            return false;
        }
        // `Direction.values()`: вниз, вверх, север, юг, запад, восток.
        for (i, d) in DIRS.iter().enumerate() {
            let air = pal.is_air(get(level, add(pos, *d)));
            let up = i == 1;
            if (air && !up) || (!air && up) {
                return false;
            }
        }
        true
    }
}

/// `stepped_column_cluster`: гроздь базальтовых столбов.
#[derive(Debug)]
pub(crate) struct ColumnCluster {
    block: Provider,
    continue_through: Predicate,
    can_replace: Predicate,
    cannot_place_on: Arc<Vec<bool>>,
    cluster_reach: IntP,
    column_count: IntP,
    column_reach: IntP,
    height: IntP,
}

impl ColumnCluster {
    pub(crate) fn parse(v: &Value, palette: &mut Palette, block: Provider) -> Result<Self, String> {
        Ok(Self {
            block,
            continue_through: Predicate::parse(&v["continue_through"], palette)?,
            can_replace: Predicate::parse(&v["can_replace"], palette)?,
            cannot_place_on: block_set(&v["cannot_place_on"], palette)?,
            cluster_reach: IntP::parse(&v["cluster_reach"])?,
            column_count: IntP::parse(&v["column_count"])?,
            column_reach: IntP::parse(&v["column_reach"])?,
            height: IntP::parse(&v["height"])?,
        })
    }

    pub(crate) fn place(&self, f: &Features, level: &mut dyn Level, r: &mut Worldgen, origin: Pos) {
        let sea = level.sea_level();
        if !self.can_place_at(f, &*level, origin) {
            return;
        }
        let column_height = self.height.sample(r);
        let reach = column_height.min(self.cluster_reach.sample(r));
        let count = self.column_count.sample(r);
        let width = 2 * reach + 1;
        // `BlockPos.randomBetweenClosed` ленив: броски точки чередуются с
        // бросками самого столба.
        for _ in 0..count {
            let x = origin[0] - reach + r.next_int_bound(width);
            let y = origin[1] + r.next_int_bound(1);
            let z = origin[2] - reach + r.next_int_bound(width);
            let pos = [x, y, z];
            let blocks_y = column_height - dist_manhattan(pos, origin);
            if blocks_y >= 0 {
                let column_reach = self.column_reach.sample(r);
                self.place_column(f, level, r, sea, pos, blocks_y, column_reach);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn place_column(
        &self,
        f: &Features,
        level: &mut dyn Level,
        r: &mut Worldgen,
        sea: i32,
        origin: Pos,
        height: i32,
        reach: i32,
    ) {
        let pal = &f.palette;
        let _ = sea;
        // `BlockPos.betweenClosed`: `x` быстрее `z`.
        for dz in -reach..=reach {
            for dx in -reach..=reach {
                let pos = [origin[0] + dx, origin[1], origin[2] + dz];
                let step = dist_manhattan(pos, origin);
                let column = if self.can_replace.test(&*level, pal, pos) {
                    self.find_surface(f, &*level, pos, step)
                } else {
                    self.find_air(f, &*level, pos, step)
                };
                let Some(mut cursor) = column else {
                    continue;
                };
                let mut blocks = height - step / 2;
                while blocks >= 0 {
                    if self.can_replace.test(&*level, pal, cursor) {
                        if let Some(s) = self.block.get(&*level, pal, r, cursor) {
                            put(level, cursor, s);
                        }
                        cursor[1] += 1;
                    } else {
                        if !self.continue_through.test(&*level, pal, cursor) {
                            break;
                        }
                        cursor[1] += 1;
                    }
                    blocks -= 1;
                }
            }
        }
    }

    fn find_surface(&self, f: &Features, level: &dyn Level, mut cursor: Pos, mut limit: i32) -> Option<Pos> {
        while cursor[1] > level.min_y() + 1 && limit > 0 {
            limit -= 1;
            if self.can_place_at(f, level, cursor) {
                return Some(cursor);
            }
            cursor[1] -= 1;
        }
        None
    }

    fn can_place_at(&self, f: &Features, level: &dyn Level, cursor: Pos) -> bool {
        let pal = &f.palette;
        if !self.can_replace.test(level, pal, cursor) {
            return false;
        }
        let below = get(level, [cursor[0], cursor[1] - 1, cursor[2]]);
        !pal.is_air(below) && !Palette::flag(&self.cannot_place_on, below)
    }

    fn find_air(&self, f: &Features, level: &dyn Level, mut cursor: Pos, mut limit: i32) -> Option<Pos> {
        let max_y = level.min_y() + level.height() - 1;
        while cursor[1] <= max_y && limit > 0 {
            limit -= 1;
            let state = get(level, cursor);
            if Palette::flag(&self.cannot_place_on, state) {
                return None;
            }
            if f.palette.is_air(state) {
                return Some(cursor);
            }
            cursor[1] += 1;
        }
        None
    }
}

/// `netherrack_replace_blobs`: пятна базальта или чернокамня в незераке.
#[derive(Debug)]
pub(crate) struct ReplaceBlobs {
    target: u32,
    state: u32,
    radius: IntP,
}

impl ReplaceBlobs {
    pub(crate) fn parse(v: &Value, palette: &mut Palette) -> Result<Self, String> {
        Ok(Self {
            target: palette.state(&v["target"])?,
            state: palette.state(&v["state"])?,
            radius: IntP::parse(&v["radius"])?,
        })
    }

    pub(crate) fn place(&self, f: &Features, level: &mut dyn Level, r: &mut Worldgen, origin: Pos) {
        let pal = &f.palette;
        let max_y = level.min_y() + level.height() - 1;
        let mut cursor = [origin[0], origin[1].clamp(level.min_y() + 1, max_y), origin[2]];
        // `findTarget`.
        let center = loop {
            if cursor[1] <= level.min_y() + 1 {
                return;
            }
            if pal.same_block(get(&*level, cursor), self.target) {
                break cursor;
            }
            cursor[1] -= 1;
        };
        let rx = self.radius.sample(r);
        let ry = self.radius.sample(r);
        let rz = self.radius.sample(r);
        let max = rx.max(ry.max(rz));
        for pos in within_box_by_manhattan(center, rx, ry, rz) {
            if dist_manhattan(pos, center) > max {
                break;
            }
            if pal.same_block(get(&*level, pos), self.target) {
                put(level, pos, self.state);
            }
        }
    }
}

/// `random_neighbor_spread`: светокамень и плакучие лианы под потолком.
#[derive(Debug)]
pub(crate) struct NeighborSpread {
    block: Provider,
    accepted: Arc<Vec<bool>>,
    can_replace: Predicate,
    attempts: IntP,
    xz: IntP,
    y: IntP,
}

impl NeighborSpread {
    pub(crate) fn parse(v: &Value, palette: &mut Palette, block: Provider) -> Result<Self, String> {
        Ok(Self {
            block,
            accepted: block_set(&v["accepted_neighbors"], palette)?,
            can_replace: Predicate::parse(&v["can_replace"], palette)?,
            attempts: IntP::parse(&v["attempts"])?,
            xz: IntP::parse(&v["xz_offset"])?,
            y: IntP::parse(&v["y_offset"])?,
        })
    }

    pub(crate) fn place(&self, f: &Features, level: &mut dyn Level, r: &mut Worldgen, origin: Pos) {
        let pal = &f.palette;
        if let Some(s) = self.block.get(&*level, pal, r, origin) {
            put(level, origin, s);
        }
        let attempts = self.attempts.sample(r);
        for _ in 0..attempts {
            let dx = self.xz.sample(r);
            let dy = self.y.sample(r);
            let dz = self.xz.sample(r);
            let at = add(origin, [dx, dy, dz]);
            if !self.can_replace.test(&*level, pal, at) {
                continue;
            }
            let mut neighbours = 0;
            for d in DIRS {
                if Palette::flag(&self.accepted, get(&*level, add(at, d))) {
                    neighbours += 1;
                }
                if neighbours > 1 {
                    break;
                }
            }
            if neighbours == 1
                && let Some(s) = self.block.get(&*level, pal, r, at)
            {
                put(level, at, s);
            }
        }
    }
}

/// `huge_fungus`: багровый и искажённый грибы.
#[derive(Debug)]
pub(crate) struct HugeFungus {
    base: u32,
    stem: u32,
    hat: u32,
    decor: u32,
    replaceable: Predicate,
    planted: bool,
    /// `hat_state` — `nether_wart_block`: с него свисают плакучие лианы.
    vines: bool,
    air: u32,
    weeping_vines: u32,
    weeping_plant: u32,
}

impl HugeFungus {
    pub(crate) fn parse(v: &Value, palette: &mut Palette) -> Result<Self, String> {
        let hat = palette.state(&v["hat_state"])?;
        let wart = palette.state(&Value::String("minecraft:nether_wart_block".into()))?;
        Ok(Self {
            base: palette.state(&v["valid_base_block"])?,
            stem: palette.state(&v["stem_state"])?,
            hat,
            decor: palette.state(&v["decor_state"])?,
            replaceable: Predicate::parse(&v["replaceable_blocks"], palette)?,
            planted: v["planted"].as_bool().unwrap_or(false),
            vines: palette.same_block(hat, wart),
            air: palette.state(&Value::String("minecraft:air".into()))?,
            weeping_vines: palette.state(&Value::String("minecraft:weeping_vines".into()))?,
            weeping_plant: palette.state(&Value::String("minecraft:weeping_vines_plant".into()))?,
        })
    }

    pub(crate) fn place(&self, f: &Features, level: &mut dyn Level, r: &mut Worldgen, origin: Pos) {
        let pal = &f.palette;
        let below = get(&*level, [origin[0], origin[1] - 1, origin[2]]);
        if !pal.same_block(below, self.base) {
            return;
        }
        let mut total = next_int(r, 4, 13);
        if r.next_int_bound(12) == 0 {
            total *= 2;
        }
        // `ChunkGenerator.getGenDepth`: высота шума, без сдвига на `minY`.
        if !self.planted && origin[1] + total + 1 >= level.height() {
            return;
        }
        let huge = !self.planted && r.next_float() < 0.06;
        put(level, origin, self.air);
        self.stem(f, level, r, origin, total, huge);
        self.hat(f, level, r, origin, total, huge);
    }

    fn replaceable(&self, f: &Features, level: &dyn Level, pos: Pos, plants: bool) -> bool {
        if Palette::flag(&f.palette.replaceable, get(level, pos)) {
            return true;
        }
        plants && self.replaceable.test(level, &f.palette, pos)
    }

    fn stem(&self, f: &Features, level: &mut dyn Level, r: &mut Worldgen, o: Pos, total: i32, huge: bool) {
        let radius = i32::from(huge);
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                let corner = huge && dx.abs() == radius && dz.abs() == radius;
                for dy in 0..total {
                    let pos = add(o, [dx, dy, dz]);
                    if !self.replaceable(f, &*level, pos, true) {
                        continue;
                    }
                    if corner {
                        if r.next_float() < 0.1 {
                            put(level, pos, self.stem);
                        }
                    } else {
                        put(level, pos, self.stem);
                    }
                }
            }
        }
    }

    fn hat(&self, f: &Features, level: &mut dyn Level, r: &mut Worldgen, o: Pos, total: i32, huge: bool) {
        let hat_height = (r.next_int_bound(1 + total / 3) + 5).min(total);
        let start = total - hat_height;
        for dy in start..=total {
            let mut radius = if dy < total - r.next_int_bound(3) { 2 } else { 1 };
            if hat_height > 8 && dy < start + 4 {
                radius = 3;
            }
            if huge {
                radius += 1;
            }
            for dx in -radius..=radius {
                for dz in -radius..=radius {
                    let edge_x = dx == -radius || dx == radius;
                    let edge_z = dz == -radius || dz == radius;
                    let inside = !edge_x && !edge_z && dy != total;
                    let corner = edge_x && edge_z;
                    let bottom = dy < start + 3;
                    let pos = add(o, [dx, dy, dz]);
                    if !self.replaceable(f, &*level, pos, false) {
                        continue;
                    }
                    let vines = self.vines;
                    if bottom {
                        if !inside {
                            self.drop_block(f, level, r, pos);
                        }
                    } else if inside {
                        self.hat_block(f, level, r, pos, 0.1, 0.2, if vines { 0.1 } else { 0.0 });
                    } else if corner {
                        self.hat_block(f, level, r, pos, 0.01, 0.7, if vines { 0.083 } else { 0.0 });
                    } else {
                        self.hat_block(f, level, r, pos, 5.0e-4, 0.98, if vines { 0.07 } else { 0.0 });
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn hat_block(
        &self,
        f: &Features,
        level: &mut dyn Level,
        r: &mut Worldgen,
        pos: Pos,
        decor: f32,
        hat: f32,
        vines: f32,
    ) {
        if r.next_float() < decor {
            put(level, pos, self.decor);
        } else if r.next_float() < hat {
            put(level, pos, self.hat);
            if r.next_float() < vines {
                self.weeping_vines(f, level, r, pos);
            }
        }
    }

    fn drop_block(&self, f: &Features, level: &mut dyn Level, r: &mut Worldgen, pos: Pos) {
        let below = get(&*level, [pos[0], pos[1] - 1, pos[2]]);
        if f.palette.same_block(below, self.hat) {
            put(level, pos, self.hat);
        } else if (r.next_float() as f64) < 0.15 {
            put(level, pos, self.hat);
            if self.vines && r.next_int_bound(11) == 0 {
                self.weeping_vines(f, level, r, pos);
            }
        }
    }

    /// `tryPlaceWeepingVines` и `placeWeepingVinesColumn`.
    fn weeping_vines(&self, f: &Features, level: &mut dyn Level, r: &mut Worldgen, hat: Pos) {
        let pal = &f.palette;
        let mut at = [hat[0], hat[1] - 1, hat[2]];
        if !pal.is_air(get(&*level, at)) {
            return;
        }
        let mut goal = next_int(r, 1, 5);
        if r.next_int_bound(7) == 0 {
            goal *= 2;
        }
        for height in 0..=goal {
            if pal.is_air(get(&*level, at)) {
                let below = [at[0], at[1] - 1, at[2]];
                if height == goal || !pal.is_air(get(&*level, below)) {
                    let age = next_int(r, 23, 25).to_string();
                    let s = pal.with_prop(self.weeping_vines, "age", &age).unwrap_or(self.weeping_vines);
                    put(level, at, s);
                    break;
                }
                put(level, at, self.weeping_plant);
            }
            at[1] -= 1;
        }
    }
}

/// `projected_random_patchy_square`: пятно, опущенное до опоры не глубже
/// `max_projection_height`.
#[derive(Debug)]
pub(crate) struct PatchySquare {
    block: Provider,
    through: Predicate,
    size: IntP,
    max_drop: i32,
}

impl PatchySquare {
    pub(crate) fn parse(v: &Value, palette: &mut Palette, block: Provider) -> Result<Self, String> {
        Ok(Self {
            block,
            through: Predicate::parse(&v["project_through"], palette)?,
            size: IntP::parse(&v["size"])?,
            max_drop: v["max_projection_height"].as_i64().unwrap_or(0) as i32,
        })
    }

    pub(crate) fn place(&self, f: &Features, level: &mut dyn Level, r: &mut Worldgen, pos: Pos) {
        let pal = &f.palette;
        let size = self.size.sample(r);
        let bound = size * size + 1;
        for dx in -size..=size {
            for dz in -size..=size {
                let probability = dx.abs() * dz.abs();
                if r.next_int_bound(bound) >= bound - probability {
                    continue;
                }
                let mut base = add(pos, [dx, 0, dz]);
                let mut drop = self.max_drop;
                while self.through.test(&*level, pal, [base[0], base[1] - 1, base[2]]) {
                    base[1] -= 1;
                    drop -= 1;
                    if drop <= 0 {
                        break;
                    }
                }
                if let Some(s) = self.block.get(&*level, pal, r, base) {
                    put(level, base, s);
                }
            }
        }
    }
}
