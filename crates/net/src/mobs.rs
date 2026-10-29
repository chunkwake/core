//! Мирные мобы 26.3 (`Cow`, `Pig`, `Sheep`, `Chicken`): физика `LivingEntity.travel`,
//! коллизия с полными блоками, цели ИИ (`PanicGoal`, `TemptGoal`, `RandomStrollGoal`,
//! `LookAtPlayerGoal`), урон с отбрасыванием и смерть.

/// Гравитация и сопротивление по вертикали (`LivingEntity.travel`).
pub const GRAVITY: f64 = 0.08;
pub const AIR_DRAG: f64 = 0.98;
/// Трение по горизонтали: блок 0.6 × 0.91 на земле, 0.91 в воздухе.
pub const GROUND_FRICTION: f64 = 0.6 * 0.91;
pub const AIR_FRICTION: f64 = 0.91;
/// Ускорение в воздухе (`getFlyingSpeed`).
pub const AIR_ACCEL: f64 = 0.02;
/// Прыжок (`getJumpPower`).
pub const JUMP: f64 = 0.42;
/// Неуязвимость после удара (`invulnerableTime`); урон проходит при > 10, только больший.
pub const INVULNERABLE: u8 = 20;
/// Анимация смерти до исчезновения (`tickDeath`).
pub const DEATH_TICKS: u16 = 20;
/// Паника после удара (`lastHurtByMob` живёт 100 тиков).
pub const PANIC_TICKS: u32 = 100;
/// Отбрасывание от удара игрока (`knockback`).
pub const KNOCKBACK: f64 = 0.4;
/// Детёныш растёт 20 минут (`AgeableMob.BABY_START_AGE`).
pub const BABY_AGE: i32 = -24000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Species {
    Cow,
    Pig,
    Sheep,
    Chicken,
}

impl Species {
    pub const ALL: [Species; 4] = [Species::Cow, Species::Pig, Species::Sheep, Species::Chicken];

    pub fn name(self) -> &'static str {
        match self {
            Species::Cow => "minecraft:cow",
            Species::Pig => "minecraft:pig",
            Species::Sheep => "minecraft:sheep",
            Species::Chicken => "minecraft:chicken",
        }
    }

    /// Имя без пространства (`cow`) — для плагинов.
    pub fn short(self) -> &'static str {
        &self.name()[10..]
    }

    pub fn from_name(name: &str) -> Option<Self> {
        let name = name.strip_prefix("minecraft:").unwrap_or(name);
        Self::ALL.into_iter().find(|s| &s.name()[10..] == name)
    }

    /// Ширина и высота взрослого (`EntityType.sized`).
    pub fn size(self) -> (f64, f64) {
        match self {
            Species::Cow => (0.9, 1.4),
            Species::Pig => (0.9, 0.9),
            Species::Sheep => (0.9, 1.3),
            Species::Chicken => (0.4, 0.7),
        }
    }

    pub fn max_health(self) -> f32 {
        match self {
            Species::Cow | Species::Pig => 10.0,
            Species::Sheep => 8.0,
            Species::Chicken => 4.0,
        }
    }

    /// Атрибут `movement_speed` (`createAttributes`).
    pub fn speed(self) -> f64 {
        match self {
            Species::Cow => 0.2,
            Species::Pig | Species::Chicken => 0.25,
            Species::Sheep => 0.23,
        }
    }

    /// Множитель скорости паники (`PanicGoal`).
    pub fn panic_speed(self) -> f64 {
        match self {
            Species::Cow => 2.0,
            Species::Pig | Species::Sheep => 1.25,
            Species::Chicken => 1.4,
        }
    }

    /// Множитель скорости, когда манят едой (`TemptGoal`).
    pub fn tempt_speed(self) -> f64 {
        match self {
            Species::Cow => 1.25,
            Species::Pig => 1.2,
            Species::Sheep | Species::Chicken => 1.0,
        }
    }

    /// Еда (`isFood`: теги `cow_food`, `pig_food`, `sheep_food`, `chicken_food`).
    pub fn is_food(self, item: &str) -> bool {
        let item = item.strip_prefix("minecraft:").unwrap_or(item);
        match self {
            Species::Cow | Species::Sheep => item == "wheat",
            Species::Pig => matches!(item, "carrot" | "potato" | "beetroot"),
            Species::Chicken => matches!(
                item,
                "wheat_seeds"
                    | "melon_seeds"
                    | "pumpkin_seeds"
                    | "beetroot_seeds"
                    | "torchflower_seeds"
                    | "pitcher_pod"
            ),
        }
    }

    /// Префикс звуков: `minecraft:entity.cow` → `.ambient`, `.hurt`, `.death`, `.step`.
    pub fn sound(self) -> &'static str {
        match self {
            Species::Cow => "minecraft:entity.cow",
            Species::Pig => "minecraft:entity.pig",
            Species::Sheep => "minecraft:entity.sheep",
            Species::Chicken => "minecraft:entity.chicken",
        }
    }

    /// Громкость звуков (`Cow.getSoundVolume` — 0.4).
    pub fn volume(self) -> f32 {
        if self == Species::Cow { 0.4 } else { 1.0 }
    }

    /// Таблица лута (`entities/<вид>`; у овцы шерсть — подтаблица по цвету).
    pub fn loot(self) -> &'static str {
        match self {
            Species::Cow => "minecraft:entities/cow",
            Species::Pig => "minecraft:entities/pig",
            Species::Sheep => "minecraft:entities/sheep",
            Species::Chicken => "minecraft:entities/chicken",
        }
    }
}

