//! Инвентарь игрока и выживание: здоровье, урон от падения и пустоты, смерть.
//! Логика без сети: сессия применяет результат и шлёт пакеты.

use chunkwake_data::tools::Tool;
use chunkwake_protocol::inventory::{Slot, Stack};

/// Слоты окна инвентаря игрока: 0 — результат крафта, 1–4 — сетка, 5–8 — броня,
/// 9–35 — основной, 36–44 — хотбар, 45 — вторая рука.
pub const SLOTS: usize = 46;
const HOTBAR: usize = 36;
/// Нагрудник: элитры носят здесь.
pub const CHEST: usize = 6;
pub const OFFHAND: usize = 45;
const MAIN: usize = 9;

#[derive(Clone, Debug)]
pub struct Inventory {
    pub slots: [Slot; SLOTS],
    pub carried: Slot,
    /// Выбранная ячейка хотбара 0..9.
    pub selected: usize,
    /// `stateId` окна: растёт при каждом изменении со стороны сервера.
    pub state: i32,
    /// Перетаскивание по слотам (между пакетами клика).
    pub drag: crate::click::Drag,
}

impl Default for Inventory {
    fn default() -> Self {
        Self {
            slots: [None; SLOTS],
            carried: None,
            selected: 0,
            state: 0,
            drag: Default::default(),
        }
    }
}

impl Inventory {
    pub fn held_slot(&self) -> usize {
        HOTBAR + self.selected
    }

    pub fn held(&self) -> Slot {
        self.slots[self.held_slot()]
    }

    /// Средняя кнопка: выбрать такой предмет или, в креативе, положить один в руку.
    /// `None` — предмета нет и создавать его не нужно. Пустой список — сменилась только ячейка.
    pub fn pick(&mut self, item: i32, creative: bool) -> Option<Vec<usize>> {
        if let Some(i) = (0..9).find(|&i| self.slots[HOTBAR + i].is_some_and(|s| s.item == item)) {
            self.selected = i;
            return Some(Vec::new());
        }
        if let Some(i) = (MAIN..HOTBAR)
            .chain(std::iter::once(OFFHAND))
            .find(|&i| self.slots[i].is_some_and(|s| s.item == item))
        {
            let held = self.held_slot();
            self.slots.swap(i, held);
            return Some(vec![i, held]);
        }
        if !creative {
            return None;
        }
        if let Some(i) = (0..9).find(|&i| self.slots[HOTBAR + i].is_none()) {
            self.selected = i;
            self.slots[HOTBAR + i] = Some(Stack::new(item, 1));
            return Some(vec![HOTBAR + i]);
        }
        let held = self.held_slot();
        let mut changed = vec![held];
        if let Some(old) = self.slots[held]
            && let Some(free) = Self::storage_order().find(|&i| self.slots[i].is_none())
        {
            self.slots[free] = Some(old);
            changed.push(free);
        }
        self.slots[held] = Some(Stack::new(item, 1));
        Some(changed)
    }

    /// Порядок заполнения как в ванилле: хотбар, затем основной инвентарь.
    fn storage_order() -> impl Iterator<Item = usize> {
        (HOTBAR..HOTBAR + 9).chain(MAIN..HOTBAR)
    }

    /// Кладёт предметы: сначала доливает такие же стопки, потом в пустые
    /// слоты. Возвращает изменённые слоты; не влезшее теряется.
    /// Кладёт предметы в инвентарь: изменённые слоты и сколько не влезло.
    pub fn add(&mut self, item: i32, mut count: i32, max_stack: i32) -> (Vec<usize>, i32) {
        let mut changed = Vec::new();
        for i in Self::storage_order() {
            if count == 0 {
                break;
            }
            if let Some(s) = &mut self.slots[i]
                && s.item == item
                && s.damage == 0
                && s.extra == 0
                && s.count < max_stack
            {
                let n = count.min(max_stack - s.count);
                s.count += n;
                count -= n;
                changed.push(i);
            }
        }
        for i in Self::storage_order() {
            if count == 0 {
                break;
            }
            if self.slots[i].is_none() {
                let n = count.min(max_stack);
                self.slots[i] = Some(Stack {
                    item,
                    count: n,
                    damage: 0,
                    extra: 0,
                });
                count -= n;
                changed.push(i);
            }
        }
        (changed, count)
    }

    /// Кладёт стопку как есть (с износом и компонентами): такая — только в пустой слот.
    /// Возвращает не влезшее.
    pub fn add_stack(&mut self, s: Stack, max_stack: i32) -> Option<Stack> {
        if s.damage == 0 && s.extra == 0 {
            let (_, left) = self.add(s.item, s.count, max_stack);
            return (left > 0).then_some(Stack { count: left, ..s });
        }
        match Self::storage_order().find(|&i| self.slots[i].is_none()) {
            Some(i) => {
                self.slots[i] = Some(s);
                None
            }
            None => Some(s),
        }
    }

    /// Тратит один предмет из руки; `false` — рука пуста.
    pub fn consume_held(&mut self) -> bool {
        let slot = &mut self.slots[HOTBAR + self.selected];
        match slot {
            Some(s) => {
                s.count -= 1;
                if s.count <= 0 {
                    *slot = None;
                }
                true
            }
            None => false,
        }
    }

