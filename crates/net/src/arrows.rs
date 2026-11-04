//! Стрелы 26.3 (`AbstractArrow`): выстрел из раздатчика (`Projectile.shoot`),
//! полёт с сопротивлением и гравитацией, втыкание в блок, попадание в игрока.

/// Гравитация за тик (`getDefaultGravity`).
pub const GRAVITY: f64 = 0.05;
/// Сопротивление воздуха и воды (`getWaterInertia`).
pub const DRAG: f64 = 0.99;
pub const WATER_DRAG: f64 = 0.6;
/// Воткнутая стрела исчезает через минуту (`tickDespawn`).
pub const DESPAWN: u32 = 1200;
/// Базовый урон (`baseDamage`).
pub const BASE_DAMAGE: f64 = 2.0;
/// Сила и разброс выстрела раздатчика (`ProjectileDispenseBehavior`: 1.1, 6).
pub const DISPENSE_POWER: f64 = 1.1;
pub const DISPENSE_SPREAD: f64 = 6.0;
/// Полный натяг лука (`BowItem`: сила × 3, разброс 1).
pub const BOW_SPEED: f64 = 3.0;
pub const BOW_SPREAD: f64 = 1.0;
/// Арбалет: стрела 3.15, ракета 1.6.
pub const CROSSBOW_ARROW: f64 = 3.15;
pub const CROSSBOW_ROCKET: f64 = 1.6;
/// Ракета без взрыва живёт секунду.
pub const FIREWORK_FUSE: u16 = 20;
/// Тройной выстрел: центр и ±10°.
/// `FireworkRocketEntity(Level, x, y, z, ItemStack)`: `10 × (1 + полёт) +
/// nextInt(6) + nextInt(7)` тиков.
pub fn rocket_lifetime(flight: i32, r6: u32, r7: u32) -> u16 {
    (10 * (1 + flight.max(0)) + (r6 % 6) as i32 + (r7 % 7) as i32).clamp(0, u16::MAX as i32) as u16
}
/// Событие сущности `explode`: клиент показывает взрыв ракеты.
pub const EVENT_FIREWORK_EXPLODE: u8 = 17;
pub const MULTISHOT_YAW: [f64; 3] = [-10.0, 0.0, 10.0];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThrownKind { Snowball, Egg, Pearl, Potion }

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Arrow {
    pub pos: [f64; 3],
    pub vel: [f64; 3],
    pub stuck: bool,
    /// Тиков в блоке.
    pub age: u32,
    /// Снежок или яйцо (`ThrowableProjectile`): гравитация 0.03, без урона, бьётся о блок.
    pub thrown: bool,
    pub thrown_kind: Option<ThrownKind>,
    pub potion_extra: u32,
    /// Ракета с арбалета: без гравитации и урона, гаснет по таймеру.
    pub firework: bool,
    pub life: u16,
    /// Ракета на элитрах (`DATA_ATTACHED_TO_TARGET`): держится у игрока, а
    /// разгон считает клиент этого игрока.
    pub attached: Option<uuid::Uuid>,
    /// Срок ракеты в тиках (`FireworkRocketEntity.lifetime`); 0 — запал
    /// [`FIREWORK_FUSE`] прежних ракет арбалета и раздатчика.
    pub lifetime: u16,
    /// Взрывов в компоненте `fireworks`: без них ракета гаснет молча и не ранит.
    pub explosions: u8,
    pub owner: Option<uuid::Uuid>,
    /// Стрелок уже снаружи хитбокса — дальше по нему можно попасть.
    pub left_owner: bool,
    pub base_damage: f64,
    pub crit: bool,
    pub punch: i32,
    /// Сколько целей ещё пробить, не останавливаясь (`piercing`).
    pub pierce: i32,
    pierced: [u128; 4],
    pierced_n: u8,
    /// Попадание в сущность закончило полёт.
    pub stopped: bool,
    /// `PickupStatus.CREATIVE_ONLY`: стрела Бесконечности или креатива — не подбирается.
    pub creative_only: bool,
}

