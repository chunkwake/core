//! Небольшие фичи: диски, родники, лианы, валуны, синий лёд, озёра и
//! данжи (`monster_room`).

use super::{DIRS, Features, Fluid, Level, Palette, Predicate, Provider, add, block_set};
use crate::carver::{FloatP, IntP, clamped_normal};
use crate::random::{Bits, Worldgen};
use serde_json::Value;
use std::sync::Arc;

type Pos = [i32; 3];

/// Горизонтальные направления в порядке `Plane.HORIZONTAL`: север, восток, юг, запад.
const HORIZONTAL: [Pos; 4] = [[0, 0, -1], [1, 0, 0], [0, 0, 1], [-1, 0, 0]];
const HORIZONTAL_NAMES: [&str; 4] = ["north", "east", "south", "west"];

fn get(level: &dyn Level, [x, y, z]: Pos) -> u32 {
    level.get(x, y, z)
}

fn put(level: &mut dyn Level, [x, y, z]: Pos, state: u32) {
    if level.can_write(x, z) {
        level.set(x, y, z, state);
    }
}

fn block(pal: &Palette, name: &str) -> Result<u32, String> {
    pal.defaults
        .get(name)
        .copied()
        .ok_or_else(|| format!("no block {name}"))
}

/// `isSolidRender`, приближённо: твёрдый блок с полными гранями, не листва.
fn solid_render(pal: &Palette, leaves: &[bool], s: u32) -> bool {
    Palette::flag(&pal.solid, s)
        && pal.sturdy.get(s as usize) == Some(&63)
        && !Palette::flag(leaves, s)
}

/// `DiskFeature`.
#[derive(Debug)]
pub(super) struct Disk {
    provider: Provider,
    target: Predicate,
    radius: IntP,
    half_height: i32,
}

impl Disk {
    pub(super) fn parse(v: &Value, pal: &mut Palette, provider: Provider) -> Result<Self, String> {
        Ok(Self {
            provider,
            target: Predicate::parse(&v["target"], pal)?,
            radius: IntP::parse(&v["radius"])?,
            half_height: v["half_height"].as_i64().unwrap_or(0) as i32,
        })
    }

    pub(super) fn place(&self, f: &Features, level: &mut dyn Level, r: &mut Worldgen, o: Pos) {
        let pal = &f.palette;
        let (top, bottom) = (o[1] + self.half_height, o[1] - self.half_height - 1);
        let rad = self.radius.sample(r);
        // `BlockPos.betweenClosed`: x быстрее всех, затем y, затем z.
        for dz in -rad..=rad {
            for dx in -rad..=rad {
                if dx * dx + dz * dz > rad * rad {
                    continue;
                }
                for y in (bottom + 1..=top).rev() {
                    let p = [o[0] + dx, y, o[2] + dz];
                    if self.target.test(&*level, pal, p)
                        && let Some(s) = self.provider.get(&*level, pal, r, p)
                    {
                        put(level, p, s);
                    }
                }
            }
        }
    }
}

/// `SpringFeature`.
#[derive(Debug)]
pub(super) struct Spring {
    state: u32,
    below: bool,
    rocks: i32,
    holes: i32,
    valid: Arc<Vec<bool>>,
}

impl Spring {
    pub(super) fn parse(v: &Value, pal: &mut Palette) -> Result<Self, String> {
        // Источник: `createLegacyBlock` даёт блок жидкости с уровнем 0.
        let state = block(pal, v["state"]["id"].as_str().unwrap_or(""))?;
        Ok(Self {
            state,
            below: v["requires_block_below"].as_bool().unwrap_or(true),
            rocks: v["rock_count"].as_i64().unwrap_or(4) as i32,
            holes: v["hole_count"].as_i64().unwrap_or(1) as i32,
            valid: block_set(&v["valid_blocks"], pal)?,
        })
    }

    pub(super) fn place(&self, f: &Features, level: &mut dyn Level, o: Pos) {
        let pal = &f.palette;
        let ok = |p: Pos| Palette::flag(&self.valid, get(level, p));
        if !ok(add(o, [0, 1, 0])) || (self.below && !ok(add(o, [0, -1, 0]))) {
            return;
        }
        let here = get(level, o);
        if !pal.is_air(here) && !Palette::flag(&self.valid, here) {
            return;
        }
        let sides = [[-1, 0, 0], [1, 0, 0], [0, 0, -1], [0, 0, 1], [0, -1, 0]];
        let rocks = sides.iter().filter(|&&d| ok(add(o, d))).count() as i32;
        let holes = sides
            .iter()
            .filter(|&&d| pal.is_air(get(level, add(o, d))))
            .count() as i32;
        if rocks == self.rocks && holes == self.holes {
            put(level, o, self.state);
        }
    }
}

/// `VinesFeature`.
pub(super) fn vines(f: &Features, level: &mut dyn Level, o: Pos) {
    let pal = &f.palette;
    if !pal.is_air(get(level, o)) {
        return;
    }
    const FACES: [&str; 6] = ["down", "up", "north", "south", "west", "east"];
    for d in 1..6 {
        // `MultifaceBlock.canAttachTo`: полная грань соседа к нам.
        if pal.sturdy(get(level, add(o, DIRS[d])), d ^ 1) {
            let vine = pal.defaults["minecraft:vine"];
            let s = pal.with_prop(vine, FACES[d], "true").unwrap_or(vine);
            put(level, o, s);
            return;
        }
    }
}

/// `BlockBlobFeature`.
#[derive(Debug)]
pub(super) struct Blob {
    state: u32,
    on: Predicate,
}

impl Blob {
    pub(super) fn parse(v: &Value, pal: &mut Palette) -> Result<Self, String> {
        Ok(Self {
            state: pal.state(&v["state"])?,
            on: Predicate::parse(&v["can_place_on"], pal)?,
        })
    }

    pub(super) fn place(&self, f: &Features, level: &mut dyn Level, r: &mut Worldgen, mut o: Pos) {
        let pal = &f.palette;
        let floor = level.min_y() + 3;
        while o[1] > floor && !self.on.test(&*level, pal, add(o, [0, -1, 0])) {
            o[1] -= 1;
        }
        if o[1] <= floor {
            return;
        }
        for _ in 0..3 {
            let xr = r.next_int_bound(2);
            let yr = r.next_int_bound(2);
            let zr = r.next_int_bound(2);
            let tr = (xr + yr + zr) as f32 * 0.333 + 0.5;
            for dz in -zr..=zr {
                for dy in -yr..=yr {
                    for dx in -xr..=xr {
                        if ((dx * dx + dy * dy + dz * dz) as f64) <= (tr * tr) as f64 {
                            put(level, add(o, [dx, dy, dz]), self.state);
                        }
                    }
                }
            }
            o = add(
                o,
                [
                    -1 + r.next_int_bound(2),
                    -r.next_int_bound(2),
                    -1 + r.next_int_bound(2),
                ],
            );
        }
    }
}

/// `BlueIceFeature`.
pub(super) fn blue_ice(f: &Features, level: &mut dyn Level, r: &mut Worldgen, o: Pos) {
    let pal = &f.palette;
    if o[1] > level.sea_level() - 1 {
        return;
    }
    let is =
        |level: &dyn Level, p: Pos, name: &str| pal.same_block(get(level, p), pal.defaults[name]);
    let water = |level: &dyn Level, p: Pos| is(level, p, "minecraft:water");
    if !water(&*level, o) && !water(&*level, add(o, [0, -1, 0])) {
        return;
    }
    if !(1..6).any(|d| is(&*level, add(o, DIRS[d]), "minecraft:packed_ice")) {
        return;
    }
    let blue = pal.defaults["minecraft:blue_ice"];
    put(level, o, blue);
    for _ in 0..200 {
        let y_off = r.next_int_bound(5) - r.next_int_bound(6);
        let mut xz = 3;
        if y_off < 2 {
            xz += y_off / 2;
        }
        if xz < 1 {
            continue;
        }
        let dx = r.next_int_bound(xz) - r.next_int_bound(xz);
        let dz = r.next_int_bound(xz) - r.next_int_bound(xz);
        let p = add(o, [dx, y_off, dz]);
        let s = get(level, p);
        let fits = pal.is_air(s)
            || water(&*level, p)
            || is(&*level, p, "minecraft:packed_ice")
            || is(&*level, p, "minecraft:ice");
        if fits && DIRS.iter().any(|&d| get(level, add(p, d)) == blue) {
            put(level, p, blue);
        }
    }
}

/// `LakeFeature`. Лава и водяной `sulfur_pool`.
#[derive(Debug)]
pub(super) struct Lake {
    fluid: Provider,
    barrier: Provider,
    can_place: Predicate,
    to_air: Predicate,
    to_barrier: Predicate,
}

impl Lake {
    pub(super) fn parse(
        v: &Value,
        pal: &mut Palette,
        fluid: Provider,
        barrier: Provider,
    ) -> Result<Self, String> {
        Ok(Self {
            fluid,
            barrier,
            can_place: Predicate::parse(&v["can_place_feature"], pal)?,
            to_air: Predicate::parse(&v["can_replace_with_air_or_fluid"], pal)?,
            to_barrier: Predicate::parse(&v["can_replace_with_barrier"], pal)?,
        })
    }

