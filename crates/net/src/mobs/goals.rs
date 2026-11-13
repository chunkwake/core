//! Цели мобов: `GoalSelector`, `LookControl` и цели из `ai.goal`, перенос 26.3.
//!
//! У моба два селектора, как в `Mob`: `goalSelector` (движение, взгляд,
//! прыжок) и `targetSelector` (выбор цели). Полный проход селекторов —
//! на нечётных тиках `(tickCount + id) % 2` и на первом; в остальные тики
//! работают только цели с `requiresUpdateEveryTick` (`Mob.serverAiStep`).
//! Поэтому задержки целей без него делятся пополам (`reducedTickDelay`).
//!
//! Случайные числа идут из `Senses::rand` в том же порядке вызовов, что и
//! `RandomSource` в ванили; сами значения, понятно, другие.

use super::path;
use super::{Mob, Prey, Senses, Species, World, dist2};

/// `Goal.Flag`.
pub const MOVE: u8 = 1;
pub const LOOK: u8 = 2;
pub const JUMP: u8 = 4;
pub const TARGET: u8 = 8;

/// `Mob.getHeadRotSpeed`, `getMaxHeadXRot`, `getMaxHeadYRot`.
pub const HEAD_ROT_SPEED: f32 = 10.0;
pub const MAX_HEAD_X: f32 = 40.0;
pub const MAX_HEAD_Y: f32 = 75.0;

/// `Goal.reducedTickDelay`: `Mth.positiveCeilDiv(t, 2)`.
pub fn reduced(t: i32) -> i32 {
    (t + 1).div_euclid(2)
}

/// Что моб сообщает серверу за тик: события, которые трогают мир или
/// других мобов.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Out {
    /// `BreedGoal.breed` с партнёром (id сущности).
    Breed(i32),
    /// `EatBlockGoal.start`: событие сущности 10 (анимация).
    EatStart,
    /// `EatBlockGoal.tick`: съесть траву в клетке (`true`) или дёрн под ногами.
    Eat([i32; 3], bool),
    /// `BreakDoorGoal.tick`: удар по двери (`levelEvent 1019`) и взмах.
    DoorHit([i32; 3]),
    /// `BreakDoorGoal.tick`: прогресс ломания (`destroyBlockProgress`), -1 — сброс.
    DoorBreakProgress { pos: [i32; 3], progress: i32 },
    /// `BreakDoorGoal.tick`: дверь сломана (`removeBlock` и события).
    DoorBroken([i32; 3]),
    /// `HurtByTargetGoal.alertOthers`: позвать своих на обидчика.
    Alert(u128),
    /// `Creeper.tick`: набухание началось (звук `creeper.primed`, `prime_fuse`).
    Primed,
    /// `Creeper.explodeCreeper`: взрыв этой мощности, моб исчез.
    Explode(f32),
    /// `AbstractSkeleton.performRangedAttack`: стрела в игрока с силой лука.
    Shoot { target: u128, power: f32 },
    /// Слизень прыгнул (`jump`) или шлёпнулся на землю: звук `jump`/`squish`.
    CubeSound { jump: bool },
}

/// Сородич рядом: для `BreedGoal` и `FollowParentGoal`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kin {
    pub id: i32,
    pub species: Species,
    pub pos: [f64; 3],
    pub age: i32,
    pub in_love: bool,
    pub panicking: bool,
}

/// Клетка для случайных точек прогулки (`getWalkTargetValue`) и еды овцы.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellInfo {
    /// Блок в клетке — дёрн (`Blocks.GRASS_BLOCK`).
    pub grass_block: bool,
    /// Блок в клетке съедобен овце (`#edible_for_sheep`).
    pub short_grass: bool,
    /// `getLightLevelDependentMagicValue`.
    pub brightness: f32,
    /// `Level.canSeeSky`.
    pub sky: bool,
}

impl Default for CellInfo {
    fn default() -> Self {
        Self {
            grass_block: false,
            short_grass: false,
            brightness: 0.5,
            sky: false,
        }
    }
}

/// `LookControl`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LookControl {
    pub wanted: [f64; 3],
    pub y_max: f32,
    pub x_max: f32,
    pub cooldown: u8,
}

impl LookControl {
    /// `setLookAt(x, y, z, yMax, xMax)`.
    pub fn set(&mut self, at: [f64; 3], y_max: f32, x_max: f32) {
        self.wanted = at;
        self.y_max = y_max;
        self.x_max = x_max;
        self.cooldown = 2;
    }

    /// `setLookAt(x, y, z)`: скорости моба по умолчанию.
    pub fn look(&mut self, at: [f64; 3]) {
        self.set(at, HEAD_ROT_SPEED, MAX_HEAD_X);
    }
}

fn degrees_difference(from: f32, to: f32) -> f32 {
    super::wrap(to - from)
}

/// `Mth.rotateIfNecessary`.
fn rotate_if_necessary(current: f32, target: f32, max: f32) -> f32 {
    let d = degrees_difference(current, target).clamp(-max, max);
    target - d
}

/// `LookControl.rotateTowards`.
fn rotate_towards(from: f32, to: f32, max: f32) -> f32 {
    from + degrees_difference(from, to).clamp(-max, max)
}

/// `LookControl.tick`. Тело (`yBodyRot`) у нас — это `yaw`.
pub fn look_tick(m: &mut Mob) {
    // `resetXRotOnTick`.
    m.pitch = 0.0;
    let lc = m.look_ctl;
    if lc.cooldown > 0 {
        m.look_ctl.cooldown -= 1;
        let eye = m.eye();
        let d = [
            lc.wanted[0] - m.pos[0],
            lc.wanted[1] - eye[1],
            lc.wanted[2] - m.pos[2],
        ];
        const EPS: f64 = 9.999999747378752E-6;
        if d[2].abs() > EPS || d[0].abs() > EPS {
            let y = (path::atan2(d[2], d[0]) * 57.2957763671875) as f32 - 90.0;
            m.head_yaw = rotate_towards(m.head_yaw, y, lc.y_max);
        }
        let h = (d[0] * d[0] + d[2] * d[2]).sqrt();
        if d[1].abs() > EPS || h.abs() > EPS {
            let x = -(path::atan2(d[1], h) * 57.2957763671875) as f32;
            m.pitch = rotate_towards(m.pitch, x, lc.x_max);
        }
    } else {
        m.head_yaw = rotate_towards(m.head_yaw, m.yaw, 10.0);
    }
    // `clampHeadRotationToBody`.
    if !m.nav.is_done() {
        m.head_yaw = rotate_if_necessary(m.head_yaw, m.yaw, MAX_HEAD_Y);
    }
}