/// Что сейчас делает моб (одна активная цель движения).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Goal {
    Idle,
    /// Идти к точке со множителем скорости, не дольше `ticks`.
    Walk {
        to: [f64; 3],
        speed: f64,
        ticks: u32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Mob {
    pub species: Species,
    pub uuid: u128,
    pub pos: [f64; 3],
    pub vel: [f64; 3],
    pub yaw: f32,
    pub head_yaw: f32,
    pub pitch: f32,
    pub health: f32,
    pub on_ground: bool,
    /// Остаток неуязвимости и последний прошедший урон.
    pub invulnerable: u8,
    pub last_damage: f32,
    /// > 0 — умирает (тики анимации).
    pub death_time: u16,
    /// Возраст: < 0 — детёныш; > 0 — пауза после размножения.
    pub age: i32,
    /// Режим любви (`inLove`), тики.
    pub love: i32,
    pub sheared: bool,
    /// Цвет шерсти (`DyeColor` id).
    pub color: u8,
    pub goal: Goal,
    /// Паника: тики и от кого бежать.
    pub panic: u32,
    pub panic_from: [f64; 3],
    /// Смотрит на точку ещё столько тиков.
    pub look: Option<([f64; 3], u32)>,
    /// Курица: тиков до яйца.
    pub egg_time: i32,
    /// Овца ест траву: тики анимации.
    pub eating: u8,
    /// Пользовательское имя (JSON) от плагинов.
    pub custom_name: Option<String>,
}

impl Mob {
    pub fn new(species: Species, pos: [f64; 3], uuid: u128) -> Self {
        Self {
            species,
            uuid,
            pos,
            vel: [0.0; 3],
            yaw: 0.0,
            head_yaw: 0.0,
            pitch: 0.0,
            health: species.max_health(),
            on_ground: false,
            invulnerable: 0,
            last_damage: 0.0,
            death_time: 0,
            age: 0,
            love: 0,
            sheared: false,
            color: 0,
            goal: Goal::Idle,
            panic: 0,
            panic_from: pos,
            look: None,
            egg_time: 6000,
            eating: 0,
            custom_name: None,
        }
    }

    pub fn is_baby(&self) -> bool {
        self.age < 0
    }

    pub fn alive(&self) -> bool {
        self.death_time == 0 && self.health > 0.0
    }

    /// Ширина и высота с учётом детёныша (`getScale` 0.5).
    pub fn size(&self) -> (f64, f64) {
        let (w, h) = self.species.size();
        if self.is_baby() {
            (w * 0.5, h * 0.5)
        } else {
            (w, h)
        }
    }

    /// Попадает ли луч в рамку моба (для подбора цели по ближайшему).
    pub fn contains(&self, p: [f64; 3]) -> bool {
        let (w, h) = self.size();
        (p[0] - self.pos[0]).abs() <= w / 2.0
            && (p[2] - self.pos[2]).abs() <= w / 2.0
            && p[1] >= self.pos[1]
            && p[1] <= self.pos[1] + h
    }
}

/// Мир для моба: твёрдый ли блок и вода ли в нём.
pub struct World<'a> {
    pub solid: &'a dyn Fn([i32; 3]) -> bool,
    pub water: &'a dyn Fn([i32; 3]) -> bool,
}