    pub(super) fn place(&self, f: &Features, level: &mut dyn Level, r: &mut Worldgen, o: Pos) {
        let pal = &f.palette;
        if o[1] <= level.min_y() + 4 {
            return;
        }
        let o = add(o, [-8, -4, -8]);
        let mut grid = [false; 2048];
        let at = |x: i32, z: i32, y: i32| ((x * 16 + z) * 8 + y) as usize;
        let spots = r.next_int_bound(4) + 4;
        for _ in 0..spots {
            let xr = r.next_double() * 6.0 + 3.0;
            let yr = r.next_double() * 4.0 + 2.0;
            let zr = r.next_double() * 6.0 + 3.0;
            let xp = r.next_double() * (16.0 - xr - 2.0) + 1.0 + xr / 2.0;
            let yp = r.next_double() * (8.0 - yr - 4.0) + 2.0 + yr / 2.0;
            let zp = r.next_double() * (16.0 - zr - 2.0) + 1.0 + zr / 2.0;
            for xx in 1..15 {
                for zz in 1..15 {
                    for yy in 1..7 {
                        let xd = (xx as f64 - xp) / (xr / 2.0);
                        let yd = (yy as f64 - yp) / (yr / 2.0);
                        let zd = (zz as f64 - zp) / (zr / 2.0);
                        if xd * xd + yd * yd + zd * zd < 1.0 {
                            grid[at(xx, zz, yy)] = true;
                        }
                    }
                }
            }
        }
        let edge = |x: i32, z: i32, y: i32| {
            !grid[at(x, z, y)]
                && (x < 15 && grid[at(x + 1, z, y)]
                    || x > 0 && grid[at(x - 1, z, y)]
                    || z < 15 && grid[at(x, z + 1, y)]
                    || z > 0 && grid[at(x, z - 1, y)]
                    || y < 7 && grid[at(x, z, y + 1)]
                    || y > 0 && grid[at(x, z, y - 1)])
        };
        let Some(fluid) = self.fluid.get(&*level, pal, r, o) else {
            return;
        };
        for xx in 0..16 {
            for zz in 0..16 {
                for yy in 0..8 {
                    if edge(xx, zz, yy) {
                        let p = add(o, [xx, yy, zz]);
                        let s = get(level, p);
                        if yy >= 4 && Palette::flag(&pal.liquid, s) {
                            return;
                        }
                        if yy < 4 && !Palette::flag(&pal.solid, s) && s != fluid {
                            return;
                        }
                        if !self.can_place.test(&*level, pal, p) {
                            return;
                        }
                    }
                }
            }
        }
        let air = pal.defaults["minecraft:cave_air"];
        for xx in 0..16 {
            for zz in 0..16 {
                for yy in 0..8 {
                    let p = add(o, [xx, yy, zz]);
                    if grid[at(xx, zz, yy)] && self.to_air.test(&*level, pal, p) {
                        put(level, p, if yy >= 4 { air } else { fluid });
                    }
                }
            }
        }
        let Some(barrier) = self.barrier.get(&*level, pal, r, o) else {
            return;
        };
        if !pal.is_air(barrier) {
            for xx in 0..16 {
                for zz in 0..16 {
                    for yy in 0..8 {
                        if edge(xx, zz, yy) && (yy < 4 || r.next_int_bound(2) != 0) {
                            let p = add(o, [xx, yy, zz]);
                            if Palette::flag(&pal.solid, get(level, p))
                                && self.to_barrier.test(&*level, pal, p)
                            {
                                put(level, p, barrier);
                            }
                        }
                    }
                }
            }
        }
        // `LakeFeature`: водяная жидкость замораживает плоскость y = 4,
        // если `shouldFreeze(..., false)` и предикат воздуха. Свет блоков
        // при генерации ещё ноль, поэтому порог яркости 10 всегда пройден.
        if pal.fluid(fluid) == Fluid::Water
            && let Some(&ice) = pal.defaults.get("minecraft:ice")
            && let Some(&water) = pal.defaults.get("minecraft:water")
        {
            let top = level.min_y() + level.height();
            for xx in 0..16 {
                for zz in 0..16 {
                    let p = add(o, [xx, 4, zz]);
                    if p[1] < level.min_y() || p[1] >= top {
                        continue;
                    }
                    if !pal.biome_freezes(level.biome(p[0], p[1], p[2])) {
                        continue;
                    }
                    if !pal.same_block(get(level, p), water) {
                        continue;
                    }
                    if self.to_air.test(&*level, pal, p) {
                        put(level, p, ice);
                    }
                }
            }
        }
    }
}

/// `MonsterRoomFeature`: блоки комнаты, сундуков и спавнера. Данные
/// спавнера и таблица добычи сундука пока не пишутся, но случайные числа
/// под них тратятся как у ванили.
#[derive(Debug)]
pub(super) struct MonsterRoom {
    blocked: Arc<Vec<bool>>,
    leaves: Arc<Vec<bool>>,
}

impl MonsterRoom {
    pub(super) fn parse(pal: &mut Palette) -> Result<Self, String> {
        Ok(Self {
            blocked: pal.tag("minecraft:features_cannot_replace")?,
            leaves: pal.tag("minecraft:leaves")?,
        })
    }

    pub(super) fn place(&self, f: &Features, level: &mut dyn Level, r: &mut Worldgen, o: Pos) {
        monster_room(f, level, r, o, &self.blocked, &self.leaves);
    }
}

fn monster_room(
    f: &Features,
    level: &mut dyn Level,
    r: &mut Worldgen,
    o: Pos,
    blocked: &[bool],
    leaves: &[bool],
) {
    let pal = &f.palette;
    let solid = |level: &dyn Level, p: Pos| Palette::flag(&pal.solid, get(level, p));
    let air_at = |level: &dyn Level, p: Pos| pal.is_air(get(level, p));
    let safe = |level: &mut dyn Level, p: Pos, s: u32| {
        if !Palette::flag(blocked, get(level, p)) {
            put(level, p, s);
        }
    };
    let xr = r.next_int_bound(2) + 2;
    let zr = r.next_int_bound(2) + 2;
    let (min_x, max_x, min_z, max_z) = (-xr - 1, xr + 1, -zr - 1, zr + 1);
    let mut holes = 0;
    for dx in min_x..=max_x {
        for dy in -1..=4 {
            for dz in min_z..=max_z {
                let p = add(o, [dx, dy, dz]);
                let s = solid(&*level, p);
                if (dy == -1 || dy == 4) && !s {
                    return;
                }
                if (dx == min_x || dx == max_x || dz == min_z || dz == max_z)
                    && dy == 0
                    && air_at(&*level, p)
                    && air_at(&*level, add(p, [0, 1, 0]))
                {
                    holes += 1;
                }
            }
        }
    }
    if !(1..=5).contains(&holes) {
        return;
    }
    let cave_air = pal.defaults["minecraft:cave_air"];
    let chest = pal.defaults["minecraft:chest"];
    let spawner = pal.defaults["minecraft:spawner"];
    let cobble = pal.defaults["minecraft:cobblestone"];
    let mossy = pal.defaults["minecraft:mossy_cobblestone"];
    for dx in min_x..=max_x {
        for dy in (-1..=3).rev() {
            for dz in min_z..=max_z {
                let p = add(o, [dx, dy, dz]);
                let s = get(level, p);
                let is_chest = pal.same_block(s, chest);
                if dx == min_x || dy == -1 || dz == min_z || dx == max_x || dz == max_z {
                    if p[1] >= level.min_y() && !solid(&*level, add(p, [0, -1, 0])) {
                        put(level, p, cave_air);
                    } else if Palette::flag(&pal.solid, s) && !is_chest {
                        if dy == -1 && r.next_int_bound(4) != 0 {
                            safe(level, p, mossy);
                        } else {
                            safe(level, p, cobble);
                        }
                    }
                } else if !is_chest && !pal.same_block(s, spawner) {
                    safe(level, p, cave_air);
                }
            }
        }
    }
    for _ in 0..2 {
        for _ in 0..3 {
            let x = o[0] + r.next_int_bound(xr * 2 + 1) - xr;
            let z = o[2] + r.next_int_bound(zr * 2 + 1) - zr;
            let p = [x, o[1], z];
            if !air_at(&*level, p) {
                continue;
            }
            let walls = HORIZONTAL
                .iter()
                .filter(|&&d| solid(&*level, add(p, d)))
                .count();
            if walls == 1 {
                let s = reorient(pal, leaves, &*level, p, chest);
                let placed = !Palette::flag(blocked, get(level, p));
                safe(level, p, s);
                if placed {
                    // `RandomizableContainer.setBlockEntityLootTable(level,
                    // random, chestPos, SIMPLE_DUNGEON)`: `nextLong` в семя.
                    let seed = r.next_long();
                    super::template::set_loot(level, p, "minecraft:chests/simple_dungeon", seed);
                }
                break;
            }
        }
    }
    let placed = !Palette::flag(blocked, get(level, o));
    safe(level, o, spawner);
    if placed {
        // `Util.getRandom(MOBS)`: скелет, зомби, зомби, паук.
        r.next_int_bound(4);
    }
}

/// `StructurePiece.reorient` для сундука.
fn reorient(pal: &Palette, leaves: &[bool], level: &dyn Level, p: Pos, chest: u32) -> u32 {
    let render = |d: usize| solid_render(pal, leaves, get(level, add(p, HORIZONTAL[d])));
    let mut solid = None;
    for (d, off) in HORIZONTAL.iter().enumerate() {
        if pal.same_block(get(level, add(p, *off)), chest) {
            return chest;
        }
        if render(d) {
            if solid.is_some() {
                solid = None;
                break;
            }
            solid = Some(d);
        }
    }
    let facing = match solid {
        Some(d) => (d + 2) % 4,
        None => {
            let current = pal.prop(chest, "facing").unwrap_or("north");
            let mut d = HORIZONTAL_NAMES
                .iter()
                .position(|n| *n == current)
                .unwrap_or(0);
            if render(d) {
                d = (d + 2) % 4;
            }
            if render(d) {
                d = (d + 1) % 4;
            }
            if render(d) {
                d = (d + 2) % 4;
            }
            d
        }
    };
    pal.with_prop(chest, "facing", HORIZONTAL_NAMES[facing])
        .unwrap_or(chest)
}

/// `GeodeFeature`.
#[derive(Debug)]
pub(super) struct Geode {
    filling: Provider,
    inner: Provider,
    alternate: Provider,
    middle: Provider,
    outer: Provider,
    placements: Vec<u32>,
    cannot_replace: Arc<Vec<bool>>,
    invalid: Arc<Vec<bool>>,
    /// Слои: воздух, внутренний, средний, внешний.
    layers: [f64; 4],
    crack_chance: f32,
    crack_size: f64,
    crack_offset: i32,
    potential_chance: f64,
    alternate_chance: f64,
    require_alternate: bool,
    wall: IntP,
    wall_max: i32,
    points: IntP,
    point_offset: IntP,
    min_offset: i32,
    max_offset: i32,
    noise_mul: f64,
    invalid_threshold: i32,
    noise: Arc<crate::noise::Stack>,
}

impl Geode {
    pub(super) fn parse(
        v: &Value,
        pal: &mut Palette,
        provider: &mut dyn FnMut(&Value, &mut Palette) -> Result<Provider, String>,
        seed: i64,
    ) -> Result<Self, String> {
        let b = &v["blocks"];
        let flt = |o: &Value, k: &str, d: f64| o[k].as_f64().unwrap_or(d);
        let int = |o: &Value, k: &str, d: i64| o[k].as_i64().unwrap_or(d) as i32;
        let uniform = |k: &str, lo: i64, hi: i64| -> Result<IntP, String> {
            match &v[k] {
                Value::Null => IntP::parse(&serde_json::json!({
                    "type": "minecraft:uniform", "min_inclusive": lo, "max_inclusive": hi
                })),
                x => IntP::parse(x),
            }
        };
        let wall_max = match &v["outer_wall_distance"] {
            Value::Null => 5,
            Value::Number(x) => x.as_i64().unwrap_or(5) as i32,
            x => x["max_inclusive"]
                .as_i64()
                .ok_or("geode: outer_wall_distance")? as i32,
        };
        let mut placements = Vec::new();
        for p in b["inner_placements"]
            .as_array()
            .ok_or("geode: inner_placements")?
        {
            placements.push(pal.state(p)?);
        }
        let l = &v["layers"];
        let c = &v["crack"];
        Ok(Self {
            filling: provider(&b["filling_provider"], pal)?,
            inner: provider(&b["inner_layer_provider"], pal)?,
            alternate: provider(&b["alternate_inner_layer_provider"], pal)?,
            middle: provider(&b["middle_layer_provider"], pal)?,
            outer: provider(&b["outer_layer_provider"], pal)?,
            placements,
            cannot_replace: block_set(&b["cannot_replace"], pal)?,
            invalid: block_set(&b["invalid_blocks"], pal)?,
            layers: [
                flt(l, "filling", 1.7),
                flt(l, "inner_layer", 2.2),
                flt(l, "middle_layer", 3.2),
                flt(l, "outer_layer", 4.2),
            ],
            crack_chance: flt(c, "generate_crack_chance", 1.0) as f32,
            crack_size: flt(c, "base_crack_size", 2.0),
            crack_offset: int(c, "crack_point_offset", 2),
            potential_chance: flt(v, "use_potential_placements_chance", 0.35),
            alternate_chance: flt(v, "use_alternate_layer0_chance", 0.0),
            require_alternate: v["placements_require_layer0_alternate"]
                .as_bool()
                .unwrap_or(true),
            wall: uniform("outer_wall_distance", 4, 5)?,
            wall_max,
            points: uniform("distribution_points", 3, 4)?,
            point_offset: uniform("point_offset", 1, 2)?,
            min_offset: int(v, "min_gen_offset", -16),
            max_offset: int(v, "max_gen_offset", 16),
            noise_mul: flt(v, "noise_multiplier", 0.05),
            invalid_threshold: int(v, "invalid_blocks_threshold", 0),
            noise: Arc::new(crate::noise::NormalNoise::parity(-4, &[1.0]).create_legacy(seed)),
        })
    }

