//! Шумы 26.3 (`synth/`): `PerlinNoise`, `SmearedPerlinNoise`, `NoiseStack`,
//! `NormalNoise`. Всё во float, как в ванили: точечный `get` и объёмный
//! `add_to_volume` считаются разными путями и должны совпасть с ними бит в бит.

use crate::random::{Bits, Legacy, Positional, Xoroshiro};
use crate::volume::Volume;
use serde_json::Value;

const GRADIENT: [[i8; 3]; 16] = [
    [1, 1, 0],
    [-1, 1, 0],
    [1, -1, 0],
    [-1, -1, 0],
    [1, 0, 1],
    [-1, 0, 1],
    [1, 0, -1],
    [-1, 0, -1],
    [0, 1, 1],
    [0, -1, 1],
    [0, 1, -1],
    [0, -1, -1],
    [1, 1, 0],
    [0, -1, 1],
    [-1, 1, 0],
    [0, -1, -1],
];

/// `Math.nextDown(1.6777216E7)`.
const HALF_ROUND_OFF: f64 = 16777215.999999998;

fn wrap(x: f64) -> f64 {
    if (-HALF_ROUND_OFF..HALF_ROUND_OFF).contains(&x) {
        x
    } else {
        x - (x / 3.3554432e7 + 0.5).floor() * 3.3554432e7
    }
}

fn floor(v: f64) -> i32 {
    v.floor() as i32
}

pub fn smoothstep(x: f32) -> f32 {
    x * x * x * (x * (x * 6.0 - 15.0) + 10.0)
}

pub fn lerp(a: f32, p0: f32, p1: f32) -> f32 {
    p0 + a * (p1 - p0)
}

#[allow(clippy::too_many_arguments)]
pub fn lerp3(
    a1: f32,
    a2: f32,
    a3: f32,
    x000: f32,
    x100: f32,
    x010: f32,
    x110: f32,
    x001: f32,
    x101: f32,
    x011: f32,
    x111: f32,
) -> f32 {
    let lerp2 = |a: f32, b: f32, c: f32, d: f32| lerp(a2, lerp(a1, a, b), lerp(a1, c, d));
    lerp(
        a3,
        lerp2(x000, x100, x010, x110),
        lerp2(x001, x101, x011, x111),
    )
}

fn grad_dot(hash: i32, x: f32, y: f32, z: f32) -> f32 {
    let g = GRADIENT[(hash & 15) as usize];
    g[0] as f32 * x + g[1] as f32 * y + g[2] as f32 * z
}

fn dot_xz(g: [i8; 3], x: f32, z: f32) -> f32 {
    g[0] as f32 * x + g[2] as f32 * z
}

/// Одна октава: `PerlinNoise`, а с `smear` — `SmearedPerlinNoise` (старый шум рельефа).
#[derive(Clone, Debug)]
pub struct Perlin {
    perms: [u8; 256],
    off: [f64; 3],
    smear: Option<f64>,
}

/// Что берут конструкторы шумов: `nextDouble` и `nextInt(bound)` своего
/// источника (у Xoroshiro `nextInt` свой, по Лемиру).
pub trait NoiseRandom {
    fn next_double(&mut self) -> f64;
    fn next_int_bound(&mut self, bound: i32) -> i32;
}

impl NoiseRandom for Xoroshiro {
    fn next_double(&mut self) -> f64 {
        Xoroshiro::next_double(self)
    }
    fn next_int_bound(&mut self, bound: i32) -> i32 {
        Xoroshiro::next_int_bound(self, bound)
    }
}

impl NoiseRandom for Legacy {
    fn next_double(&mut self) -> f64 {
        Bits::next_double(self)
    }
    fn next_int_bound(&mut self, bound: i32) -> i32 {
        Bits::next_int_bound(self, bound)
    }
}

/// `String.hashCode` Java.
pub fn java_hash(s: &str) -> i32 {
    s.encode_utf16()
        .fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(c as i32))
}