/// Окружение для ИИ: игроки рядом (`ноги`, держит ли еду этого вида) и случайные числа.
pub struct Senses<'a> {
    pub players: &'a [([f64; 3], bool)],
    pub rand: &'a mut dyn FnMut() -> f64,
}

const EPS: f64 = 1e-7;

/// Сколько можно пройти по оси `axis` на `d`, не входя в твёрдые блоки
/// (`Shapes.collide` для полных кубов).
fn collide(pos: [f64; 3], half: f64, h: f64, axis: usize, d: f64, w: &World) -> f64 {
    if d == 0.0 {
        return 0.0;
    }
    let lo = [pos[0] - half, pos[1], pos[2] - half];
    let hi = [pos[0] + half, pos[1] + h, pos[2] + half];
    let (mut a, mut b) = (lo, hi);
    if d > 0.0 {
        b[axis] += d;
    } else {
        a[axis] += d;
    }
    let mut allowed = d;
    let r = |v: f64| v.floor() as i32;
    let up = |v: f64| (v - EPS).floor() as i32;
    for x in r(a[0] + EPS)..=up(b[0]) {
        for y in r(a[1] + EPS)..=up(b[1]) {
            for z in r(a[2] + EPS)..=up(b[2]) {
                if !(w.solid)([x, y, z]) {
                    continue;
                }
                let c = [x as f64, y as f64, z as f64][axis];
                if d > 0.0 {
                    let gap = c - hi[axis];
                    if gap >= -EPS {
                        allowed = allowed.min(gap.max(0.0));
                    }
                } else {
                    let gap = c + 1.0 - lo[axis];
                    if gap <= EPS {
                        allowed = allowed.max(gap.min(0.0));
                    }
                }
            }
        }
    }
    allowed
}

/// `LivingEntity.getFrictionInfluencedSpeed`: на обычном блоке (`friction` 0.6,
/// `friction_modifier` 1.0 → `f = 0.6`) это `speed * 0.216 / f³`, то есть `speed`.
/// На скользком блоке (лёд, `f` больше 0.6) скорость выше.
fn friction_speed(speed: f64, friction: f64) -> f64 {
    if friction > 0.6 {
        speed * 0.21600002 / (friction * friction * friction)
    } else {
        speed
    }
}

fn in_water(m: &Mob, w: &World) -> bool {
    (w.water)([
        m.pos[0].floor() as i32,
        (m.pos[1] + 0.1).floor() as i32,
        m.pos[2].floor() as i32,
    ])
}