/// Цель и её состояние.
#[derive(Clone, Debug, PartialEq)]
pub enum Goal {
    /// `FloatGoal`.
    Float,
    /// `PanicGoal`.
    Panic { speed: f64, to: [f64; 3] },
    /// `BreedGoal`.
    Breed {
        speed: f64,
        partner: Option<i32>,
        love_time: i32,
    },
    /// `TemptGoal` (без `canScare`): игрок с едой.
    Tempt {
        speed: f64,
        calm_down: i32,
        player: Option<[f64; 3]>,
    },
    /// `FollowParentGoal`.
    FollowParent {
        speed: f64,
        parent: Option<i32>,
        recalc: i32,
    },
    /// `RandomStrollGoal` / `WaterAvoidingRandomStrollGoal` (`avoid_water`).
    Stroll {
        speed: f64,
        interval: i32,
        avoid_water: bool,
        to: [f64; 3],
    },
    /// `LookAtPlayerGoal(Player, range)`.
    LookAtPlayer {
        range: f64,
        at: Option<u128>,
        time: i32,
    },
    /// `RandomLookAroundGoal`.
    LookAround { rel: [f64; 2], time: i32 },
    /// `EatBlockGoal`.
    EatBlock { tick: i32 },
    /// `BreakDoorGoal`: закрытая деревянная дверь в узлах пути.
    BreakDoor(BreakDoor),
    /// `MeleeAttackGoal`; `zombie` — `ZombieAttackGoal` (поднятые руки).
    Melee(Box<Melee>),
    /// `NearestAttackableTargetGoal<Player>(mustSee = true)`.
    NearestPlayer {
        interval: i32,
        /// `SpiderTargetGoal`: при яркости ≥ 0.5 не ищет.
        day_neutral: bool,
        found: Option<u128>,
        tg: TargetState,
    },
    /// `HurtByTargetGoal` (`alert` — `setAlertOthers`).
    HurtBy {
        alert: bool,
        timestamp: u32,
        tg: TargetState,
    },
    /// `SwellGoal` крипера.
    Swell { target: Option<u128> },
    /// `RangedBowAttackGoal`.
    RangedBow(Box<RangedBow>),
    /// `RestrictSunGoal`: путь обрезается у неба.
    RestrictSun,
    /// `FleeSunGoal`.
    FleeSun { speed: f64, to: [f64; 3] },
    /// `LeapAtTargetGoal`.
    Leap { y: f64, target: Option<u128> },
    /// `CubeMobFloatGoal`: всплывать прыжками.
    CubeFloat,
    /// `CubeMobAttackGoal`: смотреть на цель и прыгать к ней, пока не устал.
    CubeAttack { tired: i32 },
    /// `CubeMobRandomDirectionGoal`: новое направление раз в 40–100 тиков.
    CubeRandomDir { chosen: f32, next: i32 },
    /// `CubeMobKeepOnJumpingGoal`: прыгать дальше.
    CubeKeepJumping,
}

/// Состояние `BreakDoorGoal`.
#[derive(Clone, Debug, PartialEq)]
pub struct BreakDoor {
    /// Верхняя половина двери (ванильный `DoorInteractGoal.doorPos`).
    pub door: Option<[i32; 3]>,
    pub break_time: i32,
    pub last_progress: i32,
}

impl BreakDoor {
    pub fn new() -> Self {
        Self {
            door: None,
            break_time: 0,
            last_progress: -1,
        }
    }
}

impl Default for BreakDoor {
    fn default() -> Self {
        Self::new()
    }
}

/// Состояние `MeleeAttackGoal`.
#[derive(Clone, Debug, PartialEq)]
pub struct Melee {
    pub speed: f64,
    pub zombie: bool,
    pub path: Option<path::Path>,
    pub pathed: [f64; 3],
    pub recalc: i32,
    pub next_attack: i32,
    pub last_can_use: i64,
    pub raise_arm: i32,
    /// `followingTargetEvenIfNotSeen`.
    pub follow: bool,
    /// `SpiderAttackGoal`: на свету бросает цель.
    pub spider: bool,
}

impl Melee {
    pub fn new(speed: f64, zombie: bool) -> Self {
        Self {
            speed,
            zombie,
            path: None,
            pathed: [0.0; 3],
            recalc: 0,
            next_attack: 0,
            // Первая проверка `canUse` проходит сразу.
            last_can_use: i64::MIN / 2,
            raise_arm: 0,
            follow: false,
            spider: false,
        }
    }
}

/// Состояние `RangedBowAttackGoal`.
#[derive(Clone, Debug, PartialEq)]
pub struct RangedBow {
    pub speed: f64,
    pub attack_time: i32,
    pub see_time: i32,
    pub strafing_time: i32,
    pub clockwise: bool,
    pub backwards: bool,
}

impl RangedBow {
    pub fn new(speed: f64) -> Self {
        Self {
            speed,
            attack_time: -1,
            see_time: 0,
            strafing_time: -1,
            clockwise: false,
            backwards: false,
        }
    }
}

/// `attackRadius` 15 у скелета, в квадрате.
const BOW_RADIUS_SQR: f32 = 15.0 * 15.0;

/// `BowItem.getPowerForTime`.
pub fn bow_power(ticks: i32) -> f32 {
    let f = ticks as f32 / 20.0;
    ((f * f + f * 2.0) / 3.0).min(1.0)
}

/// `AbstractSkeleton.reassessWeaponGoal`: пауза между выстрелами.
pub fn bow_interval(difficulty: u8) -> i32 {
    if difficulty == 3 { 20 } else { 40 }
}

/// `Mob.lookAt(entity, yMax, xMax)`: поворот тела сразу, без `LookControl`.
fn look_at(m: &mut Mob, p: &Prey, y_max: f32, x_max: f32) {
    let dx = p.pos[0] - m.pos[0];
    let dz = p.pos[2] - m.pos[2];
    let dy = p.eye()[1] - m.eye()[1];
    let d = (dx * dx + dz * dz).sqrt();
    let yaw = (path::atan2(dz, dx) * 57.2957763671875) as f32 - 90.0;
    let pitch = -(path::atan2(dy, d) * 57.2957763671875) as f32;
    m.pitch += degrees_difference(m.pitch, pitch).clamp(-x_max, x_max);
    m.yaw += degrees_difference(m.yaw, yaw).clamp(-y_max, y_max);
}

/// `getLightLevelDependentMagicValue` моба: яркость у глаз.
fn mob_brightness(m: &Mob, c: &Ctx) -> f32 {
    let e = m.eye();
    c.info([
        e[0].floor() as i32,
        e[1].floor() as i32,
        e[2].floor() as i32,
    ])
    .brightness
}

/// Общее у `TargetGoal`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TargetState {
    /// `targetMob`.
    pub target_mob: Option<u128>,
    pub unseen: i32,
    pub unseen_memory: i32,
}

impl Default for TargetState {
    fn default() -> Self {
        Self {
            target_mob: None,
            unseen: 0,
            unseen_memory: 60,
        }
    }
}

impl Goal {
    pub fn flags(&self) -> u8 {
        match self {
            Goal::Float => JUMP,
            Goal::Panic { .. } | Goal::Stroll { .. } => MOVE,
            Goal::Breed { .. } | Goal::Tempt { .. } | Goal::Melee(_) => MOVE | LOOK,
            Goal::FollowParent { .. } => 0,
            Goal::LookAtPlayer { .. } => LOOK,
            Goal::LookAround { .. } => MOVE | LOOK,
            Goal::EatBlock { .. } => MOVE | LOOK | JUMP,
            Goal::BreakDoor(_) => 0,
            Goal::NearestPlayer { .. } | Goal::HurtBy { .. } => TARGET,
            Goal::Swell { .. } | Goal::FleeSun { .. } => MOVE,
            Goal::RangedBow(_) => MOVE | LOOK,
            Goal::RestrictSun => 0,
            Goal::Leap { .. } => JUMP | MOVE,
            Goal::CubeFloat | Goal::CubeKeepJumping => JUMP | MOVE,
            Goal::CubeAttack { .. } | Goal::CubeRandomDir { .. } => LOOK,
        }
    }

    pub fn every_tick(&self) -> bool {
        matches!(
            self,
            Goal::Float
                | Goal::LookAround { .. }
                | Goal::Melee(_)
                | Goal::Swell { .. }
                | Goal::RangedBow(_)
                | Goal::CubeFloat
                | Goal::CubeAttack { .. }
                | Goal::BreakDoor(_)
        )
    }
}

/// `WrappedGoal`.
#[derive(Clone, Debug, PartialEq)]
pub struct Wrapped {
    pub priority: u8,
    pub running: bool,
    pub goal: Goal,
}

/// `GoalSelector`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Selector {
    pub goals: Vec<Wrapped>,
    /// `lockedFlags` по битам флагов.
    locked: [Option<usize>; 4],
}

impl Selector {
    pub fn new(goals: Vec<(u8, Goal)>) -> Self {
        Self {
            goals: goals
                .into_iter()
                .map(|(priority, goal)| Wrapped {
                    priority,
                    running: false,
                    goal,
                })
                .collect(),
            locked: [None; 4],
        }
    }

