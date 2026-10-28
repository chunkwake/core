//! Предметы-сущности на земле: физика падения (как `ItemEntity.tick`) и
//! дальность подбора. Состояние мира и рассылка — в `play`.

use chunkwake_protocol::inventory::Stack;
use std::time::{Duration, Instant};

/// Предмет-сущность. Физику ведёт сервер каждый тик (как ванила), клиенту
/// шлёт скорость и позицию при толчках и раз в секунду в полёте.
#[derive(Clone, Copy, Debug)]
pub struct Dropped {
    pub pos: [f64; 3],
    pub vel: [f64; 3],
    pub on_ground: bool,
    pub stack: Stack,
    pub pickup_at: Instant,
    pub despawn_at: Instant,
    /// Тиков с появления: от него — период синхронизации и слияния.
    pub age: u32,
}

impl Dropped {
    pub fn new(pos: [f64; 3], vel: [f64; 3], stack: Stack, delay: Duration) -> Self {
        let now = Instant::now();
        Self {
            pos,
            vel,
            on_ground: false,
            stack,
            pickup_at: now + delay,
            despawn_at: now + LIFETIME,
            age: 0,
        }
    }

    /// Лежит неподвижно: тикать не нужно, пока под ним есть блок.
    pub fn resting(&self) -> bool {
        self.on_ground && self.vel[0].abs() < REST && self.vel[2].abs() < REST
    }
}

/// Задержка подбора: выброшенный игроком (40 тиков) и выпавший из блока (10).
pub const THROW_DELAY: Duration = Duration::from_millis(2000);
pub const BLOCK_DELAY: Duration = Duration::from_millis(500);
/// Предмет исчезает через 5 минут (6000 тиков).
pub const LIFETIME: Duration = Duration::from_secs(300);
/// Глаза игрока над ногами и высота выброса под ними.
pub const EYE_HEIGHT: f64 = 1.62;
const THROW_BELOW_EYES: f64 = 0.3;
/// Ниже `min_y` на столько — предмет исчезает (урон пустоты у ванилы).
pub const VOID_DEPTH: f64 = 64.0;
/// Синхронизация летящего предмета (`EntityType.ITEM.updateInterval` = 20).
pub const SYNC_TICKS: u32 = 20;

const GRAVITY: f64 = 0.04;
const DRAG: f64 = 0.98;
/// Трение о землю (`Block.friction` 0.6 у обычных блоков).
const GROUND_FRICTION: f64 = 0.6;
const REST: f64 = 1e-3;

/// Один тик физики (`ItemEntity.tick`): гравитация, движение со столкновениями,
/// трение. `true` — скорость сменилась скачком (упал, упёрся): клиенту нужна
/// синхронизация.
pub fn step(d: &mut Dropped, solid: impl Fn([i32; 3]) -> bool) -> bool {
    let block = |p: [f64; 3]| p.map(|c| c.floor() as i32);
    let was_on_ground = d.on_ground;
    let mut hit = false;
    d.vel[1] -= GRAVITY;
    // Горизонталь: упёрся в блок — скорость по оси гасится.
    for axis in [0, 2] {
        let mut next = d.pos;
        next[axis] += d.vel[axis];
        if solid(block(next)) {
            hit |= d.vel[axis].abs() > REST;
            d.vel[axis] = 0.0;
        } else {
            d.pos = next;
        }
    }
    let mut next = d.pos;
    next[1] += d.vel[1];
    d.on_ground = false;
    if d.vel[1] < 0.0 && solid(block(next)) {
        d.pos[1] = next[1].floor() + 1.0;
        d.vel[1] = 0.0;
        d.on_ground = true;
    } else if d.vel[1] > 0.0 && solid(block(next)) {
        d.vel[1] = 0.0;
        hit = true;
    } else {
        d.pos = next;
    }
    let friction = if d.on_ground {
        GROUND_FRICTION * DRAG
    } else {
        DRAG
    };
    d.vel = [d.vel[0] * friction, d.vel[1] * DRAG, d.vel[2] * friction];
    if d.resting() {
        d.vel = [0.0; 3];
    }
    d.age += 1;
    hit || d.on_ground != was_on_ground
}

/// Где остановится предмет (прогон `step` до покоя); `None` — упал в пустоту.
pub fn settle(
    pos: [f64; 3],
    vel: [f64; 3],
    min_y: f64,
    solid: impl Fn([i32; 3]) -> bool,
) -> Option<[f64; 3]> {
    let mut d = Dropped::new(pos, vel, Stack { item: 0, count: 1 }, Duration::ZERO);
    for _ in 0..600 {
        step(&mut d, &solid);
        if d.resting() {
            break;
        }
        if d.pos[1] < min_y - VOID_DEPTH {
            return None;
        }
    }
    Some(d.pos)
}

/// Сливаются ли два предмета на земле (`ItemEntity.mergeWithNeighbours`):
/// тот же предмет, влезает в одну стопку, хитбоксы 0.25 касаются с запасом
/// 0.5 по горизонтали.
pub fn can_merge(a: &Dropped, b: &Dropped, max_stack: i32) -> bool {
    a.stack.item == b.stack.item
        && a.stack.count + b.stack.count <= max_stack
        && (a.pos[0] - b.pos[0]).abs() < 0.75
        && (a.pos[2] - b.pos[2]).abs() < 0.75
        && (a.pos[1] - b.pos[1]).abs() < 0.25
}