/// Таблица перестановок `GradientNoise`: сдвиги, затем тасовка.
fn shuffle(r: &mut impl NoiseRandom) -> [u8; 256] {
    let mut perms = [0u8; 256];
    for (i, p) in perms.iter_mut().enumerate() {
        *p = i as u8;
    }
    for i in 0..256 {
        let o = r.next_int_bound(256 - i as i32) as usize;
        perms.swap(i, o + i);
    }
    perms
}

/// `SimplexNoise` (двумерный срез): `Biome.BIOME_INFO_NOISE` для числа
/// цветов и травы.
pub struct Simplex {
    perms: [u8; 256],
    off: [f64; 2],
}

impl Simplex {
    /// `discard_offset` — `new SimplexNoise(random, true)`: смещения
    /// бросаются, но умножаются на ноль.
    pub fn new(r: &mut impl NoiseRandom, discard_offset: bool) -> Self {
        let scale = if discard_offset { 0.0 } else { 256.0 };
        let off = [r.next_double() * scale, r.next_double() * scale];
        r.next_double();
        Self {
            perms: shuffle(r),
            off,
        }
    }

    fn permute(&self, x: i32) -> i32 {
        self.perms[(x & 0xFF) as usize] as i32
    }

    fn corner(index: i32, x: f64, y: f64) -> f64 {
        let t = 0.5 - x * x - y * y;
        if t < 0.0 {
            return 0.0;
        }
        let g = GRADIENT[index as usize];
        let t = t * t;
        t * t * (g[0] as f64 * x + g[1] as f64 * y)
    }

    pub fn get(&self, x: f64, y: f64) -> f32 {
        let sqrt3 = 3f64.sqrt();
        let f2 = 0.5 * (sqrt3 - 1.0);
        let g2 = (3.0 - sqrt3) / 6.0;
        let (xin, yin) = (x + self.off[0], y + self.off[1]);
        let s = (xin + yin) * f2;
        let i = (xin + s).floor() as i32;
        let j = (yin + s).floor() as i32;
        let t = (i + j) as f64 * g2;
        let x0 = xin - (i as f64 - t);
        let y0 = yin - (j as f64 - t);
        let (i1, j1) = if x0 > y0 { (1, 0) } else { (0, 1) };
        let x1 = x0 - i1 as f64 + g2;
        let y1 = y0 - j1 as f64 + g2;
        let x2 = x0 - 1.0 + 2.0 * g2;
        let y2 = y0 - 1.0 + 2.0 * g2;
        let (ii, jj) = (i & 0xFF, j & 0xFF);
        let g0 = self.permute(ii + self.permute(jj)) % 12;
        let g1 = self.permute(ii + i1 + self.permute(jj + j1)) % 12;
        let gi2 = self.permute(ii + 1 + self.permute(jj + 1)) % 12;
        (70.0 * (Self::corner(g0, x0, y0) + Self::corner(g1, x1, y1) + Self::corner(gi2, x2, y2))) as f32
    }
}

impl Perlin {
    pub fn new(r: &mut impl NoiseRandom, smear: Option<f64>) -> Self {
        let off = [
            r.next_double() * 256.0,
            r.next_double() * 256.0,
            r.next_double() * 256.0,
        ];
        let perms = shuffle(r);
        Self { perms, off, smear }
    }

    fn permute(&self, x: i32) -> i32 {
        self.perms[(x & 0xFF) as usize] as i32
    }

    fn grad(&self, x: i32) -> [i8; 3] {
        GRADIENT[(self.permute(x) & 15) as usize]
    }

    /// `computeFudgeY` у размазанного шума.
    fn fudge(&self, scale: f64, original_y: f64, rel_y: f64) -> f64 {
        let limit = if original_y >= 0.0 && original_y < rel_y {
            original_y
        } else {
            rel_y
        };
        floor(limit / scale + 1.0e-7_f32 as f64) as f64 * scale
    }

