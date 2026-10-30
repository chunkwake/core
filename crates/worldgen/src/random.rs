//! Случайные числа генератора: `XoroshiroRandomSource` и его позиционная фабрика
//! (`RandomSupport`, `Xoroshiro128PlusPlus`), бит в бит как в ванили.

use md5::{Digest, Md5};

const GOLDEN: i64 = -7046029254386353131;
const SILVER: i64 = 7640891576956012809;

fn mix_stafford13(z: i64) -> i64 {
    let z = (z ^ ((z as u64) >> 30) as i64).wrapping_mul(-4658895280553007687);
    let z = (z ^ ((z as u64) >> 27) as i64).wrapping_mul(-7723592293110705685);
    z ^ ((z as u64) >> 31) as i64
}

/// `RandomSupport.seedFromHashOf`: MD5 строки, две половины big-endian.
pub fn hash_of(name: &str) -> (i64, i64) {
    let h = Md5::digest(name.as_bytes());
    (
        i64::from_be_bytes(h[..8].try_into().unwrap()),
        i64::from_be_bytes(h[8..].try_into().unwrap()),
    )
}

/// `Mth.getSeed(x, y, z)`.
pub fn pos_seed(x: i32, y: i32, z: i32) -> i64 {
    let s = (x.wrapping_mul(3129871) as i64) ^ (z as i64).wrapping_mul(116129781) ^ y as i64;
    let s = s
        .wrapping_mul(s)
        .wrapping_mul(42317861)
        .wrapping_add(s.wrapping_mul(11));
    s >> 16
}

#[derive(Clone, Debug)]
pub struct Xoroshiro {
    lo: i64,
    hi: i64,
}

impl Xoroshiro {
    pub fn new(lo: i64, hi: i64) -> Self {
        if lo | hi == 0 {
            Self {
                lo: GOLDEN,
                hi: SILVER,
            }
        } else {
            Self { lo, hi }
        }
    }

    /// `new XoroshiroRandomSource(long)`: сид через `upgradeSeedTo128bit`.
    pub fn from_seed(seed: i64) -> Self {
        let lo = seed ^ SILVER;
        let hi = lo.wrapping_add(GOLDEN);
        Self::new(mix_stafford13(lo), mix_stafford13(hi))
    }

    pub fn next_long(&mut self) -> i64 {
        let (s0, mut s1) = (self.lo, self.hi);
        let r = s0.wrapping_add(s1).rotate_left(17).wrapping_add(s0);
        s1 ^= s0;
        self.lo = s0.rotate_left(49) ^ s1 ^ (s1 << 21);
        self.hi = s1.rotate_left(28);
        r
    }

    fn next_bits(&mut self, bits: u32) -> i64 {
        ((self.next_long() as u64) >> (64 - bits)) as i64
    }

    pub fn next_int(&mut self) -> i32 {
        self.next_long() as i32
    }

    /// `RandomSource.nextBoolean` — старший бит.
    pub fn next_bool(&mut self) -> bool {
        self.next_int() as u32 >> 31 != 0
    }

    /// `nextInt(bound)`: метод Лемира без смещения.
    pub fn next_int_bound(&mut self, bound: i32) -> i32 {
        let b = bound as u64;
        let mut m = (self.next_int() as u32 as u64).wrapping_mul(b);
        let mut frac = m & 0xFFFF_FFFF;
        if frac < b {
            let start = (bound.wrapping_neg() as u32 % bound as u32) as u64;
            while frac < start {
                m = (self.next_int() as u32 as u64).wrapping_mul(b);
                frac = m & 0xFFFF_FFFF;
            }
        }
        (m >> 32) as i32
    }

    pub fn next_double(&mut self) -> f64 {
        self.next_bits(53) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    pub fn next_float(&mut self) -> f32 {
        self.next_bits(24) as f32 * 5.9604645e-8
    }

    pub fn fork_positional(&mut self) -> Positional {
        let lo = self.next_long();
        let hi = self.next_long();
        Positional { lo, hi }
    }
}

/// `XoroshiroPositionalRandomFactory`.
#[derive(Clone, Copy, Debug)]
pub struct Positional {
    pub lo: i64,
    pub hi: i64,
}

impl Positional {
    pub fn at(&self, x: i32, y: i32, z: i32) -> Xoroshiro {
        Xoroshiro::new(pos_seed(x, y, z) ^ self.lo, self.hi)
    }