    pub fn set(&mut self, slot: i16, stack: Slot) -> bool {
        if (0..SLOTS as i16).contains(&slot) {
            self.slots[slot as usize] = stack;
            true
        } else {
            false
        }
    }

    pub fn next_state(&mut self) -> i32 {
        self.state = (self.state + 1) & 0x7fff;
        self.state
    }
}

pub const MAX_HEALTH: f32 = 20.0;
pub const MAX_FOOD: i32 = 20;
pub const START_SATURATION: f32 = 5.0;

/// Ниже этой глубины под миром — урон пустотой.
pub const VOID_DEPTH: f64 = 64.0;
pub const VOID_DAMAGE: f32 = 4.0;
/// Урон пустотой — не чаще раза в 10 тиков.
pub const VOID_INTERVAL_TICKS: u64 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    Fall,
    Void,
    Starve,
    /// Смерть без урона (`Player.setHealth(0)` плагином).
    Generic,
    /// Отравление (`minecraft:magic`, здоровье не падает до нуля).
    Poison,
    /// Иссушение (`minecraft:wither`).
    Wither,
    /// Мгновенный урон (`minecraft:magic`).
    Magic,
}

impl Cause {
    /// Ключ сообщения о смерти и тип урона в реестре `damage_type`.
    pub fn death_key(self) -> &'static str {
        match self {
            Cause::Fall => "death.attack.fall",
            Cause::Void => "death.attack.outOfWorld",
            Cause::Starve => "death.attack.starve",
            Cause::Generic => "death.attack.generic",
            Cause::Poison | Cause::Magic => "death.attack.magic",
            Cause::Wither => "death.attack.wither",
        }
    }

    /// `EntityDamageEvent.DamageCause` Bukkit.
    pub fn bukkit(self) -> &'static str {
        match self {
            Cause::Fall => "FALL",
            Cause::Void => "VOID",
            Cause::Starve => "STARVATION",
            Cause::Generic => "CUSTOM",
            Cause::Poison => "POISON",
            Cause::Wither => "WITHER",
            Cause::Magic => "MAGIC",
        }
    }

    /// Ключ в реестре `damage_type`.
    pub fn damage_type(self) -> &'static str {
        match self {
            Cause::Fall => "minecraft:fall",
            Cause::Void => "minecraft:out_of_world",
            Cause::Starve => "minecraft:starve",
            Cause::Generic => "minecraft:generic",
            Cause::Poison | Cause::Magic => "minecraft:magic",
            Cause::Wither => "minecraft:wither",
        }
    }

    /// Пустота (`bypasses_resistance`) и голод (`bypasses_effects`) не режутся Сопротивлением.
    pub fn bypasses_resistance(self) -> bool {
        matches!(self, Cause::Void | Cause::Starve)
    }
}

/// Годится ли предмет для дропа блока (`Player.hasCorrectToolForDrops`).
pub fn correct_tool(requires_tool: bool, tool: Option<&Tool>, block: &str) -> bool {
    !requires_tool || tool.is_some_and(|t| t.correct_for_drops(block))
}

/// Прогресс ломки за тик (`BlockBehaviour.getDestroyProgress` и
/// `Player.getDestroySpeed`; `dig` — множитель эффектов, `efficiency` — уровень чар):
/// ≥ 1 — блок ломается сразу.
#[allow(clippy::too_many_arguments)]
pub fn destroy_progress(
    hardness: f32,
    requires_tool: bool,
    tool: Option<&Tool>,
    block: &str,
    in_water: bool,
    on_ground: bool,
    dig: f32,
    efficiency: i32,
) -> f32 {
    if hardness == -1.0 {
        return 0.0;
    }
    let mut speed = tool.map_or(1.0, |t| t.speed(block));
    // Эффективность (`mining_efficiency`): +lvl²+1, если инструмент вообще ускоряет.
    if speed > 1.0 && efficiency > 0 {
        speed += (efficiency * efficiency + 1) as f32;
    }
    speed *= dig;
    // `submerged_mining_speed` по умолчанию 0.2 (подводник на шлеме — 1); в воздухе — в 5 раз медленнее.
    if in_water {
        speed *= 0.2;
    }
    if !on_ground {
        speed /= 5.0;
    }
    let divisor = if correct_tool(requires_tool, tool, block) {
        30.0
    } else {
        100.0
    };
    speed / hardness / divisor
}

/// Сложность (`difficulty` в `server.properties`): от неё зависит голод.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Difficulty {
    Peaceful,
    Easy,
    Normal,
    Hard,
}

impl Difficulty {
    pub const fn id(self) -> u8 {
        match self {
            Self::Peaceful => 0,
            Self::Easy => 1,
            Self::Normal => 2,
            Self::Hard => 3,
        }
    }