    pub(super) fn place(&self, f: &Features, level: &mut dyn Level, r: &mut Worldgen, o: Pos) {
        let pal = &f.palette;
        let n = self.points.sample(r);
        let adjust = n as f64 / self.wall_max as f64;
        let inner_air = 1.0 / self.layers[0].sqrt();
        let innermost = 1.0 / (self.layers[1] + adjust).sqrt();
        let inner_crust = 1.0 / (self.layers[2] + adjust).sqrt();
        let outer_crust = 1.0 / (self.layers[3] + adjust).sqrt();
        let crack_size = 1.0
            / (self.crack_size + r.next_double() / 2.0 + if n > 3 { adjust } else { 0.0 }).sqrt();
        let crack = r.next_float() < self.crack_chance;
        let mut invalid = 0;
        let mut points: Vec<(Pos, i32)> = Vec::new();
        for _ in 0..n {
            let x = self.wall.sample(r);
            let y = self.wall.sample(r);
            let z = self.wall.sample(r);
            let p = add(o, [x, y, z]);
            let s = get(level, p);
            if pal.is_air(s) || Palette::flag(&self.invalid, s) {
                invalid += 1;
                if invalid > self.invalid_threshold {
                    return;
                }
            }
            points.push((p, self.point_offset.sample(r)));
        }
        let mut cracks: Vec<Pos> = Vec::new();
        if crack {
            let c = n * 2 + 1;
            let (dx, dz) = match r.next_int_bound(4) {
                0 => (c, 0),
                1 => (0, c),
                2 => (c, c),
                _ => (0, 0),
            };
            for y in [7, 5, 1] {
                cracks.push(add(o, [dx, y, dz]));
            }
        }
        let dist = |a: Pos, b: Pos| {
            let d = [
                (a[0] - b[0]) as f64,
                (a[1] - b[1]) as f64,
                (a[2] - b[2]) as f64,
            ];
            d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
        };
        let safe = |level: &mut dyn Level, p: Pos, s: u32| {
            if !Palette::flag(&self.cannot_replace, get(level, p)) {
                put(level, p, s);
            }
        };
        let with = |level: &mut dyn Level, r: &mut Worldgen, prov: &Provider, p: Pos| {
            if let Some(s) = prov.get(&*level, pal, r, p) {
                safe(level, p, s);
            }
        };
        let mut potential = Vec::new();
        let (lo, hi) = (self.min_offset, self.max_offset);
        // `BlockPos.betweenClosed`: x быстрее всех, затем y, затем z.
        for dz in lo..=hi {
            for dy in lo..=hi {
                for dx in lo..=hi {
                    let p = add(o, [dx, dy, dz]);
                    let noise = self.noise.get(p[0] as f64, p[1] as f64, p[2] as f64) as f64
                        * self.noise_mul;
                    let mut shell = 0.0;
                    for &(q, off) in &points {
                        shell += 1.0 / (dist(p, q) + off as f64).sqrt() + noise;
                    }
                    if shell < outer_crust {
                        continue;
                    }
                    if shell >= inner_air {
                        with(level, r, &self.filling, p);
                        continue;
                    }
                    let mut crack_sum = 0.0;
                    for &q in &cracks {
                        crack_sum += 1.0 / (dist(p, q) + self.crack_offset as f64).sqrt() + noise;
                    }
                    if crack && crack_sum >= crack_size {
                        safe(level, p, pal.defaults["minecraft:air"]);
                    } else if shell >= innermost {
                        let alt = (r.next_float() as f64) < self.alternate_chance;
                        with(level, r, if alt { &self.alternate } else { &self.inner }, p);
                        if (!self.require_alternate || alt)
                            && (r.next_float() as f64) < self.potential_chance
                        {
                            potential.push(p);
                        }
                    } else if shell >= inner_crust {
                        with(level, r, &self.middle, p);
                    } else {
                        with(level, r, &self.outer, p);
                    }
                }
            }
        }
        const FACES: [&str; 6] = ["down", "up", "north", "south", "west", "east"];
        let water = pal.defaults["minecraft:water"];
        for p in potential {
            let mut s = self.placements[r.next_int_bound(self.placements.len() as i32) as usize];
            for d in 0..6 {
                s = pal.with_prop(s, "facing", FACES[d]).unwrap_or(s);
                let at = add(p, DIRS[d]);
                let here = get(level, at);
                // `getFluidState().isSource()`.
                let source = matches!(pal.fluid(here), Fluid::Water | Fluid::Lava);
                s = pal
                    .with_prop(s, "waterlogged", if source { "true" } else { "false" })
                    .unwrap_or(s);
                // `canClusterGrowAtState`: воздух или полная вода.
                if pal.is_air(here)
                    || (pal.same_block(here, water) && pal.fluid(here) == Fluid::Water)
                {
                    safe(level, at, s);
                    break;
                }
            }
        }
    }
}

/// `RootSystemFeature`: дерево над корнями и корни под ним.
#[derive(Debug)]
pub(super) struct RootSystem {
    tree: Arc<super::Placed>,
    space: i32,
    level_test: i32,
    deviation: i32,
    root_radius: i32,
    replaceable: Arc<Vec<bool>>,
    root: Provider,
    root_attempts: i32,
    column: i32,
    hanging_radius: i32,
    hanging_span: i32,
    hanging: Provider,
    hanging_attempts: i32,
    water: i32,
    allowed: Predicate,
}

impl RootSystem {
    pub(super) fn parse(
        v: &Value,
        pal: &mut Palette,
        provider: &mut dyn FnMut(&Value, &mut Palette) -> Result<Provider, String>,
        tree: Arc<super::Placed>,
    ) -> Result<Self, String> {
        let int = |k: &str| v[k].as_i64().unwrap_or(0) as i32;
        Ok(Self {
            tree,
            space: int("required_vertical_space_for_tree"),
            level_test: int("level_test_distance"),
            deviation: int("max_level_deviation"),
            root_radius: int("root_radius"),
            replaceable: block_set(&v["root_replaceable"], pal)?,
            root: provider(&v["root_state_provider"], pal)?,
            root_attempts: int("root_placement_attempts"),
            column: int("root_column_max_height"),
            hanging_radius: int("hanging_root_radius"),
            hanging_span: int("hanging_roots_vertical_span"),
            hanging: provider(&v["hanging_root_state_provider"], pal)?,
            hanging_attempts: int("hanging_root_placement_attempts"),
            water: int("allowed_vertical_water_for_tree"),
            allowed: Predicate::parse(&v["allowed_tree_position"], pal)?,
        })
    }

    pub(super) fn place(&self, f: &Features, level: &mut dyn Level, r: &mut Worldgen, o: Pos) {
        let pal = &f.palette;
        if !pal.is_air(get(level, o)) {
            return;
        }
        if self.dirt_and_tree(f, level, r, o) {
            self.hanging_roots(f, level, r, o);
        }
    }

    fn space_for_tree(&self, pal: &Palette, level: &dyn Level, p: Pos) -> bool {
        for i in 1..=self.space {
            let s = get(level, add(p, [0, i, 0]));
            let ok = pal.is_air(s) || (i < self.water && pal.fluid(s).is_water());
            if !ok {
                return false;
            }
        }
        if self.level_test > 0 {
            // `Direction.from2DDataValue`: юг, запад, север, восток.
            for d in [[0, 0, 1], [-1, 0, 0], [0, 0, -1], [1, 0, 0]] {
                let c = [
                    p[0] + d[0] * self.level_test,
                    p[1],
                    p[2] + d[2] * self.level_test,
                ];
                let below = get(level, add(c, [0, -self.deviation, 0]));
                let above = get(level, add(c, [0, self.deviation, 0]));
                if pal.is_air(below) || !pal.is_air(above) {
                    return false;
                }
            }
        }
        true
    }

    fn dirt_and_tree(&self, f: &Features, level: &mut dyn Level, r: &mut Worldgen, o: Pos) -> bool {
        let pal = &f.palette;
        let mut p = o;
        for y in 0..self.column {
            p[1] += 1;
            if level.surface(super::Heightmap::WorldSurface, p[0], p[2]) - 1 < p[1] {
                return false;
            }
            if self.allowed.test(&*level, pal, p) && self.space_for_tree(pal, &*level, p) {
                let below = get(level, add(p, [0, -1, 0]));
                if pal.fluid(below).is_lava() || !Palette::flag(&pal.solid, below) {
                    return false;
                }
                if f.place_ok(level, &self.tree, r, p) {
                    for yy in o[1]..o[1] + y {
                        self.rooted_dirt(f, level, r, [o[0], yy, o[2]]);
                    }
                    return true;
                }
            }
        }
        false
    }

    fn rooted_dirt(&self, f: &Features, level: &mut dyn Level, r: &mut Worldgen, o: Pos) {
        let pal = &f.palette;
        let rad = self.root_radius;
        for _ in 0..self.root_attempts {
            let dx = r.next_int_bound(rad) - r.next_int_bound(rad);
            let dz = r.next_int_bound(rad) - r.next_int_bound(rad);
            let p = add(o, [dx, 0, dz]);
            if Palette::flag(&self.replaceable, get(level, p))
                && let Some(s) = self.root.get(&*level, pal, r, p)
            {
                put(level, p, s);
            }
        }
    }

    fn hanging_roots(&self, f: &Features, level: &mut dyn Level, r: &mut Worldgen, o: Pos) {
        let pal = &f.palette;
        let (rad, span) = (self.hanging_radius, self.hanging_span);
        for _ in 0..self.hanging_attempts {
            let dx = r.next_int_bound(rad) - r.next_int_bound(rad);
            let dy = r.next_int_bound(span) - r.next_int_bound(span);
            let dz = r.next_int_bound(rad) - r.next_int_bound(rad);
            let p = add(o, [dx, dy, dz]);
            if !pal.is_air(get(level, p)) {
                continue;
            }
            let Some(s) = self.hanging.get(&*level, pal, r, p) else {
                continue;
            };
            // `HangingRootsBlock.canSurvive` — та же прочная грань сверху.
            if pal.sturdy(get(level, add(p, [0, 1, 0])), 0) {
                put(level, p, s);
            }
        }
    }
}

/// Грани многогранных блоков в порядке `Direction`.
const FACES: [&str; 6] = ["down", "up", "north", "south", "west", "east"];

/// `Direction.allShuffled`: копия D, U, N, S, W, E, перемешанная `Util.shuffle`.
fn shuffled(r: &mut Worldgen, mut list: Vec<usize>) -> Vec<usize> {
    for i in (2..=list.len()).rev() {
        let j = r.next_int_bound(i as i32) as usize;
        list.swap(i - 1, j);
    }
    list
}

