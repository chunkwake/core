//! Костёр (`CampfireBlockEntity` 26.3): четыре слота по одной штуке.
//! Горит — прогресс растёт. Погас — остывает на 2 за тик. Опыта нет.

use chunkwake_protocol::inventory::{Slot, Stack};

pub const SLOTS: usize = 4;
/// `CampfireBlockEntity.BURN_COOL_SPEED`.
pub const COOL: i32 = 2;

/// Что сделать со слотом, который дошёл до срока.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Finish {
    /// Выбросить результат и очистить слот. `None` — очистить без дропа.
    Drop(Option<Stack>),
    /// `BlockCookEvent` отменён: слот остаётся, остальные в этом тике не идут.
    Cancel,
}

/// Один шаг горящего костра (`cookTick` до события или до конца слотов).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Слоты этого тика просмотрены.
    Done,
    /// Слот дошёл до срока. Прогресс уже увеличен, слот ещё занят.
    Ready { slot: usize, source: Stack, result: Stack },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Campfire {
    pub slots: [Slot; SLOTS],
    /// `CookingTimes`.
    pub progress: [i32; SLOTS],
    /// `CookingTotalTimes`.
    pub total: [i32; SLOTS],
    /// `Paper.StopCooking`: слот не копит прогресс.
    pub stopped: [bool; SLOTS],
    /// Какой слот смотреть следующим в этом тике. В NBT не пишется.
    cursor: u8,
}

impl Campfire {
    pub fn new() -> Self {
        Self {
            slots: [None; SLOTS],
            progress: [0; SLOTS],
            total: [0; SLOTS],
            stopped: [false; SLOTS],
            cursor: 0,
        }
    }

    /// Есть еда или недоостывший прогресс — костёр ещё тикать.
    pub fn active(&self) -> bool {
        self.slots.iter().any(|s| s.is_some()) || self.progress.iter().any(|&p| p > 0)
    }

    pub fn has_room(&self) -> bool {
        self.slots.iter().any(|s| s.is_none())
    }

    /// Первая пустая ячейка, одна штука. `false` — все четыре заняты.
    pub fn place(&mut self, item: Stack, time: i32) -> bool {
        let Some(i) = self.slots.iter().position(|s| s.is_none()) else {
            return false;
        };
        self.slots[i] = Some(Stack { count: 1, ..item });
        self.progress[i] = 0;
        self.total[i] = time.max(0);
        self.stopped[i] = false;
        true
    }

    pub fn begin_tick(&mut self) {
        self.cursor = 0;
    }

    /// Следующий слот горящего костра. Рецепт — `(предмет, число)`; нет рецепта —
    /// результат тот же предмет (`orElse(itemStack)`).
    pub fn next_lit(&mut self, recipe: impl Fn(i32) -> Option<(i32, i32)>) -> Action {
        while (self.cursor as usize) < SLOTS {
            let i = self.cursor as usize;
            self.cursor += 1;
            let Some(item) = self.slots[i] else {
                continue;
            };
            if !self.stopped[i] {
                self.progress[i] += 1;
            }
            if self.progress[i] >= self.total[i] {
                let result = match recipe(item.item) {
                    Some((id, count)) => Stack {
                        item: id,
                        count,
                        damage: 0,
                        extra: 0,
                    },
                    None => item,
                };
                return Action::Ready {
                    slot: i,
                    source: item,
                    result,
                };
            }
        }
        Action::Done
    }

    /// После `Ready`: снять слот или оставить его, если событие отменено.
    pub fn finish(&mut self, slot: usize, done: Finish) -> Option<Stack> {
        match done {
            Finish::Cancel => None,
            Finish::Drop(stack) => {
                if slot < SLOTS {
                    self.slots[slot] = None;
                    self.progress[slot] = 0;
                    self.total[slot] = 0;
                    self.stopped[slot] = false;
                }
                stack
            }
        }
    }

