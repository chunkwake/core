//! `Aquifer.NoiseBasedAquifer`: вода и лава в пустотах под землёй и в морях.

use crate::density::{Ctx, S};
use crate::random::Positional;
use crate::volume::Volume;
use std::collections::HashMap;
use std::sync::Arc;

/// `DimensionType.WAY_BELOW_MIN_Y`.
const WAY_BELOW_MIN_Y: i32 = -32512;
const LAVA_LEVEL: i32 = -54;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fluid {
    Air,
    Water,
    Lava,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Status {
    level: i32,
    fluid: Fluid,
}

impl Status {
    fn at(&self, y: i32) -> Fluid {
        if y < self.level {
            self.fluid
        } else {
            Fluid::Air
        }
    }
}

/// Функции акифера из `noise_settings.aquifers`.
pub struct Samplers {
    pub barrier: Arc<S>,
    pub floodedness: Arc<S>,
    pub spread: Arc<S>,
    pub lava: Arc<S>,
    pub exclusion: Arc<S>,
    pub surface: Arc<S>,
}

fn global(sea: i32, y: i32) -> Status {
    if y < LAVA_LEVEL.min(sea) {
        Status {
            level: LAVA_LEVEL,
            fluid: Fluid::Lava,
        }
    } else {
        Status {
            level: sea,
            fluid: Fluid::Water,
        }
    }
}

fn similarity(d1: i32, d2: i32) -> f64 {
    1.0 - (d2 - d1) as f64 / 25.0
}

const SURFACE_OFFSETS: [[i32; 2]; 13] = [
    [0, 0],
    [-2, -1],
    [-1, -1],
    [0, -1],
    [1, -1],
    [-3, 0],
    [-2, 0],
    [-1, 0],
    [1, 0],
    [-2, 1],
    [-1, 1],
    [0, 1],
    [1, 1],
];

fn grid_xz(b: i32) -> i32 {
    b >> 4
}

fn grid_y(b: i32) -> i32 {
    b.div_euclid(12)
}

fn from_grid_xz(g: i32, o: i32) -> i32 {
    (g << 4) + o
}

fn from_grid_y(g: i32, o: i32) -> i32 {
    g * 12 + o
}

fn lerp(a: f64, p0: f64, p1: f64) -> f64 {
    p0 + a * (p1 - p0)
}

fn map(v: f64, a: f64, b: f64, c: f64, d: f64) -> f64 {
    lerp((v - a) / (b - a), c, d)
}

fn clamped_map(v: f64, a: f64, b: f64, c: f64, d: f64) -> f64 {
    let f = (v - a) / (b - a);
    if f < 0.0 {
        c
    } else if f > 1.0 {
        d
    } else {
        lerp(f, c, d)
    }
}

fn clampd(v: f64, lo: f64, hi: f64) -> f64 {
    if v < lo { lo } else { v.min(hi) }
}

pub struct Aquifer<'a> {
    s: &'a Samplers,
    random: Positional,
    sea: i32,
    min_grid: [i32; 3],
    size_x: i32,
    size_z: i32,
    cache: Vec<Option<Status>>,
    locations: Vec<Option<(i32, i32, i32)>>,
    skip_above_y: i32,
    surface_cache: HashMap<(i32, i32), i32>,
}

impl<'a> Aquifer<'a> {
    pub fn new(s: &'a Samplers, random: Positional, sea: i32, ctx: &mut Ctx, v: &Volume) -> Self {
        let min_x = grid_xz(v.min[0] - 5);
        let max_x = grid_xz(v.max_block(0) - 5) + 1;
        let min_y = grid_y(v.min[1] + 1) - 1;
        let max_y = grid_y(v.max_block(1) + 1) + 1;
        let min_z = grid_xz(v.min[2] - 5);
        let max_z = grid_xz(v.max_block(2) - 5) + 1;
        let size_x = max_x - min_x + 1;
        let size_y = max_y - min_y + 1;
        let size_z = max_z - min_z + 1;
        let total = (size_x * size_y * size_z) as usize;
        let mut a = Self {
            s,
            random,
            sea,
            min_grid: [min_x, min_y, min_z],
            size_x,
            size_z,
            cache: vec![None; total],
            locations: vec![None; total],
            skip_above_y: 0,
            surface_cache: HashMap::new(),
        };
        let max_surface = a.max_surface_level(
            ctx,
            from_grid_xz(min_x, 0),
            from_grid_xz(min_z, 0),
            from_grid_xz(max_x, 9),
            from_grid_xz(max_z, 9),
        ) + 8;
        let skip_grid_y = grid_y(max_surface + 12) + 1;
        a.skip_above_y = from_grid_y(skip_grid_y, 11) - 1;
        a
    }