/// `MultifaceGrowthFeature` со `MultifaceSpreader` блока: лишайник или
/// скалковая жила.
#[derive(Debug)]
pub(super) struct Multiface {
    block: u32,
    sculk: bool,
    range: i32,
    floor: bool,
    ceiling: bool,
    wall: bool,
    spread: f32,
    on: Arc<Vec<bool>>,
    sculk_blocks: [u32; 3],
    fire: Arc<Vec<bool>>,
}

impl Multiface {
    pub(super) fn parse(v: &Value, pal: &mut Palette) -> Result<Self, String> {
        let name = v["block"].as_str().unwrap_or("");
        let flag = |k: &str| v[k].as_bool().unwrap_or(false);
        Ok(Self {
            block: block(pal, name)?,
            sculk: name == "minecraft:sculk_vein",
            range: v["search_range"].as_i64().unwrap_or(10) as i32,
            floor: flag("can_place_on_floor"),
            ceiling: flag("can_place_on_ceiling"),
            wall: flag("can_place_on_wall"),
            spread: v["chance_of_spreading"].as_f64().unwrap_or(0.5) as f32,
            on: block_set(&v["can_be_placed_on"], pal)?,
            sculk_blocks: [
                block(pal, "minecraft:sculk")?,
                block(pal, "minecraft:sculk_catalyst")?,
                block(pal, "minecraft:moving_piston")?,
            ],
            fire: pal.tag("minecraft:fire")?,
        })
    }

    fn valid_dirs(&self) -> Vec<usize> {
        let mut v = Vec::new();
        if self.ceiling {
            v.push(1);
        }
        if self.floor {
            v.push(0);
        }
        if self.wall {
            // `Plane.HORIZONTAL`: север, восток, юг, запад.
            v.extend([2, 5, 3, 4]);
        }
        v
    }

    fn is(&self, pal: &Palette, s: u32) -> bool {
        pal.same_block(s, self.block)
    }

    fn has_face(pal: &Palette, s: u32, d: usize) -> bool {
        pal.prop(s, FACES[d]) == Some("true")
    }

    fn water_source(pal: &Palette, s: u32) -> bool {
        pal.fluid(s) == Fluid::Water
    }

    /// `MultifaceBlock.isValidStateForPlacement`.
    fn valid(&self, pal: &Palette, level: &dyn Level, old: u32, p: Pos, d: usize) -> bool {
        if self.is(pal, old) && Self::has_face(pal, old, d) {
            return false;
        }
        pal.sturdy(get(level, add(p, DIRS[d])), d ^ 1)
    }

    /// `MultifaceBlock.getStateForPlacement`.
    fn state_for(
        &self,
        pal: &Palette,
        level: &dyn Level,
        old: u32,
        p: Pos,
        d: usize,
    ) -> Option<u32> {
        if !self.valid(pal, level, old, p, d) {
            return None;
        }
        let base = if self.is(pal, old) {
            old
        } else if Self::water_source(pal, old) {
            pal.with_prop(self.block, "waterlogged", "true")
                .unwrap_or(self.block)
        } else {
            self.block
        };
        pal.with_prop(base, FACES[d], "true")
    }

    /// `stateCanBeReplaced` конфигурации распространения.
    fn replaceable(
        &self,
        pal: &Palette,
        level: &dyn Level,
        source: Pos,
        p: Pos,
        d: usize,
        existing: u32,
    ) -> bool {
        let default =
            pal.is_air(existing) || self.is(pal, existing) || Self::water_source(pal, existing);
        if !self.sculk {
            return default;
        }
        let against = get(level, add(p, DIRS[d]));
        if self
            .sculk_blocks
            .iter()
            .any(|&b| pal.same_block(against, b))
        {
            return false;
        }
        let manhattan = (0..3).map(|i| (source[i] - p[i]).abs()).sum::<i32>();
        if manhattan == 2 && pal.sturdy(get(level, add(source, DIRS[d ^ 1])), d) {
            return false;
        }
        let fluid = pal.fluid(existing);
        if fluid != Fluid::None && !fluid.is_water() {
            return false;
        }
        if Palette::flag(&self.fire, existing) {
            return false;
        }
        Palette::flag(&pal.replaceable, existing) || default
    }

    /// `getSpreadFromFaceTowardDirection` + `spreadToFace`.
    fn spread_toward(
        &self,
        pal: &Palette,
        level: &mut dyn Level,
        state: u32,
        p: Pos,
        from: usize,
        to: usize,
    ) -> bool {
        if to / 2 == from / 2 {
            return false;
        }
        let other_source = self.sculk && !self.is(pal, state);
        if !(other_source || (Self::has_face(pal, state, from) && !Self::has_face(pal, state, to)))
        {
            return false;
        }
        // `SAME_POSITION`, `SAME_PLANE`, `WRAP_AROUND`.
        let spots = [
            (p, to),
            (add(p, DIRS[to]), from),
            (add(add(p, DIRS[to]), DIRS[from]), to ^ 1),
        ];
        for (at, face) in spots {
            let existing = get(level, at);
            if self.replaceable(pal, &*level, p, at, face, existing)
                && self.valid(pal, &*level, existing, at, face)
            {
                if let Some(s) = self.state_for(pal, &*level, existing, at, face) {
                    put(level, at, s);
                    return true;
                }
                return false;
            }
        }
        false
    }

    fn place_growth(
        &self,
        pal: &Palette,
        level: &mut dyn Level,
        r: &mut Worldgen,
        p: Pos,
        old: u32,
        dirs: &[usize],
    ) -> bool {
        for &d in dirs {
            if !Palette::flag(&self.on, get(level, add(p, DIRS[d]))) {
                continue;
            }
            let Some(s) = self.state_for(pal, &*level, old, p, d) else {
                return false;
            };
            put(level, p, s);
            if r.next_float() < self.spread {
                // `spreadFromFaceTowardRandomDirection`.
                for to in shuffled(r, (0..6).collect()) {
                    if self.spread_toward(pal, level, s, p, d, to) {
                        break;
                    }
                }
            }
            return true;
        }
        false
    }

    pub(super) fn place(&self, f: &Features, level: &mut dyn Level, r: &mut Worldgen, o: Pos) {
        let pal = &f.palette;
        let water = pal.defaults["minecraft:water"];
        let air_or_water = |s: u32| pal.is_air(s) || pal.same_block(s, water);
        let here = get(level, o);
        if !air_or_water(here) {
            return;
        }
        let dirs = shuffled(r, self.valid_dirs());
        if self.place_growth(pal, level, r, o, here, &dirs) {
            return;
        }
        for &search in &dirs {
            let except: Vec<usize> = self
                .valid_dirs()
                .into_iter()
                .filter(|&d| d != search ^ 1)
                .collect();
            let placement = shuffled(r, except);
            // Ванильный цикл каждый раз ставит точку `origin + направление`.
            let p = add(o, DIRS[search]);
            for _ in 0..self.range {
                let s = get(level, p);
                if !air_or_water(s) && !self.is(pal, s) {
                    break;
                }
                if self.place_growth(pal, level, r, p, s, &placement) {
                    return;
                }
            }
        }
    }
}

/// Общие блоки `SpeleothemUtils`: основание, острие и заменяемые блоки.
#[derive(Debug)]
pub(super) struct SpeleoBlocks {
    base: u32,
    pointed: u32,
    replaceable: Arc<Vec<bool>>,
    water: u32,
    lava: u32,
}

impl SpeleoBlocks {
    fn parse(v: &Value, pal: &mut Palette) -> Result<Self, String> {
        Ok(Self {
            base: pal.state(&v["base_block"])?,
            pointed: pal.state(&v["pointed_block"])?,
            replaceable: block_set(&v["replaceable_blocks"], pal)?,
            water: block(pal, "minecraft:water")?,
            lava: block(pal, "minecraft:lava")?,
        })
    }

    fn is_base(&self, pal: &Palette, s: u32) -> bool {
        pal.same_block(s, self.base) || Palette::flag(&self.replaceable, s)
    }

    fn empty_or_water(&self, pal: &Palette, s: u32) -> bool {
        pal.is_air(s) || pal.same_block(s, self.water)
    }

    fn place_base(&self, level: &mut dyn Level, p: Pos) -> bool {
        if Palette::flag(&self.replaceable, get(level, p)) {
            put(level, p, self.base);
            true
        } else {
            false
        }
    }

    /// `SpeleothemUtils.growSpeleothem`: `d` — направление острия.
    fn grow(
        &self,
        pal: &Palette,
        level: &mut dyn Level,
        start: Pos,
        d: usize,
        height: i32,
        merged: bool,
    ) {
        if !self.is_base(pal, get(level, add(start, DIRS[d ^ 1]))) {
            return;
        }
        let mut parts = Vec::new();
        if height >= 3 {
            parts.push("base");
            parts.extend(std::iter::repeat_n("middle", (height - 3) as usize));
        }
        if height >= 2 {
            parts.push("frustum");
        }
        if height >= 1 {
            parts.push(if merged { "tip_merge" } else { "tip" });
        }
        let face = ["down", "up"][d];
        let pointed = self.pointed;
        let mut p = start;
        for thickness in parts {
            let mut s = pal
                .with_prop(pointed, "vertical_direction", face)
                .unwrap_or(pointed);
            s = pal.with_prop(s, "thickness", thickness).unwrap_or(s);
            let water = pal.fluid(get(level, p)).is_water();
            s = pal
                .with_prop(s, "waterlogged", if water { "true" } else { "false" })
                .unwrap_or(s);
            put(level, p, s);
            p = add(p, DIRS[d]);
        }
    }
}

/// `SpeleothemFeature`: одиночный сталактит или сталагмит.
#[derive(Debug)]
pub(super) struct Speleothem {
    b: SpeleoBlocks,
    taller: f32,
    directional: f32,
    radius2: f32,
    radius3: f32,
}

impl Speleothem {
    pub(super) fn parse(v: &Value, pal: &mut Palette) -> Result<Self, String> {
        let f = |k: &str, d: f64| v[k].as_f64().unwrap_or(d) as f32;
        Ok(Self {
            b: SpeleoBlocks::parse(v, pal)?,
            taller: f("chance_of_taller_generation", 0.2),
            directional: f("chance_of_directional_spread", 0.7),
            radius2: f("chance_of_spread_radius2", 0.5),
            radius3: f("chance_of_spread_radius3", 0.5),
        })
    }

    pub(super) fn place(&self, f: &Features, level: &mut dyn Level, r: &mut Worldgen, o: Pos) {
        let pal = &f.palette;
        let above = self.b.is_base(pal, get(level, add(o, [0, 1, 0])));
        let below = self.b.is_base(pal, get(level, add(o, [0, -1, 0])));
        let d = match (above, below) {
            (true, true) => usize::from(!r.next_bool()),
            (true, false) => 0,
            (false, true) => 1,
            _ => return,
        };
        let root = add(o, DIRS[d ^ 1]);
        self.b.place_base(level, root);
        for h in HORIZONTAL {
            if r.next_float() > self.directional {
                continue;
            }
            let p1 = add(root, h);
            self.b.place_base(level, p1);
            if r.next_float() > self.radius2 {
                continue;
            }
            let p2 = add(p1, DIRS[r.next_int_bound(6) as usize]);
            self.b.place_base(level, p2);
            if r.next_float() > self.radius3 {
                continue;
            }
            let p3 = add(p2, DIRS[r.next_int_bound(6) as usize]);
            self.b.place_base(level, p3);
        }
        let height = if r.next_float() < self.taller
            && self.b.empty_or_water(pal, get(level, add(o, DIRS[d])))
        {
            2
        } else {
            1
        };
        self.b.grow(pal, level, o, d, height, false);
    }
}

