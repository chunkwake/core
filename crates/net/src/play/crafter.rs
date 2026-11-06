//! Крафтер (`CrafterBlock`, `CrafterBlockEntity`, `CrafterMenu`): девять
//! слотов, фронт сигнала через 4 тика собирает рецепт и выдаёт результат и
//! остатки вперёд (в контейнер или предметом), выключаемые слоты, сигнал
//! компаратора — число занятых и выключенных слотов.

use super::*;

pub(super) const CRAFTER: &str = "minecraft:crafter";
/// `setCraftingTicksRemaining(6)`: столько тиков держится `crafting=true`.
const CRAFTING_TICKS: i32 = 6;
/// Рецепта нет (`levelEvent(1050)`).
const EVENT_FAIL: i32 = 1050;
/// Выдан предмет: звук (`1049`) и дым в сторону выдачи (`2010`).
const EVENT_CRAFT: i32 = 1049;
const EVENT_SMOKE: i32 = 2010;

/// Направление `front` из `orientation` (`down_east`, `north_up` …).
pub(super) fn crafter_front(orientation: &str) -> Option<usize> {
    let front = orientation.split('_').next()?;
    crate::redstone::DIRS.iter().position(|d| *d == front)
}

impl PlayPlan {
    /// Маска выключенных слотов (`disabled_slots`): бит `i` — слот `i`.
    pub(super) fn crafter_disabled(&self, pos: [i32; 3]) -> u16 {
        let Some(be) = self.block_entity(pos).filter(|b| b.id == CRAFTER) else {
            return 0;
        };
        match be.nbt.get("disabled_slots") {
            Some(Tag::IntArray(a)) => a.iter().filter(|&&i| (0..9).contains(&i)).fold(0, |m, &i| m | 1 << i),
            _ => 0,
        }
    }

    /// `ContainerData` меню: 0–8 — слот выключен, 9 — `triggered`.
    pub(super) fn crafter_data(&self, pos: [i32; 3]) -> [i32; 10] {
        let mask = self.crafter_disabled(pos);
        let triggered = self
            .block_entity(pos)
            .and_then(|b| b.nbt.get("triggered").and_then(Tag::as_i64))
            .unwrap_or(0);
        let mut d = [0; 10];
        for (i, v) in d.iter_mut().take(9).enumerate() {
            *v = i32::from(mask & (1 << i) != 0);
        }
        d[9] = triggered as i32;
        d
    }

    /// Правка блок-сущности крафтера с сохранением остального (`Items`).
    fn crafter_edit(&self, pos: [i32; 3], f: impl FnOnce(&mut Compound)) {
        let mut nbt = self.block_entity(pos).filter(|b| b.id == CRAFTER).map(|b| b.nbt).unwrap_or_default();
        f(&mut nbt);
        self.set_block_entity(pos, Some(BlockEntity { id: CRAFTER.into(), nbt }));
    }

    /// Тем, у кого крафтер открыт: слоты, результат и данные меню.
    fn crafter_notify(&self, pos: [i32; 3]) {
        for tx in self.controls.lock().unwrap().values() {
            let _ = tx.send(Control::ContainerChanged(pos));
        }
    }

    /// `CrafterBlockEntity.setTriggered`: флаг для меню («питание»).
    pub(super) fn crafter_set_triggered(&self, pos: [i32; 3], on: bool) {
        self.crafter_edit(pos, |n| {
            n.insert("triggered".into(), Tag::Int(i32::from(on)));
        });
        self.crafter_notify(pos);
    }

    /// `setSlotState`: выключить можно только пустой слот.
    pub(super) fn crafter_set_slot_state(&self, pos: [i32; 3], slot: i32, enabled: bool) {
        if !(0..9).contains(&slot) || self.container_items(pos)[slot as usize].is_some() {
            return;
        }
        let mut mask = self.crafter_disabled(pos);
        if enabled {
            mask &= !(1 << slot);
        } else {
            mask |= 1 << slot;
        }
        self.crafter_edit(pos, |n| {
            let list = (0..9).filter(|i| mask & (1 << i) != 0).collect();
            n.insert("disabled_slots".into(), Tag::IntArray(list));
        });
        self.redstone_update(&crate::redstone::around(pos));
        self.crafter_notify(pos);
    }

    /// `getRedstoneSignal`: слоты с предметом или выключенные.
    pub(super) fn crafter_analog(&self, pos: [i32; 3]) -> i32 {
        let mask = self.crafter_disabled(pos);
        self.container_items(pos)
            .iter()
            .enumerate()
            .filter(|(i, s)| s.is_some() || mask & (1 << i) != 0)
            .count() as i32
    }

    /// Слоты, куда воронка может положить `st` (`canPlaceItem`): не
    /// выключенный, не полный; занятый — только если дальше нет пустого
    /// слота или такой же стопки меньше (`smallerStackExist`).
    pub(super) fn crafter_allowed(&self, pos: [i32; 3], slots: &[Slot], st: Stack) -> Vec<usize> {
        let mask = self.crafter_disabled(pos);
        let on = |i: usize| mask & (1 << i) == 0;
        (0..slots.len().min(9))
            .filter(|&i| on(i))
            .filter(|&i| match slots[i] {
                None => true,
                Some(cur) => {
                    cur.count < self.max_stack(cur.item)
                        && cur.same_kind(&st)
                        && !(i + 1..9).filter(|&j| on(j)).any(|j| match slots[j] {
                            None => true,
                            Some(o) => o.count < cur.count && o.same_kind(&cur),
                        })
                }
            })
            .collect()
    }