    fn max_surface_level(&mut self, ctx: &mut Ctx, x0: i32, z0: i32, x1: i32, z1: i32) -> i32 {
        let (qx0, qx1, qz0, qz1) = (x0 >> 2, x1 >> 2, z0 >> 2, z1 >> 2);
        let v = Volume::new(
            [qx1 - qx0 + 1, 1, qz1 - qz0 + 1],
            [qx0 << 2, 0, qz0 << 2],
            [4, 1, 4],
        );
        let mut b = vec![0f32; v.len()];
        self.s.surface.volume(ctx, &mut b, &v);
        let mut max = i32::MIN;
        for z in 0..v.size[2] {
            for x in 0..v.size[0] {
                let level = (b[v.index(x, 0, z)] as f64).floor() as i32;
                self.surface_cache
                    .insert((v.block_x(x), v.block_z(z)), level);
                max = max.max(level);
            }
        }
        max
    }

    fn surface_level(&mut self, ctx: &mut Ctx, x: i32, z: i32) -> i32 {
        let (qx, qz) = ((x >> 2) << 2, (z >> 2) << 2);
        if let Some(&l) = self.surface_cache.get(&(qx, qz)) {
            return l;
        }
        let l = (self.s.surface.value(ctx, qx, 0, qz) as f64).floor() as i32;
        self.surface_cache.insert((qx, qz), l);
        l
    }

    fn index(&self, gx: i32, gy: i32, gz: i32) -> usize {
        let x = gx - self.min_grid[0];
        let y = gy - self.min_grid[1];
        let z = gz - self.min_grid[2];
        ((y * self.size_z + z) * self.size_x + x) as usize
    }

    /// `computeSubstance`: `None` — твёрдый блок, иначе жидкость или воздух.
    pub fn substance(
        &mut self,
        ctx: &mut Ctx,
        x: i32,
        y: i32,
        z: i32,
        density: f64,
    ) -> Option<Fluid> {
        if density > 0.0 {
            return None;
        }
        let g = global(self.sea, y);
        if y > self.skip_above_y {
            return Some(g.at(y));
        }
        if g.at(y) == Fluid::Lava {
            return Some(Fluid::Lava);
        }
        let (ax, ay, az) = (grid_xz(x - 5), grid_y(y + 1), grid_xz(z - 5));
        let mut dist = [i32::MAX; 4];
        let mut closest = [0usize; 4];
        for x1 in 0..=1 {
            for y1 in -1..=1 {
                for z1 in 0..=1 {
                    let (gx, gy, gz) = (ax + x1, ay + y1, az + z1);
                    let index = self.index(gx, gy, gz);
                    let loc = match self.locations[index] {
                        Some(l) => l,
                        None => {
                            let mut r = self.random.at(gx, gy, gz);
                            let lx = from_grid_xz(gx, r.next_int_bound(10));
                            let ly = from_grid_y(gy, r.next_int_bound(9));
                            let lz = from_grid_xz(gz, r.next_int_bound(10));
                            self.locations[index] = Some((lx, ly, lz));
                            (lx, ly, lz)
                        }
                    };
                    let (dx, dy, dz) = (loc.0 - x, loc.1 - y, loc.2 - z);
                    let d = dx * dx + dy * dy + dz * dz;
                    if dist[0] >= d {
                        closest = [index, closest[0], closest[1], closest[2]];
                        dist = [d, dist[0], dist[1], dist[2]];
                    } else if dist[1] >= d {
                        closest = [closest[0], index, closest[1], closest[2]];
                        dist = [dist[0], d, dist[1], dist[2]];
                    } else if dist[2] >= d {
                        closest[3] = closest[2];
                        closest[2] = index;
                        dist[3] = dist[2];
                        dist[2] = d;
                    } else if dist[3] >= d {
                        closest[3] = index;
                        dist[3] = d;
                    }
                }
            }
        }
        let s1 = self.status(ctx, closest[0]);
        let sim12 = similarity(dist[0], dist[1]);
        let fluid = s1.at(y);
        if sim12 <= 0.0 {
            return Some(fluid);
        }
        if fluid == Fluid::Water && global(self.sea, y - 1).at(y - 1) == Fluid::Lava {
            return Some(fluid);
        }
        let mut barrier = f64::NAN;
        let s2 = self.status(ctx, closest[1]);
        let b12 = sim12 * self.pressure(ctx, x, y, z, &mut barrier, s1, s2);
        if density + b12 > 0.0 {
            return None;
        }
        let s3 = self.status(ctx, closest[2]);
        let sim13 = similarity(dist[0], dist[2]);
        if sim13 > 0.0 {
            let b13 = sim12 * sim13 * self.pressure(ctx, x, y, z, &mut barrier, s1, s3);
            if density + b13 > 0.0 {
                return None;
            }
        }
        let sim23 = similarity(dist[1], dist[2]);
        if sim23 > 0.0 {
            let b23 = sim12 * sim23 * self.pressure(ctx, x, y, z, &mut barrier, s2, s3);
            if density + b23 > 0.0 {
                return None;
            }
        }
        Some(fluid)
    }