/// `Mth.clampedMap`.
fn clamped_map(v: f64, from_min: f64, from_max: f64, to_min: f64, to_max: f64) -> f64 {
    let t = (v - from_min) / (from_max - from_min);
    if t < 0.0 {
        to_min
    } else if t > 1.0 {
        to_max
    } else {
        to_min + t * (to_max - to_min)
    }
}

/// `SpeleothemClusterFeature`.
#[derive(Debug)]
pub(super) struct SpeleothemCluster {
    b: SpeleoBlocks,
    search: i32,
    height: IntP,
    radius: IntP,
    max_diff: i32,
    deviation: i32,
    thickness: IntP,
    density: FloatP,
    wetness: FloatP,
    chance_at_max: f32,
    edge: i32,
    center: i32,
    base_stone: Arc<Vec<bool>>,
}

impl SpeleothemCluster {
    pub(super) fn parse(v: &Value, pal: &mut Palette) -> Result<Self, String> {
        let int = |k: &str| {
            v[k].as_i64()
                .map(|x| x as i32)
                .ok_or(format!("speleothem_cluster: {k}"))
        };
        Ok(Self {
            b: SpeleoBlocks::parse(v, pal)?,
            search: int("floor_to_ceiling_search_range")?,
            height: IntP::parse(&v["height"])?,
            radius: IntP::parse(&v["radius"])?,
            max_diff: int("max_stalagmite_stalactite_height_diff")?,
            deviation: int("height_deviation")?,
            thickness: IntP::parse(&v["speleothem_block_layer_thickness"])?,
            density: FloatP::parse(&v["density"])?,
            wetness: FloatP::parse(&v["wetness"])?,
            chance_at_max: v["chance_of_speleothem_at_max_distance_from_center"]
                .as_f64()
                .unwrap_or(0.0) as f32,
            edge: int("max_distance_from_edge_affecting_chance_of_speleothem")?,
            center: int("max_distance_from_center_affecting_height_bias")?,
            base_stone: pal.tag("minecraft:base_stone_overworld")?,
        })
    }

    pub(super) fn place(&self, f: &Features, level: &mut dyn Level, r: &mut Worldgen, o: Pos) {
        let pal = &f.palette;
        if !self.b.empty_or_water(pal, get(level, o)) {
            return;
        }
        let height = self.height.sample(r);
        let wetness = self.wetness.sample(r);
        let density = self.density.sample(r);
        let xr = self.radius.sample(r);
        let zr = self.radius.sample(r);
        for dx in -xr..=xr {
            for dz in -zr..=zr {
                let from_edge = (xr - dx.abs()).min(zr - dz.abs());
                // `Mth.clampedMap(float..)`.
                let t = from_edge as f32 / self.edge as f32;
                let chance = if t < 0.0 {
                    self.chance_at_max
                } else if t > 1.0 {
                    1.0
                } else {
                    self.chance_at_max + t * (1.0 - self.chance_at_max)
                };
                self.column(
                    pal,
                    level,
                    r,
                    add(o, [dx, 0, dz]),
                    dx,
                    dz,
                    wetness,
                    chance as f64,
                    height,
                    density,
                );
            }
        }
    }

    /// `Column.scan` с `isEmptyOrWater` внутри и `isNeitherEmptyNorWater` на краю.
    fn scan(&self, pal: &Palette, level: &dyn Level, p: Pos) -> Option<(Option<i32>, Option<i32>)> {
        let inside = |y: i32| self.b.empty_or_water(pal, get(level, [p[0], y, p[2]]));
        if !inside(p[1]) {
            return None;
        }
        let dir = |step: i32| {
            let mut y = p[1];
            let mut i = 1;
            while i < self.search && inside(y) {
                y += step;
                i += 1;
            }
            (!inside(y)).then_some(y)
        };
        let ceiling = dir(1);
        let floor = dir(-1);
        Some((floor, ceiling))
    }

    #[allow(clippy::too_many_arguments)]
    fn column(
        &self,
        pal: &Palette,
        level: &mut dyn Level,
        r: &mut Worldgen,
        p: Pos,
        dx: i32,
        dz: i32,
        wet: f32,
        chance: f64,
        cluster: i32,
        density: f32,
    ) {
        let Some((base_floor, ceiling)) = self.scan(pal, &*level, p) else {
            return;
        };
        if ceiling.is_none() && base_floor.is_none() {
            return;
        }
        let at = |y: i32| [p[0], y, p[2]];
        let want_pool = r.next_float() < wet;
        let mut floor = base_floor;
        if want_pool
            && let Some(fy) = base_floor
            && self.can_pool(pal, &*level, at(fy))
        {
            floor = Some(fy - 1);
            put(level, at(fy), self.b.water);
        }
        let is_lava = |level: &dyn Level, y: i32| pal.same_block(get(level, at(y)), self.b.lava);
        let want = r.next_double() < chance;
        let stalactite = match ceiling {
            Some(c) if want && !is_lava(&*level, c) => {
                let t = self.thickness.sample(r);
                self.replace_with_base(level, at(c), t, 1);
                let max = match floor {
                    Some(fl) => cluster.min(c - fl),
                    None => cluster,
                };
                self.height_for(r, dx, dz, density, max)
            }
            _ => 0,
        };
        let want = r.next_double() < chance;
        let stalagmite = match floor {
            Some(fl) if want && !is_lava(&*level, fl) => {
                let t = self.thickness.sample(r);
                self.replace_with_base(level, at(fl), t, 0);
                if ceiling.is_some() {
                    0.max(stalactite + r.between_inclusive(-self.max_diff, self.max_diff))
                } else {
                    self.height_for(r, dx, dz, density, cluster)
                }
            }
            _ => 0,
        };
        let (mut up_h, mut down_h) = (stalagmite, stalactite);
        if let (Some(c), Some(fl)) = (ceiling, floor)
            && c - stalactite <= fl + stalagmite
        {
            let lowest = (c - stalactite).max(fl + 1);
            let highest = (fl + stalagmite).min(c - 1);
            let bottom = r.between_inclusive(lowest, highest + 1);
            down_h = c - bottom;
            up_h = bottom - 1 - fl;
        }
        // `Column.Range.height`: только когда есть и пол, и потолок.
        let col_height = match (ceiling, floor) {
            (Some(c), Some(fl)) => Some(c - fl - 1),
            _ => None,
        };
        let merge = r.next_bool() && down_h > 0 && up_h > 0 && col_height == Some(down_h + up_h);
        if let Some(c) = ceiling {
            self.b.grow(pal, level, at(c - 1), 0, down_h, merge);
        }
        if let Some(fl) = floor {
            self.b.grow(pal, level, at(fl + 1), 1, up_h, merge);
        }
    }

    fn height_for(&self, r: &mut Worldgen, dx: i32, dz: i32, density: f32, max: i32) -> i32 {
        if r.next_float() > density {
            return 0;
        }
        let dist = dx.abs() + dz.abs();
        let mean = clamped_map(dist as f64, 0.0, self.center as f64, max as f64 / 2.0, 0.0) as f32;
        clamped_normal(r, mean, self.deviation as f32, 0.0, max as f32) as i32
    }

    fn can_pool(&self, pal: &Palette, level: &dyn Level, p: Pos) -> bool {
        let s = get(level, p);
        if pal.same_block(s, self.b.water)
            || pal.same_block(s, self.b.base)
            || pal.same_block(s, self.b.pointed)
        {
            return false;
        }
        if pal.fluid(get(level, add(p, [0, 1, 0]))).is_water() {
            return false;
        }
        let ok = |q: Pos| {
            let s = get(level, q);
            Palette::flag(&self.base_stone, s) || pal.fluid(s).is_water()
        };
        HORIZONTAL.iter().all(|&h| ok(add(p, h))) && ok(add(p, [0, -1, 0]))
    }

    /// `replaceBlocksWithBaseBlocks`: `d` — 1 вверх, 0 вниз.
    fn replace_with_base(&self, level: &mut dyn Level, first: Pos, count: i32, d: usize) {
        let mut p = first;
        for _ in 0..count {
            if !self.b.place_base(level, p) {
                return;
            }
            p = add(p, DIRS[d]);
        }
    }
}

/// `SpeleothemUtils.getSpeleothemHeight`.
fn speleothem_height(dist: f64, radius: f64, scale: f64, bluntness: f64) -> f64 {
    let d = dist.max(bluntness);
    let r = d / radius * 0.384;
    let part1 = 0.75 * r.powf(1.3333333333333333);
    let part2 = r.powf(0.6666666666666666);
    let part3 = 0.3333333333333333 * r.ln();
    let h = (scale * (part1 - part2 - part3)).max(0.0);
    h / 0.384 * radius
}

/// `LargeDripstoneFeature.WindOffsetter`.
struct Wind {
    origin_y: i32,
    speed: Option<(f64, f64)>,
    max: i32,
}

impl Wind {
    fn offset(&self, p: Pos) -> Pos {
        let Some((sx, sz)) = self.speed else {
            return p;
        };
        let dy = (self.origin_y - p[1]) as f64;
        let dx = ((sx * dy).floor() as i32).clamp(-self.max, self.max);
        let dz = ((sz * dy).floor() as i32).clamp(-self.max, self.max);
        [p[0] + dx, p[1], p[2] + dz]
    }
}

/// `LargeDripstoneFeature.LargeDripstone`.
struct Large {
    root: Pos,
    up: bool,
    radius: i32,
    bluntness: f64,
    scale: f64,
}

impl Large {
    fn height_at(&self, dist: f32) -> i32 {
        speleothem_height(dist as f64, self.radius as f64, self.scale, self.bluntness) as i32
    }

    fn wind_ok(&self, min_radius: i32, min_bluntness: f32) -> bool {
        self.radius >= min_radius && self.bluntness >= min_bluntness as f64
    }
}

/// `LargeDripstoneFeature`.
#[derive(Debug)]
pub(super) struct LargeDripstone {
    replaceable: Arc<Vec<bool>>,
    search: i32,
    radius: (i32, i32),
    scale: FloatP,
    ratio: f32,
    stalactite: FloatP,
    stalagmite: FloatP,
    wind: FloatP,
    wind_radius: i32,
    wind_bluntness: f32,
    dripstone: u32,
    water: u32,
    lava: u32,
    base_stone: Arc<Vec<bool>>,
}

impl LargeDripstone {
    pub(super) fn parse(v: &Value, pal: &mut Palette) -> Result<Self, String> {
        let flt = |k: &str| {
            v[k].as_f64()
                .map(|x| x as f32)
                .ok_or(format!("large_dripstone: {k}"))
        };
        Ok(Self {
            replaceable: block_set(&v["replaceable_blocks"], pal)?,
            search: v["floor_to_ceiling_search_range"].as_i64().unwrap_or(30) as i32,
            radius: IntP::parse(&v["column_radius"])?.bounds(),
            scale: FloatP::parse(&v["height_scale"])?,
            ratio: flt("max_column_radius_to_cave_height_ratio")?,
            stalactite: FloatP::parse(&v["stalactite_bluntness"])?,
            stalagmite: FloatP::parse(&v["stalagmite_bluntness"])?,
            wind: FloatP::parse(&v["wind_speed"])?,
            wind_radius: v["min_radius_for_wind"].as_i64().unwrap_or(0) as i32,
            wind_bluntness: flt("min_bluntness_for_wind")?,
            dripstone: block(pal, "minecraft:dripstone_block")?,
            water: block(pal, "minecraft:water")?,
            lava: block(pal, "minecraft:lava")?,
            base_stone: pal.tag("minecraft:base_stone_overworld")?,
        })
    }