/// Один тик движения (`LivingEntity.travel` + `Entity.move`): `forward` — ускорение вдоль `yaw`.
/// Возвращает, упёрся ли моб по горизонтали.
pub fn travel(m: &mut Mob, forward: f64, w: &World) -> bool {
    let water = in_water(m, w);
    let accel = if water {
        // `travelInWater`: `moveRelative(0.02, input)`, `input.zza` — скорость моба.
        0.02 * forward
    } else if m.on_ground {
        // `LivingEntity.travelInAir`: `moveRelative(getFrictionInfluencedSpeed(f), input)`,
        // где `input.zza` уже равен скорости моба (`Mob.setSpeed` вызывает `setZza(f)`).
        // Поэтому на земле ускорение — скорость в квадрате: корова с `movement_speed`
        // 0.2 разгоняется на 0.04 за тик, а не на 0.2.
        let friction = friction_speed(m.species.speed(), GROUND_FRICTION);
        friction * forward
    } else {
        // В воздухе `getFrictionInfluencedSpeed` — это `getFlyingSpeed()` (0.02),
        // и он тоже умножается на `zza`.
        AIR_ACCEL * forward
    };
    if forward != 0.0 {
        let yaw = (m.yaw as f64).to_radians();
        m.vel[0] += -yaw.sin() * accel;
        m.vel[2] += yaw.cos() * accel;
    }
    let (wd, h) = m.size();
    let half = wd / 2.0;
    let want = m.vel;
    let dy = collide(m.pos, half, h, 1, want[1], w);
    m.pos[1] += dy;
    let dx = collide(m.pos, half, h, 0, want[0], w);
    m.pos[0] += dx;
    let dz = collide(m.pos, half, h, 2, want[2], w);
    m.pos[2] += dz;
    m.on_ground = want[1] < 0.0 && dy > want[1];
    let blocked = (dx - want[0]).abs() > EPS || (dz - want[2]).abs() > EPS;
    if (dy - want[1]).abs() > EPS {
        m.vel[1] = 0.0;
    }
    if (dx - want[0]).abs() > EPS {
        m.vel[0] = 0.0;
    }
    if (dz - want[2]).abs() > EPS {
        m.vel[2] = 0.0;
    }
    if water {
        m.vel = [m.vel[0] * 0.8, m.vel[1] * 0.8 - 0.02, m.vel[2] * 0.8];
        // `FloatGoal`: всплывать.
        if (w.water)([
            m.pos[0].floor() as i32,
            (m.pos[1] + h * 0.6).floor() as i32,
            m.pos[2].floor() as i32,
        ]) {
            m.vel[1] += 0.04;
        }
    } else {
        let f = if m.on_ground {
            GROUND_FRICTION
        } else {
            AIR_FRICTION
        };
        m.vel[1] = (m.vel[1] - GRAVITY) * AIR_DRAG;
        // Курица планирует (`Chicken.aiStep`).
        if m.species == Species::Chicken && !m.on_ground && m.vel[1] < 0.0 {
            m.vel[1] *= 0.6;
        }
        m.vel[0] *= f;
        m.vel[2] *= f;
    }
    blocked
}

fn wrap(a: f32) -> f32 {
    let mut a = a % 360.0;
    if a >= 180.0 {
        a -= 360.0;
    }
    if a < -180.0 {
        a += 360.0;
    }
    a
}

/// Поворот к цели не больше `max` градусов (`MoveControl.rotlerp`).
fn turn(from: f32, to: f32, max: f32) -> f32 {
    from + wrap(to - from).clamp(-max, max)
}

fn yaw_to(from: [f64; 3], to: [f64; 3]) -> f32 {
    ((to[2] - from[2]).atan2(to[0] - from[0]).to_degrees() as f32) - 90.0
}