    #[allow(clippy::too_many_arguments)]
    fn pressure(
        &self,
        ctx: &mut Ctx,
        x: i32,
        y: i32,
        z: i32,
        barrier: &mut f64,
        a: Status,
        b: Status,
    ) -> f64 {
        let (t1, t2) = (a.at(y), b.at(y));
        if (t1 == Fluid::Lava && t2 == Fluid::Water) || (t1 == Fluid::Water && t2 == Fluid::Lava) {
            return 2.0;
        }
        let diff = (a.level - b.level).abs();
        if diff == 0 {
            return 0.0;
        }
        let avg = 0.5 * (a.level + b.level) as f64;
        let above = y as f64 + 0.5 - avg;
        let base = diff as f64 / 2.0;
        let toward_middle = base - above.abs();
        let gradient = if above > 0.0 {
            let c = 0.0 + toward_middle;
            if c > 0.0 { c / 1.5 } else { c / 2.5 }
        } else {
            let c = 3.0 + toward_middle;
            if c > 0.0 { c / 3.0 } else { c / 10.0 }
        };
        let noise = if !(-2.0..=2.0).contains(&gradient) {
            0.0
        } else {
            if barrier.is_nan() {
                *barrier = self.s.barrier.value(ctx, x, y, z) as f64;
            }
            *barrier
        };
        2.0 * (noise + gradient)
    }

    fn status(&mut self, ctx: &mut Ctx, index: usize) -> Status {
        if let Some(s) = self.cache[index] {
            return s;
        }
        let (x, y, z) = self.locations[index].expect("aquifer location");
        let s = self.compute_fluid(ctx, x, y, z);
        self.cache[index] = Some(s);
        s
    }

    fn compute_fluid(&mut self, ctx: &mut Ctx, x: i32, y: i32, z: i32) -> Status {
        let g = global(self.sea, y);
        let mut lowest = i32::MAX;
        let top = y + 12;
        let bottom = y - 12;
        let mut center_under = false;
        for [ox, oz] in SURFACE_OFFSETS {
            let sx = x + (ox << 4);
            let sz = z + (oz << 4);
            let level = self.surface_level(ctx, sx, sz);
            let adjusted = level + 8;
            let start = ox == 0 && oz == 0;
            if start && bottom > adjusted {
                return g;
            }
            let pokes = top > adjusted;
            if pokes || start {
                let at_surface = global(self.sea, adjusted);
                if at_surface.at(adjusted) != Fluid::Air {
                    if start {
                        center_under = true;
                    }
                    if pokes {
                        return at_surface;
                    }
                }
            }
            lowest = lowest.min(level);
        }
        let level = self.fluid_surface_level(ctx, x, y, z, g, lowest, center_under);
        Status {
            level,
            fluid: self.fluid_type(ctx, x, y, z, g, level),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn fluid_surface_level(
        &mut self,
        ctx: &mut Ctx,
        x: i32,
        y: i32,
        z: i32,
        g: Status,
        lowest: i32,
        center_under: bool,
    ) -> i32 {
        let (partial, full) = if self.s.exclusion.value(ctx, x, y, z) > 0.0 {
            (-1.0, -1.0)
        } else {
            let below = lowest + 8 - y;
            let factor = if center_under {
                clamped_map(below as f64, 0.0, 64.0, 1.0, 0.0)
            } else {
                0.0
            };
            let noise = clampd(self.s.floodedness.value(ctx, x, y, z) as f64, -1.0, 1.0);
            let full_threshold = map(factor, 1.0, 0.0, -0.3, 0.8);
            let partial_threshold = map(factor, 1.0, 0.0, -0.8, 0.4);
            (noise - partial_threshold, noise - full_threshold)
        };
        if full > 0.0 {
            g.level
        } else if partial > 0.0 {
            let (cx, cy, cz) = (x.div_euclid(16), y.div_euclid(40), z.div_euclid(16));
            let middle = cy * 40 + 20;
            let spread = (self.s.spread.value(ctx, cx, cy, cz) * 10.0) as f64;
            let quantized = (spread / 3.0).floor() as i32 * 3;
            lowest.min(middle + quantized)
        } else {
            WAY_BELOW_MIN_Y
        }
    }

    fn fluid_type(
        &mut self,
        ctx: &mut Ctx,
        x: i32,
        y: i32,
        z: i32,
        g: Status,
        level: i32,
    ) -> Fluid {
        if level <= -10 && level != WAY_BELOW_MIN_Y && g.fluid != Fluid::Lava {
            let (cx, cy, cz) = (x.div_euclid(64), y.div_euclid(40), z.div_euclid(64));
            let lava = self.s.lava.value(ctx, cx, cy, cz) as f64;
            if lava.abs() > 0.3 {
                return Fluid::Lava;
            }
        }
        g.fluid
    }
}