    pub fn running(&self, f: impl Fn(&Goal) -> bool) -> bool {
        self.goals.iter().any(|g| g.running && f(&g.goal))
    }

    fn stop(&mut self, i: usize, m: &mut Mob, c: &mut Ctx) {
        if self.goals[i].running {
            self.goals[i].running = false;
            stop(&mut self.goals[i].goal, m, c);
        }
    }

    /// `GoalSelector.tick`: снять отпавшие, запустить подходящие, тикнуть.
    pub fn tick(&mut self, m: &mut Mob, c: &mut Ctx) {
        for i in 0..self.goals.len() {
            if self.goals[i].running && !can_continue(&mut self.goals[i].goal, m, c) {
                self.stop(i, m, c);
            }
        }
        for slot in &mut self.locked {
            if slot.is_some_and(|i| !self.goals[i].running) {
                *slot = None;
            }
        }
        for i in 0..self.goals.len() {
            if self.goals[i].running {
                continue;
            }
            let flags = self.goals[i].goal.flags();
            let priority = self.goals[i].priority;
            // `goalCanBeReplacedForAllFlags`: у всех целей прерываемо.
            let free = (0..4)
                .filter(|b| flags & (1 << b) != 0)
                .all(|b| self.locked[b].is_none_or(|j| priority < self.goals[j].priority));
            if !free || !can_use(&mut self.goals[i].goal, m, c) {
                continue;
            }
            for b in 0..4 {
                if flags & (1 << b) != 0 {
                    if let Some(j) = self.locked[b] {
                        self.stop(j, m, c);
                    }
                    self.locked[b] = Some(i);
                }
            }
            self.goals[i].running = true;
            start(&mut self.goals[i].goal, m, c);
        }
        self.tick_running(true, m, c);
    }

    /// `tickRunningGoals`.
    pub fn tick_running(&mut self, all: bool, m: &mut Mob, c: &mut Ctx) {
        for g in &mut self.goals {
            if g.running && (all || g.goal.every_tick()) {
                tick(&mut g.goal, m, c);
            }
        }
    }
}

/// Всё, что цели читают из мира за тик.
pub struct Ctx<'c, 'a, 's, 'r, 'e> {
    pub w: &'c World<'a, 's, 'r>,
    pub s: &'c mut Senses<'e>,
}

impl Ctx<'_, '_, '_, '_, '_> {
    fn next_float(&mut self) -> f32 {
        (self.s.rand)() as f32
    }

    fn next_double(&mut self) -> f64 {
        (self.s.rand)()
    }

    fn next_int(&mut self, n: i32) -> i32 {
        if n <= 0 {
            return 0;
        }
        (((self.s.rand)() * f64::from(n)) as i32).min(n - 1)
    }

    fn player(&self, uuid: u128) -> Option<Prey> {
        self.s.players.iter().find(|p| p.uuid == uuid).copied()
    }

    fn prey(&self, uuid: u128) -> Option<Prey> {
        self.s.prey.iter().find(|p| p.uuid == uuid).copied()
    }

    fn kin(&self, id: i32) -> Option<Kin> {
        self.s.kin.iter().find(|k| k.id == id).copied()
    }

    fn info(&self, p: [i32; 3]) -> CellInfo {
        self.w.info.map_or_else(CellInfo::default, |f| f(p))
    }

    /// `Mob.hasLineOfSight`: от глаз до глаз.
    fn sees(&self, m: &Mob, p: &Prey) -> bool {
        (self.s.sight)(m.eye(), p.eye())
    }
}

/// `navigation.moveTo(x, y, z | entity, reach, speed)`.
pub fn move_to(m: &mut Mob, c: &mut Ctx, to: [f64; 3], reach: i32, speed: f64) -> bool {
    let water = super::in_water(m, c.w);
    let walk = super::walker(m, c.w, c.s.difficulty, water);
    let found = m
        .nav
        .path_to(c.w, &walk, to, reach, m.species.follow_range());
    m.nav.move_to(c.w, &walk, found, speed)
}

/// `DoorInteractGoal`: расстояние по горизонтали до центра двери.
fn door_flat_dist2(pos: [f64; 3], door: [i32; 3]) -> f64 {
    let dx = pos[0] - (f64::from(door[0]) + 0.5);
    let dz = pos[2] - (f64::from(door[2]) + 0.5);
    dx * dx + dz * dz
}

/// `doorPos.closerToCenterThan(mob.position(), 2.0)`.
fn door_close(pos: [f64; 3], door: [i32; 3]) -> bool {
    let dx = pos[0] - (f64::from(door[0]) + 0.5);
    let dy = pos[1] - (f64::from(door[1]) + 0.5);
    let dz = pos[2] - (f64::from(door[2]) + 0.5);
    dx * dx + dy * dy + dz * dz < 4.0
}

/// `DoorBlock.isWoodenDoor` для закрытой двери: тип клетки пути.
fn wooden_door_closed(w: &World, p: [i32; 3]) -> bool {
    path::cell(w, p).kind == path::PathType::DoorWoodClosed
}

fn door_open(w: &World, p: [i32; 3]) -> bool {
    path::cell(w, p).kind == path::PathType::DoorOpen
}

/// `BreakDoorGoal.canUse`/`DoorInteractGoal.canUse`.
fn break_door_can_use(g: &mut BreakDoor, m: &Mob, c: &Ctx) -> bool {
    if !m.can_break_doors || c.s.difficulty != 3 || !c.s.mob_griefing || !m.horizontal_collision {
        return false;
    }
    let Some(path) = m.nav.path.as_ref().filter(|p| !p.is_done()) else {
        return false;
    };
    g.door = None;
    for node in path.nodes.iter().take(path.next + 2) {
        let n = node.0;
        // Ванильный `DoorInteractGoal` берёт блок над узлом пути.
        let door = [n[0], n[1] + 1, n[2]];
        if door_flat_dist2(m.pos, door) > 2.25 || !wooden_door_closed(c.w, door) {
            continue;
        }
        g.door = Some(door);
        return true;
    }
    // `blockPosition().above()`.
    let b = m.pos.map(|v| v.floor() as i32);
    let door = [b[0], b[1] + 1, b[2]];
    if wooden_door_closed(c.w, door) {
        g.door = Some(door);
        return true;
    }
    false
}

// --- Случайные точки (`RandomPos`, `DefaultRandomPos`, `LandRandomPos`) ----

/// `getWalkTargetValue`.
fn walk_value(m: &Mob, c: &Ctx, p: [i32; 3]) -> f64 {
    if m.species.is_animal() {
        if c.info([p[0], p[1] - 1, p[2]]).grass_block {
            10.0
        } else {
            f64::from(c.info(p).brightness - 0.5)
        }
    } else if m.species.is_monster() {
        f64::from(-(c.info(p).brightness - 0.5))
    } else {
        0.0
    }
}

/// `RandomPos.generateRandomDirection`.
fn random_dir(c: &mut Ctx, h: i32, v: i32) -> [i32; 3] {
    let x = c.next_int(2 * h + 1) - h;
    let y = c.next_int(2 * v + 1) - v;
    let z = c.next_int(2 * h + 1) - h;
    [x, y, z]
}

/// `RandomPos.generateRandomPosTowardDirection` без дома: `BlockPos.containing`.
fn toward(m: &Mob, d: [i32; 3]) -> [i32; 3] {
    [
        (f64::from(d[0]) + m.pos[0]).floor() as i32,
        (f64::from(d[1]) + m.pos[1]).floor() as i32,
        (f64::from(d[2]) + m.pos[2]).floor() as i32,
    ]
}

/// `GoalUtils.isOutsideLimits`.
fn outside(c: &Ctx, p: [i32; 3]) -> bool {
    p[1] < c.w.min_y || p[1] >= c.w.min_y + 384
}

/// `GroundPathNavigation.isStableDestination`: под точкой цельный блок.
fn stable(c: &Ctx, p: [i32; 3]) -> bool {
    super::full_block((c.w.shape)([p[0], p[1] - 1, p[2]]))
}