impl Arrow {
    pub fn flight(pos: [f64; 3], vel: [f64; 3], thrown: bool) -> Self {
        Self {
            pos,
            vel,
            stuck: false,
            age: 0,
            thrown,
            thrown_kind: None,
            potion_extra: 0,
            firework: false,
            life: 0,
            attached: None,
            lifetime: 0,
            explosions: 0,
            owner: None,
            left_owner: true,
            base_damage: if thrown { 0.0 } else { BASE_DAMAGE },
            crit: false,
            punch: 0,
            pierce: 0,
            pierced: [0; 4],
            pierced_n: 0,
            stopped: false,
            creative_only: false,
        }
    }
}

/// Гравитация и сопротивление воды брошенного (`ThrowableItemProjectile`).
pub const THROWN_GRAVITY: f64 = 0.03;
pub const THROWN_WATER_DRAG: f64 = 0.8;
/// `EntityEvent`: снежок или яйцо разбились — частицы предмета.
pub const EVENT_BREAK: u8 = 3;
/// `LevelEvent.DISPENSER_LAUNCH`: звук запуска снаряда раздатчиком.
pub const EVENT_LAUNCH: i32 = 1002;

/// Что случилось за тик. За один тик пронзающая стрела может задеть нескольких.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    /// Воткнулась в блок.
    Block,
    /// Ракета кончилась или брошенный снаряд разбился.
    Pop,
    /// Попала в игрока `i`. Скорость — на момент удара, для отдачи.
    Player(usize, f32, [f64; 3]),
    /// Попала в моба `i`.
    Mob(usize, f32, [f64; 3]),
}

/// `BowItem.getPowerForTime`: ниже 0.1 выстрела нет.
pub fn bow_power(ticks: u32) -> f64 {
    let t = ticks as f64 / 20.0;
    ((t * t + t * 2.0) / 3.0).min(1.0)
}

/// Тики зарядки арбалета: 25 − 5 × Быстрая перезарядка (0…3).
pub fn crossbow_charge(quick: i32) -> u32 {
    25u32.saturating_sub(5 * quick.clamp(0, 3) as u32)
}

/// Взгляд игрока с добавкой к рысканью (градусы). `x = −sin yaw · cos pitch`.
pub fn aim(yaw: f64, pitch: f64, yaw_offset: f64) -> [f64; 3] {
    let y = (yaw + yaw_offset).to_radians();
    let p = pitch.to_radians();
    [-y.sin() * p.cos(), -p.sin(), y.cos() * p.cos()]
}

/// `AbstractArrow`: ⌈|v| × урон⌉, критический добавляет `0 .. ⌈урон/2⌉+1`.
pub fn arrow_damage(speed: f64, base: f64, crit_roll: Option<u32>) -> f32 {
    let d = (speed * base).clamp(0.0, i32::MAX as f64).ceil() as i32;
    let extra = crit_roll.map_or(0, |roll| {
        let span = (d / 2 + 2).max(1) as u32;
        (roll % span) as i32
    });
    (d.max(0) + extra) as f32
}