    /// Результат рецепта в сетке крафтера (для меню и выдачи).
    pub(super) fn crafter_result(&self, grid: &[Slot]) -> Slot {
        self.craft_result(&grid[..9], 3)
    }

    /// `CrafterBlock.dispenseFrom`: рецепт → `crafting` на 6 тиков, результат
    /// и остатки вперёд, каждый слот сетки −1.
    pub(super) fn crafter_craft(&self, pos: [i32; 3]) {
        let Some(s) = self.block(pos).filter(|&s| self.state_name(s) == CRAFTER) else {
            return;
        };
        let Some(mut me) = self.cont_get(pos) else { return };
        let Some(result) = self.crafter_result(&me.slots) else {
            self.hub.send(None, play::level_event(self.ids.level_event, EVENT_FAIL, pos, 0, false));
            return;
        };
        let pl = &self.survival.placement;
        if let Some(n) = pl.with_prop(s, "crafting", "true") {
            self.set_block(pos, n);
        }
        self.sim().crafters.lock().unwrap().insert(pos, CRAFTING_TICKS);
        self.crafter_edit(pos, |n| {
            n.insert("crafting_ticks_remaining".into(), Tag::Int(CRAFTING_TICKS));
        });
        self.mark_work();
        let f = pl.prop(s, "orientation").and_then(crafter_front).unwrap_or(2);
        self.crafter_dispense(pos, f, result);
        let rests: Vec<Stack> = me.slots.iter().flatten().filter_map(|x| self.craft_remainder(x.item)).collect();
        for r in rests {
            self.crafter_dispense(pos, f, r);
        }
        for slot in me.slots.iter_mut() {
            if let Some(x) = slot {
                x.count -= 1;
                if x.count <= 0 {
                    *slot = None;
                }
            }
        }
        self.cont_put(&me);
    }

    /// `CrafterBlock.dispenseItem`: в контейнер впереди (в крафтер — по
    /// одному), остаток — предметом со скоростью 6, звук и дым.
    fn crafter_dispense(&self, pos: [i32; 3], f: usize, stack: Stack) {
        use crate::hoppers as hp;
        let mut left = stack;
        if let Some(mut t) = self.cont_get(crate::redstone::step(pos, f)) {
            let mut changed = false;
            while left.count > 0 {
                let (allowed, cap) = self.insert_target(&t, left, f ^ 1);
                if !hp::insert_one(&mut t.slots, &allowed, left, |_| cap) {
                    break;
                }
                left.count -= 1;
                changed = true;
            }
            if changed {
                self.cont_put(&t);
            }
        }
        if left.count <= 0 {
            return;
        }
        let d = crate::placement::step(f);
        let (at, vel) = hp::dispense_motion(pos, d, rand::random());
        self.drop_item(at, vel, left, std::time::Duration::ZERO);
        self.hub.send(None, play::level_event(self.ids.level_event, EVENT_CRAFT, pos, 0, false));
        self.hub.send(None, play::level_event(self.ids.level_event, EVENT_SMOKE, pos, f as i32, false));
    }

    /// `CrafterBlockEntity.serverTick`: по истечении `crafting` гаснет.
    pub(super) fn tick_crafters(&self) {
        let done: Vec<[i32; 3]> = {
            let mut map = self.sim().crafters.lock().unwrap();
            if map.is_empty() {
                return;
            }
            for t in map.values_mut() {
                *t -= 1;
            }
            let done: Vec<_> = map.iter().filter(|(_, t)| **t <= 0).map(|(p, _)| *p).collect();
            for p in &done {
                map.remove(p);
            }
            done
        };
        for pos in done {
            let Some(s) = self.block(pos).filter(|&s| self.state_name(s) == CRAFTER) else {
                continue;
            };
            self.crafter_edit(pos, |n| {
                n.insert("crafting_ticks_remaining".into(), Tag::Int(0));
            });
            if let Some(n) = self.survival.placement.with_prop(s, "crafting", "false")
                && n != s
            {
                self.set_block(pos, n);
                self.redstone_update(&crate::redstone::around(pos));
            }
        }
    }

    /// Крафтер с диска посреди `crafting`: досчитать его тики.
    pub(super) fn adopt_crafter(&self, pos: [i32; 3], be: &BlockEntity) {
        let left = be.nbt.get("crafting_ticks_remaining").and_then(Tag::as_i64).unwrap_or(0);
        if left > 0 {
            self.sim().crafters.lock().unwrap().insert(pos, left as i32);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn front_from_orientation() {
        assert_eq!(crafter_front("north_up"), Some(2));
        assert_eq!(crafter_front("down_east"), Some(0));
        assert_eq!(crafter_front("up_west"), Some(1));
    }
}