/// Скорость выброшенного игроком предмета (`Player.drop`). `r` — три
/// случайных числа из [0, 1) для разброса; нули — без разброса.
pub fn throw_velocity(yaw: f32, pitch: f32, r: [f64; 3]) -> [f64; 3] {
    let (yaw, pitch) = ((yaw as f64).to_radians(), (pitch as f64).to_radians());
    let (angle, power) = (r[0] * std::f64::consts::TAU, 0.02 * r[1]);
    [
        -yaw.sin() * pitch.cos() * 0.3 + angle.cos() * power,
        -pitch.sin() * 0.3 + 0.1 + (r[2] - r[1]) * 0.1,
        yaw.cos() * pitch.cos() * 0.3 + angle.sin() * power,
    ]
}

/// Откуда вылетает выброшенный предмет: чуть ниже глаз.
pub fn throw_origin(feet: [f64; 3]) -> [f64; 3] {
    [feet[0], feet[1] + EYE_HEIGHT - THROW_BELOW_EYES, feet[2]]
}

/// Скорость предмета при смерти (`Player.drop(stack, true)`): во все стороны.
/// `angle` и `power` — случайные числа из [0, 1).
pub fn scatter_velocity(angle: f64, power: f64) -> [f64; 3] {
    let (a, f) = (angle * std::f64::consts::TAU, power * 0.5);
    [-a.sin() * f, 0.2, a.cos() * f]
}

/// Скорость предмета, выпавшего из блока (`Block.popResource`).
pub fn pop_velocity(rx: f64, rz: f64) -> [f64; 3] {
    [rx * 0.2 - 0.1, 0.2, rz * 0.2 - 0.1]
}

/// Дотянется ли игрок с ногами в `feet`: его хитбокс 0.6×1.8, расширенный
/// на (1, 0.5, 1), против хитбокса предмета 0.25×0.25.
pub fn in_reach(feet: [f64; 3], item: [f64; 3]) -> bool {
    const H: f64 = 0.3 + 1.0 + 0.125;
    (item[0] - feet[0]).abs() <= H
        && (item[2] - feet[2]).abs() <= H
        && item[1] + 0.25 >= feet[1] - 0.5
        && item[1] <= feet[1] + 1.8 + 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Плоский мир: всё ниже y = 0 твёрдое.
    fn floor(p: [i32; 3]) -> bool {
        p[1] < 0
    }

    #[test]
    fn falls_onto_floor() {
        let p = settle([0.5, 10.0, 0.5], [0.0; 3], -64.0, floor).unwrap();
        assert_eq!(p, [0.5, 0.0, 0.5]);
    }

    #[test]
    fn thrown_forward_lands_ahead() {
        // Взгляд на юг (+z), горизонтально.
        let v = throw_velocity(0.0, 0.0, [0.0; 3]);
        let p = settle(throw_origin([0.5, 0.0, 0.5]), v, -64.0, floor).unwrap();
        assert_eq!(p[1], 0.0);
        assert!(p[2] > 1.5 && p[2] < 4.0, "{p:?}");
        assert!((p[0] - 0.5).abs() < 1e-9);
    }

    #[test]
    fn stops_at_wall_and_void() {
        let wall = |p: [i32; 3]| p[1] < 0 || p[2] >= 2;
        let p = settle([0.5, 1.0, 0.5], [0.0, 0.0, 1.0], -64.0, wall).unwrap();
        assert!(p[2] < 2.0);
        assert_eq!(settle([0.5, 1.0, 0.5], [0.0; 3], 0.0, |_| false), None);
    }

    #[test]
    fn reach() {
        let feet = [0.0, 64.0, 0.0];
        assert!(in_reach(feet, [1.0, 64.0, -1.0]));
        assert!(in_reach(feet, [0.0, 63.3, 0.0]));
        assert!(!in_reach(feet, [2.0, 64.0, 0.0]));
        assert!(!in_reach(feet, [0.0, 67.0, 0.0]));
    }

    #[test]
    fn merging() {
        let now = Instant::now();
        let _ = now;
        let at = |x: f64, item, count| {
            Dropped::new(
                [x, 64.0, 0.0],
                [0.0; 3],
                Stack { item, count },
                Duration::ZERO,
            )
        };
        assert!(can_merge(&at(0.0, 1, 10), &at(0.5, 1, 20), 64));
        assert!(!can_merge(&at(0.0, 1, 10), &at(1.0, 1, 20), 64));
        assert!(!can_merge(&at(0.0, 1, 10), &at(0.5, 2, 20), 64));
        assert!(!can_merge(&at(0.0, 1, 40), &at(0.5, 1, 30), 64));
    }

    #[test]
    fn step_lands_and_rests() {
        let mut d = Dropped::new(
            [0.5, 2.0, 0.5],
            [0.0; 3],
            Stack { item: 1, count: 1 },
            Duration::ZERO,
        );
        let mut synced = 0;
        for _ in 0..40 {
            synced += step(&mut d, floor) as u32;
        }
        assert!(d.resting());
        assert_eq!(d.pos[1], 0.0);
        // Одна синхронизация — при приземлении.
        assert_eq!(synced, 1);
    }
}
