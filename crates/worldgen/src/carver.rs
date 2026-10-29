//! Карверы оверворлда (`WorldCarver`): пещеры `minecraft:cave` и каньоны
//! `minecraft:canyon`, перенесённые с ванили 26.3 бит в бит. Карвер только
//! размечает маску (`CarvingMask`); блоки по ней меняет генератор.

use crate::random::{Bits, Legacy};
use serde_json::Value;

/// `Mth.SIN`: таблица синуса на 65536 значений.
fn sin_table() -> &'static [f32; 65536] {
    static TABLE: std::sync::OnceLock<Box<[f32; 65536]>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t = Box::new([0f32; 65536]);
        for (i, v) in t.iter_mut().enumerate() {
            *v = (i as f64 / 10430.378350470453).sin() as f32;
        }
        t
    })
}

/// `Mth.sin(double)`.
pub fn sin(v: f64) -> f32 {
    sin_table()[((v * 10430.378350470453) as i64 & 65535) as usize]
}

/// `Mth.cos(double)`.
pub fn cos(v: f64) -> f32 {
    sin_table()[((v * 10430.378350470453 + 16384.0) as i64 & 65535) as usize]
}

/// `FloatProvider`.
#[derive(Clone, Debug)]
pub enum FloatP {
    Const(f32),
    Uniform(f32, f32),
    Trapezoid { min: f32, max: f32, plateau: f32 },
}

impl FloatP {
    pub fn parse(v: &Value) -> Result<Self, String> {
        if let Some(f) = v.as_f64() {
            return Ok(Self::Const(f as f32));
        }
        let f = |k: &str| {
            v[k].as_f64()
                .map(|x| x as f32)
                .ok_or_else(|| format!("float provider: no {k}"))
        };
        match v["type"].as_str().unwrap_or("") {
            "minecraft:constant" => Ok(Self::Const(f("value")?)),
            "minecraft:uniform" => Ok(Self::Uniform(f("min_inclusive")?, f("max_exclusive")?)),
            "minecraft:trapezoid" => Ok(Self::Trapezoid {
                min: f("min")?,
                max: f("max")?,
                plateau: f("plateau")?,
            }),
            other => Err(format!("float provider {other} is not supported")),
        }
    }

    pub fn sample(&self, r: &mut impl Bits) -> f32 {
        match *self {
            Self::Const(v) => v,
            // `Mth.randomBetween`.
            Self::Uniform(min, max) => r.next_float() * (max - min) + min,
            Self::Trapezoid { min, max, plateau } => {
                let range = max - min;
                let start = (range - plateau) / 2.0;
                let end = range - start;
                min + r.next_float() * end + r.next_float() * start
            }
        }
    }
}

/// `IntProvider`.
#[derive(Clone, Debug)]
pub enum IntP {
    Const(i32),
    Uniform(i32, i32),
    VeryBiasedToBottom(i32, i32),
    BiasedToBottom(i32, i32),
    /// `clamped`: значение источника, зажатое в `[min, max]`.
    Clamped(Box<IntP>, i32, i32),
    /// `weighted_list`: общий вес и пары (вес, провайдер) по порядку.
    Weighted(i32, Vec<(i32, IntP)>),
    /// `trapezoid`: min, max, plateau.
    Trapezoid(i32, i32, i32),
}

impl IntP {
    pub fn parse(v: &Value) -> Result<Self, String> {
        if let Some(i) = v.as_i64() {
            return Ok(Self::Const(i as i32));
        }
        let i = |k: &str| {
            v[k].as_i64()
                .map(|x| x as i32)
                .ok_or_else(|| format!("int provider: no {k}"))
        };
        match v["type"].as_str().unwrap_or("") {
            "minecraft:constant" => Ok(Self::Const(i("value")?)),
            "minecraft:uniform" => Ok(Self::Uniform(i("min_inclusive")?, i("max_inclusive")?)),
            "minecraft:very_biased_to_bottom" => {
                Ok(Self::VeryBiasedToBottom(i("min_inclusive")?, i("max_inclusive")?))
            }
            "minecraft:biased_to_bottom" => {
                Ok(Self::BiasedToBottom(i("min_inclusive")?, i("max_inclusive")?))
            }
            "minecraft:clamped" => Ok(Self::Clamped(
                Box::new(Self::parse(&v["source"])?),
                i("min_inclusive")?,
                i("max_inclusive")?,
            )),
            "minecraft:trapezoid" => Ok(Self::Trapezoid(
                i("min")?,
                i("max")?,
                v["plateau"].as_i64().unwrap_or(0) as i32,
            )),
            "minecraft:weighted_list" => {
                let mut items = Vec::new();
                for e in v["distribution"].as_array().ok_or("weighted_list: no distribution")? {
                    let w = e["weight"].as_i64().ok_or("weighted_list: no weight")? as i32;
                    items.push((w, Self::parse(&e["data"])?));
                }
                let total = items.iter().map(|(w, _)| w).sum();
                if total <= 0 {
                    return Err("weighted_list: empty".into());
                }
                Ok(Self::Weighted(total, items))
            }
            other => Err(format!("int provider {other} is not supported")),
        }
    }