    pub fn from_hash_of(&self, name: &str) -> Xoroshiro {
        let (lo, hi) = hash_of(name);
        Xoroshiro::new(lo ^ self.lo, hi ^ self.hi)
    }
}

/// `BitRandomSource`: всё, что ванильный `RandomSource` строит из `next(bits)`.
/// Сиды `WorldgenRandom` тоже здесь — они не зависят от источника битов.
pub trait Bits {
    fn next(&mut self, bits: u32) -> i32;
    fn set_seed(&mut self, seed: i64);

    fn next_int(&mut self) -> i32 {
        self.next(32)
    }

    /// `BitRandomSource.nextInt(bound)`.
    fn next_int_bound(&mut self, bound: i32) -> i32 {
        if bound & (bound - 1) == 0 {
            return ((bound as i64 * self.next(31) as i64) >> 31) as i32;
        }
        loop {
            let sample = self.next(31);
            let modulo = sample % bound;
            if sample.wrapping_sub(modulo).wrapping_add(bound - 1) >= 0 {
                return modulo;
            }
        }
    }

    /// `Mth.nextInt(random, min, max)`: при `min >= max` — `min` без броска.
    fn next_between(&mut self, min: i32, max: i32) -> i32 {
        if min >= max {
            min
        } else {
            self.next_int_bound(max - min + 1) + min
        }
    }

    /// `Mth.randomBetweenInclusive`: бросок всегда.
    fn between_inclusive(&mut self, min: i32, max: i32) -> i32 {
        self.next_int_bound(max - min + 1) + min
    }

    fn next_long(&mut self) -> i64 {
        let upper = self.next(32) as i64;
        let lower = self.next(32) as i64;
        (upper << 32).wrapping_add(lower)
    }

    fn next_bool(&mut self) -> bool {
        self.next(1) != 0
    }

    fn next_float(&mut self) -> f32 {
        self.next(24) as f32 * 5.9604645e-8
    }

    fn next_double(&mut self) -> f64 {
        let upper = self.next(26) as i64;
        let lower = self.next(27) as i64;
        ((upper << 27) + lower) as f64 * 1.110223e-16f32 as f64
    }

    /// Отложенное второе значение `MarsagliaPolarGaussian`.
    fn gaussian_slot(&mut self) -> &mut Option<f64>;

    /// `MarsagliaPolarGaussian.nextGaussian` поверх `nextDouble`.
    fn next_gaussian(&mut self) -> f64 {
        if let Some(g) = self.gaussian_slot().take() {
            return g;
        }
        loop {
            let x = 2.0 * self.next_double() - 1.0;
            let y = 2.0 * self.next_double() - 1.0;
            let rr = x * x + y * y;
            if rr < 1.0 && rr != 0.0 {
                let m = (-2.0 * rr.ln() / rr).sqrt();
                *self.gaussian_slot() = Some(y * m);
                return x * m;
            }
        }
    }

    /// `WorldgenRandom.setLargeFeatureSeed`: карверы и старты структур.
    fn set_large_feature_seed(&mut self, seed: i64, cx: i32, cz: i32) {
        self.set_seed(seed);
        let xs = self.next_long();
        let zs = self.next_long();
        self.set_seed((cx as i64).wrapping_mul(xs) ^ (cz as i64).wrapping_mul(zs) ^ seed);
    }

    /// `WorldgenRandom.setLargeFeatureWithSalt`: клетка `random_spread`.
    fn set_large_feature_with_salt(&mut self, seed: i64, x: i32, z: i32, salt: i32) {
        let mixed = (x as i64)
            .wrapping_mul(341873128712)
            .wrapping_add((z as i64).wrapping_mul(132897987541))
            .wrapping_add(seed)
            .wrapping_add(salt as i64);
        self.set_seed(mixed);
    }