    /// Погасший костёр: прогресс минус 2, не ниже нуля. Еда остаётся.
    pub fn tick_cool(&mut self) {
        for p in &mut self.progress {
            if *p > 0 {
                *p = (*p - COOL).max(0);
            }
        }
    }
}

impl Default for Campfire {
    fn default() -> Self {
        Self::new()
    }
}

/// Куски дропа Paper: `split(random(21) + 10)`, то есть 10–30, но не больше остатка.
pub fn split_counts(count: i32, mut roll: impl FnMut() -> i32) -> Vec<i32> {
    let mut left = count.max(0);
    let mut out = Vec::new();
    while left > 0 {
        let n = roll().clamp(10, 30).min(left);
        out.push(n);
        left -= n;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw() -> Stack {
        Stack {
            item: 1,
            count: 3,
            damage: 0,
            extra: 0,
        }
    }

    fn recipe(item: i32) -> Option<(i32, i32)> {
        (item == 1).then_some((2, 1))
    }

    #[test]
    fn first_empty_slot_and_fifth_is_refused() {
        let mut c = Campfire::new();
        for i in 0..4 {
            assert!(c.place(raw(), 600), "{i}");
            assert_eq!(c.slots[i].unwrap().count, 1);
        }
        assert!(!c.place(raw(), 600));
        assert!(!c.has_room());
    }

    #[test]
    fn cooks_on_the_600th_tick() {
        let mut c = Campfire::new();
        c.place(raw(), 600);
        for tick in 1..600 {
            c.begin_tick();
            assert_eq!(c.next_lit(recipe), Action::Done, "tick {tick}");
            assert_eq!(c.progress[0], tick);
        }
        c.begin_tick();
        match c.next_lit(recipe) {
            Action::Ready { slot, result, .. } => {
                assert_eq!(slot, 0);
                assert_eq!((result.item, result.count), (2, 1));
                assert_eq!(
                    c.finish(slot, Finish::Drop(Some(result))),
                    Some(result)
                );
            }
            Action::Done => panic!("600th tick should finish"),
        }
        assert!(c.slots[0].is_none());
        assert!(!c.active());
    }

    #[test]
    fn unlit_cools_by_two() {
        let mut c = Campfire::new();
        c.place(raw(), 600);
        c.progress[0] = 5;
        c.tick_cool();
        assert_eq!(c.progress[0], 3);
        c.tick_cool();
        c.tick_cool();
        assert_eq!(c.progress[0], 0);
        assert!(c.slots[0].is_some());
    }

    #[test]
    fn paused_slot_does_not_gain_progress() {
        let mut c = Campfire::new();
        c.place(raw(), 600);
        c.stopped[0] = true;
        c.progress[0] = 4;
        c.begin_tick();
        assert_eq!(c.next_lit(recipe), Action::Done);
        assert_eq!(c.progress[0], 4);
    }

    #[test]
    fn cancel_keeps_the_slot_and_stops_the_tick() {
        let mut c = Campfire::new();
        c.place(raw(), 1);
        c.place(raw(), 1);
        c.begin_tick();
        let Action::Ready { slot, .. } = c.next_lit(recipe) else {
            panic!("first slot");
        };
        assert!(c.finish(slot, Finish::Cancel).is_none());
        assert!(c.slots[0].is_some());
        assert!(c.progress[0] >= c.total[0]);
        // Отмена выходит из тика: второй слот в этом вызове не смотрим.
        // Следующий тик снова начинает с готового слота.
        c.begin_tick();
        assert!(matches!(c.next_lit(recipe), Action::Ready { slot: 0, .. }));
    }

    #[test]
    fn split_counts_follow_paper_pieces() {
        assert_eq!(split_counts(1, || 10), vec![1]);
        assert_eq!(split_counts(25, || 10), vec![10, 10, 5]);
        assert_eq!(split_counts(0, || 10), Vec::<i32>::new());
    }
}