pub fn dist2(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// Выбор цели (`GoalSelector`): паника > еда в руке > прогулка; взгляд на игрока.
pub fn think(m: &mut Mob, s: &mut Senses) {
    if !m.alive() {
        return;
    }
    let speed = m.species.speed();
    if m.panic > 0 {
        m.panic -= 1;
        let fleeing =
            matches!(m.goal, Goal::Walk { speed: v, ticks, .. } if v > speed * 1.01 && ticks > 0);
        if !fleeing {
            // `DefaultRandomPos.getPosAway`: точка до 5 блоков прочь от обидчика.
            let away = [m.pos[0] - m.panic_from[0], m.pos[2] - m.panic_from[2]];
            let len = (away[0] * away[0] + away[1] * away[1]).sqrt().max(1e-3);
            let spread = ((s.rand)() - 0.5) * 3.0;
            let to = [
                m.pos[0] + away[0] / len * 5.0 + spread,
                m.pos[1],
                m.pos[2] + away[1] / len * 5.0 - spread,
            ];
            m.goal = Goal::Walk {
                to,
                speed: speed * m.species.panic_speed(),
                ticks: 60,
            };
        }
        return;
    }
    // `TemptGoal`: ближайший игрок с едой в 10 блоках.
    let tempt = s
        .players
        .iter()
        .filter(|(p, food)| *food && dist2(*p, m.pos) < 100.0)
        .min_by(|a, b| dist2(a.0, m.pos).total_cmp(&dist2(b.0, m.pos)))
        .map(|p| p.0);
    if let Some(p) = tempt {
        m.look = Some(([p[0], p[1] + 1.62, p[2]], 2));
        m.goal = if dist2(p, m.pos) < 6.25 {
            Goal::Idle
        } else {
            Goal::Walk {
                to: p,
                speed: speed * m.species.tempt_speed(),
                ticks: 20,
            }
        };
        return;
    }
    if m.goal == Goal::Idle && (s.rand)() < 1.0 / 120.0 {
        // `RandomStrollGoal`: случайная точка до 10 блоков.
        let to = [
            m.pos[0] + ((s.rand)() * 2.0 - 1.0) * 10.0,
            m.pos[1],
            m.pos[2] + ((s.rand)() * 2.0 - 1.0) * 10.0,
        ];
        m.goal = Goal::Walk {
            to,
            speed,
            ticks: 120,
        };
    }
    // `LookAtPlayerGoal`: 2% за тик, в 6 блоках, на 40 тиков.
    if m.look.is_none()
        && (s.rand)() < 0.02
        && let Some(p) = s
            .players
            .iter()
            .map(|p| p.0)
            .find(|p| dist2(*p, m.pos) < 36.0)
    {
        m.look = Some(([p[0], p[1] + 1.62, p[2]], 40 + ((s.rand)() * 40.0) as u32));
    }
}

/// Тик моба: ИИ, движение, повороты, таймеры. Возвращает `true`, когда моб исчез.
pub fn tick(m: &mut Mob, w: &World, s: &mut Senses) -> bool {
    if m.death_time > 0 {
        m.death_time += 1;
        let _ = travel(m, 0.0, w);
        return m.death_time > DEATH_TICKS;
    }
    m.invulnerable = m.invulnerable.saturating_sub(1);
    if m.age < 0 {
        m.age += 1;
    } else if m.age > 0 {
        m.age -= 1;
    }
    m.love = (m.love - 1).max(0);
    think(m, s);
    let mut forward = 0.0;
    if let Goal::Walk { to, speed, ticks } = m.goal {
        let dx = to[0] - m.pos[0];
        let dz = to[2] - m.pos[2];
        if ticks == 0 || dx * dx + dz * dz < 0.25 {
            m.goal = Goal::Idle;
        } else {
            m.goal = Goal::Walk {
                to,
                speed,
                ticks: ticks - 1,
            };
            m.yaw = turn(m.yaw, yaw_to(m.pos, to), 90.0);
            forward = speed;
        }
    }
    let blocked = travel(m, forward, w);
    // `JumpControl`: упёрся на земле — прыжок; в воде всплываем сами.
    if blocked && forward > 0.0 && m.on_ground {
        m.vel[1] = JUMP;
    }
    // Голова: на цель взгляда, иначе по телу.
    let target_head = match m.look {
        Some((p, t)) => {
            m.look = (t > 1).then_some((p, t - 1));
            let eye = [m.pos[0], m.pos[1] + m.size().1 * 0.85, m.pos[2]];
            let horiz = ((p[0] - eye[0]).powi(2) + (p[2] - eye[2]).powi(2)).sqrt();
            m.pitch = (-(p[1] - eye[1]).atan2(horiz).to_degrees() as f32).clamp(-40.0, 40.0);
            yaw_to(m.pos, p)
        }
        None => {
            m.pitch = 0.0;
            m.yaw
        }
    };
    m.head_yaw = turn(m.head_yaw, target_head, 10.0);
    // Голова не уходит от тела дальше 75° (`BodyRotationControl`).
    m.head_yaw = m.yaw + wrap(m.head_yaw - m.yaw).clamp(-75.0, 75.0);
    false
}

/// Удар по мобу (`LivingEntity.hurt`): неуязвимость, отбрасывание от `from`, паника.
/// `None` — урон не прошёл; `Some(true)` — моб умер.
pub fn hurt(m: &mut Mob, amount: f32, from: Option<[f64; 3]>, knockback: f64) -> Option<bool> {
    if !m.alive() {
        return None;
    }
    let dealt = if m.invulnerable > INVULNERABLE / 2 {
        if amount <= m.last_damage {
            return None;
        }
        amount - m.last_damage
    } else {
        m.invulnerable = INVULNERABLE;
        amount
    };
    m.last_damage = amount;
    m.health = (m.health - dealt).max(0.0);
    if let Some(src) = from {
        let d = [src[0] - m.pos[0], src[2] - m.pos[2]];
        let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
        if len > 1e-4 && knockback > 0.0 {
            m.vel[0] = m.vel[0] / 2.0 - d[0] / len * knockback;
            m.vel[2] = m.vel[2] / 2.0 - d[1] / len * knockback;
            if m.on_ground {
                m.vel[1] = (m.vel[1] / 2.0 + knockback).min(0.4);
            }
        }
        m.panic = PANIC_TICKS;
        m.panic_from = src;
        m.goal = Goal::Idle;
    }
    if m.health <= 0.0 {
        m.death_time = 1;
        return Some(true);
    }
    Some(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn floor(p: [i32; 3]) -> bool {
        p[1] < 64
    }

    fn no_water(_: [i32; 3]) -> bool {
        false
    }

    fn settle(m: &mut Mob, w: &World) {
        for _ in 0..60 {
            travel(m, 0.0, w);
        }
    }

    /// Скорость ходьбы закреплена на ванильном значении.
    ///
    /// Сверено с байткодом 26.3. `travelInAir` передаёт в `moveRelative`
    /// `getFrictionInfluencedSpeed(f)`, а `input.zza` у моба равен его скорости
    /// (`Mob.setSpeed` вызывает `setZza(f)`). На обычном блоке
    /// `f = computeModifiedFriction(0.6, friction_modifier=1.0) = 0.6`, поэтому
    /// `getFrictionInfluencedSpeed = getSpeed() * 0.216 / f³ = getSpeed()`.
    /// Итоговое ускорение за тик — `speed²`, установившаяся скорость —
    /// `speed² / (1 - 0.6 * 0.91)`: для коровы (0.2) это 0.0881 блока за тик.
    #[test]
    fn walk_speed_matches_vanilla() {
        let w = World {
            solid: &floor,
            water: &no_water,
        };
        let mut m = Mob::new(Species::Cow, [0.5, 64.0, 0.5], 1);
        m.on_ground = true;
        m.goal = Goal::Walk {
            to: [1000.0, 64.0, 0.5],
            speed: Species::Cow.speed(),
            ticks: 10_000,
        };
        let mut rnd = || 0.5;
        let mut s = Senses {
            players: &[],
            rand: &mut rnd,
        };
        // Первые тики — разгон от нуля; установившаяся скорость дальше.
        for _ in 0..150 {
            tick(&mut m, &w, &mut s);
        }
        let start = m.pos;
        let ticks = 100u32;
        for _ in 0..ticks {
            tick(&mut m, &w, &mut s);
        }
        let per_tick = (m.pos[0] - start[0]) / ticks as f64;
        let speed = Species::Cow.speed();
        let expected = friction_speed(speed, 0.6) * speed / (1.0 - GROUND_FRICTION);
        assert!(
            (per_tick - expected).abs() < 1e-4,
            "cow walks {per_tick:.5} blocks/tick, vanilla gives {expected:.5}"
        );
        // И это заметно медленнее прежней ошибочной формулы (speed/(1-slip)).
        assert!(per_tick < 0.09, "cow still too fast: {per_tick:.5}");
    }

    #[test]
    fn falls_and_lands() {
        let w = World {
            solid: &floor,
            water: &no_water,
        };
        let mut m = Mob::new(Species::Cow, [0.5, 70.0, 0.5], 1);
        settle(&mut m, &w);
        assert!((m.pos[1] - 64.0).abs() < 1e-6, "{:?}", m.pos);
        assert!(m.on_ground);
    }

    #[test]
    fn chicken_glides() {
        let w = World {
            solid: &floor,
            water: &no_water,
        };
        let mut c = Mob::new(Species::Chicken, [0.5, 80.0, 0.5], 1);
        let mut p = Mob::new(Species::Pig, [0.5, 80.0, 0.5], 2);
        for _ in 0..10 {
            travel(&mut c, 0.0, &w);
            travel(&mut p, 0.0, &w);
        }
        assert!(c.pos[1] > p.pos[1] + 1.0, "{} {}", c.pos[1], p.pos[1]);
    }

    #[test]
    fn wall_stops_and_jumps() {
        // Стена из одного блока на x = 3: моб упирается, прыгает и забирается.
        let wall = |p: [i32; 3]| p[1] < 64 || (p[0] == 3 && p[1] == 64);
        let w = World {
            solid: &wall,
            water: &no_water,
        };
        let mut m = Mob::new(Species::Pig, [0.5, 64.0, 0.5], 1);
        m.on_ground = true;
        m.goal = Goal::Walk {
            to: [10.5, 64.0, 0.5],
            speed: 0.25,
            ticks: 400,
        };
        let mut rnd = || 0.9;
        let mut s = Senses {
            players: &[],
            rand: &mut rnd,
        };
        let mut max_x: f64 = 0.0;
        for _ in 0..200 {
            tick(&mut m, &w, &mut s);
            max_x = max_x.max(m.pos[0]);
            // Внутри стены не бывает.
            assert!(
                !(m.pos[0] + 0.45 > 3.0 + EPS && m.pos[0] - 0.45 < 4.0 && m.pos[1] < 65.0 - EPS),
                "{:?}",
                m.pos
            );
        }
        assert!(max_x > 4.0, "{max_x}");
    }

    #[test]
    fn panics_away_from_attacker() {
        let w = World {
            solid: &floor,
            water: &no_water,
        };
        let mut m = Mob::new(Species::Cow, [0.5, 64.0, 0.5], 1);
        m.on_ground = true;
        assert_eq!(
            hurt(&mut m, 1.0, Some([-2.0, 64.0, 0.5]), KNOCKBACK),
            Some(false)
        );
        // Повторный такой же удар в неуязвимости не проходит.
        assert_eq!(hurt(&mut m, 1.0, Some([-2.0, 64.0, 0.5]), KNOCKBACK), None);
        let mut rnd = || 0.5;
        let mut s = Senses {
            players: &[],
            rand: &mut rnd,
        };
        for _ in 0..40 {
            tick(&mut m, &w, &mut s);
        }
        assert!(m.pos[0] > 3.0, "{:?}", m.pos);
        assert_eq!(m.health, 9.0);
    }

    #[test]
    fn dies_and_despawns() {
        let w = World {
            solid: &floor,
            water: &no_water,
        };
        let mut m = Mob::new(Species::Chicken, [0.5, 64.0, 0.5], 1);
        assert_eq!(hurt(&mut m, 10.0, None, 0.0), Some(true));
        let mut rnd = || 0.5;
        let mut s = Senses {
            players: &[],
            rand: &mut rnd,
        };
        let gone = (0..30).position(|_| tick(&mut m, &w, &mut s));
        assert_eq!(gone, Some(19));
    }

    #[test]
    fn follows_food() {
        let w = World {
            solid: &floor,
            water: &no_water,
        };
        let mut m = Mob::new(Species::Sheep, [0.5, 64.0, 0.5], 1);
        m.on_ground = true;
        let players = [([0.5, 64.0, 8.5], true)];
        let mut rnd = || 0.99;
        let mut s = Senses {
            players: &players,
            rand: &mut rnd,
        };
        for _ in 0..100 {
            tick(&mut m, &w, &mut s);
        }
        assert!(m.pos[2] > 5.0 && m.pos[2] < 7.0, "{:?}", m.pos);
    }

    #[test]
    fn species_names() {
        assert_eq!(Species::from_name("minecraft:cow"), Some(Species::Cow));
        assert_eq!(Species::from_name("sheep"), Some(Species::Sheep));
        assert!(Species::Pig.is_food("minecraft:carrot"));
    }
}