/// `GoalUtils.hasMalus`.
fn has_malus(m: &Mob, c: &Ctx, p: [i32; 3]) -> bool {
    let walk = super::walker(m, c.w, c.s.difficulty, false);
    walk.malus(path::type_static(c.w, &walk, p)) != 0.0
}

/// `RandomPos.generateRandomPos`: 10 попыток, лучшая по `getWalkTargetValue`.
fn generate(
    m: &Mob,
    c: &mut Ctx,
    mut next: impl FnMut(&Mob, &mut Ctx) -> Option<[i32; 3]>,
) -> Option<[f64; 3]> {
    let mut best = f64::NEG_INFINITY;
    let mut found = None;
    for _ in 0..10 {
        if let Some(p) = next(m, c) {
            let v = walk_value(m, c, p);
            if v > best {
                best = v;
                found = Some(p);
            }
        }
    }
    found.map(|p| {
        [
            f64::from(p[0]) + 0.5,
            f64::from(p[1]),
            f64::from(p[2]) + 0.5,
        ]
    })
}

/// `DefaultRandomPos.getPos`.
pub fn default_pos(m: &Mob, c: &mut Ctx, h: i32, v: i32) -> Option<[f64; 3]> {
    generate(m, c, |m, c| {
        let d = random_dir(c, h, v);
        let p = toward(m, d);
        (!outside(c, p) && stable(c, p) && !has_malus(m, c, p)).then_some(p)
    })
}

/// `LandRandomPos.getPos`.
pub fn land_pos(m: &Mob, c: &mut Ctx, h: i32, v: i32) -> Option<[f64; 3]> {
    generate(m, c, |m, c| {
        let d = random_dir(c, h, v);
        let mut p = toward(m, d);
        if outside(c, p) || !stable(c, p) {
            return None;
        }
        // `RandomPos.moveUpOutOfSolid`.
        let max = c.w.min_y + 383;
        while p[1] <= max && path::cell(c.w, p).has(path::Cell::SOLID) {
            p[1] += 1;
        }
        (!(c.w.water)(p) && !has_malus(m, c, p)).then_some(p)
    })
}

// --- Сами цели ------------------------------------------------------------

/// `isInFluidDeeperThan(WATER, getFluidJumpThreshold)`.
pub fn deep_water(m: &Mob, w: &World) -> bool {
    let threshold = if m.eye_height() < 0.4 { 0.0 } else { 0.4 };
    (w.water)([
        m.pos[0].floor() as i32,
        (m.pos[1] + threshold).floor() as i32,
        m.pos[2].floor() as i32,
    ])
}

/// `PanicGoal.shouldPanic`: урон из `panic_causes` за последние 40 тиков.
fn should_panic(m: &Mob) -> bool {
    m.last_damage_source
        .is_some_and(|(stamp, panic)| panic && m.tick_count.wrapping_sub(stamp) <= 40)
}

/// `TargetingConditions.forCombat` к игроку: в дальности (от ног моба) и
/// виден, если надо.
fn combat_ok(m: &Mob, c: &Ctx, p: &Prey, range: f64, see: bool) -> bool {
    dist2(p.pos, m.pos) <= range.max(2.0) * range.max(2.0) && (!see || c.sees(m, p))
}

/// `TargetGoal.canContinueToUse`.
fn target_continue(tg: &mut TargetState, must_see: bool, m: &mut Mob, c: &mut Ctx) -> bool {
    let Some(uuid) = m.target.or(tg.target_mob) else {
        return false;
    };
    // `canAttack`: игрок жив и не в творческом.
    let Some(p) = c.prey(uuid) else {
        return false;
    };
    let range = m.species.follow_range();
    if dist2(m.pos, p.pos) > range * range {
        return false;
    }
    if must_see {
        if c.sees(m, &p) {
            tg.unseen = 0;
        } else {
            tg.unseen += 1;
            if tg.unseen > reduced(tg.unseen_memory) {
                return false;
            }
        }
    }
    m.target = Some(uuid);
    true
}

