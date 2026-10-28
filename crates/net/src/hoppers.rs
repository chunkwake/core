//! Воронки, выбрасыватели и раздатчики 26.3: по одному предмету раз в 8 тиков
//! (`HopperBlockEntity`), грани печи (`WorldlyContainer`), выброс предмета
//! (`DefaultDispenseItemBehavior`).

use chunkwake_protocol::inventory::{Slot, Stack};

/// Пауза воронки после переноса (`MOVE_ITEM_SPEED`).
pub const COOLDOWN: u32 = 8;
/// Задержка срабатывания выбрасывателя и раздатчика (`TRIGGER_DURATION`).
pub const DISPENSE_DELAY: u64 = 4;
/// `LevelEvent`: звук выдачи, осечка, дым у отверстия.
pub const EVENT_DISPENSE: i32 = 1000;
pub const EVENT_FAIL: i32 = 1001;
pub const EVENT_SMOKE: i32 = 2000;

/// Слоты, куда предмет входит через грань `face` (0 низ, 1 верх, 2–5 бока).
/// Печь: сверху — сырьё, сбоку — топливо, снизу — ничего; прочие — все.
pub fn insert_slots(furnace: bool, size: usize, face: usize, fuel: bool) -> Vec<usize> {
    if !furnace {
        return (0..size).collect();
    }
    match face {
        1 => vec![0],
        0 => Vec::new(),
        _ if fuel => vec![1],
        _ => Vec::new(),
    }
}

/// Слоты, откуда воронка снизу забирает: у печи — результат, затем топливо.
/// Топливо отдаётся только если это пустое ведро; это проверяет вызывающий.
pub fn extract_slots(furnace: bool, size: usize) -> Vec<usize> {
    if furnace {
        vec![2, 1]
    } else {
        (0..size).collect()
    }
}

/// `HopperBlockEntity.addItem` для одного предмета: первый слот по порядку,
/// где пусто или та же стопка не полна.
pub fn insert_one(
    slots: &mut [Slot],
    allowed: &[usize],
    s: Stack,
    max: impl Fn(i32) -> i32,
) -> bool {
    for &i in allowed {
        match &mut slots[i] {
            None => {
                slots[i] = Some(Stack { count: 1, ..s });
                return true;
            }
            Some(t) if t.same_kind(&s) && t.count < max(s.item) => {
                t.count += 1;
                return true;
            }
            _ => {}
        }
    }
    false
}

/// Положить сколько влезет из `s` (подбор предмета с земли); остаток.
pub fn insert_all(slots: &mut [Slot], s: Stack, max: impl Fn(i32) -> i32) -> i32 {
    let mut left = s.count;
    let cap = max(s.item);
    for slot in slots.iter_mut() {
        if left == 0 {
            break;
        }
        match slot {
            None => {
                let n = left.min(cap);
                *slot = Some(Stack { count: n, ..s });
                left -= n;
            }
            Some(t) if t.same_kind(&s) && t.count < cap => {
                let n = left.min(cap - t.count);
                t.count += n;
                left -= n;
            }
            _ => {}
        }
    }
    left
}

/// Забрать один предмет из слота; `None` — пусто.
pub fn take_one(slots: &mut [Slot], i: usize) -> Option<Stack> {
    let s = slots[i]?;
    slots[i] = (s.count > 1).then_some(Stack {
        count: s.count - 1,
        ..s
    });
    Some(Stack { count: 1, ..s })
}

/// Попадает ли предмет на земле в зону всасывания воронки (`getSuckAabb`):
/// над воронкой до двух блоков вверх от её низа.
pub fn in_suck_area(hopper: [i32; 3], item: [f64; 3]) -> bool {
    let [x, y, z] = hopper.map(f64::from);
    item[0] >= x - 0.125
        && item[0] < x + 1.125
        && item[2] >= z - 0.125
        && item[2] < z + 1.125
        && item[1] >= y + 0.5
        && item[1] < y + 2.0
}

/// Где появляется выброшенный предмет и с какой скоростью
/// (`DispenseItemBehavior.spawnItem`): `r` — случайные числа [0, 1).
pub fn dispense_motion(pos: [i32; 3], d: [i32; 3], r: [f64; 4]) -> ([f64; 3], [f64; 3]) {
    let c = pos.map(|v| v as f64 + 0.5);
    let mut at = [
        c[0] + 0.7 * d[0] as f64,
        c[1] + 0.7 * d[1] as f64,
        c[2] + 0.7 * d[2] as f64,
    ];
    at[1] -= if d[1] != 0 { 0.125 } else { 0.15625 };
    let speed = r[0] * 0.1 + 0.2;
    // `triangle(mean, 0.0172275 × 6)`.
    let spread = 0.0172275 * 6.0;
    let tri = |x: f64| (x - 0.5) * 2.0 * spread;
    let vel = [
        d[0] as f64 * speed + tri(r[1]),
        0.2 + tri(r[2]),
        d[2] as f64 * speed + tri(r[3]),
    ];
    (at, vel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_take() {
        let s = Stack::new(5, 1);
        let mut slots = vec![Some(Stack::new(5, 64)), Some(Stack::new(7, 3)), None];
        assert!(insert_one(&mut slots, &[0, 1, 2], s, |_| 64));
        assert_eq!(slots[2], Some(Stack::new(5, 1)));
        assert!(insert_one(&mut slots, &[0, 1, 2], s, |_| 64));
        assert_eq!(slots[2].unwrap().count, 2);
        assert_eq!(take_one(&mut slots, 1).unwrap().count, 1);
        assert_eq!(slots[1].unwrap().count, 2);
        let mut five = vec![None; 5];
        assert_eq!(insert_all(&mut five, Stack::new(1, 200), |_| 64), 0);
        assert_eq!(five[3].unwrap().count, 8);
        assert_eq!(insert_slots(true, 3, 1, false), vec![0]);
        assert_eq!(insert_slots(true, 3, 2, true), vec![1]);
        assert!(insert_slots(true, 3, 2, false).is_empty());
        assert!(
            in_suck_area([0, 0, 0], [0.5, 1.0, 0.5]) && !in_suck_area([0, 0, 0], [0.5, 2.5, 0.5])
        );
        let (at, vel) = dispense_motion([0, 0, 0], [1, 0, 0], [0.0, 0.5, 0.5, 0.5]);
        assert!(
            (at[0] - 1.2).abs() < 1e-9
                && (vel[0] - 0.2).abs() < 1e-9
                && (vel[1] - 0.2).abs() < 1e-9
        );
    }
}