    pub const fn from_id(id: u8) -> Self {
        match id {
            0 => Self::Peaceful,
            1 => Self::Easy,
            3 => Self::Hard,
            _ => Self::Normal,
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "peaceful" | "0" => Self::Peaceful,
            "easy" | "1" => Self::Easy,
            "normal" | "2" => Self::Normal,
            "hard" | "3" => Self::Hard,
            _ => return None,
        })
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Peaceful => "peaceful",
            Self::Easy => "easy",
            Self::Normal => "normal",
            Self::Hard => "hard",
        }
    }
}

/// Истощение (как в ванилле): за метр спринта, прыжок, прыжок со спринтом,
/// ломку блока, восстановление единицы здоровья.
pub const EXHAUSTION_SPRINT: f32 = 0.1;
pub const EXHAUSTION_JUMP: f32 = 0.05;
pub const EXHAUSTION_SPRINT_JUMP: f32 = 0.2;
pub const EXHAUSTION_BREAK: f32 = 0.005;
const EXHAUSTION_HEAL: f32 = 6.0;
/// Каждые 4 единицы истощения — минус насыщение, а без него — минус еда.
const EXHAUSTION_STEP: f32 = 4.0;

/// Итог тика голода.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FoodTick {
    /// Здоровье, еда или насыщение изменились: отправить `set_health`.
    pub changed: bool,
    /// Урон от голода.
    pub starve: Option<f32>,
}

/// Активный эффект (`MobEffectInstance`): ID в `mob_effect`, усиление,
/// оставшиеся тики (-1 — бесконечный), фон маяка, частицы и значок.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Effect {
    pub id: i32,
    pub amplifier: u8,
    pub duration: i32,
    pub ambient: bool,
    pub particles: bool,
    pub show_icon: bool,
    /// Тики с наложения: для бесконечных интервал считается отсюда, на диск не пишется.
    pub age: u32,
}

impl Effect {
    /// Обычное наложение: частицы и значок, без фона маяка.
    pub fn plain(id: i32, amplifier: u8, duration: i32) -> Self {
        Self {
            id,
            amplifier,
            duration,
            ambient: false,
            particles: true,
            show_icon: true,
            age: 0,
        }
    }

    /// Флаги пакета `update_mob_effect`: 1 — фон, 2 — частицы, 4 — значок.
    pub fn flags(&self) -> u8 {
        let mut f = 0;
        if self.ambient {
            f |= 1;
        }
        if self.particles {
            f |= 2;
        }
        if self.show_icon {
            f |= 4;
        }
        f
    }
}

/// Скорость движения игрока без модификаторов (`Player.createAttributes`).
pub const BASE_MOVEMENT_SPEED: f64 = 0.1;

/// Модификаторы `movement_speed`: бег и эффекты Скорость/Замедление
/// (`MobEffects`: +20% и −15% от итога за уровень).
pub fn speed_modifiers(
    sprinting: bool,
    speed: Option<u8>,
    slowness: Option<u8>,
) -> Vec<(String, f64, i32)> {
    let mut out = Vec::new();
    if sprinting {
        out.push(("minecraft:sprinting".to_owned(), 0.3, 2));
    }
    if let Some(a) = speed {
        out.push((
            "minecraft:effect.speed".to_owned(),
            0.2 * (a as f64 + 1.0),
            2,
        ));
    }
    if let Some(a) = slowness {
        out.push((
            "minecraft:effect.slowness".to_owned(),
            -0.15 * (a as f64 + 1.0),
            2,
        ));
    }
    out
}

/// Множитель скорости ломки от эффектов (`Player.getDestroySpeed`):
/// `haste` — сильнейшее из Спешки и Силы источника, `fatigue` — Утомление.
pub fn dig_multiplier(haste: Option<u8>, fatigue: Option<u8>) -> f32 {
    let mut m = haste.map_or(1.0, |a| 1.0 + (a as f32 + 1.0) * 0.2);
    if let Some(a) = fatigue {
        m *= match a {
            0 => 0.3,
            1 => 0.09,
            2 => 0.0027,
            _ => 8.1e-4,
        };
    }
    m
}

/// Стартовая скорость прыжка (`Attributes.JUMP_STRENGTH` игрока).
pub const BASE_JUMP_STRENGTH: f64 = 0.42;
/// Гравитация игрока без эффектов.
pub const BASE_GRAVITY: f64 = 0.08;

/// Прыжок: `jump_strength` +0.1 за уровень, операция add.
pub fn jump_modifiers(jump: Option<u8>) -> Vec<(String, f64, i32)> {
    jump.map(|a| {
        vec![(
            "minecraft:effect.jump_boost".to_owned(),
            0.1 * (a as f64 + 1.0),
            0,
        )]
    })
    .unwrap_or_default()
}

/// Плавное падение: `gravity` −0.07, от уровня не зависит (в 1.20.5+ клиент
/// всё равно ограничивает падение 0.01).
pub fn gravity_modifiers(slow_falling: bool) -> Vec<(String, f64, i32)> {
    if slow_falling {
        vec![("minecraft:effect.slow_falling".to_owned(), -0.07, 0)]
    } else {
        Vec::new()
    }
}

/// Интервал тика (`shouldApplyEffectTickThisTick`): `base >> amplifier`,
/// а если сдвиг обнулил число — каждый тик.
pub fn effect_interval(base: i32, amplifier: u8) -> i32 {
    let i = base >> i32::from(amplifier.min(31));
    if i > 0 { i } else { 1 }
}

