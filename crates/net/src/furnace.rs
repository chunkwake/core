//! Печь, доменная печь и коптильня (`AbstractFurnaceBlockEntity.serverTick` 26.3):
//! слоты 0 — сырьё, 1 — топливо, 2 — результат.

use chunkwake_protocol::inventory::{Slot, Stack};

pub const INPUT: usize = 0;
pub const FUEL: usize = 1;
pub const RESULT: usize = 2;

#[derive(Clone, Debug, PartialEq)]
pub struct Furnace {
    /// Индекс в `chunkwake_data::world::FURNACES`.
    pub kind: u8,
    pub slots: [Slot; 3],
    pub lit_remaining: i32,
    pub lit_total: i32,
    pub cook: i32,
    pub cook_total: i32,
    /// Множитель скорости от текущего топлива.
    pub speed: f32,
}

/// Рецепт для печи: результат, количество, время готовки.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cook {
    pub result: i32,
    pub count: i32,
    pub time: i32,
}

/// Внешние данные: рецепты, топливо, размеры стопок, остатки.
pub struct Env<'a> {
    pub recipe: &'a dyn Fn(i32) -> Option<Cook>,
    /// Время горения и множитель скорости для этой печи.
    pub fuel: &'a dyn Fn(i32) -> Option<(i32, f32)>,
    pub max_stack: &'a dyn Fn(i32) -> i32,
    pub remainder: &'a dyn Fn(i32) -> Slot,
    /// Мокрая губка, ведро, ведро воды (губка в печи наполняет ведро).
    pub sponge: Option<(i32, i32, i32)>,
}

impl Furnace {
    pub fn new(kind: u8) -> Self {
        Self {
            kind,
            slots: [None; 3],
            lit_remaining: 0,
            lit_total: 0,
            cook: 0,
            cook_total: 0,
            speed: 1.0,
        }
    }

    pub fn lit(&self) -> bool {
        self.lit_remaining > 0
    }

    /// Свойства окна (`ContainerData`): горение, его длительность, прогресс, время.
    pub fn data(&self) -> [i32; 4] {
        [
            self.lit_remaining,
            self.lit_total,
            self.cook,
            self.cook_total,
        ]
    }

    /// Простаивает: ничего не горит и не остывает.
    pub fn idle(&self) -> bool {
        !self.lit() && self.cook == 0
    }

    fn total_time(&self, c: Cook) -> i32 {
        if self.speed > 0.0 {
            (c.time as f32 / self.speed).ceil() as i32
        } else {
            c.time
        }
    }

    /// `setItem(0)`: новое сырьё — прогресс сначала (`getTotalCookTime`).
    pub fn set_slots(&mut self, slots: [Slot; 3], env: &Env) {
        let same = match (self.slots[INPUT], slots[INPUT]) {
            (Some(a), Some(b)) => a.same_kind(&b),
            (None, None) => true,
            _ => false,
        };
        self.slots = slots;
        if !same {
            self.cook_total = slots[INPUT]
                .and_then(|s| (env.recipe)(s.item))
                .map_or(200, |c| self.total_time(c));
            self.cook = 0;
        }
    }

    fn can_burn(&self, c: Cook, env: &Env) -> bool {
        match self.slots[RESULT] {
            None => true,
            Some(r) if r.item == c.result && r.damage == 0 => {
                r.count + c.count <= (env.max_stack)(c.result).min(99)
            }
            Some(_) => false,
        }
    }

    /// Один тик. `true` — сменились предметы или горение (`setChanged`).
    pub fn tick(&mut self, env: &Env) -> bool {
        let mut changed = false;
        let was_lit = self.lit();
        let mut lit = was_lit;
        if was_lit {
            self.lit_remaining -= 1;
            lit = self.lit_remaining > 0;
        }
        let has_fuel = self.slots[FUEL].is_some();
        let input = self.slots[INPUT];
        if lit || (has_fuel && input.is_some()) {
            match input.and_then(|s| (env.recipe)(s.item)) {
                Some(c) if c.count > 0 && self.can_burn(c, env) => {
                    if !lit {
                        let (burn, speed) = self.slots[FUEL]
                            .and_then(|f| (env.fuel)(f.item))
                            .unwrap_or((0, 1.0));
                        self.lit_remaining = burn;
                        self.lit_total = burn;
                        self.speed = speed;
                        if self.cook_total > 0 && self.cook < self.cook_total {
                            let f = self.cook as f32 / self.cook_total as f32;
                            self.cook_total = self.total_time(c);
                            self.cook = (f * self.cook_total as f32).ceil() as i32;
                        }
                        if burn > 0 {
                            self.consume_fuel(env);
                            lit = true;
                            changed = true;
                        }
                    }
                    if lit {
                        self.cook += 1;
                        if self.cook >= self.cook_total {
                            self.cook = 0;
                            self.cook_total = self.total_time(c);
                            self.burn(c, env);
                            changed = true;
                        }
                    } else {
                        self.cook = 0;
                    }
                }
                Some(_) => self.cook = 0,
                None if input.is_none() => self.cook = 0,
                None => {}
            }
        } else if self.cook > 0 {
            self.cook = (self.cook - 2).clamp(0, self.cook_total.max(0));
        }
        if was_lit != lit {
            changed = true;
        }
        changed
    }