    pub fn sample(&self, r: &mut impl Bits) -> i32 {
        match *self {
            Self::Const(v) => v,
            Self::Uniform(min, max) => r.next_int_bound(max - min + 1) + min,
            Self::VeryBiasedToBottom(min, max) => {
                let a = r.next_int_bound(max - min + 1) + 1;
                let b = r.next_int_bound(a) + 1;
                min + r.next_int_bound(b)
            }
            Self::BiasedToBottom(min, max) => {
                let a = r.next_int_bound(max - min + 1) + 1;
                min + r.next_int_bound(a)
            }
            Self::Clamped(ref source, min, max) => source.sample(r).clamp(min, max),
            Self::Trapezoid(min, max, plateau) => {
                if plateau == 0 && max == -min {
                    return r.next_int_bound(max + 1) - r.next_int_bound(max + 1);
                }
                let range = max - min;
                if plateau == range {
                    return r.next_int_bound(range + 1) + min;
                }
                let start = (range - plateau) / 2;
                let end = range - start;
                let a = r.next_int_bound(end + 1);
                min + a + r.next_int_bound(start + 1)
            }
            // `WeightedList.getRandomOrThrow`: первый, чей накопленный вес больше броска.
            Self::Weighted(total, ref items) => {
                let mut pick = r.next_int_bound(total);
                for (w, p) in items {
                    if pick < *w {
                        return p.sample(r);
                    }
                    pick -= w;
                }
                unreachable!("weight sum is total")
            }
        }
    }
}

/// `VerticalAnchor`, разрешённый в мировой y при загрузке.
pub fn anchor(v: &Value, min_y: i32, height: i32) -> Result<i32, String> {
    if let Some(y) = v["absolute"].as_i64() {
        Ok(y as i32)
    } else if let Some(o) = v["above_bottom"].as_i64() {
        Ok(min_y + o as i32)
    } else if let Some(o) = v["below_top"].as_i64() {
        Ok(height - 1 + min_y - o as i32)
    } else {
        Err(format!("vertical anchor {v} is not supported"))
    }
}

/// `HeightProvider` с якорями, уже разрешёнными в мировой y.
#[derive(Clone, Debug)]
pub enum HeightP {
    Const(i32),
    Uniform(i32, i32),
    Trapezoid(i32, i32, i32),
    /// `biased_to_bottom` / `very_biased_to_bottom`: min, max, inner.
    Biased(i32, i32, i32),
    VeryBiased(i32, i32, i32),
}

impl HeightP {
    pub fn parse(v: &Value, min_y: i32, height: i32) -> Result<Self, String> {
        match v["type"].as_str() {
            Some("minecraft:uniform") => Ok(Self::Uniform(
                anchor(&v["min_inclusive"], min_y, height)?,
                anchor(&v["max_inclusive"], min_y, height)?,
            )),
            Some("minecraft:constant") => Ok(Self::Const(anchor(&v["value"], min_y, height)?)),
            Some(t @ ("minecraft:trapezoid"
            | "minecraft:biased_to_bottom"
            | "minecraft:very_biased_to_bottom")) => {
                let min = anchor(&v["min_inclusive"], min_y, height)?;
                let max = anchor(&v["max_inclusive"], min_y, height)?;
                Ok(match t {
                    "minecraft:trapezoid" => {
                        Self::Trapezoid(min, max, v["plateau"].as_i64().unwrap_or(0) as i32)
                    }
                    "minecraft:biased_to_bottom" => {
                        Self::Biased(min, max, v["inner"].as_i64().unwrap_or(1) as i32)
                    }
                    _ => Self::VeryBiased(min, max, v["inner"].as_i64().unwrap_or(1) as i32),
                })
            }
            None => Ok(Self::Const(anchor(v, min_y, height)?)),
            Some(other) => Err(format!("height provider {other} is not supported")),
        }
    }