/// Пора ли пульсировать. Конечная длительность — по оставшимся тикам,
/// бесконечная (`-1`) — по `age` с момента наложения.
pub fn effect_due(duration: i32, age: u32, base: i32, amplifier: u8) -> bool {
    let i = effect_interval(base, amplifier);
    if duration < 0 {
        age.is_multiple_of(i as u32)
    } else {
        duration > 0 && duration % i == 0
    }
}

/// Мгновенный эффект в момент наложения (иконка потом просто тикает).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Instant {
    None,
    Heal(f32),
    Harm(f32),
    /// Еда и сытость. Ванильный `FoodData.eat(уровень, 1)` добавляет сытость `уровень * 2`.
    Eat(i32, f32),
}

pub fn instant_effect(name: &str, amplifier: u8) -> Instant {
    // `4 << amplifier` / `6 << amplifier`; сдвиг ограничен, чтобы не переполнить `u32`.
    let shift = u32::from(amplifier.min(20));
    let scaled = |base: u32| (base << shift) as f32;
    match name {
        "minecraft:instant_health" => Instant::Heal(scaled(4)),
        "minecraft:instant_damage" => Instant::Harm(scaled(6)),
        "minecraft:saturation" => {
            let n = i32::from(amplifier) + 1;
            Instant::Eat(n, (n * 2) as f32)
        }
        _ => Instant::None,
    }
}

/// Сопротивление: `урон * max(0, 1 − 0.2 * (уровень))`. `bypass` — пустота и голод.
pub fn resisted(amount: f32, amplifier: Option<u8>, bypass: bool) -> f32 {
    if bypass {
        return amount;
    }
    match amplifier {
        Some(a) => amount * (1.0 - 0.2 * (f32::from(a) + 1.0)).max(0.0),
        None => amount,
    }
}

/// Отравление бьёт на 1, только пока здоровье строго больше 1, и не опускает его ниже 1.
pub fn poison_hit(health: f32) -> Option<f32> {
    (health > 1.0).then(|| (health - 1.0).min(1.0))
}

/// `MobEffectInstance.update`: сильнее заменяет, равный по силе продлевает.
/// `force` — заменить в любом случае (`addPotionEffect(effect, true)`).
/// `false` — не применён.
pub fn will_apply(effects: &[Effect], e: Effect, force: bool) -> bool {
    match effects.iter().find(|x| x.id == e.id) {
        Some(x) => {
            let longer = x.duration != -1 && (e.duration == -1 || e.duration > x.duration);
            force || e.amplifier > x.amplifier || (e.amplifier == x.amplifier && longer)
        }
        None => true,
    }
}

pub fn add_effect(effects: &mut Vec<Effect>, e: Effect, force: bool) -> bool {
    if !will_apply(effects, e, force) {
        return false;
    }
    if let Some(x) = effects.iter_mut().find(|x| x.id == e.id) {
        *x = e;
    } else {
        effects.push(e);
    }
    true
}

/// Какие эффекты из реестра дают периодический тик.
#[derive(Clone, Copy, Debug, Default)]
pub struct EffectKinds {
    pub regeneration: Option<i32>,
    pub poison: Option<i32>,
    pub wither: Option<i32>,
    pub hunger: Option<i32>,
}

/// Итог тика эффектов: лечение, флаги урона, истощение и истёкшие.
#[derive(Clone, Debug, Default)]
pub struct EffectTick {
    pub expired: Vec<Effect>,
    pub heal: f32,
    pub poison: bool,
    pub wither: bool,
    pub exhaustion: f32,
}

#[derive(Clone, Debug)]
pub struct Vitals {
    pub health: f32,
    pub food: i32,
    pub saturation: f32,
    /// Пройденное вниз расстояние в текущем падении.
    fall: f64,
    last_pos: Option<[f64; 3]>,
    on_ground: bool,
    pub sprinting: bool,
    pub exhaustion: f32,
    /// Счётчик тиков регенерации или голодания.
    food_timer: u32,
    pub dead: bool,
    pub effects: Vec<Effect>,
}

impl Default for Vitals {
    fn default() -> Self {
        Self {
            health: MAX_HEALTH,
            food: MAX_FOOD,
            saturation: START_SATURATION,
            fall: 0.0,
            last_pos: None,
            on_ground: true,
            sprinting: false,
            exhaustion: 0.0,
            food_timer: 0,
            dead: false,
            effects: Vec::new(),
        }
    }
}