fn can_use(g: &mut Goal, m: &mut Mob, c: &mut Ctx) -> bool {
    match g {
        Goal::Float => deep_water(m, c.w),
        Goal::Panic { to, .. } => {
            if !should_panic(m) {
                return false;
            }
            match default_pos(m, c, 5, 4) {
                Some(p) => {
                    *to = p;
                    true
                }
                None => false,
            }
        }
        Goal::Breed { partner, .. } => {
            if m.love <= 0 {
                return false;
            }
            // `getFreePartner`: влюблённый того же вида в 8 блоках, не в панике.
            let me = c.s.entity_id;
            *partner =
                c.s.kin
                    .iter()
                    .filter(|k| {
                        k.id != me
                            && k.species == m.species
                            && k.in_love
                            && !k.panicking
                            && dist2(k.pos, m.pos) <= 64.0
                            && (k.pos[0] - m.pos[0]).abs() <= 8.0 + m.size().0
                            && (k.pos[1] - m.pos[1]).abs() <= 8.0 + m.size().1
                            && (k.pos[2] - m.pos[2]).abs() <= 8.0 + m.size().0
                    })
                    .min_by(|a, b| dist2(a.pos, m.pos).total_cmp(&dist2(b.pos, m.pos)))
                    .map(|k| k.id);
            partner.is_some()
        }
        Goal::Tempt {
            calm_down, player, ..
        } => {
            if *calm_down > 0 {
                *calm_down -= 1;
                return false;
            }
            // Атрибут `tempt_range` — 10.
            *player =
                c.s.tempters
                    .iter()
                    .filter(|p| dist2(**p, m.pos) <= 100.0)
                    .min_by(|a, b| dist2(**a, m.pos).total_cmp(&dist2(**b, m.pos)))
                    .copied();
            player.is_some()
        }
        Goal::FollowParent { parent, .. } => {
            if m.age >= 0 {
                return false;
            }
            let (w, h) = m.size();
            let near =
                c.s.kin
                    .iter()
                    .filter(|k| {
                        k.species == m.species
                            && k.age >= 0
                            && (k.pos[0] - m.pos[0]).abs() <= 8.0 + w
                            && (k.pos[1] - m.pos[1]).abs() <= 4.0 + h
                            && (k.pos[2] - m.pos[2]).abs() <= 8.0 + w
                    })
                    .min_by(|a, b| dist2(a.pos, m.pos).total_cmp(&dist2(b.pos, m.pos)));
            match near {
                Some(k) if dist2(k.pos, m.pos) >= 9.0 => {
                    *parent = Some(k.id);
                    true
                }
                _ => false,
            }
        }
        Goal::Stroll {
            interval,
            avoid_water,
            to,
            ..
        } => {
            if m.no_action >= 100 {
                return false;
            }
            if c.next_int(reduced(*interval)) != 0 {
                return false;
            }
            let p = if *avoid_water {
                if super::in_water(m, c.w) {
                    land_pos(m, c, 15, 7)
                } else if c.next_float() >= 0.001 {
                    land_pos(m, c, 10, 7)
                } else {
                    default_pos(m, c, 10, 7)
                }
            } else {
                default_pos(m, c, 10, 7)
            };
            match p {
                Some(p) => {
                    *to = p;
                    true
                }
                None => false,
            }
        }
        Goal::LookAtPlayer { range, at, .. } => {
            if c.next_float() >= 0.02 {
                return false;
            }
            *at = if let Some(t) = m.target {
                Some(t)
            } else {
                // `getNearestPlayer(lookAtContext, x, eyeY, z)`: в дальности и на виду.
                let eye = m.eye();
                let r = range.max(2.0);
                c.s.players
                    .iter()
                    .filter(|p| dist2(p.pos, m.pos) <= r * r && c.sees(m, p))
                    .min_by(|a, b| dist2(a.pos, eye).total_cmp(&dist2(b.pos, eye)))
                    .map(|p| p.uuid)
            };
            at.is_some()
        }
        Goal::LookAround { .. } => c.next_float() < 0.02,
        Goal::EatBlock { .. } => {
            let chance = reduced(if m.is_baby() { 50 } else { 1000 });
            if c.next_int(chance) != 0 {
                return false;
            }
            let p = m.pos.map(|v| v.floor() as i32);
            c.info(p).short_grass || c.info([p[0], p[1] - 1, p[2]]).grass_block
        }
        Goal::Melee(g) => {
            if i64::from(m.tick_count) - g.last_can_use < 20 {
                return false;
            }
            g.last_can_use = i64::from(m.tick_count);
            let Some(p) = m.target.and_then(|t| c.prey(t)) else {
                return false;
            };
            let water = super::in_water(m, c.w);
            let walk = super::walker(m, c.w, c.s.difficulty, water);
            g.path = m
                .nav
                .path_to(c.w, &walk, p.pos, 0, m.species.follow_range());
            g.path.is_some() || super::in_reach(m, &p)
        }
        Goal::NearestPlayer {
            interval,
            day_neutral,
            found,
            ..
        } => {
            if *day_neutral && mob_brightness(m, c) >= 0.5 {
                return false;
            }
            if *interval > 0 && c.next_int(*interval) != 0 {
                return false;
            }
            let range = m.species.follow_range();
            let eye = m.eye();
            *found =
                c.s.prey
                    .iter()
                    // `Slime`: цель не дальше 4 блоков по высоте.
                    .filter(|p| m.species != Species::Slime || (p.pos[1] - m.pos[1]).abs() <= 4.0)
                    .filter(|p| combat_ok(m, c, p, range, true))
                    .min_by(|a, b| dist2(a.pos, eye).total_cmp(&dist2(b.pos, eye)))
                    .map(|p| p.uuid);
            found.is_some()
        }
        Goal::HurtBy { timestamp, .. } => {
            let Some((attacker, stamp)) = m.last_hurt_by else {
                return false;
            };
            // `HURT_BY_TARGETING` без дальности и взгляда: лишь бы можно было бить.
            stamp != *timestamp && c.prey(attacker).is_some()
        }
        Goal::Swell { .. } => {
            m.swell_dir > 0
                || m.target
                    .and_then(|t| c.player(t))
                    .is_some_and(|p| dist2(m.pos, p.pos) < 9.0)
        }
        Goal::RangedBow(_) => m.target.is_some(),
        // Голова пуста: шлемов у мобов пока нет.
        Goal::RestrictSun => c.s.bright_outside,
        Goal::FleeSun { to, .. } => {
            if m.target.is_some() || !c.s.bright_outside || !m.on_fire {
                return false;
            }
            let b = m.pos.map(|v| v.floor() as i32);
            if !c.info(b).sky {
                return false;
            }
            // `getHidePos`.
            for _ in 0..10 {
                let p = [
                    b[0] + c.next_int(20) - 10,
                    b[1] + c.next_int(6) - 3,
                    b[2] + c.next_int(20) - 10,
                ];
                if !c.info(p).sky && walk_value(m, c, p) < 0.0 {
                    *to = [
                        f64::from(p[0]) + 0.5,
                        f64::from(p[1]),
                        f64::from(p[2]) + 0.5,
                    ];
                    return true;
                }
            }
            false
        }
        Goal::CubeFloat => super::in_liquid(m, c.w),
        Goal::CubeAttack { .. } => m.target.and_then(|t| c.prey(t)).is_some(),
        Goal::CubeRandomDir { .. } => {
            m.target.is_none() && (m.on_ground || super::in_liquid(m, c.w))
        }
        Goal::CubeKeepJumping => true,
        Goal::BreakDoor(g) => break_door_can_use(g, m, c),
        Goal::Leap { target, .. } => {
            let Some(p) = m.target.and_then(|t| c.player(t)) else {
                return false;
            };
            *target = Some(p.uuid);
            let d = dist2(m.pos, p.pos);
            if !(4.0..=16.0).contains(&d) || !m.on_ground {
                return false;
            }
            c.next_int(reduced(5)) == 0
        }
    }
}

fn can_continue(g: &mut Goal, m: &mut Mob, c: &mut Ctx) -> bool {
    match g {
        Goal::Panic { .. } | Goal::Stroll { .. } => !m.nav.is_done(),
        Goal::Breed {
            partner, love_time, ..
        } => partner
            .and_then(|id| c.kin(id))
            .is_some_and(|k| k.in_love && *love_time < 60 && !k.panicking),
        Goal::FollowParent { parent, .. } => {
            m.age < 0
                && parent.and_then(|id| c.kin(id)).is_some_and(|k| {
                    let d = dist2(m.pos, k.pos);
                    (9.0..=256.0).contains(&d)
                })
        }
        Goal::LookAtPlayer { range, at, time } => at
            .and_then(|u| c.player(u))
            .is_some_and(|p| dist2(m.pos, p.pos) <= *range * *range && *time > 0),
        Goal::LookAround { time, .. } => *time >= 0,
        Goal::EatBlock { tick } => *tick > 0,
        Goal::Melee(g) => {
            if g.spider && mob_brightness(m, c) >= 0.5 && c.next_int(100) == 0 {
                m.target = None;
                return false;
            }
            // Творческий и наблюдатель — не добыча. Без `follow` — пока не дошли.
            m.target.and_then(|t| c.prey(t)).is_some() && (g.follow || !m.nav.is_done())
        }
        Goal::NearestPlayer { tg, .. } => target_continue(tg, true, m, c),
        Goal::HurtBy { tg, .. } => target_continue(tg, true, m, c),
        Goal::RangedBow(_) => m.target.is_some() || !m.nav.is_done(),
        Goal::FleeSun { .. } => !m.nav.is_done(),
        Goal::Leap { .. } => !m.on_ground,
        Goal::CubeAttack { tired } => {
            m.target.and_then(|t| c.prey(t)).is_some() && {
                *tired -= 1;
                *tired > 0
            }
        }
        Goal::CubeFloat | Goal::CubeRandomDir { .. } | Goal::CubeKeepJumping => can_use(g, m, c),
        Goal::BreakDoor(g) => {
            let Some(door) = g.door else {
                return false;
            };
            // `BreakDoorGoal.canContinueToUse`: 240 тиков, дверь ещё закрыта,
            // моб рядом, сложность всё ещё сложная.
            g.break_time <= 240
                && !door_open(c.w, door)
                && door_close(m.pos, door)
                && c.s.difficulty == 3
        }
        // `Goal.canContinueToUse` по умолчанию — снова `canUse`.
        Goal::Float | Goal::Tempt { .. } | Goal::Swell { .. } | Goal::RestrictSun => {
            can_use(g, m, c)
        }
    }
}

