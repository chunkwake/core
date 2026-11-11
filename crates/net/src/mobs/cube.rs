//! Слизень (`AbstractCubeMob`, `Slime` 26.3): прыжковое управление
//! `CubeMobMoveControl`, слизневые чанки и размер при появлении.

use super::goals::Out;
use super::{Mob, path};

/// `CubeMobMoveControl`: куда смотрит (`yRot`), задержка прыжка, агрессия
/// (`setDirection`) и множитель скорости с операцией `MOVE_TO`
/// (`setWantedMovement`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CubeMove {
    pub y_rot: f32,
    pub jump_delay: i32,
    pub aggressive: bool,
    pub speed_mod: f64,
    pub move_to: bool,
}

impl CubeMove {
    /// `setDirection`.
    pub fn set_direction(&mut self, y_rot: f32, aggressive: bool) {
        self.y_rot = y_rot;
        self.aggressive = aggressive;
    }

    /// `setWantedMovement`.
    pub fn set_wanted_movement(&mut self, speed: f64) {
        self.speed_mod = speed;
        self.move_to = true;
    }
}

/// Атрибут `movement_speed` слизня: `0.2 + 0.1 × размер` во `float`.
pub fn speed(size: i32) -> f64 {
    f64::from(0.2f32 + 0.1f32 * size as f32)
}

/// `isTiny`: размер не больше 1 — не кусается, звуки `_small`.
pub fn tiny(m: &Mob) -> bool {
    m.cube_size <= 1
}

/// `isDealsDamage`: бьёт касанием всякий не крошечный.
pub fn deals_damage(m: &Mob) -> bool {
    !tiny(m)
}

/// `CubeMobMoveControl.tick`. `rand` — `nextInt(20)` в `getJumpDelay`.
pub fn move_control(m: &mut Mob, rand: &mut dyn FnMut() -> f64) {
    m.yaw = path::rotlerp(m.yaw, m.cube.y_rot, 90.0);
    m.head_yaw = m.yaw;
    if !m.cube.move_to {
        m.zza = 0.0;
        return;
    }
    m.cube.move_to = false;
    let speed = (m.cube.speed_mod * m.speed()) as f32;
    if !m.on_ground {
        m.set_speed(speed);
        return;
    }
    m.set_speed(speed);
    let delay = m.cube.jump_delay;
    m.cube.jump_delay -= 1;
    if delay <= 0 {
        // `Slime.getJumpDelay`: `random.nextInt(20) + 10`.
        let mut next = ((rand() * 20.0) as i32).min(19) + 10;
        if m.cube.aggressive {
            next /= 3;
        }
        m.cube.jump_delay = next;
        m.jump = true;
        m.out.push(Out::CubeSound { jump: true });
    } else {
        m.xxa = 0.0;
        m.zza = 0.0;
        m.set_speed(0.0);
    }
}

/// `Slime.tick` после `super.tick`: приземлился — звук шлепка.
pub fn after_tick(m: &mut Mob) {
    if m.on_ground && !m.was_on_ground {
        m.out.push(Out::CubeSound { jump: false });
    }
    m.was_on_ground = m.on_ground;
}

/// `java.util.Random`: столько, сколько нужно для `nextInt(bound)`.
struct JavaRandom(i64);

impl JavaRandom {
    fn new(seed: i64) -> Self {
        Self((seed ^ 0x5DEE_CE66D) & ((1 << 48) - 1))
    }

    fn next(&mut self, bits: u32) -> i32 {
        self.0 = (self.0.wrapping_mul(0x5DEE_CE66D).wrapping_add(0xB)) & ((1 << 48) - 1);
        (self.0 >> (48 - bits)) as i32
    }

    fn next_int(&mut self, bound: i32) -> i32 {
        if (bound as u32).is_power_of_two() {
            return ((i64::from(bound) * i64::from(self.next(31))) >> 31) as i32;
        }
        loop {
            let bits = self.next(31);
            let val = bits % bound;
            if bits - val + (bound - 1) >= 0 {
                return val;
            }
        }
    }
}

/// Слизневый чанк: `WorldgenRandom.seedSlimeChunk(x, z, seed, 987234911)`
/// и `nextInt(10) == 0`. Арифметика — как в байткоде: `int` до приведения.
pub fn slime_chunk(cx: i32, cz: i32, seed: i64) -> bool {
    let s = seed
        .wrapping_add(i64::from(cx.wrapping_mul(cx).wrapping_mul(4_987_142)))
        .wrapping_add(i64::from(cx.wrapping_mul(5_947_611)))
        .wrapping_add(i64::from(cz.wrapping_mul(cz)).wrapping_mul(4_392_871))
        .wrapping_add(i64::from(cz.wrapping_mul(389_711)))
        ^ 987_234_911;
    JavaRandom::new(s).next_int(10) == 0
}