    /// `WorldgenRandom.setDecorationSeed`: сид декорации чанка.
    fn set_decoration_seed(&mut self, seed: i64, x: i32, z: i32) -> i64 {
        self.set_seed(seed);
        let xs = self.next_long() | 1;
        let zs = self.next_long() | 1;
        let result = (x as i64)
            .wrapping_mul(xs)
            .wrapping_add((z as i64).wrapping_mul(zs))
            ^ seed;
        self.set_seed(result);
        result
    }

    /// `WorldgenRandom.setFeatureSeed`.
    fn set_feature_seed(&mut self, seed: i64, index: i32, step: i32) {
        self.set_seed(
            seed.wrapping_add(index as i64)
                .wrapping_add(10000 * step as i64),
        );
    }
}

/// `LegacyRandomSource` / `SingleThreadedRandomSource` (линейный конгруэнтный
/// генератор `java.util.Random`).
#[derive(Clone, Debug)]
pub struct Legacy {
    seed: i64,
    gaussian: Option<f64>,
}

const LCG_MUL: i64 = 0x5DEECE66D;
const LCG_MASK: i64 = (1 << 48) - 1;

impl Legacy {
    pub fn new(seed: i64) -> Self {
        Self {
            seed: (seed ^ LCG_MUL) & LCG_MASK,
            gaussian: None,
        }
    }
}

impl Bits for Legacy {
    fn set_seed(&mut self, seed: i64) {
        self.seed = (seed ^ LCG_MUL) & LCG_MASK;
        self.gaussian = None;
    }

    fn gaussian_slot(&mut self) -> &mut Option<f64> {
        &mut self.gaussian
    }

    fn next(&mut self, bits: u32) -> i32 {
        self.seed = self.seed.wrapping_mul(LCG_MUL).wrapping_add(11) & LCG_MASK;
        (self.seed >> (48 - bits)) as i32
    }
}

/// `new WorldgenRandom(new XoroshiroRandomSource(..))` декорации: методы
/// `LegacyRandomSource`, но биты — старшие биты `nextLong` Xoroshiro.
#[derive(Clone, Debug)]
pub struct Worldgen {
    x: Xoroshiro,
    /// `WorldgenRandom.setSeed` не сбрасывает гауссиан `LegacyRandomSource`.
    gaussian: Option<f64>,
}

impl Worldgen {
    pub fn new(seed: i64) -> Self {
        Self {
            x: Xoroshiro::from_seed(seed),
            gaussian: None,
        }
    }

    /// `XoroshiroRandomSource.nextBoolean`: младший бит одного `nextLong`.
    pub(crate) fn next_boolean(&mut self) -> bool {
        self.x.next_long() & 1 != 0
    }

    /// `XoroshiroRandomSource.nextLong`, один шаг генератора.
    pub(crate) fn next_raw_long(&mut self) -> i64 {
        self.x.next_long()
    }
}

impl Bits for Worldgen {
    fn set_seed(&mut self, seed: i64) {
        self.x = Xoroshiro::from_seed(seed);
    }

    fn gaussian_slot(&mut self) -> &mut Option<f64> {
        &mut self.gaussian
    }

    fn next(&mut self, bits: u32) -> i32 {
        ((self.x.next_long() as u64) >> (64 - bits)) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_halves() {
        // MD5("") = d41d8cd98f00b204e9800998ecf8427e
        assert_eq!(hash_of("").0 as u64, 0xd41d8cd98f00b204);
        assert_eq!(hash_of("").1 as u64, 0xe9800998ecf8427e);
    }

    #[test]
    fn legacy_matches_java_util_random() {
        // new java.util.Random(42): nextInt() = -1170105035, nextInt(10) = 3,
        // nextLong() = -5843495416241995736 (после двух предыдущих), nextFloat.
        let mut r = Legacy::new(42);
        assert_eq!(r.next_int(), -1170105035);
        assert_eq!(r.next_int_bound(10), 3);
        let mut r = Legacy::new(42);
        assert_eq!(r.next_long(), -5025562857975149833);
        let mut r = Legacy::new(0);
        assert_eq!(r.next_float(), 0.73096776);
    }

    #[test]
    fn lemire_bound_in_range() {
        let mut r = Xoroshiro::from_seed(42);
        for _ in 0..1000 {
            let v = r.next_int_bound(10);
            assert!((0..10).contains(&v));
        }
    }
}