fn start(g: &mut Goal, m: &mut Mob, c: &mut Ctx) {
    match g {
        Goal::Panic { speed, to } => {
            let (speed, to) = (*speed, *to);
            move_to(m, c, to, 1, speed);
        }
        Goal::FollowParent { recalc, .. } => *recalc = 0,
        Goal::Stroll { speed, to, .. } => {
            let (speed, to) = (*speed, *to);
            move_to(m, c, to, 1, speed);
        }
        Goal::LookAtPlayer { time, .. } => {
            let t = 40 + c.next_int(40);
            *time = reduced(t);
        }
        Goal::LookAround { rel, time } => {
            let d = std::f64::consts::TAU * c.next_double();
            *rel = [d.cos(), d.sin()];
            *time = 20 + c.next_int(20);
        }
        Goal::EatBlock { tick } => {
            *tick = reduced(40);
            m.out.push(Out::EatStart);
            m.nav.stop();
        }
        Goal::Melee(g) => {
            let path = g.path.take();
            let water = super::in_water(m, c.w);
            let walk = super::walker(m, c.w, c.s.difficulty, water);
            m.nav.move_to(c.w, &walk, path, g.speed);
            m.aggressive = true;
            g.recalc = 0;
            g.next_attack = 0;
            g.raise_arm = 0;
        }
        Goal::NearestPlayer { found, tg, .. } => {
            m.target = *found;
            tg.unseen = 0;
        }
        Goal::HurtBy {
            alert,
            timestamp,
            tg,
        } => {
            let (attacker, stamp) = m.last_hurt_by.expect("canUse проверил");
            m.target = Some(attacker);
            tg.target_mob = Some(attacker);
            *timestamp = stamp;
            tg.unseen_memory = 300;
            if *alert {
                m.out.push(Out::Alert(attacker));
            }
            tg.unseen = 0;
        }
        Goal::Swell { target } => {
            m.nav.stop();
            *target = m.target;
        }
        Goal::RangedBow(_) => m.aggressive = true,
        Goal::RestrictSun => m.nav.avoid_sun = true,
        Goal::FleeSun { speed, to } => {
            let (speed, to) = (*speed, *to);
            move_to(m, c, to, 1, speed);
        }
        Goal::Leap { y, target } => {
            let Some(p) = target.and_then(|t| c.player(t)) else {
                return;
            };
            let d = [p.pos[0] - m.pos[0], 0.0, p.pos[2] - m.pos[2]];
            let l2 = d[0] * d[0] + d[2] * d[2];
            let mut v = d;
            if l2 > 1.0e-7 {
                let l = l2.sqrt();
                v = [
                    d[0] / l * 0.4 + m.vel[0] * 0.2,
                    0.0,
                    d[2] / l * 0.4 + m.vel[2] * 0.2,
                ];
            }
            m.vel = [v[0], *y, v[2]];
        }
        Goal::CubeAttack { tired } => *tired = reduced(300),
        Goal::BreakDoor(g) => {
            g.break_time = 0;
            g.last_progress = -1;
        }
        Goal::Float
        | Goal::Breed { .. }
        | Goal::Tempt { .. }
        | Goal::CubeFloat
        | Goal::CubeRandomDir { .. }
        | Goal::CubeKeepJumping => {}
    }
}

fn stop(g: &mut Goal, m: &mut Mob, c: &mut Ctx) {
    match g {
        Goal::Breed {
            partner, love_time, ..
        } => {
            *partner = None;
            *love_time = 0;
        }
        Goal::Tempt {
            calm_down, player, ..
        } => {
            *player = None;
            m.nav.stop();
            *calm_down = reduced(100);
        }
        Goal::FollowParent { parent, .. } => *parent = None,
        Goal::Stroll { .. } => m.nav.stop(),
        Goal::LookAtPlayer { at, .. } => *at = None,
        Goal::EatBlock { tick } => *tick = 0,
        Goal::Melee(_) => {
            // Творческий или наблюдатель — цель сбрасывается.
            if m.target.is_some_and(|t| c.prey(t).is_none()) {
                m.target = None;
            }
            m.aggressive = false;
            m.nav.stop();
        }
        Goal::NearestPlayer { tg, .. } | Goal::HurtBy { tg, .. } => {
            m.target = None;
            tg.target_mob = None;
        }
        Goal::Swell { target } => *target = None,
        Goal::RangedBow(g) => {
            m.aggressive = false;
            g.see_time = 0;
            g.attack_time = -1;
            m.using_item = None;
        }
        Goal::RestrictSun => m.nav.avoid_sun = false,
        Goal::BreakDoor(g) => {
            // `BreakDoorGoal.stop`: убрать сетку ломания.
            if let Some(door) = g.door {
                m.out.push(Out::DoorBreakProgress {
                    pos: door,
                    progress: -1,
                });
            }
            g.door = None;
        }
        Goal::Float
        | Goal::Panic { .. }
        | Goal::LookAround { .. }
        | Goal::FleeSun { .. }
        | Goal::Leap { .. }
        | Goal::CubeFloat
        | Goal::CubeAttack { .. }
        | Goal::CubeRandomDir { .. }
        | Goal::CubeKeepJumping => {}
    }
}

fn tick(g: &mut Goal, m: &mut Mob, c: &mut Ctx) {
    match g {
        Goal::Float => {
            if c.next_float() < 0.8 {
                m.jump = true;
            }
        }
        Goal::Breed {
            speed,
            partner,
            love_time,
        } => {
            let Some(k) = partner.and_then(|id| c.kin(id)) else {
                return;
            };
            let eye = [k.pos[0], k.pos[1] + k.species.eye_height(), k.pos[2]];
            m.look_ctl.set(eye, 10.0, MAX_HEAD_X);
            move_to(m, c, k.pos, 1, *speed);
            *love_time += 1;
            if *love_time >= reduced(60) && dist2(m.pos, k.pos) < 9.0 {
                m.out.push(Out::Breed(k.id));
            }
        }
        Goal::Tempt { speed, player, .. } => {
            let Some(p) = *player else {
                return;
            };
            m.look_ctl.set(
                [p[0], p[1] + super::PLAYER_EYE, p[2]],
                MAX_HEAD_Y + 20.0,
                MAX_HEAD_X,
            );
            if dist2(m.pos, p) < 6.25 {
                m.nav.stop();
            } else {
                move_to(m, c, p, 1, *speed);
            }
        }
        Goal::FollowParent {
            speed,
            parent,
            recalc,
        } => {
            *recalc -= 1;
            if *recalc <= 0 {
                *recalc = reduced(10);
                if let Some(k) = parent.and_then(|id| c.kin(id)) {
                    move_to(m, c, k.pos, 1, *speed);
                }
            }
        }
        Goal::LookAtPlayer { at, time, .. } => {
            if let Some(p) = at.and_then(|u| c.player(u)) {
                let e = p.eye();
                m.look_ctl.look([p.pos[0], e[1], p.pos[2]]);
                *time -= 1;
            }
        }
        Goal::LookAround { rel, time } => {
            *time -= 1;
            let eye = m.eye();
            m.look_ctl
                .look([m.pos[0] + rel[0], eye[1], m.pos[2] + rel[1]]);
        }
        Goal::EatBlock { tick } => {
            *tick = (*tick - 1).max(0);
            if *tick == reduced(4) {
                let p = m.pos.map(|v| v.floor() as i32);
                let below = [p[0], p[1] - 1, p[2]];
                if c.info(p).short_grass {
                    m.out.push(Out::Eat(p, true));
                } else if c.info(below).grass_block {
                    m.out.push(Out::Eat(below, false));
                }
            }
        }
        Goal::BreakDoor(g) => {
            let Some(door) = g.door else { return };
            // `BreakDoorGoal.tick`: раз в 20 тиков звук удара и взмах.
            if c.next_int(20) == 0 {
                m.out.push(Out::DoorHit(door));
            }
            g.break_time += 1;
            let progress = (g.break_time * 10 / 240).min(10);
            if progress != g.last_progress {
                g.last_progress = progress;
                m.out.push(Out::DoorBreakProgress {
                    pos: door,
                    progress,
                });
            }
            if g.break_time == 240 {
                m.out.push(Out::DoorBroken(door));
            }
        }
        Goal::Melee(g) => melee_tick(g, m, c),
        Goal::Swell { target } => {
            // `SwellGoal.tick`: цель далеко (> 7) или не видна — сдуваться.
            m.swell_dir = match target.and_then(|t| c.player(t)) {
                Some(p) if dist2(m.pos, p.pos) <= 49.0 && c.sees(m, &p) => 1,
                _ => -1,
            };
        }
        Goal::RangedBow(g) => bow_tick(g, m, c),
        Goal::CubeFloat => {
            if c.next_float() < 0.8 {
                m.jump = true;
            }
            m.cube.set_wanted_movement(1.2);
        }
        Goal::CubeAttack { .. } => {
            if let Some(p) = m.target.and_then(|t| c.prey(t)) {
                look_at(m, &p, 10.0, 10.0);
            }
            let deals = super::cube::deals_damage(m);
            m.cube.set_direction(m.yaw, deals);
        }
        Goal::CubeRandomDir { chosen, next } => {
            *next -= 1;
            if *next <= 0 {
                *next = reduced(40 + c.next_int(60));
                *chosen = c.next_int(360) as f32;
            }
            m.cube.set_direction(*chosen, false);
        }
        Goal::CubeKeepJumping => m.cube.set_wanted_movement(1.0),
        Goal::Panic { .. }
        | Goal::Stroll { .. }
        | Goal::NearestPlayer { .. }
        | Goal::HurtBy { .. }
        | Goal::RestrictSun
        | Goal::FleeSun { .. }
        | Goal::Leap { .. } => {}
    }
}