    pub fn get(&self, x0: f64, y0: f64, z0: f64) -> f32 {
        let x = wrap(x0) + self.off[0];
        let y = wrap(y0) + self.off[1];
        let z = wrap(z0) + self.off[2];
        let (fx, fy, fz) = (floor(x), floor(y), floor(z));
        let rx = (x - fx as f64) as f32;
        let rz = (z - fz as f64) as f32;
        let ry = y - fy as f64;
        match self.smear {
            None => self.sample_and_lerp(fx, fy, fz, rx, ry as f32, rz, ry as f32),
            Some(s) => {
                let fudged = (ry - self.fudge(s, y0, ry)) as f32;
                self.sample_and_lerp(fx, fy, fz, rx, fudged, rz, ry as f32)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn sample_and_lerp(&self, x: i32, y: i32, z: i32, rx: f32, ry: f32, rz: f32, ory: f32) -> f32 {
        let x0 = self.permute(x);
        let x1 = self.permute(x.wrapping_add(1));
        let xy00 = self.permute(x0 + y);
        let xy01 = self.permute(x0 + y + 1);
        let xy10 = self.permute(x1 + y);
        let xy11 = self.permute(x1 + y + 1);
        let d000 = grad_dot(self.permute(xy00 + z), rx, ry, rz);
        let d100 = grad_dot(self.permute(xy10 + z), rx - 1.0, ry, rz);
        let d010 = grad_dot(self.permute(xy01 + z), rx, ry - 1.0, rz);
        let d110 = grad_dot(self.permute(xy11 + z), rx - 1.0, ry - 1.0, rz);
        let d001 = grad_dot(self.permute(xy00 + z + 1), rx, ry, rz - 1.0);
        let d101 = grad_dot(self.permute(xy10 + z + 1), rx - 1.0, ry, rz - 1.0);
        let d011 = grad_dot(self.permute(xy01 + z + 1), rx, ry - 1.0, rz - 1.0);
        let d111 = grad_dot(self.permute(xy11 + z + 1), rx - 1.0, ry - 1.0, rz - 1.0);
        lerp3(
            smoothstep(rx),
            smoothstep(ory),
            smoothstep(rz),
            d000,
            d100,
            d010,
            d110,
            d001,
            d101,
            d011,
            d111,
        )
    }

    /// `PerlinNoise.addToVolume` / `SmearedPerlinNoise.addToVolume`.
    pub fn add_to_volume(&self, buf: &mut [f32], v: &Volume, xz: f64, ys: f64, amp: f32) {
        let mut dxz = [0f32; 8];
        let mut gy = [0f32; 8];
        let mut index = 0;
        for iz in 0..v.size[2] {
            let z = wrap(v.block_z(iz) as f64 * xz) + self.off[2];
            let fz = floor(z);
            let rz = (z - fz as f64) as f32;
            let az = smoothstep(rz);
            for ix in 0..v.size[0] {
                let x = wrap(v.block_x(ix) as f64 * xz) + self.off[0];
                let fx = floor(x);
                let rx = (x - fx as f64) as f32;
                let x0 = self.permute(fx);
                let x1 = self.permute(fx.wrapping_add(1));
                let ax = smoothstep(rx);
                let mut last = i32::MIN;
                for iy in 0..v.size[1] {
                    let original_y = v.block_y(iy) as f64 * ys;
                    let y = wrap(original_y) + self.off[1];
                    let fy = floor(y);
                    let ry = y - fy as f64;
                    let ay = smoothstep(ry as f32);
                    if last != fy {
                        let xy00 = self.permute(x0 + fy);
                        let xy01 = self.permute(x0 + fy + 1);
                        let xy10 = self.permute(x1 + fy);
                        let xy11 = self.permute(x1 + fy + 1);
                        let corners = [
                            (xy00 + fz, rx, rz),
                            (xy10 + fz, rx - 1.0, rz),
                            (xy01 + fz, rx, rz),
                            (xy11 + fz, rx - 1.0, rz),
                            (xy00 + fz + 1, rx, rz - 1.0),
                            (xy10 + fz + 1, rx - 1.0, rz - 1.0),
                            (xy01 + fz + 1, rx, rz - 1.0),
                            (xy11 + fz + 1, rx - 1.0, rz - 1.0),
                        ];
                        for (i, (h, cx, cz)) in corners.into_iter().enumerate() {
                            let g = self.grad(h);
                            dxz[i] = dot_xz(g, cx, cz);
                            gy[i] = g[1] as f32;
                        }
                        last = fy;
                    }
                    let r = match self.smear {
                        None => ry as f32,
                        Some(s) => (ry - self.fudge(s, original_y, ry)) as f32,
                    };
                    let r1 = r - 1.0;
                    buf[index] += amp
                        * lerp3(
                            ax,
                            ay,
                            az,
                            dxz[0] + gy[0] * r,
                            dxz[1] + gy[1] * r,
                            dxz[2] + gy[2] * r1,
                            dxz[3] + gy[3] * r1,
                            dxz[4] + gy[4] * r,
                            dxz[5] + gy[5] * r,
                            dxz[6] + gy[6] * r1,
                            dxz[7] + gy[7] * r1,
                        );
                    index += 1;
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
struct Layer {
    noise: Perlin,
    freq: f64,
    amp: f32,
}

/// `NoiseStack`: сумма октав со своими частотой и амплитудой.
#[derive(Clone, Debug, Default)]
pub struct Stack {
    layers: Vec<Layer>,
}

impl Stack {
    pub fn add(&mut self, noise: Perlin, freq: f64, amp: f32) {
        self.layers.push(Layer { noise, freq, amp });
    }

    pub fn get(&self, x: f64, y: f64, z: f64) -> f32 {
        let mut v = 0f32;
        for l in &self.layers {
            v += l.amp * l.noise.get(x * l.freq, y * l.freq, z * l.freq);
        }
        v
    }

    pub fn add_to_volume(&self, buf: &mut [f32], v: &Volume, xz: f64, ys: f64, amp: f32) {
        for l in &self.layers {
            l.noise
                .add_to_volume(buf, v, xz * l.freq, ys * l.freq, amp * l.amp);
        }
    }
}

/// Параметры `worldgen/noise/*.json` (`NormalNoise.Parameters`).
#[derive(Clone, Debug)]
pub struct NormalNoise {
    /// (номер октавы, частота, амплитуда).
    octaves: Vec<(i32, f64, f64)>,
    norm: f64,
}

const PERLIN_DEVIATION: f64 = 0.2702247831245211;
const INPUT_FACTOR: f64 = 1.0181268882175227;

impl NormalNoise {
    pub fn from_json(v: &Value) -> Result<Self, String> {
        let base_amp = v["base_amplitude"].as_f64().unwrap_or(1.0);
        let base = v["base_octave"].as_i64().ok_or("noise: no base_octave")? as i32;
        let count = v["octave_count"].as_i64().unwrap_or(1) as i32;
        let normalize = match &v["normalize"] {
            Value::Bool(b) => *b,
            Value::Null => true,
            _ => return Err("noise: legacy normalization is not supported".into()),
        };
        let mods: Vec<f64> = v["amplitude_modifiers"]
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_f64).collect())
            .unwrap_or_default();
        let modifier = |i: i32| {
            if mods.is_empty() {
                1.0
            } else {
                mods[i as usize]
            }
        };
        let mut freq = 2f64.powi(base);
        let mut amp = if normalize {
            base_amp * (0.5f64.powi(-(count - 1)) / (0.5f64.powi(-count) - 1.0))
        } else {
            base_amp
        };
        let mut octaves = Vec::new();
        for i in 0..count {
            let m = modifier(i);
            if m != 0.0 {
                octaves.push((base + i, freq, amp * m));
            }
            freq *= 2.0;
            amp *= 0.5;
        }
        let target: f64 = octaves.iter().map(|o| o.2.abs()).sum();
        let variance: f64 = octaves
            .iter()
            .map(|o| {
                let d = PERLIN_DEVIATION * o.2.abs();
                d * d
            })
            .sum();
        let dev = variance.sqrt();
        let norm = if dev == 0.0 {
            0.0
        } else {
            target * 0.3333333333333333 / (dev * 2f64.sqrt())
        };
        Ok(Self { octaves, norm })
    }

    /// `NormalNoise.createParity`: базовая амплитуда подобрана так, чтобы
    /// нормировка совпала со старой (до 26.x).
    pub fn parity(first_octave: i32, amplitudes: &[f64]) -> Self {
        let n = amplitudes.len() as i32;
        let json = |base_amp: f64| {
            serde_json::json!({
                "base_octave": first_octave,
                "octave_count": n,
                "base_amplitude": base_amp,
                "amplitude_modifiers": amplitudes,
            })
        };
        let unit = Self::from_json(&json(1.0)).expect("parity noise");
        let base_amp = if unit.norm == 0.0 {
            1.0
        } else {
            let used: Vec<i32> = (0..n).filter(|&i| amplitudes[i as usize] != 0.0).collect();
            let span = used.last().unwrap_or(&0) - used.first().unwrap_or(&0);
            let old = 0.5 * 0.3333333333333333 / (0.1 * (1.0 + 1.0 / (span + 1) as f64));
            old / unit.norm
        };
        Self::from_json(&json(base_amp)).expect("parity noise")
    }

    /// `create(new WorldgenRandom(new LegacyRandomSource(seed)))`: шум
    /// провайдеров блоков с собственным сидом.
    pub fn create_legacy(&self, seed: i64) -> Stack {
        let mut r = Legacy::new(seed);
        let first = r.next_long();
        let second = r.next_long();
        let mut stack = Stack::default();
        for &(index, freq, amp) in &self.octaves {
            let hash = java_hash(&format!("octave_{index}")) as i64;
            let factor = (self.norm * amp) as f32;
            stack.add(Perlin::new(&mut Legacy::new(hash ^ first), None), freq, factor);
            stack.add(
                Perlin::new(&mut Legacy::new(hash ^ second), None),
                freq * INPUT_FACTOR,
                factor,
            );
        }
        stack
    }

    /// `NormalNoise.create`: две стопки октав, вторая со сдвигом частоты.
    pub fn create(&self, r: &mut Xoroshiro) -> Stack {
        let first = r.fork_positional();
        let second = r.fork_positional();
        let mut stack = Stack::default();
        for &(index, freq, amp) in &self.octaves {
            let name = format!("octave_{index}");
            let factor = (self.norm * amp) as f32;
            stack.add(
                Perlin::new(&mut first.from_hash_of(&name), None),
                freq,
                factor,
            );
            stack.add(
                Perlin::new(&mut second.from_hash_of(&name), None),
                freq * INPUT_FACTOR,
                factor,
            );
        }
        stack
    }
}

/// `BlendedNoise.createFbm`: октавы размазанного шума от крупной к мелкой.
pub fn blended_fbm(r: &mut Xoroshiro, first_octave: i32, smear_y: f64, value: f64) -> Stack {
    let octaves = -first_octave + 1;
    let mut factor = 1.0;
    let mut value = value / (2f64.powi(octaves) - 1.0);
    let mut stack = Stack::default();
    for _ in 0..octaves {
        stack.add(Perlin::new(r, Some(smear_y * factor)), factor, value as f32);
        factor /= 2.0;
        value *= 2.0;
    }
    stack
}

/// Позиционная фабрика, общая для всех шумов мира.
pub fn instantiate(params: &NormalNoise, world: &Positional, id: &str) -> Stack {
    params.create(&mut world.from_hash_of(id))
}
