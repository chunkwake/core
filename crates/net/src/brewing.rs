//! Варочная стойка: пять слотов, двадцать варок на огненный порошок.

use chunkwake_data::recipes::BrewingData;
use chunkwake_protocol::inventory::{Slot, Stack};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stand {
    pub slots: [Slot; 5],
    pub time: i32,
    pub fuel: i32,
    pub ingredient: Option<i32>,
}

impl Default for Stand {
    fn default() -> Self {
        Self {
            slots: [None; 5],
            time: 0,
            fuel: 0,
            ingredient: None,
        }
    }
}

impl Stand {
    pub fn data(&self) -> [i32; 2] {
        [self.time, self.fuel]
    }

    /// Тик по рецептам 26.3; возвращает, изменилось ли состояние, и остаток
    /// реагента (например, бутылку после драконьего дыхания).
    pub fn tick(
        &mut self,
        recipes: &BrewingData,
        blaze_powder: i32,
        potion_of: impl Fn(Stack) -> Option<i32>,
        output: impl Fn(Stack, i32, i32) -> Stack,
        remainder: impl Fn(i32) -> Slot,
    ) -> (bool, Slot) {
        let before = self.clone();
        let mut leftover = None;
        if self.fuel <= 0 && self.slots[4].is_some_and(|s| s.item == blaze_powder) {
            consume(&mut self.slots[4]);
            self.fuel = 20;
        }
        let reagent = self.slots[3].map(|s| s.item);
        let results: [Option<(i32, i32)>; 3] = std::array::from_fn(|i| {
            let bottle = self.slots[i]?;
            let potion = potion_of(bottle)?;
            recipes
                .recipes
                .get(&(bottle.item, potion, reagent?))
                .copied()
        });
        let can_brew = results.iter().any(Option::is_some);
        if self.time > 0 {
            self.time -= 1;
            if self.time == 0 && can_brew && self.ingredient == reagent {
                for (i, result) in results.into_iter().enumerate() {
                    if let (Some(bottle), Some((item, potion))) = (self.slots[i], result) {
                        self.slots[i] = Some(output(bottle, item, potion));
                    }
                }
                if let Some(item) = reagent {
                    let rest = remainder(item);
                    consume(&mut self.slots[3]);
                    if self.slots[3].is_none() {
                        self.slots[3] = rest;
                    } else {
                        leftover = rest;
                    }
                }
                self.ingredient = None;
            } else if !can_brew || self.ingredient != reagent {
                self.time = 0;
            }
        } else if can_brew && self.fuel > 0 {
            self.fuel -= 1;
            self.time = 400;
            self.ingredient = reagent;
        }
        (*self != before, leftover)
    }
}

fn consume(slot: &mut Slot) {
    if let Some(s) = slot.as_mut() {
        s.count -= 1;
        if s.count <= 0 {
            *slot = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuel_brews_three_bottles_with_one_reagent() {
        let mut recipes = BrewingData::default();
        recipes.recipes.insert((1, 10, 3), (1, 11));
        let mut stand = Stand::default();
        stand.slots[..3].fill(Some(Stack::new(1, 1)));
        stand.slots[3] = Some(Stack::new(3, 2));
        stand.slots[4] = Some(Stack::new(4, 1));
        let step = |stand: &mut Stand| {
            stand.tick(
                &recipes,
                4,
                |_| Some(10),
                |mut s, _, potion| {
                    s.damage = potion;
                    s
                },
                |_| None,
            )
        };
        step(&mut stand);
        assert_eq!(stand.data(), [400, 19]);
        for _ in 0..400 {
            step(&mut stand);
        }
        assert_eq!(stand.slots[3], Some(Stack::new(3, 1)));
        assert!(
            stand.slots[..3]
                .iter()
                .all(|s| s.is_some_and(|s| s.damage == 11))
        );
        assert_eq!(stand.fuel, 19);
    }

    #[test]
    fn changing_reagent_cancels_running_brew() {
        let mut recipes = BrewingData::default();
        recipes.recipes.insert((1, 10, 3), (1, 11));
        let mut stand = Stand::default();
        stand.slots[0] = Some(Stack::new(1, 1));
        stand.slots[3] = Some(Stack::new(3, 1));
        stand.fuel = 1;
        let step = |stand: &mut Stand| stand.tick(&recipes, 4, |_| Some(10), |s, _, _| s, |_| None);
        step(&mut stand);
        stand.slots[3] = Some(Stack::new(5, 1));
        step(&mut stand);
        assert_eq!(stand.time, 0);
        assert_eq!(stand.slots[0], Some(Stack::new(1, 1)));
    }

    #[test]
    fn dragon_breath_returns_bottle_without_overwriting_reagent_stack() {
        let mut recipes = BrewingData::default();
        recipes.recipes.insert((1, 10, 3), (2, 10));
        let mut stand = Stand::default();
        stand.slots[0] = Some(Stack::new(1, 1));
        stand.slots[3] = Some(Stack::new(3, 2));
        stand.time = 1;
        stand.fuel = 19;
        stand.ingredient = Some(3);
        let (_, rest) = stand.tick(
            &recipes,
            4,
            |_| Some(10),
            |mut stack, item, _| {
                stack.item = item;
                stack
            },
            |_| Some(Stack::new(5, 1)),
        );
        assert_eq!(stand.slots[0], Some(Stack::new(2, 1)));
        assert_eq!(stand.slots[3], Some(Stack::new(3, 1)));
        assert_eq!(rest, Some(Stack::new(5, 1)));
    }
}