    pub fn sample(&self, r: &mut impl Bits) -> i32 {
        match *self {
            Self::Const(y) => y,
            Self::Uniform(min, max) if min > max => min,
            Self::Uniform(min, max) => r.next_int_bound(max - min + 1) + min,
            Self::Trapezoid(min, max, _) if min > max => min,
            Self::Trapezoid(min, max, plateau) => {
                let range = max - min;
                if plateau >= range {
                    return r.between_inclusive(min, max);
                }
                let start = (range - plateau) / 2;
                let end = range - start;
                min + r.between_inclusive(0, end) + r.between_inclusive(0, start)
            }
            Self::Biased(min, max, inner) => {
                if max - min - inner < 0 {
                    return min;
                }
                let n = r.next_int_bound(max - min - inner + 1);
                r.next_int_bound(n + inner) + min
            }
            Self::VeryBiased(min, max, inner) => {
                if max - min - inner < 0 {
                    return min;
                }
                let upper = r.next_between(min + inner, max);
                let biased = r.next_between(min, upper - 1);
                r.next_between(min, biased - 1 + inner)
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct Cave {
    probability: f32,
    y: HeightP,
    count: IntP,
    thickness: FloatP,
    weird_thickness_bias: bool,
    room_vertical: FloatP,
    horizontal: FloatP,
    vertical: FloatP,
    start_vertical: FloatP,
    floor_level: FloatP,
}

#[derive(Clone, Debug)]
pub struct Canyon {
    probability: f32,
    y: HeightP,
    vertical_rotation: FloatP,
    distance_factor: FloatP,
    thickness: FloatP,
    width_smoothness: i32,
    horizontal_radius_factor: FloatP,
    vertical_default: f32,
    vertical_center: f32,
    y_scale: FloatP,
}

#[derive(Clone, Debug)]
pub enum Carver {
    Cave(Cave),
    Canyon(Canyon),
}

/// `CarvingMask`: бит на блок, индекс `y - min + (z + (x << 4)) * height`.
pub struct Mask {
    pub min_y: i32,
    pub max_y: i32,
    height: i32,
    bits: Vec<u64>,
}

impl Mask {
    pub fn new(min_y: i32, max_y: i32) -> Self {
        let height = max_y - min_y + 1;
        Self {
            min_y,
            max_y,
            height,
            bits: vec![0; (256 * height as usize).div_ceil(64)],
        }
    }

    fn index(&self, x: usize, y: i32, z: usize) -> usize {
        (y - self.min_y) as usize + (z + (x << 4)) * self.height as usize
    }

    pub fn carve(&mut self, x: usize, y: i32, z: usize) {
        let i = self.index(x, y, z);
        self.bits[i >> 6] |= 1 << (i & 63);
    }

    pub fn get(&self, x: usize, y: i32, z: usize) -> bool {
        let i = self.index(x, y, z);
        self.bits[i >> 6] >> (i & 63) & 1 != 0
    }

    pub fn is_empty(&self) -> bool {
        self.bits.iter().all(|&b| b == 0)
    }

    /// `CarvingMask.visit`: отрезки подряд идущих битов, разбитые по столбцам.
    /// Посетитель получает `(x, z, нижний y, верхний y)`.
    pub fn visit(&self, mut visitor: impl FnMut(usize, usize, i32, i32)) {
        let h = self.height as usize;
        for column in 0..256usize {
            let base = column * h;
            let mut y = 0;
            while y < h {
                if !self.bit(base + y) {
                    y += 1;
                    continue;
                }
                let start = y;
                while y < h && self.bit(base + y) {
                    y += 1;
                }
                visitor(
                    column >> 4 & 15,
                    column & 15,
                    start as i32 + self.min_y,
                    (y - 1) as i32 + self.min_y,
                );
            }
        }
    }

    fn bit(&self, i: usize) -> bool {
        self.bits[i >> 6] >> (i & 63) & 1 != 0
    }
}

/// Чанк, который режется: позиция и маска.
struct Out<'a> {
    cx: i32,
    cz: i32,
    mask: &'a mut Mask,
}

impl Out<'_> {
    /// `WorldCarver.carveEllipsoid`.
    #[allow(clippy::too_many_arguments)]
    fn ellipsoid(
        &mut self,
        x: f64,
        y: f64,
        z: f64,
        hr: f64,
        vr: f64,
        skip: &dyn Fn(f64, f64, f64, i32) -> bool,
    ) {
        let (min_x, min_z) = (self.cx * 16, self.cz * 16);
        let (mid_x, mid_z) = ((min_x + 8) as f64, (min_z + 8) as f64);
        let max_delta = 16.0 + hr * 2.0;
        if (x - mid_x).abs() > max_delta || (z - mid_z).abs() > max_delta {
            return;
        }
        let x0 = ((x - hr).floor() as i32 - min_x - 1).max(0);
        let x1 = ((x + hr).floor() as i32 - min_x).min(15);
        let y0 = ((y - vr).floor() as i32 - 1).max(self.mask.min_y);
        let y1 = ((y + vr).floor() as i32 + 1).min(self.mask.max_y);
        let z0 = ((z - hr).floor() as i32 - min_z - 1).max(0);
        let z1 = ((z + hr).floor() as i32 - min_z).min(15);
        for xi in x0..=x1 {
            let xd = ((min_x + xi) as f64 + 0.5 - x) / hr;
            for zi in z0..=z1 {
                let zd = ((min_z + zi) as f64 + 0.5 - z) / hr;
                if xd * xd + zd * zd >= 1.0 {
                    continue;
                }
                let mut wy = y1;
                while wy > y0 {
                    let yd = (wy as f64 - 0.5 - y) / vr;
                    if !skip(xd, yd, zd, wy) {
                        self.mask.carve(xi as usize, wy, zi as usize);
                    }
                    wy -= 1;
                }
            }
        }
    }

    /// `WorldCarver.canReach`.
    fn can_reach(&self, x: f64, z: f64, step: i32, total: i32, thickness: f32) -> bool {
        let xd = x - (self.cx * 16 + 8) as f64;
        let zd = z - (self.cz * 16 + 8) as f64;
        let remaining = (total - step) as f64;
        let rr = (thickness + 2.0 + 16.0) as f64;
        xd * xd + zd * zd - remaining * remaining <= rr * rr
    }
}

const PI_F: f32 = std::f32::consts::PI;

impl Carver {
    pub fn parse(v: &Value, min_y: i32, height: i32) -> Result<Self, String> {
        let prob = v["probability"].as_f64().ok_or("carver: no probability")? as f32;
        let y = HeightP::parse(&v["y"], min_y, height)?;
        match v["type"].as_str().unwrap_or("") {
            "minecraft:cave" | "minecraft:nether_cave" => Ok(Self::Cave(Cave {
                probability: prob,
                y,
                count: IntP::parse(&v["count"])?,
                thickness: FloatP::parse(&v["thickness"])?,
                weird_thickness_bias: v["weird_thickness_bias"].as_bool().unwrap_or(false),
                room_vertical: FloatP::parse(&v["room_vertical_radius_multiplier"])?,
                horizontal: FloatP::parse(&v["horizontal_radius_multiplier"])?,
                vertical: FloatP::parse(&v["vertical_radius_multiplier"])?,
                start_vertical: if v["start_vertical_radius_multiplier"].is_null() {
                    FloatP::Const(1.0)
                } else {
                    FloatP::parse(&v["start_vertical_radius_multiplier"])?
                },
                floor_level: FloatP::parse(&v["floor_level"])?,
            })),
            "minecraft:canyon" => {
                let s = &v["shape"];
                Ok(Self::Canyon(Canyon {
                    probability: prob,
                    y,
                    vertical_rotation: FloatP::parse(&v["vertical_rotation"])?,
                    distance_factor: FloatP::parse(&s["distance_factor"])?,
                    thickness: FloatP::parse(&s["thickness"])?,
                    width_smoothness: s["width_smoothness"].as_i64().unwrap_or(1) as i32,
                    horizontal_radius_factor: FloatP::parse(&s["horizontal_radius_factor"])?,
                    vertical_default: s["vertical_radius_default_factor"].as_f64().unwrap_or(1.0)
                        as f32,
                    vertical_center: s["vertical_radius_center_factor"].as_f64().unwrap_or(0.0)
                        as f32,
                    y_scale: FloatP::parse(&s["y_scale"])?,
                }))
            }
            other => Err(format!("carver {other} is not supported")),
        }
    }

    /// `isStartChunk`.
    pub fn is_start(&self, r: &mut Legacy) -> bool {
        let p = match self {
            Self::Cave(c) => c.probability,
            Self::Canyon(c) => c.probability,
        };
        r.next_float() <= p
    }

    /// `carve`: вырезы из чанка-источника `(sx, sz)` в маску чанка `(cx, cz)`.
    /// `gen_min_y` и `gen_depth` — `WorldGenerationContext`.
    #[allow(clippy::too_many_arguments)]
    pub fn carve(
        &self,
        r: &mut Legacy,
        cx: i32,
        cz: i32,
        sx: i32,
        sz: i32,
        gen_min_y: i32,
        gen_depth: i32,
        mask: &mut Mask,
    ) {
        let mut out = Out { cx, cz, mask };
        match self {
            Self::Cave(c) => c.carve(r, sx, sz, &mut out),
            Self::Canyon(c) => c.carve(r, sx, sz, gen_min_y, gen_depth, &mut out),
        }
    }
}

impl Cave {
    fn carve(&self, r: &mut Legacy, sx: i32, sz: i32, out: &mut Out) {
        let max_distance = (4 * 2 - 1) * 16;
        let count = self.count.sample(r);
        for _ in 0..count {
            let x = (sx * 16 + r.next_int_bound(16)) as f64;
            let y = self.y.sample(r) as f64;
            let z = (sz * 16 + r.next_int_bound(16)) as f64;
            let hrm = self.horizontal.sample(r) as f64;
            let vrm = self.vertical.sample(r) as f64;
            let start_vrm = self.start_vertical.sample(r) as f64;
            let floor = self.floor_level.sample(r) as f64;
            let skip = move |xd: f64, yd: f64, zd: f64, _y: i32| {
                yd <= floor || xd * xd + yd * yd + zd * zd >= 1.0
            };
            let mut tunnels = 1;
            if r.next_int_bound(4) == 0 {
                let y_scale = self.room_vertical.sample(r) as f64;
                let thickness = 1.0 + r.next_float() * 6.0;
                // `createRoom`.
                let hr = 1.5 + (sin((PI_F / 2.0) as f64) * thickness) as f64;
                let vr = hr * y_scale;
                out.ellipsoid(x + 1.0, y, z, hr, vr, &skip);
                tunnels += r.next_int_bound(4);
            }
            for _ in 0..tunnels {
                let hrot = r.next_float() * (std::f64::consts::PI * 2.0) as f32;
                let vrot = (r.next_float() - 0.5) / 4.0;
                let thickness = self.thickness(r);
                let distance = max_distance - r.next_int_bound(max_distance / 4);
                let seed = r.next_long();
                self.tunnel(
                    out, seed, x, y, z, hrm, vrm, thickness, hrot, vrot, 0, distance, start_vrm,
                    &skip,
                );
            }
        }
    }

    fn thickness(&self, r: &mut Legacy) -> f32 {
        let mut t = self.thickness.sample(r);
        if self.weird_thickness_bias && r.next_int_bound(10) == 0 {
            t *= r.next_float() * r.next_float() * 3.0 + 1.0;
        }
        t
    }

    #[allow(clippy::too_many_arguments)]
    fn tunnel(
        &self,
        out: &mut Out,
        seed: i64,
        mut x: f64,
        mut y: f64,
        mut z: f64,
        hrm: f64,
        vrm: f64,
        thickness: f32,
        mut hrot: f32,
        mut vrot: f32,
        step: i32,
        dist: i32,
        y_scale: f64,
        skip: &dyn Fn(f64, f64, f64, i32) -> bool,
    ) {
        let mut r = Legacy::new(seed);
        let split = r.next_int_bound(dist / 2) + dist / 4;
        let steep = r.next_int_bound(6) == 0;
        let mut y_rota = 0f32;
        let mut x_rota = 0f32;
        for current in step..dist {
            let hr = 1.5 + (sin((PI_F * current as f32 / dist as f32) as f64) * thickness) as f64;
            let vr = hr * y_scale;
            let cos_x = cos(vrot as f64);
            x += (cos(hrot as f64) * cos_x) as f64;
            y += sin(vrot as f64) as f64;
            z += (sin(hrot as f64) * cos_x) as f64;
            vrot *= if steep { 0.92 } else { 0.7 };
            vrot += x_rota * 0.1;
            hrot += y_rota * 0.1;
            x_rota *= 0.9;
            y_rota *= 0.75;
            x_rota += (r.next_float() - r.next_float()) * r.next_float() * 2.0;
            y_rota += (r.next_float() - r.next_float()) * r.next_float() * 4.0;
            if current == split && thickness > 1.0 {
                for side in [-1.0f32, 1.0] {
                    let seed = r.next_long();
                    let t = r.next_float() * 0.5 + 0.5;
                    self.tunnel(
                        out,
                        seed,
                        x,
                        y,
                        z,
                        hrm,
                        vrm,
                        t,
                        hrot + side * (std::f64::consts::PI / 2.0) as f32,
                        vrot / 3.0,
                        current,
                        dist,
                        1.0,
                        skip,
                    );
                }
                return;
            }
            if r.next_int_bound(4) != 0 {
                if !out.can_reach(x, z, current, dist, thickness) {
                    return;
                }
                out.ellipsoid(x, y, z, hr * hrm, vr * vrm, skip);
            }
        }
    }
}

impl Canyon {
    fn carve(&self, r: &mut Legacy, sx: i32, sz: i32, min_y: i32, depth: i32, out: &mut Out) {
        let max_distance = (4 * 2 - 1) * 16;
        let x = (sx * 16 + r.next_int_bound(16)) as f64;
        let y = self.y.sample(r) as f64;
        let z = (sz * 16 + r.next_int_bound(16)) as f64;
        let mut hrot = r.next_float() * (std::f64::consts::PI * 2.0) as f32;
        let mut vrot = self.vertical_rotation.sample(r);
        let y_scale = self.y_scale.sample(r) as f64;
        let thickness = self.thickness.sample(r);
        let distance = (max_distance as f32 * self.distance_factor.sample(r)) as i32;
        let seed = r.next_long();

        // `doCarve`.
        let mut r = Legacy::new(seed);
        let mut widths = vec![0f32; depth as usize];
        let mut width = 1f32;
        for (i, w) in widths.iter_mut().enumerate() {
            if i == 0 || r.next_int_bound(self.width_smoothness) == 0 {
                width = 1.0 + r.next_float() * r.next_float();
            }
            *w = width * width;
        }
        let skip = |xd: f64, yd: f64, zd: f64, wy: i32| {
            let i = (wy - min_y) as usize;
            (xd * xd + zd * zd) * widths[i - 1] as f64 + yd * yd / 6.0 >= 1.0
        };
        let (mut x, mut y, mut z) = (x, y, z);
        let mut y_rota = 0f32;
        let mut x_rota = 0f32;
        for current in 0..distance {
            let mut hr =
                1.5 + (sin((current as f32 * PI_F / distance as f32) as f64) * thickness) as f64;
            let vr = hr * y_scale;
            hr *= self.horizontal_radius_factor.sample(&mut r) as f64;
            // `updateVerticalRadius`.
            let mul = 1.0 - (0.5 - current as f32 / distance as f32).abs() * 2.0;
            let factor = self.vertical_default + self.vertical_center * mul;
            let between = r.next_float() * (1.0 - 0.75) + 0.75;
            let vr = factor as f64 * vr * between as f64;
            let xc = cos(vrot as f64);
            let xs = sin(vrot as f64);
            x += (cos(hrot as f64) * xc) as f64;
            y += xs as f64;
            z += (sin(hrot as f64) * xc) as f64;
            vrot *= 0.7;
            vrot += x_rota * 0.05;
            hrot += y_rota * 0.05;
            x_rota *= 0.8;
            y_rota *= 0.5;
            x_rota += (r.next_float() - r.next_float()) * r.next_float() * 2.0;
            y_rota += (r.next_float() - r.next_float()) * r.next_float() * 4.0;
            if r.next_int_bound(4) != 0 {
                if !out.can_reach(x, z, current, distance, thickness) {
                    return;
                }
                out.ellipsoid(x, y, z, hr, vr, &skip);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sin_table_matches_mth() {
        assert_eq!(sin(0.0), 0.0);
        assert!((sin(std::f64::consts::FRAC_PI_2) - 1.0).abs() < 1e-6);
        assert!((cos(0.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn mask_visit_splits_runs_by_column() {
        let mut m = Mask::new(-63, 312);
        m.carve(1, 10, 2);
        m.carve(1, 11, 2);
        m.carve(1, 13, 2);
        m.carve(0, 312, 15);
        let mut seen = Vec::new();
        m.visit(|x, z, a, b| seen.push((x, z, a, b)));
        assert_eq!(seen, vec![(0, 15, 312, 312), (1, 2, 10, 11), (1, 2, 13, 13)]);
    }
}
