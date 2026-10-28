//! Сферы опыта (`ExperienceOrb`): дробление награды, физика, притяжение к
//! ближайшему игроку и починка Починкой (`repairPlayerItems`).

/// Сфера на земле или в полёте.
#[derive(Clone, Copy, Debug)]
pub struct Orb {
    pub pos: [f64; 3],
    pub vel: [f64; 3],
    pub on_ground: bool,
    pub value: i32,
    /// Слитые сферы той же величины (`count`): подбор забирает по одной.
    pub count: u32,
    /// Тиков с появления; исчезает через 6000.
    pub age: u32,
}

pub const LIFETIME_TICKS: u32 = 6000;
/// Радиус притяжения к игроку (`ExperienceOrb.scanForEntities`).
pub const FOLLOW_RANGE: f64 = 8.0;
/// Задержка между подборами сфер одним игроком (`takeXpDelay`).
pub const TAKE_DELAY: u32 = 2;

/// `getExperienceValue`: величина очередной сферы из остатка награды.
pub fn split_value(total: i32) -> i32 {
    [2477, 1237, 617, 307, 149, 73, 37, 17, 7, 3]
        .into_iter()
        .find(|&v| total >= v)
        .unwrap_or(1)
}

/// `ExperienceOrb.award`: награда `amount` — сферы с их величинами.
pub fn award(mut amount: i32) -> Vec<i32> {
    let mut out = Vec::new();
    while amount > 0 {
        let v = split_value(amount);
        amount -= v;
        out.push(v);
    }
    out
}

/// Начальная скорость (`ExperienceOrb(level, x, y, z, value)`) из трёх случайных 0..1.
pub fn spawn_velocity(r: [f64; 3]) -> [f64; 3] {
    [
        (r[0] * 0.2 - 0.1) * 2.0,
        r[1] * 0.2 * 2.0,
        (r[2] * 0.2 - 0.1) * 2.0,
    ]
}

/// Один тик (`ExperienceOrb.tick`): гравитация, тяга к игроку (`target` — ноги),
/// движение со столкновениями, трение. `true` — нужна синхронизация с клиентом.
pub fn step(o: &mut Orb, target: Option<[f64; 3]>, solid: impl Fn([i32; 3]) -> bool) -> bool {
    let block = |p: [f64; 3]| p.map(|c| c.floor() as i32);
    let was = (o.on_ground, target.is_some());
    o.vel[1] -= 0.03;
    if let Some(t) = target {
        // К середине игрока (глаза / 2).
        let d = [
            (t[0] - o.pos[0]) / FOLLOW_RANGE,
            (t[1] + 0.81 - o.pos[1]) / FOLLOW_RANGE,
            (t[2] - o.pos[2]) / FOLLOW_RANGE,
        ];
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let e = 1.0 - len;
        if e > 0.0 && len > 1e-6 {
            let e = e * e;
            for (v, d) in o.vel.iter_mut().zip(d) {
                *v += d / len * e * 0.1;
            }
        }
    }
    for axis in [0, 2] {
        let mut next = o.pos;
        next[axis] += o.vel[axis];
        if solid(block(next)) {
            o.vel[axis] = 0.0;
        } else {
            o.pos = next;
        }
    }
    let mut next = o.pos;
    next[1] += o.vel[1];
    o.on_ground = false;
    if o.vel[1] < 0.0 && solid(block(next)) {
        o.pos[1] = next[1].floor() + 1.0;
        o.on_ground = true;
        // Подскок: `vel.y * -0.9` у ванилы.
        o.vel[1] *= -0.9;
        if o.vel[1].abs() < 0.05 {
            o.vel[1] = 0.0;
        }
    } else if o.vel[1] > 0.0 && solid(block(next)) {
        o.vel[1] = 0.0;
    } else {
        o.pos = next;
    }
    let f = if o.on_ground { 0.6 * 0.98 } else { 0.98 };
    o.vel = [o.vel[0] * f, o.vel[1] * 0.98, o.vel[2] * f];
    o.age += 1;
    // Ведомую сферу клиент сам не тянет: синхронизация каждые 2 тика.
    target.is_some() && o.age.is_multiple_of(2)
        || o.age.is_multiple_of(20)
        || was != (o.on_ground, target.is_some())
}

/// Пересекаются ли рамки сфер (0.5 блока) с запасом `slack` по каждой оси.
pub fn touching(a: [f64; 3], b: [f64; 3], slack: f64) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() <= 0.5 + slack)
}

/// `canMerge(orb, i, value)` при появлении: та же величина и `(id − i) % 40 == 0`.
pub fn can_absorb_spawn(id: i32, roll: i32, value: i32, orb: &Orb) -> bool {
    orb.value == value && (id - roll).rem_euclid(40) == 0
}

/// `repairPlayerItems`: Починка чинит 2 прочности за очко. `damaged` — (износ)
/// предметов с Починкой; выбор — `pick(n)`. Возвращает остаток опыта.
pub fn mend(mut value: i32, damaged: &mut [i32], mut pick: impl FnMut(usize) -> usize) -> i32 {
    loop {
        let open: Vec<usize> = (0..damaged.len()).filter(|&i| damaged[i] > 0).collect();
        if open.is_empty() || value <= 0 {
            return value;
        }
        let i = open[pick(open.len())];
        let k = mend_repair(value, damaged[i]);
        damaged[i] -= k;
        let left = mend_left(value, k);
        if k == 0 || left <= 0 {
            return left.max(0);
        }
        value = left;
    }
}

/// Сколько прочности чинят `value` очков при износе `damage` (`value × 2`, не больше износа).
pub fn mend_repair(value: i32, damage: i32) -> i32 {
    (value * 2).min(damage).max(0)
}

/// Остаток опыта после починки на `repair` (`value − repair × value / (value × 2)`).
pub fn mend_left(value: i32, repair: i32) -> i32 {
    if value <= 0 {
        return 0;
    }
    (value - repair * value / (value * 2)).max(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn award_and_mend() {
        assert_eq!(award(10), vec![7, 3]);
        assert_eq!(award(1), vec![1]);
        // 5 очков чинят до 10 прочности: при износе 4 — тратится 2, остаётся 3.
        let mut d = [4];
        assert_eq!(mend(5, &mut d, |_| 0), 3);
        assert_eq!(d, [0]);
        let mut d = [100];
        assert_eq!(mend(5, &mut d, |_| 0), 0);
        assert_eq!(d, [90]);
    }

    #[test]
    fn merge_rules() {
        let o = Orb {
            pos: [0.0; 3],
            vel: [0.0; 3],
            on_ground: true,
            value: 7,
            count: 1,
            age: 0,
        };
        assert!(can_absorb_spawn(43, 3, 7, &o));
        assert!(!can_absorb_spawn(44, 3, 7, &o));
        assert!(!can_absorb_spawn(43, 3, 3, &o));
        assert!(touching([0.0; 3], [0.9, 0.0, 0.0], 0.5));
        assert!(!touching([0.0; 3], [1.1, 0.0, 0.0], 0.5));
    }

    #[test]
    fn falls_and_rests() {
        let mut o = Orb {
            pos: [0.5, 5.0, 0.5],
            vel: [0.0; 3],
            on_ground: false,
            value: 1,
            count: 1,
            age: 0,
        };
        for _ in 0..200 {
            step(&mut o, None, |p| p[1] < 0);
        }
        assert!(o.on_ground && (o.pos[1] - 0.0).abs() < 1e-9);
    }
}