impl Vitals {
    /// Движение игрока: падение и истощение от спринта и прыжков. Возвращает
    /// урон от падения при приземлении: `ceil(высота − 3)`, как в ванилле.
    /// Плавное падение урон обнуляет.
    pub fn on_move(
        &mut self,
        pos: Option<[f64; 3]>,
        on_ground: bool,
        takes_damage: bool,
        slow_falling: bool,
    ) -> Option<f32> {
        if let Some(p) = pos {
            if let Some(last) = self.last_pos {
                if p[1] < last[1] {
                    self.fall += last[1] - p[1];
                }
                if self.sprinting {
                    let (dx, dz) = (p[0] - last[0], p[2] - last[2]);
                    self.exhaust(EXHAUSTION_SPRINT * (dx * dx + dz * dz).sqrt() as f32);
                }
                // Прыжок: оторвались от земли, двигаясь вверх.
                if self.on_ground && !on_ground && p[1] > last[1] {
                    self.exhaust(if self.sprinting {
                        EXHAUSTION_SPRINT_JUMP
                    } else {
                        EXHAUSTION_JUMP
                    });
                }
            }
            self.last_pos = Some(p);
        }
        self.on_ground = on_ground;
        if !on_ground {
            return None;
        }
        let fall = std::mem::take(&mut self.fall);
        let damage = (fall - 3.0).ceil() as f32;
        (takes_damage && !slow_falling && damage > 0.0).then_some(damage)
    }

    /// Применяет урон; `true` — игрок умер.
    pub fn on_ground(&self) -> bool {
        self.on_ground
    }

    pub fn damage(&mut self, amount: f32) -> bool {
        if self.dead {
            return false;
        }
        self.health = (self.health - amount).max(0.0);
        self.dead = self.health <= 0.0;
        self.dead
    }

    pub fn exhaust(&mut self, amount: f32) {
        self.exhaustion = (self.exhaustion + amount).min(40.0);
    }

    /// Тик эффектов: пульс регенерации, яда, иссушения и голода, затем минус тик.
    /// Истёкшие отдаются с длительностью 1 — их можно вернуть, если событие отменено.
    pub fn tick_effects(&mut self, kinds: EffectKinds) -> EffectTick {
        let mut out = EffectTick::default();
        let mut gone = Vec::new();
        for e in &mut self.effects {
            if kinds.regeneration == Some(e.id) && effect_due(e.duration, e.age, 50, e.amplifier) {
                out.heal += 1.0;
            }
            if kinds.poison == Some(e.id) && effect_due(e.duration, e.age, 25, e.amplifier) {
                out.poison = true;
            }
            if kinds.wither == Some(e.id) && effect_due(e.duration, e.age, 40, e.amplifier) {
                out.wither = true;
            }
            if kinds.hunger == Some(e.id) {
                out.exhaustion += 0.005 * (f32::from(e.amplifier) + 1.0);
            }
            if e.duration < 0 {
                e.age = e.age.wrapping_add(1);
            } else if e.duration > 0 {
                if e.duration == 1 {
                    gone.push(*e);
                }
                e.duration -= 1;
            }
        }
        if !gone.is_empty() {
            self.effects.retain(|e| e.duration != 0);
        }
        out.expired = gone;
        out
    }

    /// Новый эффект (`MobEffectInstance.update`): сильнее заменяет, равный продлевает.
    /// `force` заменяет даже более слабый. `false` — не применён.
    pub fn add_effect(&mut self, e: Effect, force: bool) -> bool {
        add_effect(&mut self.effects, e, force)
    }

    pub fn effect(&self, id: i32) -> Option<&Effect> {
        self.effects.iter().find(|e| e.id == id)
    }

    pub fn tick(&mut self, difficulty: Difficulty) -> FoodTick {
        let mut out = FoodTick::default();
        if self.dead {
            return out;
        }
        if self.exhaustion > EXHAUSTION_STEP {
            self.exhaustion -= EXHAUSTION_STEP;
            if self.saturation > 0.0 {
                self.saturation = (self.saturation - 1.0).max(0.0);
                out.changed = true;
            } else if difficulty != Difficulty::Peaceful {
                self.food = (self.food - 1).max(0);
                out.changed = true;
            }
        }
        let hurt = self.health < MAX_HEALTH;
        if hurt && self.saturation > 0.0 && self.food >= MAX_FOOD {
            // Быстрая регенерация при полной еде: раз в 10 тиков, за счёт насыщения.
            self.food_timer += 1;
            if self.food_timer >= 10 {
                let amount = self.saturation.min(EXHAUSTION_HEAL);
                self.heal(amount / EXHAUSTION_HEAL);
                self.exhaust(amount);
                self.food_timer = 0;
                out.changed = true;
            }
        } else if hurt && self.food >= 18 {
            self.food_timer += 1;
            if self.food_timer >= 80 {
                self.heal(1.0);
                self.exhaust(EXHAUSTION_HEAL);
                self.food_timer = 0;
                out.changed = true;
            }
        } else if self.food <= 0 {
            self.food_timer += 1;
            if self.food_timer >= 80 {
                let starves = match difficulty {
                    Difficulty::Hard => true,
                    Difficulty::Normal => self.health > 1.0,
                    _ => self.health > 10.0,
                };
                if starves {
                    out.starve = Some(1.0);
                }
                self.food_timer = 0;
            }
        } else {
            self.food_timer = 0;
        }
        out
    }

    /// Съеденная еда (`FoodData.eat`): насыщение не выше уровня еды.
    pub fn eat(&mut self, nutrition: i32, saturation: f32) {
        self.food = (self.food + nutrition).clamp(0, MAX_FOOD);
        self.saturation = (self.saturation + saturation).clamp(0.0, self.food as f32);
    }