/// `RangedBowAttackGoal.tick`.
fn bow_tick(g: &mut RangedBow, m: &mut Mob, c: &mut Ctx) {
    let Some(p) = m.target.and_then(|t| c.player(t)) else {
        return;
    };
    let d2 = dist2(m.pos, p.pos);
    let see = c.sees(m, &p);
    if see != (g.see_time > 0) {
        g.see_time = 0;
    }
    if see {
        g.see_time += 1;
    } else {
        g.see_time -= 1;
    }
    if d2 <= f64::from(BOW_RADIUS_SQR) && g.see_time >= 20 {
        m.nav.stop();
        g.strafing_time += 1;
    } else {
        move_to(m, c, p.pos, 0, g.speed);
        g.strafing_time = -1;
    }
    if g.strafing_time >= 20 {
        if c.next_float() < 0.3 {
            g.clockwise = !g.clockwise;
        }
        if c.next_float() < 0.3 {
            g.backwards = !g.backwards;
        }
        g.strafing_time = 0;
    }
    if g.strafing_time > -1 {
        if d2 > f64::from(BOW_RADIUS_SQR * 0.75) {
            g.backwards = false;
        } else if d2 < f64::from(BOW_RADIUS_SQR * 0.25) {
            g.backwards = true;
        }
        m.move_ctl.strafe(
            if g.backwards { -0.5 } else { 0.5 },
            if g.clockwise { 0.5 } else { -0.5 },
        );
        look_at(m, &p, 30.0, 30.0);
    } else {
        m.look_ctl.set(p.eye(), 30.0, 30.0);
    }
    if let Some(ticks) = m.using_item {
        if !see && g.see_time < -60 {
            m.using_item = None;
        } else if see && ticks >= 20 {
            m.using_item = None;
            m.out.push(Out::Shoot {
                target: p.uuid,
                power: bow_power(ticks),
            });
            g.attack_time = bow_interval(c.s.difficulty);
        }
    } else {
        g.attack_time -= 1;
        if g.attack_time <= 0 && g.see_time >= -60 {
            m.using_item = Some(0);
        }
    }
}

/// `MeleeAttackGoal.tick` (+ `ZombieAttackGoal.tick`).
fn melee_tick(g: &mut Melee, m: &mut Mob, c: &mut Ctx) {
    if let Some(p) = m.target.and_then(|t| c.prey(t)) {
        m.look_ctl.set(p.eye(), 30.0, 30.0);
        g.recalc = (g.recalc - 1).max(0);
        if (g.follow || c.sees(m, &p))
            && g.recalc <= 0
            && (g.pathed == [0.0; 3] || dist2(p.pos, g.pathed) >= 1.0 || c.next_float() < 0.05)
        {
            g.pathed = p.pos;
            g.recalc = 4 + c.next_int(7);
            let d = dist2(m.pos, p.pos);
            if d > 1024.0 {
                g.recalc += 10;
            } else if d > 256.0 {
                g.recalc += 5;
            }
            if !move_to(m, c, p.pos, 0, g.speed) {
                g.recalc += 15;
            }
            // `adjustedTickDelay` у цели каждого тика — без изменений.
        }
        g.next_attack = (g.next_attack - 1).max(0);
        // `checkAndPerformAttack`.
        if g.next_attack <= 0 && super::in_reach(m, &p) && c.sees(m, &p) {
            g.next_attack = 20;
            m.strike = Some(p.uuid);
        }
    }
    if g.zombie {
        g.raise_arm += 1;
        m.aggressive = g.raise_arm >= 5 && g.next_attack < 10;
    }
}