/// `Slime.finalizeSpawn`: `i = nextInt(3)`, при `i < 2` и
/// `nextFloat < 0.5 × specialMultiplier` — на ступень больше; размер `1 << i`.
pub fn spawn_size(r1: f64, r2: f64, special: f32) -> i32 {
    let mut i = ((r1 * 3.0) as i32).min(2);
    if i < 2 && (r2 as f32) < 0.5 * special {
        i += 1;
    }
    1 << i
}

/// `gameplay/surface_slime_spawn_chance` из `timeline/moon.json`: по фазе
/// луны (сутки по 24000 тиков, цикл 8), от 0.5 в полнолуние до 0 в новолуние.
pub fn surface_spawn_chance(day_time: i64) -> f32 {
    const BY_PHASE: [f32; 8] = [0.5, 0.375, 0.25, 0.125, 0.0, 0.125, 0.25, 0.375];
    BY_PHASE[(day_time.div_euclid(24000)).rem_euclid(8) as usize]
}

/// `DifficultyInstance.getSpecialMultiplier` без обжитости чанка и луны
/// (их Chunkwake пока не считает: оба слагаемых — 0). `day_time` —
/// `Level.getDayTime`.
pub fn special_multiplier(difficulty: u8, day_time: i64) -> f32 {
    if difficulty == 0 {
        return 0.0;
    }
    let g = ((day_time as f32 - 72000.0) / 1_440_000.0).clamp(0.0, 1.0) * 0.25;
    let effective = f32::from(difficulty) * (0.75 + g);
    if effective < 2.0 {
        0.0
    } else if effective > 4.0 {
        1.0
    } else {
        (effective - 2.0) / 2.0
    }
}

/// Смещения детей при делении (`Slime.remove`): `count` штук размера
/// `size / 2` вокруг родителя шириной `width`.
pub fn split_offsets(count: i32, width: f64) -> Vec<[f64; 2]> {
    let half = width / 2.0;
    (0..count)
        .map(|i| {
            [
                (f64::from(i % 2) - 0.5) * half,
                (f64::from(i / 2) - 0.5) * half,
            ]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_random_matches_jdk() {
        // `new Random(0).nextInt(10)` = 0, `new Random(42).nextInt(10)` = 0,
        // `new Random(1).nextInt(10)` = 5.
        assert_eq!(JavaRandom::new(0).next_int(10), 0);
        assert_eq!(JavaRandom::new(42).next_int(10), 0);
        assert_eq!(JavaRandom::new(1).next_int(10), 5);
    }

    #[test]
    fn slime_chunks_match_jdk() {
        // Посчитано в jshell по `seedSlimeChunk` из байткода 26.3, зерно 12345.
        let want = [
            (-5, 2),
            (-4, 0),
            (-2, -4),
            (-2, 1),
            (-2, 4),
            (-1, 2),
            (0, -6),
            (0, -2),
            (3, 0),
            (3, 5),
            (4, -6),
            (4, -5),
            (4, -3),
            (4, 1),
            (5, -3),
            (6, -2),
        ];
        let got: Vec<(i32, i32)> = (-6..=6)
            .flat_map(|x| (-6..=6).map(move |z| (x, z)))
            .filter(|&(x, z)| slime_chunk(x, z, 12345))
            .collect();
        assert_eq!(got, want);
    }

    #[test]
    fn slime_chunks_are_about_a_tenth() {
        let n = (-50..50)
            .flat_map(|x| (-50..50).map(move |z| (x, z)))
            .filter(|&(x, z)| slime_chunk(x, z, 12345))
            .count();
        assert!((800..1200).contains(&n), "{n}");
    }

    #[test]
    fn size_scales_stats() {
        let mut m = Mob::new(super::super::Species::Slime, [0.0; 3], 1);
        m.set_size(4, true);
        assert_eq!(m.health, 16.0);
        assert_eq!(m.max_health(), 16.0);
        assert_eq!(m.attack_damage(), 4.0);
        assert_eq!(m.xp(0.5), 4);
        assert!((m.size().0 - 2.08).abs() < 1e-6);
        assert_eq!(m.sound("hurt"), "minecraft:entity.slime.hurt");
        assert!(deals_damage(&m));
        m.set_size(1, false);
        assert_eq!(m.health, 1.0);
        assert_eq!(m.sound("death"), "minecraft:entity.slime.death_small");
        assert!(!deals_damage(&m));
        m.set_size(500, false);
        assert_eq!(m.cube_size, 127);
    }

    #[test]
    fn sizes_and_speed() {
        assert_eq!(spawn_size(0.0, 0.9, 1.0), 1);
        assert_eq!(spawn_size(0.0, 0.1, 1.0), 2);
        assert_eq!(spawn_size(0.99, 0.0, 1.0), 4);
        assert_eq!(speed(1), f64::from(0.3f32));
        assert_eq!(speed(4), f64::from(0.6f32));
    }
}