    pub fn heal(&mut self, amount: f32) {
        self.health = (self.health + amount).min(MAX_HEALTH);
    }

    pub fn respawn(&mut self) {
        *self = Self::default();
    }

    /// Телепорт (вход, респаун): падение с прежней высоты не считается.
    pub fn reset_fall(&mut self, pos: [f64; 3]) {
        self.fall = 0.0;
        self.last_pos = Some(pos);
        self.on_ground = true;
    }
}

/// Сохранённое состояние игрока (`playerdata`): слоты — индексы окна игрока.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerSave {
    pub pos: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub health: f32,
    pub food: i32,
    pub saturation: f32,
    pub exhaustion: f32,
    pub selected: usize,
    pub slots: Vec<(usize, Stack)>,
    /// Режим игры (`GameType`); `None` — режим сервера.
    pub game_mode: Option<u8>,
    /// Активные эффекты (`active_effects`).
    pub effects: Vec<Effect>,
    pub xp: crate::xp::Experience,
}

/// Живое состояние игрока в сессии: инвентарь и здоровье.
#[derive(Default)]
pub struct PlayerState {
    pub inv: Inventory,
    pub vitals: Vitals,
    pub game_mode: Option<u8>,
    pub xp: crate::xp::Experience,
}

impl PlayerState {
    pub fn restore(s: &PlayerSave) -> Self {
        let mut st = Self::default();
        for &(i, stack) in &s.slots {
            if i < SLOTS && stack.count > 0 {
                st.inv.slots[i] = Some(stack);
            }
        }
        st.inv.selected = s.selected.min(8);
        st.game_mode = s.game_mode.filter(|&m| m <= 3);
        st.vitals.health = s.health.clamp(0.0, MAX_HEALTH);
        st.vitals.food = s.food.clamp(0, MAX_FOOD);
        st.vitals.saturation = s.saturation.clamp(0.0, st.vitals.food as f32);
        st.vitals.exhaustion = s.exhaustion.clamp(0.0, 40.0);
        st.vitals.effects = s.effects.clone();
        st.xp = s.xp;
        st
    }