/// Цели вида (`registerGoals`, `addBehaviourGoals`): движение и выбор цели.
pub fn for_species(species: Species) -> (Selector, Selector) {
    let info = species.info();
    let stroll_at = |p, speed| {
        (
            p,
            Goal::Stroll {
                speed,
                interval: 120,
                avoid_water: true,
                to: [0.0; 3],
            },
        )
    };
    let look = |p, range| {
        (
            p,
            Goal::LookAtPlayer {
                range,
                at: None,
                time: 0,
            },
        )
    };
    let around = |p| {
        (
            p,
            Goal::LookAround {
                rel: [0.0; 2],
                time: 0,
            },
        )
    };
    let hurt_by = |p, alert| {
        (
            p,
            Goal::HurtBy {
                alert,
                timestamp: 0,
                tg: TargetState::default(),
            },
        )
    };
    let nearest = |p, day_neutral| {
        (
            p,
            Goal::NearestPlayer {
                interval: reduced(10),
                day_neutral,
                found: None,
                tg: TargetState::default(),
            },
        )
    };
    match species {
        Species::Zombie | Species::Drowned => {
            let float = if species == Species::Drowned {
                vec![(0, Goal::Float)]
            } else {
                Vec::new()
            };
            return (
                Selector::new(
                    float
                        .into_iter()
                        .chain([
                            (1, Goal::BreakDoor(BreakDoor::new())),
                            (3, Goal::Melee(Box::new(Melee::new(1.0, true)))),
                            stroll_at(7, 1.0),
                            look(8, 8.0),
                            around(8),
                        ])
                        .collect(),
                ),
                Selector::new(vec![hurt_by(1, true), nearest(2, false)]),
            );
        }
        // `AvoidEntityGoal` от кошек (3) пропущен: кошек нет.
        Species::Creeper => {
            return (
                Selector::new(vec![
                    (1, Goal::Float),
                    (2, Goal::Swell { target: None }),
                    (4, Goal::Melee(Box::new(Melee::new(1.0, false)))),
                    stroll_at(5, 0.8),
                    look(6, 8.0),
                    around(6),
                ]),
                Selector::new(vec![nearest(1, false), hurt_by(2, false)]),
            );
        }
        // `AvoidEntityGoal` от волков (3) пропущен; лук в руке всегда, так что
        // вместо ближнего боя — `RangedBowAttackGoal`.
        Species::Skeleton => {
            return (
                Selector::new(vec![
                    (2, Goal::RestrictSun),
                    (
                        3,
                        Goal::FleeSun {
                            speed: 1.0,
                            to: [0.0; 3],
                        },
                    ),
                    (4, Goal::RangedBow(Box::new(RangedBow::new(1.0)))),
                    stroll_at(5, 1.0),
                    look(6, 8.0),
                    around(6),
                ]),
                Selector::new(vec![hurt_by(1, false), nearest(2, false)]),
            );
        }
        // `AvoidEntityGoal` от броненосцев (2) пропущен.
        Species::Spider => {
            let mut attack = Melee::new(1.0, false);
            attack.follow = true;
            attack.spider = true;
            return (
                Selector::new(vec![
                    (1, Goal::Float),
                    (
                        3,
                        Goal::Leap {
                            y: f64::from(0.4f32),
                            target: None,
                        },
                    ),
                    (4, Goal::Melee(Box::new(attack))),
                    stroll_at(5, 0.8),
                    look(6, 8.0),
                    around(6),
                ]),
                Selector::new(vec![hurt_by(1, false), nearest(2, true)]),
            );
        }
        // `AbstractCubeMob.registerGoals` + `Slime`: атака (2) после
        // направления и прыжков; цель — игрок, железный голем (3) пропущен.
        Species::Slime => {
            return (
                Selector::new(vec![
                    (1, Goal::CubeFloat),
                    (
                        4,
                        Goal::CubeRandomDir {
                            chosen: 0.0,
                            next: 0,
                        },
                    ),
                    (5, Goal::CubeKeepJumping),
                    (2, Goal::CubeAttack { tired: 0 }),
                ]),
                Selector::new(vec![nearest(1, false)]),
            );
        }
        // Рыба: без целей, только плавает на месте.
        Species::Cod => return (Selector::default(), Selector::default()),
        // `Piglin`/`Wolf`: для проб и взаимодействия достаточно прогулки.
        Species::Piglin
        | Species::Wolf
        | Species::IronGolem
        | Species::Phantom
        | Species::Pillager
        | Species::Wither => {
            return (
                Selector::new(vec![stroll_at(5, 1.0), look(6, 8.0), around(8)]),
                Selector::default(),
            );
        }
        _ => {}
    }
    // Животные: `Cow`, `Pig`, `Sheep`, `Chicken`. У овцы своя нумерация:
    // `EatBlockGoal` на 5, прогулка и взгляд сдвинуты на единицу.
    let sheep = species == Species::Sheep;
    let pig = u8::from(species == Species::Pig);
    let shift = u8::from(sheep) + pig;
    let mut goals = vec![
        (0, Goal::Float),
        (
            1,
            Goal::Panic {
                speed: info.panic_speed,
                to: [0.0; 3],
            },
        ),
        (
            2 + pig,
            Goal::Breed {
                speed: 1.0,
                partner: None,
                love_time: 0,
            },
        ),
        (
            3 + pig,
            Goal::Tempt {
                speed: info.tempt_speed,
                calm_down: 0,
                player: None,
            },
        ),
        (
            4 + pig,
            Goal::FollowParent {
                speed: info.follow_parent_speed,
                parent: None,
                recalc: 0,
            },
        ),
    ];
    if sheep {
        goals.push((5, Goal::EatBlock { tick: 0 }));
    }
    goals.push(stroll_at(5 + shift, 1.0));
    goals.push(look(6 + shift, 6.0));
    goals.push(around(7 + shift));
    (Selector::new(goals), Selector::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduced_delay() {
        assert_eq!(reduced(0), 0);
        assert_eq!(reduced(1), 1);
        assert_eq!(reduced(10), 5);
        assert_eq!(reduced(60), 30);
        assert_eq!(reduced(79), 40);
    }

    #[test]
    fn species_priorities_match_register_goals() {
        let pri = |s: Species| -> Vec<(u8, &'static str)> {
            for_species(s)
                .0
                .goals
                .iter()
                .map(|g| {
                    (
                        g.priority,
                        match g.goal {
                            Goal::Float => "float",
                            Goal::Panic { .. } => "panic",
                            Goal::Breed { .. } => "breed",
                            Goal::Tempt { .. } => "tempt",
                            Goal::FollowParent { .. } => "follow",
                            Goal::EatBlock { .. } => "eat",
                            Goal::Stroll { .. } => "stroll",
                            Goal::LookAtPlayer { .. } => "look",
                            Goal::LookAround { .. } => "around",
                            Goal::Melee(_) => "melee",
                            Goal::Swell { .. } => "swell",
                            Goal::RangedBow(_) => "bow",
                            Goal::RestrictSun => "restrict_sun",
                            Goal::FleeSun { .. } => "flee_sun",
                            Goal::Leap { .. } => "leap",
                            Goal::CubeFloat => "cube_float",
                            Goal::CubeAttack { .. } => "cube_attack",
                            Goal::CubeRandomDir { .. } => "cube_direction",
                            Goal::CubeKeepJumping => "cube_jump",
                            Goal::BreakDoor(_) => "break_door",
                            _ => "?",
                        },
                    )
                })
                .collect()
        };
        assert_eq!(
            pri(Species::Cow),
            [
                (0, "float"),
                (1, "panic"),
                (2, "breed"),
                (3, "tempt"),
                (4, "follow"),
                (5, "stroll"),
                (6, "look"),
                (7, "around")
            ]
        );
        assert_eq!(
            pri(Species::Pig),
            [
                (0, "float"),
                (1, "panic"),
                (3, "breed"),
                (4, "tempt"),
                (5, "follow"),
                (6, "stroll"),
                (7, "look"),
                (8, "around")
            ]
        );
        assert_eq!(
            pri(Species::Sheep),
            [
                (0, "float"),
                (1, "panic"),
                (2, "breed"),
                (3, "tempt"),
                (4, "follow"),
                (5, "eat"),
                (6, "stroll"),
                (7, "look"),
                (8, "around")
            ]
        );
        assert_eq!(
            pri(Species::Zombie),
            [
                (1, "break_door"),
                (3, "melee"),
                (7, "stroll"),
                (8, "look"),
                (8, "around")
            ]
        );
        assert_eq!(
            pri(Species::Creeper),
            [
                (1, "float"),
                (2, "swell"),
                (4, "melee"),
                (5, "stroll"),
                (6, "look"),
                (6, "around")
            ]
        );
        assert_eq!(
            pri(Species::Skeleton),
            [
                (2, "restrict_sun"),
                (3, "flee_sun"),
                (4, "bow"),
                (5, "stroll"),
                (6, "look"),
                (6, "around")
            ]
        );
        assert_eq!(
            pri(Species::Spider),
            [
                (1, "float"),
                (3, "leap"),
                (4, "melee"),
                (5, "stroll"),
                (6, "look"),
                (6, "around")
            ]
        );
        assert_eq!(
            pri(Species::Slime),
            [
                (1, "cube_float"),
                (4, "cube_direction"),
                (5, "cube_jump"),
                (2, "cube_attack")
            ]
        );
    }

    #[test]
    fn break_door_goal_breaks_wooden_door_on_hard() {
        let shape = |_: [i32; 3]| crate::mobs::FULL;
        let water = |_: [i32; 3]| false;
        let cell = |p: [i32; 3]| path::Cell {
            kind: if p == [0, 65, 0] {
                path::PathType::DoorWoodClosed
            } else {
                path::PathType::Open
            },
            flags: 0,
        };
        let w = World {
            shape: &shape,
            water: &water,
            cell: Some(&cell),
            info: None,
            min_y: -64,
        };
        let mut m = Mob::new(Species::Zombie, [0.5, 64.0, 0.5], 1);
        m.can_break_doors = true;
        m.horizontal_collision = true;
        m.nav.path = Some(path::Path {
            nodes: vec![([0, 64, 0], path::PathType::WalkableDoor)],
            next: 0,
            target: [0, 64, 0],
            reached: false,
            dist_to_target: 0.0,
        });
        let mut goals = Selector::new(vec![(1, Goal::BreakDoor(BreakDoor::new()))]);
        let sight = |_: [f64; 3], _: [f64; 3]| true;
        let mut rnd = || 0.5;
        let mut s = Senses {
            players: &[],
            rand: &mut rnd,
            prey: &[],
            tempters: &[],
            kin: &[],
            sight: &sight,
            difficulty: 3,
            entity_id: 1,
            bright_outside: false,
            mob_griefing: true,
            jump_boost: None,
        };
        let mut c = Ctx { w: &w, s: &mut s };
        let mut last = -1;
        let mut broken = false;
        for _ in 0..240 {
            m.out.clear();
            goals.tick(&mut m, &mut c);
            for out in &m.out {
                match out {
                    Out::DoorBreakProgress { progress, .. } => last = *progress,
                    Out::DoorBroken(pos) => broken = *pos == [0, 65, 0],
                    _ => {}
                }
            }
        }
        assert_eq!(last, 10);
        assert!(broken);
        let Goal::BreakDoor(g) = &goals.goals[0].goal else {
            panic!("цель сменилась");
        };
        assert_eq!(g.break_time, 240);
    }
}
