//! Инвентарь игрока и выживание: здоровье, урон от падения и пустоты, смерть.
//! Логика без сети: сессия применяет результат и шлёт пакеты.

use chunkwake_protocol::inventory::{ContainerClick, Slot, Stack};

/// Слоты окна инвентаря игрока: 0 — результат крафта, 1–4 — сетка, 5–8 — броня,
/// 9–35 — основной, 36–44 — хотбар, 45 — вторая рука.
pub const SLOTS: usize = 46;
const HOTBAR: usize = 36;
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
}

impl Default for Inventory {
    fn default() -> Self {
        Self {
            slots: [None; SLOTS],
            carried: None,
            selected: 0,
            state: 0,
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
                self.slots[i] = Some(Stack { item, count: n });
                count -= n;
                changed.push(i);
            }
        }
        (changed, count)
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

    /// Итог клика от клиента: изменённые слоты и курсор. Пока доверяем клиенту
    /// (проверка по правилам кликов — позже); слоты крафта игнорируем.
    /// Применяет клик клиента. Возвращает выброшенное: Q над слотом (THROW)
    /// или клик за окном (слот -999) — то, что пропало из инвентаря.
    pub fn apply_click(&mut self, c: &ContainerClick) -> Vec<Stack> {
        let before = self.totals();
        for &(slot, stack) in &c.changed {
            if (5..SLOTS as i16).contains(&slot) {
                self.slots[slot as usize] = stack;
            }
        }
        self.carried = c.carried;
        if c.mode != THROW && c.slot != OUTSIDE {
            return Vec::new();
        }
        let after = self.totals();
        before
            .into_iter()
            .filter_map(|(item, n)| {
                let gone = n - after.get(&item).copied().unwrap_or(0);
                (gone > 0).then_some(Stack { item, count: gone })
            })
            .collect()
    }

    /// Сколько каждого предмета в слотах и на курсоре.
    fn totals(&self) -> std::collections::HashMap<i32, i32> {
        let mut m = std::collections::HashMap::new();
        for s in self
            .slots
            .iter()
            .chain(std::iter::once(&self.carried))
            .flatten()
        {
            *m.entry(s.item).or_insert(0) += s.count;
        }
        m
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

/// `ContainerInput.THROW` и слот «за окном» в `container_click`.
const THROW: i32 = 4;
const OUTSIDE: i16 = -999;

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
}

impl Cause {
    /// Ключ сообщения о смерти и тип урона в реестре `damage_type`.
    pub fn death_key(self) -> &'static str {
        match self {
            Cause::Fall => "death.attack.fall",
            Cause::Void => "death.attack.outOfWorld",
            Cause::Starve => "death.attack.starve",
        }
    }
}

/// Сложность (`difficulty` в `server.properties`): от неё зависит голод.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Difficulty {
    Peaceful,
    Easy,
    Normal,
    Hard,
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
        }
    }
}

impl Vitals {
    /// Движение игрока: падение и истощение от спринта и прыжков. Возвращает
    /// урон от падения при приземлении: `ceil(высота − 3)`, как в ванилле.
    pub fn on_move(
        &mut self,
        pos: Option<[f64; 3]>,
        on_ground: bool,
        takes_damage: bool,
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
        (takes_damage && damage > 0.0).then_some(damage)
    }

    /// Применяет урон; `true` — игрок умер.
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

    /// Тик сервера (`FoodData.tick`): трата сытости, регенерация, голодание.
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

    fn heal(&mut self, amount: f32) {
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
}

/// Живое состояние игрока в сессии: инвентарь и здоровье.
#[derive(Default)]
pub struct PlayerState {
    pub inv: Inventory,
    pub vitals: Vitals,
    pub game_mode: Option<u8>,
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
            slots: (0..SLOTS)
                .filter_map(|i| self.inv.slots[i].map(|s| (i, s)))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_merges_then_fills_hotbar_first() {
        let mut inv = Inventory::default();
        inv.slots[MAIN] = Some(Stack { item: 1, count: 60 });
        let changed = inv.add(1, 10, 64);
        assert_eq!(changed, (vec![MAIN, HOTBAR], 0));
        assert_eq!(inv.slots[MAIN].unwrap().count, 64);
        assert_eq!(inv.slots[HOTBAR], Some(Stack { item: 1, count: 6 }));
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
        inv.slots[HOTBAR + 2] = Some(Stack { item: 5, count: 1 });
        assert!(inv.consume_held());
        assert_eq!(inv.held(), None);
        assert!(!inv.consume_held());
    }

    #[test]
    fn fall_damage_on_landing() {
        let mut v = Vitals::default();
        let at = |y| Some([0.0, y, 0.0]);
        v.reset_fall([0.0, 100.0, 0.0]);
        assert_eq!(v.on_move(at(95.0), false, true), None);
        assert_eq!(v.on_move(at(90.0), false, true), None);
        // 10 блоков: 7 урона.
        assert_eq!(v.on_move(at(90.0), true, true), Some(7.0));
        // Падение сбросилось.
        assert_eq!(v.on_move(at(90.0), true, true), None);
        // 3 блока — без урона; в креативе урона нет.
        v.on_move(at(87.0), true, true);
        v.on_move(at(80.0), false, false);
        assert_eq!(v.on_move(at(70.0), true, false), None);
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
            v.on_move(Some([x as f64, 0.0, 0.0]), true, true);
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
        st.inv.slots[HOTBAR + 2] = Some(Stack { item: 7, count: 3 });
        st.inv.slots[OFFHAND] = Some(Stack { item: 9, count: 1 });
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
    fn throw_click_returns_dropped() {
        let mut inv = Inventory::default();
        inv.slots[HOTBAR] = Some(Stack { item: 4, count: 10 });
        let click = |slot, mode, changed, carried| ContainerClick {
            window: 0,
            state: 0,
            slot,
            button: 0,
            mode,
            changed,
            carried,
        };
        // Q над слотом: 10 -> 9.
        let c = click(
            HOTBAR as i16,
            4,
            vec![(HOTBAR as i16, Some(Stack { item: 4, count: 9 }))],
            None,
        );
        assert_eq!(inv.apply_click(&c), vec![Stack { item: 4, count: 1 }]);
        // Обычный клик ничего не выбрасывает.
        let c = click(
            HOTBAR as i16,
            0,
            vec![(HOTBAR as i16, None)],
            Some(Stack { item: 4, count: 9 }),
        );
        assert!(inv.apply_click(&c).is_empty());
        // Клик за окном с предметом на курсоре.
        let c = click(-999, 0, vec![], None);
        assert_eq!(inv.apply_click(&c), vec![Stack { item: 4, count: 9 }]);
    }
}