fn len(v: [f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

/// `Projectile.shoot`: направление, разброс `triangle(0, 0.0172275 × spread)`, сила.
pub fn shoot(dir: [f64; 3], power: f64, spread: f64, r: [f64; 3]) -> [f64; 3] {
    let l = len(dir).max(1e-9);
    let s = 0.0172275 * spread;
    let mut v = [0.0; 3];
    for a in 0..3 {
        v[a] = (dir[a] / l + (r[a] - 0.5) * 2.0 * s) * power;
    }
    v
}

/// Поворот сущности по скорости (`yRot`, `xRot` в градусах) для спавна у клиента.
pub fn rotation(v: [f64; 3]) -> (f32, f32) {
    let h = (v[0] * v[0] + v[2] * v[2]).sqrt();
    let yaw = (v[0].atan2(v[2])).to_degrees() as f32;
    let pitch = (v[1].atan2(h)).to_degrees() as f32;
    (yaw, pitch)
}

/// Попадает ли точка в рамку игрока (0.6 × 1.8), расширенную на 0.3 (`inflate`).
fn touches(feet: [f64; 3], p: [f64; 3]) -> bool {
    (p[0] - feet[0]).abs() <= 0.6
        && (p[2] - feet[2]).abs() <= 0.6
        && p[1] >= feet[1] - 0.3
        && p[1] <= feet[1] + 2.1
}

fn remembers(a: &Arrow, id: uuid::Uuid) -> bool {
    let n = a.pierced_n as usize;
    a.pierced[..n].contains(&id.as_u128())
}

fn remember(a: &mut Arrow, id: uuid::Uuid) {
    if (a.pierced_n as usize) < a.pierced.len() {
        a.pierced[a.pierced_n as usize] = id.as_u128();
        a.pierced_n += 1;
    }
}

/// Тик летящей стрелы: путь пробегается мелкими шагами — первый твёрдый блок
/// или игрок останавливают её. `solid` — есть ли коллизия у клетки, `water` — в воде ли.
/// Стрелок пропускается, пока снаряд внутри его хитбокса.
pub fn step(
    a: &mut Arrow,
    players: &[(uuid::Uuid, [f64; 3])],
    mobs: &[(u128, [f64; 3], (f64, f64))],
    solid: impl Fn([i32; 3]) -> bool,
    water: impl Fn([i32; 3]) -> bool,
) -> Vec<Hit> {
    if a.stuck {
        a.age += 1;
        return Vec::new();
    }
    if a.firework && a.life >= FIREWORK_FUSE && a.lifetime == 0 {
        return vec![Hit::Pop];
    }
    // `FireworkRocketEntity.tick` с `attachedToEntity`: ракета стоит на месте
    // игрока, столкновений своих не ищет, а `life > lifetime` — взрыв.
    if let Some(owner) = a.attached {
        if let Some((_, feet)) = players.iter().find(|(id, _)| *id == owner) {
            a.pos = *feet;
        }
        a.life = a.life.saturating_add(1);
        return if a.life > a.lifetime { vec![Hit::Pop] } else { Vec::new() };
    }
    let speed = len(a.vel);
    let n = (speed / 0.1).ceil().max(1.0) as usize;
    let mut prev = a.pos;
    let mut hits = Vec::new();
    for i in 1..=n {
        let t = i as f64 / n as f64;
        let p = [
            a.pos[0] + a.vel[0] * t,
            a.pos[1] + a.vel[1] * t,
            a.pos[2] + a.vel[2] * t,
        ];
        if !a.firework
            && let Some(k) = players.iter().position(|(id, f)| {
                touches(*f, p) && !remembers(a, *id) && (a.left_owner || a.owner != Some(*id))
            })
        {
            let roll = a.crit.then(rand::random::<u32>);
            let dmg = if a.thrown {
                0.0
            } else {
                arrow_damage(speed, a.base_damage, roll)
            };
            a.pos = p;
            let id = players[k].0;
            hits.push(Hit::Player(k, dmg, a.vel));
            if a.pierce > 0 && (a.pierced_n as usize) < a.pierced.len() {
                remember(a, id);
                a.pierce -= 1;
                prev = p;
                continue;
            }
            a.stopped = true;
            return hits;
        }
        if !a.firework
            && let Some(k) = mobs.iter().position(|(id, feet, (width, height))| {
                (p[0] - feet[0]).abs() <= width / 2.0 + 0.3
                    && (p[2] - feet[2]).abs() <= width / 2.0 + 0.3
                    && p[1] >= feet[1] - 0.3
                    && p[1] <= feet[1] + height + 0.3
                    && !remembers(a, uuid::Uuid::from_u128(*id))
                    && (a.left_owner || a.owner.map(|owner| owner.as_u128()) != Some(*id))
            })
        {
            let roll = a.crit.then(rand::random::<u32>);
            let dmg = if a.thrown { 0.0 } else { arrow_damage(speed, a.base_damage, roll) };
            a.pos = p;
            hits.push(Hit::Mob(k, dmg, a.vel));
            if a.pierce > 0 && (a.pierced_n as usize) < a.pierced.len() {
                remember(a, uuid::Uuid::from_u128(mobs[k].0));
                a.pierce -= 1;
                prev = p;
                continue;
            }
            a.stopped = true;
            return hits;
        }
        if solid(p.map(|c| c.floor() as i32)) {
            a.pos = prev;
            if a.firework {
                hits.push(Hit::Pop);
            } else if a.thrown {
                hits.push(Hit::Block);
            } else {
                a.vel = [0.0; 3];
                a.stuck = true;
                hits.push(Hit::Block);
            }
            return hits;
        }
        if !a.left_owner
            && let Some(owner) = a.owner
            && players
                .iter()
                .find(|(id, _)| *id == owner)
                .is_none_or(|(_, f)| !touches(*f, p))
        {
            a.left_owner = true;
        }
        prev = p;
    }
    a.pos = prev;
    if a.firework {
        a.life = a.life.saturating_add(1);
        if a.life >= FIREWORK_FUSE {
            hits.push(Hit::Pop);
        }
        return hits;
    }
    let drag = if water(a.pos.map(|c| c.floor() as i32)) {
        if a.thrown {
            THROWN_WATER_DRAG
        } else {
            WATER_DRAG
        }
    } else {
        DRAG
    };
    a.vel = a.vel.map(|v| v * drag);
    a.vel[1] -= if a.thrown { THROWN_GRAVITY } else { GRAVITY };
    hits
}

/// Слот брони игрока (окно игрока: 5 шлем, 6 нагрудник, 7 поножи, 8 ботинки) для предмета.
pub fn armor_slot(item: &str) -> Option<usize> {
    let p = item.strip_prefix("minecraft:").unwrap_or(item);
    if p.ends_with("_helmet")
        || p == "carved_pumpkin"
        || p.ends_with("_head")
        || p.ends_with("_skull")
    {
        Some(5)
    } else if p.ends_with("_chestplate") || p == "elytra" {
        Some(6)
    } else if p.ends_with("_leggings") {
        Some(7)
    } else if p.ends_with("_boots") {
        Some(8)
    } else {
        None
    }
}

/// Стоит ли игрок (ноги `feet`) в клетке `cell` (`AABB(blockPos)` пересекает рамку игрока).
pub fn in_cell(feet: [f64; 3], cell: [i32; 3]) -> bool {
    let c = cell.map(f64::from);
    feet[0] + 0.3 > c[0]
        && feet[0] - 0.3 < c[0] + 1.0
        && feet[2] + 0.3 > c[2]
        && feet[2] - 0.3 < c[2] + 1.0
        && feet[1] + 1.8 > c[1]
        && feet[1] < c[1] + 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flies_sticks_and_hits() {
        let v = shoot([1.0, 0.1, 0.0], DISPENSE_POWER, DISPENSE_SPREAD, [0.5; 3]);
        assert!((len(v) - 1.1).abs() < 1e-9);
        let mut a = Arrow::flight([0.5, 10.5, 0.5], v, false);
        // Стена на x = 5.
        let mut hit = Vec::new();
        for _ in 0..20 {
            hit = step(&mut a, &[], &[], |p| p[0] >= 5, |_| false);
            if !hit.is_empty() {
                break;
            }
        }
        assert_eq!(hit, vec![Hit::Block]);
        assert!(a.stuck && a.pos[0] < 5.0 && a.pos[0] > 4.8);
        let mut b = Arrow::flight([0.5, 1.5, 0.5], [1.1, 0.0, 0.0], false);
        assert_eq!(
            step(
                &mut b,
                &[(uuid::Uuid::nil(), [1.2, 1.0, 0.5])],
                &[],
                |_| false,
                |_| false
            ),
            vec![Hit::Player(0, 3.0, [1.1, 0.0, 0.0])]
        );
        assert_eq!(armor_slot("minecraft:iron_boots"), Some(8));
        assert!(in_cell([2.5, 64.0, 0.5], [2, 64, 0]) && !in_cell([3.9, 64.0, 0.5], [2, 64, 0]));
    }

    #[test]
    fn bow_and_crossbow_shots() {
        assert!(bow_power(0) < 0.1);
        assert!((bow_power(20) - 1.0).abs() < 1e-9);
        let mid = bow_power(10);
        assert!(mid > 0.1 && mid < 1.0);
        assert_eq!(crossbow_charge(0), 25);
        assert_eq!(crossbow_charge(1), 20);
        assert_eq!(crossbow_charge(3), 10);
        assert_eq!(crossbow_charge(9), 10);
        let full = shoot(aim(0.0, 0.0, 0.0), BOW_SPEED, BOW_SPREAD, [0.5; 3]);
        assert!((len(full) - BOW_SPEED).abs() < 1e-9);
        let bolt = shoot([0.0, 0.0, 1.0], CROSSBOW_ARROW, 0.0, [0.5; 3]);
        assert!((len(bolt) - CROSSBOW_ARROW).abs() < 1e-9);
        let rocket = shoot([0.0, 0.0, 1.0], CROSSBOW_ROCKET, 0.0, [0.5; 3]);
        assert!((len(rocket) - CROSSBOW_ROCKET).abs() < 1e-9);
        assert!((arrow_damage(3.0, 2.0, None) - 6.0).abs() < 1e-6);
        // Сила I: 2 + 0.5 + 0.5.
        assert!((arrow_damage(3.0, 3.0, None) - 9.0).abs() < 1e-6);
        let side = aim(0.0, 0.0, MULTISHOT_YAW[0]);
        assert!(side[0].abs() > 0.1);

        let shooter = uuid::Uuid::from_u128(7);
        let mut own = Arrow::flight([0.5, 64.5, 0.5], [0.0, 0.0, 0.0], false);
        own.owner = Some(shooter);
        own.left_owner = false;
        let inside = step(
            &mut own,
            &[(shooter, [0.5, 64.0, 0.5])],
            &[],
            |_| false,
            |_| false,
        );
        assert!(inside.is_empty(), "{inside:?}");

        let mut pierce = Arrow::flight([0.5, 1.5, 0.5], [1.1, 0.0, 0.0], false);
        pierce.pierce = 1;
        let hits = step(
            &mut pierce,
            &[(uuid::Uuid::from_u128(1), [1.2, 1.0, 0.5])],
            &[],
            |_| false,
            |_| false,
        );
        assert_eq!(hits.len(), 1);
        assert!(!pierce.stopped && !pierce.stuck && pierce.pierce == 0);

        let mut snowball = Arrow::flight([0.5, 1.5, 0.5], [1.1, 0.0, 0.0], true);
        let chicken = (9, [1.2, 1.0, 0.5], (0.4, 0.7));
        assert_eq!(step(&mut snowball, &[], &[chicken], |_| false, |_| false),
            vec![Hit::Mob(0, 0.0, [1.1, 0.0, 0.0])]);
    }

    #[test]
    fn rocket_lifetime_matches_vanilla_range() {
        // Полёт 1: 20..=31 тик, полёт 3: 40..=51.
        assert_eq!(rocket_lifetime(1, 0, 0), 20);
        assert_eq!(rocket_lifetime(1, 5, 6), 31);
        assert_eq!(rocket_lifetime(3, 11, 13), 40 + 5 + 6);
    }

    #[test]
    fn attached_rocket_follows_its_player_and_pops_after_lifetime() {
        let owner = uuid::Uuid::from_u128(7);
        let mut r = Arrow::flight([0.0, 64.0, 0.0], [0.0; 3], false);
        r.firework = true;
        r.attached = Some(owner);
        r.lifetime = 2;
        // Твёрдый блок вокруг не мешает: прицепленная ракета столкновений не ищет.
        for x in [5.0, 6.0] {
            assert!(step(&mut r, &[(owner, [x, 80.0, 1.0])], &[], |_| true, |_| false).is_empty());
            assert_eq!(r.pos, [x, 80.0, 1.0]);
        }
        assert_eq!(step(&mut r, &[(owner, [7.0, 80.0, 1.0])], &[], |_| true, |_| false), vec![Hit::Pop]);
    }
}