    fn consume_fuel(&mut self, env: &Env) {
        let Some(f) = self.slots[FUEL].as_mut() else {
            return;
        };
        let item = f.item;
        f.count -= 1;
        if f.count <= 0 {
            self.slots[FUEL] = (env.remainder)(item);
        }
    }

    fn burn(&mut self, c: Cook, env: &Env) {
        match self.slots[RESULT].as_mut() {
            Some(r) => r.count += c.count,
            None => {
                self.slots[RESULT] = Some(Stack {
                    item: c.result,
                    count: c.count,
                    damage: 0,
                    extra: 0,
                })
            }
        }
        if let (Some(input), Some((sponge, bucket, water))) = (self.slots[INPUT], env.sponge)
            && input.item == sponge
            && self.slots[FUEL].is_some_and(|f| f.item == bucket)
        {
            self.slots[FUEL] = Some(Stack {
                item: water,
                count: 1,
                damage: 0,
                extra: 0,
            });
        }
        if let Some(s) = self.slots[INPUT].as_mut() {
            s.count -= 1;
            if s.count <= 0 {
                self.slots[INPUT] = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORE: i32 = 1;
    const INGOT: i32 = 2;
    const COAL: i32 = 3;
    const LAVA: i32 = 4;
    const BUCKET: i32 = 5;

    fn st(item: i32, count: i32) -> Slot {
        Some(Stack {
            item,
            count,
            damage: 0,
            extra: 0,
        })
    }

    #[allow(clippy::type_complexity)]
    fn env_for(
        fast: bool,
    ) -> (
        impl Fn(i32) -> Option<Cook>,
        impl Fn(i32) -> Option<(i32, f32)>,
    ) {
        let recipe = |i| {
            (i == ORE).then_some(Cook {
                result: INGOT,
                count: 1,
                time: 200,
            })
        };
        let fuel = move |i| match i {
            COAL => Some(if fast { (800, 2.0) } else { (1600, 1.0) }),
            LAVA => Some((20000, 1.0)),
            _ => None,
        };
        (recipe, fuel)
    }

    fn run(f: &mut Furnace, fast: bool, ticks: usize) {
        let (recipe, fuel) = env_for(fast);
        let max = |_| 64;
        let rem = |i| {
            (i == LAVA).then_some(Stack {
                item: BUCKET,
                count: 1,
                damage: 0,
                extra: 0,
            })
        };
        let env = Env {
            recipe: &recipe,
            fuel: &fuel,
            max_stack: &max,
            remainder: &rem,
            sponge: None,
        };
        for _ in 0..ticks {
            f.tick(&env);
        }
    }

    #[test]
    fn smelts_in_200_ticks_with_coal() {
        let mut f = Furnace::new(0);
        let (recipe, fuel) = env_for(false);
        let max = |_| 64;
        let rem = |_| None;
        let env = Env {
            recipe: &recipe,
            fuel: &fuel,
            max_stack: &max,
            remainder: &rem,
            sponge: None,
        };
        f.set_slots([st(ORE, 3), st(COAL, 1), None], &env);
        assert_eq!(f.cook_total, 200);
        run(&mut f, false, 199);
        assert_eq!((f.slots[RESULT], f.slots[FUEL]), (None, None));
        run(&mut f, false, 1);
        assert_eq!(
            (f.slots[INPUT], f.slots[RESULT]),
            (st(ORE, 2), st(INGOT, 1))
        );
        // В тик зажигания горение не убывает (как в ванилле).
        assert_eq!(f.lit_remaining, 1600 - 199);
        // Без сырья горение продолжается, затем гаснет.
        run(&mut f, false, 400);
        assert_eq!(f.slots[RESULT], st(INGOT, 3));
        run(&mut f, false, 1001);
        assert!(!f.lit());
    }

    #[test]
    fn blast_furnace_twice_as_fast_and_lava_leaves_bucket() {
        let mut f = Furnace::new(1);
        f.slots = [st(ORE, 1), st(COAL, 1), None];
        f.cook_total = 200;
        run(&mut f, true, 1);
        // Первый тик зажигает и пересчитывает время под множитель 2.
        assert_eq!((f.cook_total, f.lit_total), (100, 800));
        run(&mut f, true, 99);
        assert_eq!(f.slots[RESULT], st(INGOT, 1));

        let mut f = Furnace::new(0);
        f.slots = [st(ORE, 1), st(LAVA, 1), None];
        f.cook_total = 200;
        run(&mut f, false, 1);
        assert_eq!(f.slots[FUEL], st(BUCKET, 1));
    }

    #[test]
    fn cools_down_without_fuel() {
        let mut f = Furnace::new(0);
        f.slots = [st(ORE, 1), None, None];
        f.cook = 10;
        f.cook_total = 200;
        run(&mut f, false, 3);
        assert_eq!(f.cook, 4);
    }
}
