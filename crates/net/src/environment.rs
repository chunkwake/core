//! Контакт с опасными блоками, воздух под водой и замерзание.

pub const WATER: u16 = 1;
pub const LAVA: u16 = 2;
pub const FIRE: u16 = 4;
pub const CACTUS: u16 = 8;
pub const BERRIES: u16 = 16;
pub const POWDER_SNOW: u16 = 32;
pub const MAGMA: u16 = 64;

pub const MAX_AIR: i32 = 300;
pub const MAX_FROZEN: u32 = 140;

pub fn block_kind(name: &str, waterlogged: bool) -> u16 {
    let mut kind = if waterlogged { WATER } else { 0 };
    kind |= match name {
        "minecraft:water" | "minecraft:bubble_column" => WATER,
        "minecraft:lava" => LAVA,
        "minecraft:fire" | "minecraft:soul_fire" => FIRE,
        "minecraft:cactus" => CACTUS,
        "minecraft:sweet_berry_bush" => BERRIES,
        "minecraft:powder_snow" => POWDER_SNOW,
        "minecraft:magma_block" => MAGMA,
        _ => 0,
    };
    kind
}

#[derive(Clone, Copy, Debug, Default)]
pub struct View {
    pub water: bool,
    pub submerged: bool,
    pub lava: bool,
    pub fire: bool,
    pub cactus: bool,
    pub berries: bool,
    pub powder_snow: bool,
    pub magma: bool,
}

/// Сканирует AABB сущности; `eye` — высота глаз от ног.
pub fn probe(
    pos: [f64; 3],
    width: f64,
    height: f64,
    eye: f64,
    mut at: impl FnMut([i32; 3]) -> u16,
) -> View {
    let half = width / 2.0;
    let min = [
        (pos[0] - half).floor() as i32,
        pos[1].floor() as i32,
        (pos[2] - half).floor() as i32,
    ];
    let max = [
        (pos[0] + half - 1e-6).floor() as i32,
        (pos[1] + height - 1e-6).floor() as i32,
        (pos[2] + half - 1e-6).floor() as i32,
    ];
    let mut bits = 0;
    for x in min[0]..=max[0] {
        for y in min[1]..=max[1] {
            for z in min[2]..=max[2] {
                bits |= at([x, y, z]);
            }
        }
    }
    let eye_cell = [
        pos[0].floor() as i32,
        (pos[1] + eye).floor() as i32,
        pos[2].floor() as i32,
    ];
    let below = [
        pos[0].floor() as i32,
        (pos[1] - 0.05).floor() as i32,
        pos[2].floor() as i32,
    ];
    View {
        water: bits & WATER != 0,
        submerged: at(eye_cell) & WATER != 0,
        lava: bits & LAVA != 0,
        fire: bits & FIRE != 0,
        cactus: bits & CACTUS != 0,
        berries: bits & BERRIES != 0,
        powder_snow: bits & POWDER_SNOW != 0,
        magma: at(below) & MAGMA != 0,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct State {
    pub air: i32,
    pub frozen: u32,
    pub burning: u32,
}

impl Default for State {
    fn default() -> Self {
        Self {
            air: MAX_AIR,
            frozen: 0,
            burning: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Pulse {
    pub drown: bool,
    pub freeze: bool,
    pub burn: bool,
    pub lava: bool,
    pub cactus: bool,
    pub berries: bool,
    pub magma: bool,
}

impl State {
    pub fn step(&mut self, view: View, tick: u64, leather_boots: bool) -> Pulse {
        let mut pulse = Pulse::default();
        if view.submerged {
            self.air -= 1;
            if self.air <= -20 {
                self.air = 0;
                pulse.drown = true;
            }
        } else {
            self.air = (self.air + 4).min(MAX_AIR);
        }
        if view.powder_snow && !leather_boots && !view.fire && !view.lava {
            self.frozen = (self.frozen + 1).min(MAX_FROZEN);
        } else {
            self.frozen = self.frozen.saturating_sub(2);
        }
        if view.water {
            self.burning = 0;
        } else if view.lava {
            self.burning = self.burning.max(300);
        } else if view.fire {
            self.burning = self.burning.max(160);
        } else {
            self.burning = self.burning.saturating_sub(1);
        }
        pulse.freeze = self.frozen >= MAX_FROZEN && tick.is_multiple_of(40);
        pulse.burn = self.burning > 0 && tick.is_multiple_of(20);
        pulse.lava = view.lava && tick.is_multiple_of(10);
        pulse.cactus = view.cactus && tick.is_multiple_of(10);
        pulse.berries = view.berries && tick.is_multiple_of(20);
        pulse.magma = view.magma && tick.is_multiple_of(10);
        pulse
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn underwater_air_damage_and_recovery() {
        let mut s = State::default();
        let wet = View {
            submerged: true,
            water: true,
            ..Default::default()
        };
        for t in 0..319 {
            assert!(!s.step(wet, t, false).drown);
        }
        assert!(s.step(wet, 319, false).drown);
        assert_eq!(s.air, 0);
        assert!(!s.step(View::default(), 320, false).drown);
        assert_eq!(s.air, 4);
    }

    #[test]
    fn snow_leather_and_water_extinguish() {
        let snow = View {
            powder_snow: true,
            ..Default::default()
        };
        let mut s = State::default();
        for t in 0..140 {
            s.step(snow, t, false);
        }
        assert!(s.step(snow, 160, false).freeze);
        s.step(snow, 161, true);
        assert_eq!(s.frozen, 138);
        s.step(
            View {
                fire: true,
                ..Default::default()
            },
            162,
            false,
        );
        assert!(s.burning > 0);
        s.step(
            View {
                water: true,
                ..Default::default()
            },
            163,
            false,
        );
        assert_eq!(s.burning, 0);
    }

    #[test]
    fn probe_checks_body_eyes_and_floor() {
        let view = probe([0.5, 1.0, 0.5], 0.6, 1.8, 1.62, |p| match p[1] {
            0 => MAGMA,
            1 => CACTUS,
            2 => WATER,
            _ => 0,
        });
        assert!(view.magma && view.cactus && view.submerged);
    }

    #[test]
    fn contact_damage_uses_independent_intervals() {
        let mut s = State::default();
        let danger = View {
            lava: true,
            cactus: true,
            berries: true,
            magma: true,
            ..Default::default()
        };
        let first = s.step(danger, 10, false);
        assert!(first.lava && first.cactus && first.magma);
        assert!(!first.berries && !first.burn);
        let second = s.step(danger, 20, false);
        assert!(second.lava && second.cactus && second.berries && second.burn);
        assert_eq!(s.burning, 300);
    }
}
