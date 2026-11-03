//! PrimedTnt и 16³ лучей ванильного взрыва.

use std::collections::HashSet;

pub const FUSE: i32 = 80;
pub const POWER: f32 = 4.0;

#[derive(Clone, Copy, Debug)]
pub struct Primed {
    pub pos: [f64; 3],
    pub vel: [f64; 3],
    pub fuse: i32,
}

impl Primed {
    pub fn new(pos: [f64; 3], angle: f64, fuse: i32) -> Self {
        Self {
            pos,
            vel: [-angle.sin() * 0.02, 0.2, -angle.cos() * 0.02],
            fuse,
        }
    }

    /// Движение и обратный отсчёт. `true` — фитиль истёк.
    pub fn step(&mut self, solid: impl Fn([i32; 3]) -> bool) -> bool {
        self.vel[1] -= 0.04;
        for axis in 0..3 { self.pos[axis] += self.vel[axis]; }
        self.vel.iter_mut().for_each(|v| *v *= 0.98);
        let below = [self.pos[0].floor() as i32, (self.pos[1] - 0.05).floor() as i32, self.pos[2].floor() as i32];
        if solid(below) && self.vel[1] < 0.0 {
            self.pos[1] = below[1] as f64 + 1.0;
            self.vel[0] *= 0.7;
            self.vel[2] *= 0.7;
            self.vel[1] *= -0.5;
        }
        self.fuse -= 1;
        self.fuse <= 0
    }
}

/// `Explosion.explode`: по 16 точек на каждой грани куба, шаг луча 0.3.
/// `resistance` возвращает `None` для воздуха и `Some(INFINITY)` для щита.
pub fn affected(
    center: [f64; 3],
    mut resistance: impl FnMut([i32; 3]) -> Option<f32>,
    mut random: impl FnMut() -> f32,
) -> HashSet<[i32; 3]> {
    let mut out = HashSet::new();
    for x in 0..16 {
        for y in 0..16 {
            for z in 0..16 {
                if x != 0 && x != 15 && y != 0 && y != 15 && z != 0 && z != 15 { continue; }
                let mut dir = [x as f64 / 15.0 * 2.0 - 1.0, y as f64 / 15.0 * 2.0 - 1.0, z as f64 / 15.0 * 2.0 - 1.0];
                let len = dir.iter().map(|v| v * v).sum::<f64>().sqrt();
                for d in &mut dir { *d = *d / len * 0.3; }
                let mut pos = center;
                let mut strength = POWER * (0.7 + random() * 0.6);
                while strength > 0.0 {
                    let block = pos.map(|v| v.floor() as i32);
                    if let Some(r) = resistance(block) {
                        strength -= (r + 0.3) * 0.3;
                        if strength > 0.0 { out.insert(block); }
                    }
                    strength -= 0.225;
                    for i in 0..3 { pos[i] += dir[i]; }
                }
            }
        }
    }
    out
}

pub fn damage(distance: f64, exposure: f64) -> f32 {
    let impact = (1.0 - distance / 8.0).max(0.0) * exposure.clamp(0.0, 1.0);
    if impact == 0.0 { return 0.0; }
    ((impact * impact + impact) / 2.0 * 7.0 * 8.0 + 1.0).floor() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuse_and_gravity() {
        let mut t = Primed::new([0.5, 2.0, 0.5], 0.0, FUSE);
        assert!(!t.step(|_| false));
        assert!(t.pos[1] > 2.0);
        for _ in 1..FUSE - 1 { assert!(!t.step(|_| false)); }
        assert!(t.step(|_| false));
    }

    #[test]
    fn rays_stop_at_resistant_blocks() {
        let open = affected([0.5; 3], |_| Some(0.0), || 0.5);
        assert!(open.contains(&[2, 0, 0]));
        let protected = affected([0.5; 3], |p| Some(if p[0] == 1 { f32::INFINITY } else { 0.0 }), || 0.5);
        assert!(!protected.iter().any(|p| p[0] >= 2));
    }

    #[test]
    fn blast_damage_falls_with_distance() {
        assert!(damage(0.0, 1.0) > damage(4.0, 1.0));
        assert_eq!(damage(0.0, 0.0), 0.0);
    }
}