    fn empty_or_water(&self, pal: &Palette, s: u32) -> bool {
        pal.is_air(s) || pal.same_block(s, self.water)
    }

    fn empty_water_lava(&self, pal: &Palette, s: u32) -> bool {
        self.empty_or_water(pal, s) || pal.same_block(s, self.lava)
    }

    pub(super) fn place(&self, f: &Features, level: &mut dyn Level, r: &mut Worldgen, o: Pos) {
        let pal = &f.palette;
        if !self.empty_or_water(pal, get(level, o)) {
            return;
        }
        // `Column.scan`: края — основание (капельник или заменяемое) или лава.
        let edge = |s: u32| {
            pal.same_block(s, self.dripstone)
                || Palette::flag(&self.replaceable, s)
                || pal.same_block(s, self.lava)
        };
        let scan = |step: i32| {
            let mut y = o[1];
            let mut i = 1;
            while i < self.search && self.empty_or_water(pal, get(level, [o[0], y, o[2]])) {
                y += step;
                i += 1;
            }
            edge(get(level, [o[0], y, o[2]])).then_some(y)
        };
        let (Some(ceiling), Some(floor)) = (scan(1), scan(-1)) else {
            return;
        };
        let height = ceiling - floor - 1;
        if height < 4 {
            return;
        }
        let (lo, hi) = self.radius;
        let max_radius = ((height as f32 * self.ratio) as i32).clamp(lo, hi);
        let radius = r.between_inclusive(lo, max_radius);
        let mut top = Large {
            root: [o[0], ceiling - 1, o[2]],
            up: false,
            radius,
            bluntness: self.stalactite.sample(r) as f64,
            scale: self.scale.sample(r) as f64,
        };
        let mut bottom = Large {
            root: [o[0], floor + 1, o[2]],
            up: true,
            radius,
            bluntness: self.stalagmite.sample(r) as f64,
            scale: self.scale.sample(r) as f64,
        };
        let wind = if top.wind_ok(self.wind_radius, self.wind_bluntness)
            && bottom.wind_ok(self.wind_radius, self.wind_bluntness)
        {
            let speed = self.wind.sample(r);
            // `Mth.randomBetween(random, 0, PI)`.
            let dir = r.next_float() * std::f32::consts::PI;
            Wind {
                origin_y: o[1],
                speed: Some((
                    (crate::carver::cos(dir as f64) * speed) as f64,
                    (crate::carver::sin(dir as f64) * speed) as f64,
                )),
                max: 16 - radius,
            }
        } else {
            Wind {
                origin_y: 0,
                speed: None,
                max: 0,
            }
        };
        let top_ok = self.embed(pal, &*level, &mut top, &wind);
        let bottom_ok = self.embed(pal, &*level, &mut bottom, &wind);
        if top_ok {
            self.place_blocks(pal, level, r, &top, &wind);
        }
        if bottom_ok {
            self.place_blocks(pal, level, r, &bottom, &wind);
        }
    }

    /// `isCircleMostlyEmbeddedInStone`.
    fn embedded(&self, pal: &Palette, level: &dyn Level, c: Pos, radius: i32) -> bool {
        if self.empty_water_lava(pal, get(level, c)) {
            return false;
        }
        let step = 6.0f32 / radius as f32;
        let mut angle = 0.0f32;
        while angle < std::f32::consts::PI * 2.0 {
            let dx = (crate::carver::cos(angle as f64) * radius as f32) as i32;
            let dz = (crate::carver::sin(angle as f64) * radius as f32) as i32;
            if self.empty_water_lava(pal, get(level, add(c, [dx, 0, dz]))) {
                return false;
            }
            angle += step;
        }
        true
    }

    /// `moveBackUntilBaseIsInsideStoneAndShrinkRadiusIfNecessary`.
    fn embed(&self, pal: &Palette, level: &dyn Level, d: &mut Large, wind: &Wind) -> bool {
        while d.radius > 1 {
            let mut p = d.root;
            let tries = 10.min(d.height_at(0.0));
            for _ in 0..tries {
                if pal.same_block(get(level, p), self.lava) {
                    return false;
                }
                if self.embedded(pal, level, wind.offset(p), d.radius) {
                    d.root = p;
                    return true;
                }
                p[1] += if d.up { -1 } else { 1 };
            }
            d.radius /= 2;
        }
        false
    }

    fn place_blocks(
        &self,
        pal: &Palette,
        level: &mut dyn Level,
        r: &mut Worldgen,
        d: &Large,
        wind: &Wind,
    ) {
        for dx in -d.radius..=d.radius {
            for dz in -d.radius..=d.radius {
                let dist = ((dx * dx + dz * dz) as f32).sqrt();
                if dist > d.radius as f32 {
                    continue;
                }
                let mut height = d.height_at(dist);
                if height <= 0 {
                    continue;
                }
                if (r.next_float() as f64) < 0.2 {
                    height = (height as f32 * (r.next_float() * (1.0f32 - 0.8) + 0.8)) as i32;
                }
                let mut p = add(d.root, [dx, 0, dz]);
                let mut out = false;
                let max_y = if d.up {
                    level.surface(super::Heightmap::WorldSurfaceWg, p[0], p[2])
                } else {
                    i32::MAX
                };
                let mut i = 0;
                while i < height && p[1] < max_y {
                    let at = wind.offset(p);
                    let s = get(level, at);
                    if self.empty_water_lava(pal, s) {
                        out = true;
                        put(level, at, self.dripstone);
                    } else if out && Palette::flag(&self.base_stone, s) {
                        break;
                    }
                    p[1] += if d.up { 1 } else { -1 };
                    i += 1;
                }
            }
        }
    }
}

// --- Фичи, которых не было: снег и лёд, магма, бамбук, шипы, айсберги, кораллы.

/// `FreezeTopLayerFeature`: лёд на воде и снег над ним в холодных биомах.
pub(super) fn freeze_top_layer(f: &Features, level: &mut dyn Level, o: Pos) {
    let pal = &f.palette;
    let (Some(ice), Some(snow)) = (
        pal.defaults.get("minecraft:ice").copied(),
        pal.defaults.get("minecraft:snow").copied(),
    ) else {
        return;
    };
    let Some(water) = pal.defaults.get("minecraft:water").copied() else {
        return;
    };
    // Ванила обходит чанк, в котором лежит точка фичи; берём ту же область.
    let (ox, oz) = (o[0] & !15, o[2] & !15);
    for x in ox..ox + 16 {
        for z in oz..oz + 16 {
            let h = level.surface(super::Heightmap::MotionBlocking, x, z);
            let top = level.min_y() + level.height();
            if h <= level.min_y() || h >= top {
                continue;
            }
            if !pal.biome_freezes(level.biome(x, h, z)) {
                continue;
            }
            let at = [x, h, z];
            let s = get(level, at);
            let below = add(at, [0, -1, 0]);
            if s == water {
                // Вода у поверхности: лёд, если под ним не вода.
                if get(level, below) != water {
                    put(level, at, ice);
                }
            } else if pal.is_air(s) && get(level, below) != water {
                put(level, at, snow);
            }
        }
    }
}

/// `UnderwaterMagmaFeature`: магма под морским дном.
#[derive(Debug)]
pub(super) struct UnderwaterMagma {
    search: i32,
    chance: f32,
    radius: i32,
}

impl UnderwaterMagma {
    pub(super) fn parse(v: &Value) -> Result<Self, String> {
        Ok(Self {
            search: v["floor_search_range"].as_i64().unwrap_or(5) as i32,
            chance: v["placement_probability_per_valid_position"]
                .as_f64()
                .unwrap_or(0.5) as f32,
            radius: v["placement_radius_around_floor"].as_i64().unwrap_or(1) as i32,
        })
    }

    pub(super) fn place(&self, f: &Features, level: &mut dyn Level, r: &mut Worldgen, o: Pos) {
        let pal = &f.palette;
        let (Some(magma), Some(water)) = (
            pal.defaults.get("minecraft:magma_block").copied(),
            pal.defaults.get("minecraft:water").copied(),
        ) else {
            return;
        };
        // Точка старта — вода. Пол — первая не-вода на спуске, не дальше
        // `search - 1` шага (`Column.scan` вниз).
        if !pal.same_block(get(level, o), water) {
            return;
        }
        let mut y = o[1];
        let mut step = 1;
        while step < self.search && pal.same_block(get(level, [o[0], y, o[2]]), water) {
            y -= 1;
            step += 1;
        }
        if pal.same_block(get(level, [o[0], y, o[2]]), water) {
            return;
        }
        // `betweenClosed`: x, затем y, затем z. Бросок на каждую клетку,
        // магма — только в закрытом твёрдом блоке, не в воде.
        let rad = self.radius;
        let span = (rad * 2 + 1) as usize;
        let end = span * span * span;
        for index in 0..end {
            if r.next_float() >= self.chance {
                continue;
            }
            let x = (index % span) as i32;
            let slice = index / span;
            let yy = (slice % span) as i32;
            let z = (slice / span) as i32;
            let p = [o[0] - rad + x, y - rad + yy, o[2] - rad + z];
            if self.enclosed(pal, water, level, p) {
                put(level, p, magma);
            }
        }
    }

    /// Клетка не вода и не воздух, а низ и четыре стороны закрыты полным кубом.
    fn enclosed(&self, pal: &Palette, water: u32, level: &dyn Level, p: Pos) -> bool {
        let here = get(level, p);
        if pal.is_air(here) || pal.same_block(here, water) {
            return false;
        }
        let covers = |q: Pos| pal.is_full_cube(get(level, q));
        covers([p[0], p[1] - 1, p[2]]) && HORIZONTAL.iter().all(|d| covers(add(p, *d)))
    }
}

/// `BambooFeature`: ствол бамбука над землёй; снизу иногда подзол.
pub(super) fn bamboo(f: &Features, level: &mut dyn Level, r: &mut Worldgen, o: Pos, podzol: f32) {
    let pal = &f.palette;
    let Some(bamboo) = pal.defaults.get("minecraft:bamboo").copied() else {
        return;
    };
    let below = add(o, [0, -1, 0]);
    let soil = [
        "minecraft:grass_block",
        "minecraft:dirt",
        "minecraft:podzol",
    ]
    .iter()
    .filter_map(|n| pal.defaults.get(*n).copied())
    .any(|s| pal.same_block(get(level, below), s));
    if !soil || !pal.is_air(get(level, o)) {
        return;
    }
    let height = r.next_int_bound(12) + 5;
    let mut p = o;
    for _ in 0..height {
        if !pal.is_air(get(level, p)) {
            break;
        }
        put(level, p, bamboo);
        p = add(p, [0, 1, 0]);
    }
    if podzol > 0.0
        && r.next_float() < podzol
        && let Some(podzol) = pal.defaults.get("minecraft:podzol").copied()
    {
        put(level, below, podzol);
    }
}