    pub fn save(&self, pos: [f64; 3], yaw: f32, pitch: f32) -> PlayerSave {
        let v = &self.vitals;
        PlayerSave {
            pos,
            yaw,
            pitch,
            health: v.health,
            food: v.food,
            saturation: v.saturation,
            exhaustion: v.exhaustion,
            selected: self.inv.selected,
            game_mode: self.game_mode,
            effects: v.effects.clone(),
            xp: self.xp,
            slots: (0..SLOTS)
                .filter_map(|i| self.inv.slots[i].map(|s| (i, s)))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Время ломки в тиках, как у ванилы: прогресс копится до 1.
    fn ticks(p: f32) -> u32 {
        (1.0 / p).ceil() as u32
    }

    #[test]
    fn destroy_times_match_vanilla() {
        use chunkwake_data::tools::ToolRule;
        use std::sync::Arc;
        let pickaxe = Tool {
            damage_per_block: 1,
            default_speed: 1.0,
            rules: vec![ToolRule {
                blocks: Arc::new(["minecraft:stone".to_owned()].into()),
                speed: Some(2.0),
                correct_for_drops: Some(true),
            }],
        };
        let stone = |tool, water, ground| {
            destroy_progress(1.5, true, tool, "minecraft:stone", water, ground, 1.0, 0)
        };
        // Камень рукой — 7.5 с, деревянной киркой — 1.15 с (23 тика).
        assert_eq!(ticks(stone(None, false, true)), 150);
        assert_eq!(ticks(stone(Some(&pickaxe), false, true)), 23);
        // Эффективность V: 2 + 26 = 28 — два тика; рукой не действует.
        let eff = |tool| destroy_progress(1.5, true, tool, "minecraft:stone", false, true, 1.0, 5);
        assert_eq!(ticks(eff(Some(&pickaxe))), 2);
        assert_eq!(ticks(eff(None)), 150);
        // Под водой и в прыжке — в 5 раз медленнее каждое.
        assert_eq!(ticks(stone(Some(&pickaxe), true, true)), 113);
        assert_eq!(ticks(stone(Some(&pickaxe), false, false)), 113);
        // Земля рукой (инструмент не нужен) — 0.75 с; трава — мгновенно; бедрок — никогда.
        assert_eq!(
            ticks(destroy_progress(
                0.5,
                false,
                None,
                "minecraft:dirt",
                false,
                true,
                1.0,
                0
            )),
            15
        );
        assert!(
            destroy_progress(
                0.0,
                false,
                None,
                "minecraft:short_grass",
                false,
                true,
                1.0,
                0
            ) >= 1.0
        );
        // Спешка II — ×1.4, Утомление I — ×0.3.
        assert!((dig_multiplier(Some(1), None) - 1.4).abs() < 1e-6);
        assert!((dig_multiplier(None, Some(0)) - 0.3).abs() < 1e-6);
        assert_eq!(
            destroy_progress(-1.0, false, None, "minecraft:bedrock", false, true, 1.0, 0),
            0.0
        );
        assert!(!correct_tool(true, None, "minecraft:stone"));
        assert!(correct_tool(true, Some(&pickaxe), "minecraft:stone"));
        assert!(correct_tool(false, None, "minecraft:dirt"));
    }

    #[test]
    fn pick_selects_existing_or_gives_one_in_creative() {
        let mut inv = Inventory::default();
        assert_eq!(inv.pick(5, true), Some(vec![HOTBAR]));
        assert_eq!(inv.selected, 0);
        assert_eq!(inv.held(), Some(Stack::new(5, 1)));
        inv.selected = 3;
        assert_eq!(inv.pick(5, true), Some(vec![]));
        assert_eq!(inv.selected, 0);

        let mut inv = Inventory::default();
        inv.slots[MAIN] = Some(Stack::new(8, 2));
        assert_eq!(inv.pick(8, false), Some(vec![MAIN, HOTBAR]));
        assert_eq!(inv.held(), Some(Stack::new(8, 2)));
        assert!(inv.slots[MAIN].is_none());
        assert!(inv.pick(9, false).is_none());
    }

    #[test]
    fn add_merges_then_fills_hotbar_first() {
        let mut inv = Inventory::default();
        inv.slots[MAIN] = Some(Stack {
            item: 1,
            count: 60,
            damage: 0,
            extra: 0,
        });
        let changed = inv.add(1, 10, 64);
        assert_eq!(changed, (vec![MAIN, HOTBAR], 0));
        assert_eq!(inv.slots[MAIN].unwrap().count, 64);
        assert_eq!(
            inv.slots[HOTBAR],
            Some(Stack {
                item: 1,
                count: 6,
                damage: 0,
                extra: 0
            })
        );
        // Неполные стопки (макс. 1): каждая в свой слот.
        let changed = inv.add(2, 2, 1);
        assert_eq!(changed, (vec![HOTBAR + 1, HOTBAR + 2], 0));
    }

    #[test]
    fn consume_held_empties_slot() {
        let mut inv = Inventory {
            selected: 2,
            ..Default::default()
        };
        inv.slots[HOTBAR + 2] = Some(Stack {
            item: 5,
            count: 1,
            damage: 0,
            extra: 0,
        });
        assert!(inv.consume_held());
        assert_eq!(inv.held(), None);
        assert!(!inv.consume_held());
    }

    #[test]
    fn fall_damage_on_landing() {
        let mut v = Vitals::default();
        let at = |y| Some([0.0, y, 0.0]);
        v.reset_fall([0.0, 100.0, 0.0]);
        assert_eq!(v.on_move(at(95.0), false, true, false), None);
        assert_eq!(v.on_move(at(90.0), false, true, false), None);
        // 10 блоков: 7 урона.
        assert_eq!(v.on_move(at(90.0), true, true, false), Some(7.0));
        // Падение сбросилось.
        assert_eq!(v.on_move(at(90.0), true, true, false), None);
        // 3 блока — без урона; в креативе урона нет.
        v.on_move(at(87.0), true, true, false);
        v.on_move(at(80.0), false, false, false);
        assert_eq!(v.on_move(at(70.0), true, false, false), None);
        // Плавное падение: высота та же, урона нет.
        v.on_move(at(80.0), true, true, false);
        v.on_move(at(70.0), false, true, true);
        assert_eq!(v.on_move(at(70.0), true, true, true), None);
    }

    #[test]
    fn death_and_respawn() {
        let mut v = Vitals::default();
        assert!(!v.damage(19.0));
        assert!(v.damage(5.0));
        assert_eq!(v.health, 0.0);
        assert!(!v.damage(1.0));
        v.respawn();
        assert_eq!(v.health, MAX_HEALTH);
        assert!(!v.dead);
    }

    #[test]
    fn sprinting_burns_saturation_then_food() {
        let mut v = Vitals::default();
        v.reset_fall([0.0, 0.0, 0.0]);
        v.sprinting = true;
        // 100 м спринта = 10 истощения: 2 единицы насыщения.
        for x in 1..=100 {
            v.on_move(Some([x as f64, 0.0, 0.0]), true, true, false);
            v.tick(Difficulty::Normal);
        }
        assert_eq!(v.saturation, START_SATURATION - 2.0);
        assert_eq!(v.food, MAX_FOOD);
        v.saturation = 0.0;
        v.exhaust(4.5);
        assert!(v.tick(Difficulty::Normal).changed);
        assert_eq!(v.food, MAX_FOOD - 1);
    }

    #[test]
    fn regenerates_when_fed() {
        let mut v = Vitals {
            health: 10.0,
            food: 18,
            saturation: 0.0,
            ..Default::default()
        };
        let healed = (0..80).filter(|_| v.tick(Difficulty::Easy).changed).count();
        assert_eq!(healed, 1);
        assert_eq!(v.health, 11.0);
    }

    #[test]
    fn starvation_depends_on_difficulty() {
        let starve = |difficulty, health| {
            let mut v = Vitals {
                health,
                food: 0,
                saturation: 0.0,
                ..Default::default()
            };
            (0..80).filter_map(|_| v.tick(difficulty).starve).count()
        };
        assert_eq!(starve(Difficulty::Easy, 10.0), 0);
        assert_eq!(starve(Difficulty::Easy, 11.0), 1);
        assert_eq!(starve(Difficulty::Normal, 1.0), 0);
        assert_eq!(starve(Difficulty::Hard, 1.0), 1);
    }

    #[test]
    fn eating_caps_saturation_at_food() {
        let mut v = Vitals {
            food: 10,
            saturation: 0.0,
            ..Default::default()
        };
        v.eat(5, 6.0);
        assert_eq!((v.food, v.saturation), (15, 6.0));
        v.eat(8, 12.8);
        assert_eq!((v.food, v.saturation), (20, 18.8));
        v.food = 2;
        v.saturation = 0.0;
        v.eat(1, 9.6);
        assert_eq!((v.food, v.saturation), (3, 3.0));
    }

    #[test]
    fn save_round_trip() {
        let mut st = PlayerState::default();
        st.inv.slots[HOTBAR + 2] = Some(Stack {
            item: 7,
            count: 3,
            damage: 0,
            extra: 0,
        });
        st.inv.slots[OFFHAND] = Some(Stack {
            item: 9,
            count: 1,
            damage: 0,
            extra: 0,
        });
        st.inv.selected = 2;
        st.vitals.health = 7.5;
        st.vitals.food = 12;
        st.game_mode = Some(1);
        let s = st.save([1.0, 2.0, 3.0], 90.0, -10.0);
        assert_eq!(s.slots.len(), 2);
        let back = PlayerState::restore(&s);
        assert_eq!(back.save([1.0, 2.0, 3.0], 90.0, -10.0), s);
    }

    #[test]
    fn regeneration_heals_every_fifty_ticks_and_poison_stops_at_one() {
        let kinds = EffectKinds {
            regeneration: Some(1),
            poison: Some(2),
            ..EffectKinds::default()
        };
        let mut v = Vitals {
            health: 10.0,
            ..Vitals::default()
        };
        v.effects.push(Effect::plain(1, 0, 100));
        let mut heals = 0;
        for _ in 0..100 {
            let t = v.tick_effects(kinds);
            if t.heal > 0.0 {
                v.heal(t.heal);
                heals += 1;
            }
        }
        // 100 и 50 — два пульса, затем эффект кончился.
        assert_eq!(heals, 2);
        assert_eq!(v.health, 12.0);
        assert!(v.effects.is_empty());

        let mut v = Vitals {
            health: 1.5,
            ..Vitals::default()
        };
        v.effects.push(Effect::plain(2, 0, 25));
        let mut hit = false;
        for _ in 0..25 {
            let t = v.tick_effects(kinds);
            if t.poison {
                hit = true;
                if let Some(d) = poison_hit(v.health) {
                    assert!(!v.damage(d));
                }
            }
        }
        assert!(hit);
        assert_eq!(v.health, 1.0);
        assert!(poison_hit(1.0).is_none());
        assert!(!v.dead);
    }

    #[test]
    fn wither_can_kill_and_hunger_adds_exhaustion() {
        let kinds = EffectKinds {
            wither: Some(3),
            hunger: Some(4),
            ..EffectKinds::default()
        };
        let mut v = Vitals {
            health: 1.0,
            ..Vitals::default()
        };
        v.effects.push(Effect::plain(3, 0, 40));
        let mut struck = false;
        for _ in 0..40 {
            if v.tick_effects(kinds).wither {
                struck = true;
                assert!(v.damage(1.0));
            }
        }
        assert!(struck && v.dead);

        let mut v = Vitals::default();
        v.effects.push(Effect::plain(4, 1, 2));
        let t = v.tick_effects(kinds);
        assert!((t.exhaustion - 0.01).abs() < 1e-6);
    }

    #[test]
    fn resistance_instant_and_jump() {
        assert!((resisted(10.0, Some(0), false) - 8.0).abs() < 1e-5);
        assert_eq!(resisted(10.0, Some(4), false), 0.0);
        assert_eq!(resisted(10.0, Some(0), true), 10.0);
        assert_eq!(
            instant_effect("minecraft:instant_health", 0),
            Instant::Heal(4.0)
        );
        assert_eq!(
            instant_effect("minecraft:instant_health", 1),
            Instant::Heal(8.0)
        );
        assert_eq!(
            instant_effect("minecraft:instant_damage", 0),
            Instant::Harm(6.0)
        );
        assert_eq!(
            instant_effect("minecraft:saturation", 0),
            Instant::Eat(1, 2.0)
        );
        assert_eq!(
            jump_modifiers(Some(1)),
            vec![("minecraft:effect.jump_boost".to_owned(), 0.2, 0)]
        );
        assert_eq!(
            gravity_modifiers(true),
            vec![("minecraft:effect.slow_falling".to_owned(), -0.07, 0)]
        );
        assert!(gravity_modifiers(false).is_empty());
        assert_eq!(Effect::plain(1, 0, 20).flags(), 2 | 4);
        let mut beacon = Effect::plain(1, 0, 20);
        beacon.ambient = true;
        beacon.particles = false;
        beacon.show_icon = false;
        assert_eq!(beacon.flags(), 1);
    }
}