/// `SpikeFeature` (`ice_spike`): ледяной шип из пакованного льда.
#[derive(Debug)]
pub(super) struct IceSpike {
    on: Predicate,
    replace: Predicate,
    state: u32,
}

impl IceSpike {
    pub(super) fn parse(v: &Value, pal: &mut Palette) -> Result<Self, String> {
        Ok(Self {
            on: Predicate::parse(&v["can_place_on"], pal)?,
            replace: Predicate::parse(&v["can_replace"], pal)?,
            state: pal.state(&v["state"])?,
        })
    }

    pub(super) fn place(&self, f: &Features, level: &mut dyn Level, r: &mut Worldgen, o: Pos) {
        let pal = &f.palette;
        if !self.on.test(&*level, pal, add(o, [0, -1, 0])) {
            return;
        }
        let height = r.between_inclusive(7, 14);
        let mut y = 0;
        while y < height {
            // Радиус сужается к вершине: `SpikeFeature` берёт долю высоты.
            let rad = ((height - y) as f32 * 3.0 / height as f32) as i32;
            for dx in -rad..=rad {
                for dz in -rad..=rad {
                    if dx * dx + dz * dz > rad * rad {
                        continue;
                    }
                    let p = add(o, [dx, y, dz]);
                    if self.replace.test(&*level, pal, p) {
                        put(level, p, self.state);
                    } else if y > 0 {
                        return;
                    }
                }
            }
            y += 1;
        }
    }
}

/// `IcebergFeature`: конус или эллипс из пакованного льда, снега и голубого
/// льда, с вырезами и обрывом под водой.
///
/// Порядок бросков и все формулы взяты из `IcebergFeature.place` 26.3: сначала
/// `nextDouble` на снег, угол, две полуоси, выбор формы, высоту над водой и
/// подводную глубину, потом по клеткам — радиусы по слоям.
#[derive(Debug, Clone)]
pub(super) struct Iceberg {
    state: u32,
    /// `true` — фича `iceberg_blue` (`state` = `blue_ice`).
    blue: bool,
}

impl Iceberg {
    /// Имя состояния айсберга: `minecraft:packed_ice` или `minecraft:blue_ice`.
    pub(super) fn state_name(&self) -> &'static str {
        if self.blue {
            "minecraft:blue_ice"
        } else {
            "minecraft:packed_ice"
        }
    }

    pub(super) fn parse(v: &Value, pal: &mut Palette) -> Result<Self, String> {
        let state = pal.state(&v["state"])?;
        let blue = pal
            .blocks
            .get("minecraft:blue_ice")
            .is_some_and(|b| b.0 == pal.block_of[state as usize]);
        Ok(Self { state, blue })
    }

    pub(super) fn place(&self, f: &Features, level: &mut dyn Level, r: &mut Worldgen, o: Pos) {
        let pal = &f.palette;
        let (Some(snow), Some(water), Some(ice), Some(blue_ice), Some(packed)) = (
            pal.defaults.get("minecraft:snow_block").copied(),
            pal.defaults.get("minecraft:water").copied(),
            pal.defaults.get("minecraft:ice").copied(),
            pal.defaults.get("minecraft:blue_ice").copied(),
            pal.defaults.get("minecraft:packed_ice").copied(),
        ) else {
            return;
        };
        let air = pal.defaults["minecraft:air"];
        // `origin = new BlockPos(x, chunkGenerator.getSeaLevel(), z)`.
        let origin = [o[0], level.sea_level(), o[2]];
        let ice_ber = Ice {
            pal,
            snow,
            water,
            ice,
            blue_ice,
            packed,
            air,
        };

        let snow_on_top = r.next_double() > 0.7;
        let main_state = self.state;
        let shape_angle = r.next_double() * 2.0 * std::f64::consts::PI;
        let ellipse_a = 11 - r.next_int_bound(5);
        let ellipse_c = 3 + r.next_int_bound(3);
        let is_ellipse = r.next_double() > 0.7;
        let mut over_water_height = if is_ellipse {
            r.next_int_bound(6) + 6
        } else {
            r.next_int_bound(15) + 3
        };
        if !is_ellipse && r.next_double() > 0.9 {
            over_water_height += r.next_int_bound(19) + 7;
        }
        let under_water_height = (over_water_height + r.next_int_bound(11)).min(18);
        let width = (over_water_height + r.next_int_bound(7) - r.next_int_bound(5)).min(11);
        let a = if is_ellipse { ellipse_a } else { 11 };

        // Над водой: `heightDependentRadiusEllipse` или `Round`.
        for xo in -a..a {
            for zo in -a..a {
                for y_off in 0..over_water_height {
                    let radius = if is_ellipse {
                        height_dependent_radius_ellipse(y_off, over_water_height, width)
                    } else {
                        height_dependent_radius_round(r, y_off, over_water_height, width)
                    };
                    if !is_ellipse && xo >= radius {
                        continue;
                    }
                    ice_ber.generate_block(
                        level,
                        r,
                        origin,
                        over_water_height,
                        xo,
                        y_off,
                        zo,
                        radius,
                        a,
                        is_ellipse,
                        ellipse_c,
                        shape_angle,
                        snow_on_top,
                        main_state,
                    );
                }
            }
        }
        ice_ber.smooth(
            level,
            origin,
            width,
            over_water_height,
            is_ellipse,
            ellipse_a,
        );

        // Под водой: `heightDependentRadiusSteep`.
        for xo in -a..a {
            for zo in -a..a {
                let mut y_off = -1;
                while y_off > -under_water_height {
                    let new_a = if is_ellipse {
                        (a as f32
                            * (1.0 - (y_off as f32).powi(2) / (under_water_height as f32 * 8.0)))
                            .ceil() as i32
                    } else {
                        a
                    };
                    let radius =
                        height_dependent_radius_steep(r, -y_off, under_water_height, width);
                    if xo >= radius {
                        y_off -= 1;
                        continue;
                    }
                    ice_ber.generate_block(
                        level,
                        r,
                        origin,
                        under_water_height,
                        xo,
                        y_off,
                        zo,
                        radius,
                        new_a,
                        is_ellipse,
                        ellipse_c,
                        shape_angle,
                        snow_on_top,
                        main_state,
                    );
                    y_off -= 1;
                }
            }
        }

        // Щербина: у эллипса с шансом 0.1, у круга с 0.3.
        let do_cut_out = if is_ellipse {
            r.next_double() > 0.1
        } else {
            r.next_double() > 0.3
        };
        if do_cut_out {
            ice_ber.cut_out(
                level,
                r,
                width,
                over_water_height,
                origin,
                is_ellipse,
                ellipse_a,
                shape_angle,
                ellipse_c,
            );
        }
    }
}

/// Вспомогательное состояние одного айсберга: блоки и палитра.
struct Ice<'a> {
    pal: &'a Palette,
    snow: u32,
    water: u32,
    ice: u32,
    blue_ice: u32,
    packed: u32,
    air: u32,
}

impl Ice<'_> {
    fn set(&self, level: &mut dyn Level, p: Pos, s: u32) {
        if level.can_write(p[0], p[2]) {
            level.set(p[0], p[1], p[2], s);
        }
    }

    fn is_iceberg(&self, s: u32) -> bool {
        s == self.packed || s == self.snow || s == self.blue_ice
    }

    fn remove_floating_snow(&self, level: &mut dyn Level, p: Pos) {
        if level.get(p[0], p[1] + 1, p[2]) == self.snow {
            self.set(level, [p[0], p[1] + 1, p[2]], self.air);
        }
    }

    /// `generateCutOut`: две выемки — над водой воздухом, под водой водой.
    #[allow(clippy::too_many_arguments)]
    fn cut_out(
        &self,
        level: &mut dyn Level,
        r: &mut Worldgen,
        width: i32,
        height: i32,
        origin: Pos,
        is_ellipse: bool,
        ellipse_a: i32,
        shape_angle: f64,
        ellipse_c: i32,
    ) {
        let sign_x = if r.next_bool() { -1 } else { 1 };
        let sign_z = if r.next_bool() { -1 } else { 1 };
        let mut x_off = r.next_int_bound((width / 2 - 2).max(1));
        if r.next_bool() {
            x_off = width / 2 + 1 - r.next_int_bound((width - width / 2 - 1).max(1));
        }
        let mut z_off = r.next_int_bound((width / 2 - 2).max(1));
        if r.next_bool() {
            z_off = width / 2 + 1 - r.next_int_bound((width - width / 2 - 1).max(1));
        }
        if is_ellipse {
            x_off = r.next_int_bound((ellipse_a - 5).max(1));
            z_off = x_off;
        }
        let local = [sign_x * x_off, 0, sign_z * z_off];
        let angle = if is_ellipse {
            shape_angle + std::f64::consts::FRAC_PI_2
        } else {
            r.next_double() * 2.0 * std::f64::consts::PI
        };
        let mut y_off = 0;
        while y_off < height - 3 {
            let radius = height_dependent_radius_round(r, y_off, height, width);
            self.carve(
                radius, y_off, origin, level, false, angle, local, ellipse_a, ellipse_c,
            );
            y_off += 1;
        }
        let bottom = -height + r.next_int_bound(5);
        let mut y_off = -1;
        while y_off > bottom {
            let radius = height_dependent_radius_steep(r, -y_off, height, width);
            self.carve(
                radius, y_off, origin, level, true, angle, local, ellipse_a, ellipse_c,
            );
            y_off -= 1;
        }
    }

    /// `carve`: эллипс вокруг `local`, над водой — воздух, под водой — вода.
    #[allow(clippy::too_many_arguments)]
    fn carve(
        &self,
        radius: i32,
        y_off: i32,
        origin: Pos,
        level: &mut dyn Level,
        under_water: bool,
        angle: f64,
        local: Pos,
        ellipse_a: i32,
        ellipse_c: i32,
    ) {
        let a = radius + 1 + ellipse_a / 3;
        let c = (radius - 3).min(3) + ellipse_c / 2 - 1;
        for xo in -a..a {
            for zo in -a..a {
                let signed = signed_distance_ellipse(xo, zo, local, a, c, angle);
                if signed >= 0.0 {
                    continue;
                }
                let p = [origin[0] + xo, origin[1] + y_off, origin[2] + zo];
                let state = level.get(p[0], p[1], p[2]);
                if !self.is_iceberg(state) && state != self.snow {
                    continue;
                }
                if under_water {
                    self.set(level, p, self.water);
                } else {
                    self.set(level, p, self.air);
                    self.remove_floating_snow(level, p);
                }
            }
        }
    }

    /// `generateIcebergBlock`: один блок конуса по знаковой дистанции.
    #[allow(clippy::too_many_arguments)]
    fn generate_block(
        &self,
        level: &mut dyn Level,
        r: &mut Worldgen,
        origin: Pos,
        height: i32,
        xo: i32,
        y_off: i32,
        zo: i32,
        radius: i32,
        a: i32,
        is_ellipse: bool,
        ellipse_c: i32,
        shape_angle: f64,
        snow_on_top: bool,
        main_state: u32,
    ) {
        let signed = if is_ellipse {
            let c = get_ellipse_c(y_off, height, ellipse_c);
            signed_distance_ellipse(xo, zo, [0, 0, 0], a, c, shape_angle)
        } else {
            signed_distance_circle(xo, zo, [0, 0, 0], radius, r)
        };
        if signed >= 0.0 {
            return;
        }
        let compare = if is_ellipse {
            -0.5
        } else {
            -6.0 - r.next_int_bound(3) as f64
        };
        if signed > compare && r.next_double() > 0.9 {
            return;
        }
        let p = [origin[0] + xo, origin[1] + y_off, origin[2] + zo];
        self.set_iceberg_block(
            level,
            r,
            p,
            height - y_off,
            height,
            is_ellipse,
            snow_on_top,
            main_state,
        );
    }

    /// `setIcebergBlock`: снег сверху, иначе лёд; только в воздух, снег, лёд и воду.
    #[allow(clippy::too_many_arguments)]
    fn set_iceberg_block(
        &self,
        level: &mut dyn Level,
        r: &mut Worldgen,
        p: Pos,
        h_diff: i32,
        height: i32,
        is_ellipse: bool,
        snow_on_top: bool,
        main_state: u32,
    ) {
        let state = level.get(p[0], p[1], p[2]);
        if !(self.pal.is_air(state)
            || state == self.snow
            || state == self.ice
            || state == self.water)
        {
            return;
        }
        let randomness = !is_ellipse || r.next_double() > 0.05;
        let divisor = if is_ellipse { 3 } else { 2 };
        let snow = snow_on_top
            && state != self.water
            && h_diff as f64
                <= r.next_int_bound((height / divisor).max(1)) as f64 + height as f64 * 0.6
            && randomness;
        self.set(level, p, if snow { self.snow } else { main_state });
    }

    /// `smooth`: убирает лёд над воздухом и блоки с тремя открытыми боками.
    fn smooth(
        &self,
        level: &mut dyn Level,
        origin: Pos,
        width: i32,
        height: i32,
        is_ellipse: bool,
        shape_ellipse_a: i32,
    ) {
        let a = if is_ellipse {
            shape_ellipse_a
        } else {
            width / 2
        };
        for x in -a..=a {
            for z in -a..=a {
                for y_off in 0..=height {
                    let p = [origin[0] + x, origin[1] + y_off, origin[2] + z];
                    let state = level.get(p[0], p[1], p[2]);
                    if !self.is_iceberg(state) && state != self.snow {
                        continue;
                    }
                    let below = level.get(p[0], p[1] - 1, p[2]);
                    if self.pal.is_air(below) {
                        self.set(level, p, self.air);
                        self.set(level, [p[0], p[1] + 1, p[2]], self.air);
                        continue;
                    }
                    if !self.is_iceberg(state) {
                        continue;
                    }
                    let sides = [
                        level.get(p[0] - 1, p[1], p[2]),
                        level.get(p[0] + 1, p[1], p[2]),
                        level.get(p[0], p[1], p[2] - 1),
                        level.get(p[0], p[1], p[2] + 1),
                    ];
                    let counter = sides.iter().filter(|s| !self.is_iceberg(**s)).count();
                    if counter < 3 {
                        continue;
                    }
                    self.set(level, p, self.air);
                }
            }
        }
    }
}

/// `getEllipseC`: верхние три слоя сужают малую полуось.
fn get_ellipse_c(y_off: i32, height: i32, shape_ellipse_c: i32) -> i32 {
    let mut c = shape_ellipse_c;
    if y_off > 0 && height - y_off <= 3 {
        c -= 4 - (height - y_off);
    }
    c
}

/// `signedDistanceCircle`: шум дробит круг на краю.
fn signed_distance_circle(xo: i32, zo: i32, origin: Pos, radius: i32, r: &mut Worldgen) -> f64 {
    let off = 10.0f32 * r.next_float().clamp(0.2, 0.8) / radius as f32;
    off as f64 + (xo - origin[0]).pow(2) as f64 + (zo - origin[2]).pow(2) as f64
        - (radius as f64).powi(2)
}

/// `signedDistanceEllipse`: повёрнутый эллипс.
fn signed_distance_ellipse(xo: i32, zo: i32, origin: Pos, a: i32, c: i32, angle: f64) -> f64 {
    let dx = (xo - origin[0]) as f64;
    let dz = (zo - origin[2]) as f64;
    let (cos, sin) = (angle.cos(), angle.sin());
    ((dx * cos - dz * sin) / a as f64).powi(2) + ((dx * sin + dz * cos) / c as f64).powi(2) - 1.0
}

/// `heightDependentRadiusRound`.
fn height_dependent_radius_round(r: &mut Worldgen, y_off: i32, height: i32, width: i32) -> i32 {
    let k = 3.5f32 - r.next_float();
    let mut scale = (1.0 - (y_off as f32).powi(2) / (height as f32 * k)) * width as f32;
    if height > 15 + r.next_int_bound(5) {
        let temp = if y_off < 3 + r.next_int_bound(6) {
            y_off / 2
        } else {
            y_off
        };
        scale = (1.0 - temp as f32 / (height as f32 * k * 0.4)) * width as f32;
    }
    (scale / 2.0).ceil() as i32
}

/// `heightDependentRadiusEllipse`: без шума.
fn height_dependent_radius_ellipse(y_off: i32, height: i32, width: i32) -> i32 {
    let scale = (1.0 - (y_off as f32).powi(2) / height as f32) * width as f32;
    (scale / 2.0).ceil() as i32
}

/// `heightDependentRadiusSteep`: подводная часть обрывается круче.
fn height_dependent_radius_steep(r: &mut Worldgen, y_off: i32, height: i32, width: i32) -> i32 {
    let k = 1.0f32 + r.next_float() / 2.0;
    let scale = (1.0 - y_off as f32 / (height as f32 * k)) * width as f32;
    (scale / 2.0).ceil() as i32
}

/// `CoralTreeFeature`, `CoralClawFeature`, `CoralMushroomFeature`: пятно
/// кораллов вокруг точки, растущее только в воде и на кораллах.
pub(super) fn coral(
    f: &Features,
    level: &mut dyn Level,
    r: &mut Worldgen,
    o: Pos,
    kind: &str,
    inner: &Arc<super::Placed>,
) {
    let pal = &f.palette;
    let water = pal.defaults.get("minecraft:water").copied();
    let corals = pal.tags.get("minecraft:corals").cloned();
    let ok = |s: u32| Some(s) == water || corals.as_ref().is_some_and(|t| Palette::flag(t, s));
    if !ok(get(level, o)) {
        return;
    }
    if kind.ends_with("tree") {
        let height = r.between_inclusive(1, 3);
        let mut p = o;
        for _ in 0..height {
            f.place_feature_at(level, inner, r, p);
            p = [p[0], p[1] + 1, p[2]];
        }
        for d in [
            [1, 0, 0],
            [-1, 0, 0],
            [0, 0, 1],
            [0, 0, -1],
            [1, -1, 0],
            [-1, -1, 0],
            [0, -1, 1],
            [0, -1, -1],
        ] {
            let q = add(p, d);
            if ok(get(level, q)) {
                f.place_feature_at(level, inner, r, q);
            }
        }
        return;
    }
    let radius = if kind.ends_with("claw") { 2 } else { 3 };
    for dx in -radius..=radius {
        for dz in -radius..=radius {
            for dy in -1..=1 {
                if dx * dx + dz * dz > radius * radius {
                    continue;
                }
                let q = add(o, [dx, dy, dz]);
                if ok(get(level, q)) {
                    f.place_feature_at(level, inner, r, q);
                }
            }
        }
    }
}

/// `SculkPatchFeature`: скалк по твёрдой поверхности пещеры.
#[derive(Debug)]
pub(super) struct SculkPatch {
    charge_count: i32,
    amount_per_charge: i32,
    spread_attempts: i32,
    spread_rounds: i32,
    growth_rounds: i32,
    replaceable: Arc<Vec<bool>>,
    sculk: u32,
    vein: u32,
    catalyst: u32,
    shrieker: u32,
    sensor: u32,
}

impl SculkPatch {
    pub(super) fn parse(v: &Value, pal: &mut Palette) -> Result<Self, String> {
        let replaceable = pal.tag("minecraft:sculk_replaceable")?;
        Ok(Self {
            charge_count: v["charge_count"].as_i64().unwrap_or(10) as i32,
            amount_per_charge: v["amount_per_charge"].as_i64().unwrap_or(32) as i32,
            spread_attempts: v["spread_attempts"].as_i64().unwrap_or(64) as i32,
            spread_rounds: v["spread_rounds"].as_i64().unwrap_or(1) as i32,
            growth_rounds: v["growth_rounds"].as_i64().unwrap_or(0) as i32,
            replaceable,
            sculk: block(pal, "minecraft:sculk")?,
            vein: block(pal, "minecraft:sculk_vein")?,
            catalyst: block(pal, "minecraft:sculk_catalyst")?,
            shrieker: block(pal, "minecraft:sculk_shrieker")?,
            sensor: block(pal, "minecraft:sculk_sensor")?,
        })
    }

    pub(super) fn place(&self, f: &Features, level: &mut dyn Level, r: &mut Worldgen, o: Pos) {
        let pal = &f.palette;
        if !pal.is_air(get(level, o)) {
            return;
        }
        // Скалк ложится на твёрдую опору снизу; иначе пятна нет.
        if !Palette::flag(&pal.solid, get(level, add(o, [0, -1, 0]))) {
            return;
        }
        let mut charges = 0;
        for _ in 0..self.spread_rounds {
            if charges >= self.charge_count {
                break;
            }
            let mut p = o;
            let mut placed = 0;
            for _ in 0..self.spread_attempts {
                let d = [
                    r.next_int_bound(3) - 1,
                    r.next_int_bound(3) - 1,
                    r.next_int_bound(3) - 1,
                ];
                p = add(p, d);
                if !pal.is_air(get(level, p)) {
                    // В воздух не растёт — ищем опору рядом.
                    if !Palette::flag(&pal.solid, get(level, add(p, [0, -1, 0]))) {
                        continue;
                    }
                }
                let s = get(level, p);
                if !Palette::flag(&self.replaceable, s) && !pal.is_air(s) {
                    continue;
                }
                put(level, p, self.sculk);
                placed += 1;
                if placed >= self.amount_per_charge {
                    break;
                }
            }
            if placed > 0 {
                charges += 1;
                // Редкие украшения пятна, как `SculkPatchFeature`.
                if r.next_int_bound(10) == 0 {
                    put(level, o, self.catalyst);
                } else if r.next_int_bound(4) == 0 {
                    let p = add(o, [r.next_int_bound(3) - 1, 0, r.next_int_bound(3) - 1]);
                    if pal.is_air(get(level, p)) {
                        put(
                            level,
                            p,
                            if r.next_int_bound(2) == 0 {
                                self.shrieker
                            } else {
                                self.sensor
                            },
                        );
                    }
                }
            }
        }
        // Вены по граням скалка — только если пятно легло.
        for _ in 0..self.growth_rounds {
            for _ in 0..self.spread_attempts {
                let p = add(
                    o,
                    [
                        r.next_int_bound(16) - r.next_int_bound(16),
                        r.next_int_bound(16) - r.next_int_bound(16),
                        r.next_int_bound(16) - r.next_int_bound(16),
                    ],
                );
                if !pal.is_air(get(level, p)) {
                    continue;
                }
                if DIRS.iter().any(|&d| get(level, add(p, d)) == self.sculk) {
                    put(level, p, self.vein);
                }
            }
        }
    }
}
