//! Клики в окнах (`AbstractContainerMenu.doClick`): итог считает сервер, а
//! присланное клиентом только сверяется с ним.

use chunkwake_protocol::inventory::{Slot, Stack};

/// `ContainerInput`.
pub const PICKUP: i32 = 0;
pub const QUICK_MOVE: i32 = 1;
pub const SWAP: i32 = 2;
pub const CLONE: i32 = 3;
pub const THROW: i32 = 4;
pub const QUICK_CRAFT: i32 = 5;
pub const PICKUP_ALL: i32 = 6;
/// Клик за окном.
pub const OUTSIDE: i16 = -999;

/// Раскладка окна: в конце любого окна — 27 слотов основного инвентаря и 9 хотбара.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `InventoryMenu`: 0 результат, 1–4 сетка, 5–8 броня, 9–35, 36–44, 45 вторая рука.
    Player,
    /// Контейнер из `n` слотов и инвентарь (`ChestMenu`, `HopperMenu`, `DispenserMenu`).
    Chest(usize),
    /// `CraftingMenu`: 0 результат, 1–9 сетка, 10–36, 37–45.
    Crafting,
    /// `AbstractFurnaceMenu`: 0 сырьё, 1 топливо, 2 результат, 3–29, 30–38.
    Furnace,
    /// `EnchantmentMenu`: 0 предмет, 1 лазурит, 2–28, 29–37.
    Enchanting,
    /// `GrindstoneMenu`: 0 и 1 входы, 2 результат, 3–29, 30–38.
    Grindstone,
    /// `StonecutterMenu`: 0 вход, 1 результат, 2–28, 29–37.
    Stonecutter,
}

impl Kind {
    pub fn size(self) -> usize {
        match self {
            Kind::Player => 46,
            Kind::Chest(n) => n + 36,
            Kind::Crafting => 46,
            Kind::Furnace | Kind::Grindstone => 39,
            Kind::Enchanting | Kind::Stonecutter => 38,
        }
    }

    /// Начало основного инвентаря игрока в окне.
    pub fn inventory_start(self) -> usize {
        match self {
            Kind::Player => 9,
            Kind::Chest(n) => n,
            Kind::Crafting => 10,
            Kind::Furnace | Kind::Grindstone => 3,
            Kind::Enchanting | Kind::Stonecutter => 2,
        }
    }

    /// Слот, куда класть нельзя (результат).
    fn output(self) -> Option<usize> {
        match self {
            Kind::Player | Kind::Crafting => Some(0),
            Kind::Furnace | Kind::Grindstone => Some(2),
            Kind::Stonecutter => Some(1),
            Kind::Chest(_) | Kind::Enchanting => None,
        }
    }

    /// Результат крафта или точила: забирается целиком и пересчитывается.
    fn craft_output(self) -> Option<usize> {
        match self {
            Kind::Player | Kind::Crafting => Some(0),
            Kind::Grindstone => Some(2),
            Kind::Stonecutter => Some(1),
            _ => None,
        }
    }

    /// Слот брони и её `EquipmentSlot`.
    fn armor(self, slot: usize) -> Option<&'static str> {
        match (self, slot) {
            (Kind::Player, 5) => Some("head"),
            (Kind::Player, 6) => Some("chest"),
            (Kind::Player, 7) => Some("legs"),
            (Kind::Player, 8) => Some("feet"),
            _ => None,
        }
    }

    /// Слот хотбара `n` (0..9) в окне.
    fn hotbar(self, n: usize) -> usize {
        self.inventory_start() + 27 + n
    }
}

/// Свойства предметов, нужные кликам.
pub struct Rules<'a> {
    pub max_stack: &'a dyn Fn(i32) -> i32,
    pub equip: &'a dyn Fn(i32) -> Option<&'static str>,
    /// Для печи: есть ли рецепт и топливо ли это (`isFuel`), ведро ли.
    pub smeltable: &'a dyn Fn(i32) -> bool,
    pub fuel: &'a dyn Fn(i32) -> bool,
    pub bucket: Option<i32>,
    /// Лазурит — единственное, что кладётся во второй слот стола.
    pub lapis: Option<i32>,
    /// Точило: изнашиваемый предмет или стопка с чарами, без сырого патча.
    pub grindable: &'a dyn Fn(&Stack) -> bool,
    pub creative: bool,
}

/// Перетаскивание (`QUICK_CRAFT`) между пакетами.
#[derive(Clone, Debug, Default)]
pub struct Drag {
    kind: Option<i8>,
    slots: Vec<usize>,
}

/// Окно на время клика. `craft` вызывается, когда игрок забрал результат: он
/// списывает ингредиенты и кладёт новый результат в слот 0.
pub struct Menu<'a> {
    pub kind: Kind,
    pub slots: &'a mut [Slot],
    pub carried: &'a mut Slot,
    pub drag: &'a mut Drag,
    pub rules: &'a Rules<'a>,
    pub craft: &'a mut dyn FnMut(&mut [Slot]),
}

fn same(a: &Stack, b: &Stack) -> bool {
    a.same_kind(b)
}

/// Забирает `n` из слота.
fn split(slot: &mut Slot, n: i32) -> Slot {
    let s = slot.as_mut()?;
    let n = n.min(s.count);
    s.count -= n;
    let out = Stack { count: n, ..*s };
    if s.count <= 0 {
        *slot = None;
    }
    (n > 0).then_some(out)
}

impl Menu<'_> {
    fn max(&self, item: i32) -> i32 {
        (self.rules.max_stack)(item).max(1)
    }

    fn slot_max(&self, slot: usize, item: i32) -> i32 {
        let bucket_fuel =
            self.kind == Kind::Furnace && slot == 1 && Some(item) == self.rules.bucket;
        let enchant_item = self.kind == Kind::Enchanting && slot == 0;
        let grind_input = self.kind == Kind::Grindstone && slot < 2;
        if self.kind.armor(slot).is_some() || bucket_fuel || enchant_item || grind_input {
            1
        } else {
            self.max(item)
        }
    }

    fn may_place(&self, slot: usize, s: &Stack) -> bool {
        if self.kind.output() == Some(slot) {
            return false;
        }
        if self.kind == Kind::Stonecutter && slot == 0 {
            return (self.rules.smeltable)(s.item);
        }
        if self.kind == Kind::Enchanting && slot == 1 {
            return Some(s.item) == self.rules.lapis;
        }
        if self.kind == Kind::Grindstone && slot < 2 {
            return (self.rules.grindable)(s);
        }
        if self.kind == Kind::Furnace && slot == 1 {
            // `FurnaceFuelSlot`: топливо или пустое ведро.
            return (self.rules.fuel)(s.item) || Some(s.item) == self.rules.bucket;
        }
        match self.kind.armor(slot) {
            Some(want) => (self.rules.equip)(s.item) == Some(want),
            None => true,
        }
    }

    /// Выполняет клик; возвращает выброшенное (Q, клик за окном).
    pub fn click(&mut self, slot: i16, button: i8, mode: i32) -> Vec<Stack> {
        let mut thrown = Vec::new();
        let size = self.slots.len();
        let index = (slot >= 0 && (slot as usize) < size).then_some(slot as usize);
        if mode == QUICK_CRAFT {
            self.quick_craft(index, button);
            return thrown;
        }
        if self.drag.kind.is_some() {
            *self.drag = Drag::default();
            return thrown;
        }
        match mode {
            PICKUP | QUICK_MOVE if button == 0 || button == 1 => {
                if slot == OUTSIDE {
                    let n = if button == 0 { i32::MAX } else { 1 };
                    thrown.extend(split(self.carried, n));
                } else if let Some(i) = index {
                    if mode == QUICK_MOVE {
                        self.quick_move(i);
                    } else {
                        self.pickup(i, button == 0);
                    }
                }
            }
            SWAP => {
                if let Some(i) = index {
                    self.swap(i, button);
                }
            }
            CLONE if self.rules.creative && self.carried.is_none() => {
                if let Some(s) = index.and_then(|i| self.slots[i]) {
                    *self.carried = Some(Stack {
                        count: self.max(s.item),
                        ..s
                    });
                }
            }
            THROW if self.carried.is_none() => {
                if let Some(i) = index {
                    if self.kind.craft_output() == Some(i) {
                        thrown.extend(self.take_output());
                    } else {
                        let n = if button == 0 { 1 } else { i32::MAX };
                        thrown.extend(split(&mut self.slots[i], n));
                    }
                }
            }
            PICKUP_ALL => {
                if let Some(i) = index {
                    self.pickup_all(i, button);
                }
            }
            _ => {}
        }
        thrown
    }

    /// Забирает результат крафта или точила целиком и пересчитывает его.
    fn take_output(&mut self) -> Slot {
        let i = self.kind.craft_output()?;
        let out = self.slots.get_mut(i)?.take()?;
        (self.craft)(self.slots);
        Some(out)
    }

    fn pickup(&mut self, i: usize, left: bool) {
        if self.kind.craft_output() == Some(i) {
            let Some(result) = self.slots[i] else { return };
            match *self.carried {
                None => *self.carried = self.take_output(),
                Some(c) if same(&c, &result) && c.count + result.count <= self.max(c.item) => {
                    *self.carried = Some(Stack {
                        count: c.count + result.count,
                        ..c
                    });
                    self.take_output();
                }
                _ => {}
            }
            return;
        }
        match (self.slots[i], *self.carried) {
            (None, None) => {}
            (Some(s), None) => {
                let n = if left { s.count } else { (s.count + 1) / 2 };
                *self.carried = split(&mut self.slots[i], n);
            }
            (None, Some(c)) => {
                if self.may_place(i, &c) {
                    let n = if left { c.count } else { 1 }.min(self.slot_max(i, c.item));
                    self.slots[i] = split(self.carried, n);
                }
            }
            (Some(s), Some(c)) => {
                if same(&s, &c) {
                    if self.may_place(i, &c) {
                        let room = self.slot_max(i, c.item) - s.count;
                        let n = if left { c.count } else { 1 }.min(room.max(0));
                        if let (Some(dst), Some(_)) =
                            (self.slots[i].as_mut(), split(self.carried, n))
                        {
                            dst.count += n;
                        }
                    }
                } else if self.may_place(i, &c) && c.count <= self.slot_max(i, c.item) {
                    self.slots[i] = Some(c);
                    *self.carried = Some(s);
                }
            }
        }
    }

    /// `moveItemStackTo`: доливает такие же стопки, затем первая подходящая пустая ячейка.
    fn move_to(&mut self, stack: &mut Stack, range: std::ops::Range<usize>, reverse: bool) -> bool {
        let order: Vec<usize> = if reverse {
            range.rev().collect()
        } else {
            range.collect()
        };
        let mut moved = false;
        if self.max(stack.item) > 1 {
            for &j in &order {
                if stack.count == 0 {
                    break;
                }
                let max = self.slot_max(j, stack.item);
                if let Some(dst) = self.slots[j].as_mut()
                    && same(dst, stack)
                    && dst.count < max
                {
                    let n = stack.count.min(max - dst.count);
                    dst.count += n;
                    stack.count -= n;
                    moved = true;
                }
            }
        }
        if stack.count > 0 {
            for &j in &order {
                if self.slots[j].is_none() && self.may_place(j, stack) {
                    let n = stack.count.min(self.slot_max(j, stack.item));
                    self.slots[j] = Some(Stack { count: n, ..*stack });
                    stack.count -= n;
                    moved = true;
                    break;
                }
            }
        }
        moved
    }

    /// `quickMoveStack` окна (shift-клик).
    fn quick_move(&mut self, i: usize) {
        let inv = self.kind.inventory_start();
        let hotbar = inv + 27;
        let end = hotbar + 9;
        if self.kind.craft_output() == Some(i) {
            // Результат: крафтим, пока влезает и рецепт тот же.
            let Some(first) = self.slots[i] else { return };
            for _ in 0..64 {
                match self.slots[i] {
                    Some(r) if same(&r, &first) => {
                        if self.kind == Kind::Stonecutter {
                            let capacity: i32 = (inv..end)
                                .map(|j| match self.slots[j] {
                                    Some(dst) if same(&dst, &r) => {
                                        (self.slot_max(j, r.item) - dst.count).max(0)
                                    }
                                    None => self.slot_max(j, r.item),
                                    _ => 0,
                                })
                                .sum();
                            if capacity < r.count {
                                break;
                            }
                        }
                        let mut s = r;
                        let before = s.count;
                        self.move_to(&mut s, inv..end, true);
                        if s.count == before {
                            break;
                        }
                        // Как в ванилле: влезла хотя бы часть — ингредиенты списаны, остаток пропадает.
                        self.take_output();
                        if s.count > 0 {
                            break;
                        }
                    }
                    _ => break,
                }
            }
            return;
        }
        let Some(mut s) = self.slots[i] else { return };
        let original = s.count;
        match self.kind {
            Kind::Player => {
                let armor = match (self.rules.equip)(s.item) {
                    Some("head") => Some(5),
                    Some("chest") => Some(6),
                    Some("legs") => Some(7),
                    Some("feet") => Some(8),
                    _ => None,
                };
                let offhand = (self.rules.equip)(s.item) == Some("offhand");
                if (1..9).contains(&i) {
                    self.move_to(&mut s, inv..end, false);
                } else if let Some(a) = armor.filter(|&a| self.slots[a].is_none() && i >= inv) {
                    self.move_to(&mut s, a..a + 1, false);
                } else if offhand && self.slots[45].is_none() && i < 45 {
                    self.move_to(&mut s, 45..46, false);
                } else if (inv..hotbar).contains(&i) {
                    self.move_to(&mut s, hotbar..end, false);
                } else if (hotbar..end).contains(&i) {
                    self.move_to(&mut s, inv..hotbar, false);
                } else {
                    self.move_to(&mut s, inv..end, false);
                }
            }
            Kind::Chest(_) => {
                if i < inv {
                    self.move_to(&mut s, inv..end, true);
                } else {
                    self.move_to(&mut s, 0..inv, false);
                }
            }
            Kind::Furnace => {
                // `AbstractFurnaceMenu.quickMoveStack`.
                if i == 2 {
                    self.move_to(&mut s, inv..end, true);
                } else if i < inv {
                    self.move_to(&mut s, inv..end, false);
                } else if (self.rules.smeltable)(s.item) {
                    self.move_to(&mut s, 0..1, false);
                } else if (self.rules.fuel)(s.item) || Some(s.item) == self.rules.bucket {
                    self.move_to(&mut s, 1..2, false);
                } else if i < hotbar {
                    self.move_to(&mut s, hotbar..end, false);
                } else {
                    self.move_to(&mut s, inv..hotbar, false);
                }
            }
            Kind::Enchanting => {
                // `EnchantmentMenu.quickMoveStack`: лазурит — во второй слот, иначе один предмет — в первый.
                if i < inv {
                    self.move_to(&mut s, inv..end, true);
                } else if Some(s.item) == self.rules.lapis {
                    self.move_to(&mut s, 1..2, true);
                } else if self.slots[0].is_none() {
                    self.slots[0] = Some(Stack { count: 1, ..s });
                    s.count -= 1;
                }
            }
            Kind::Stonecutter => {
                if i == 0 {
                    self.move_to(&mut s, inv..end, true);
                } else if !self.move_to(&mut s, 0..1, false) {
                    if i < hotbar {
                        self.move_to(&mut s, hotbar..end, false);
                    } else {
                        self.move_to(&mut s, inv..hotbar, false);
                    }
                }
            }
            Kind::Grindstone => {
                // Входы — в инвентарь. Из инвентаря по штуке в каждый свободный вход
                // (`move_to` останавливается на первой пустой ячейке).
                if i < inv {
                    self.move_to(&mut s, inv..end, true);
                } else {
                    let before = s.count;
                    for _ in 0..2 {
                        if s.count == 0 || !self.move_to(&mut s, 0..2, false) {
                            break;
                        }
                    }
                    if s.count == before {
                        if i < hotbar {
                            self.move_to(&mut s, hotbar..end, false);
                        } else {
                            self.move_to(&mut s, inv..hotbar, false);
                        }
                    }
                }
            }
            Kind::Crafting => {
                if i < inv {
                    self.move_to(&mut s, inv..end, false);
                } else if !self.move_to(&mut s, 1..10, false) || s.count > 0 {
                    if i < hotbar {
                        self.move_to(&mut s, hotbar..end, false);
                    } else {
                        self.move_to(&mut s, inv..hotbar, false);
                    }
                }
            }
        }
        if s.count != original {
            self.slots[i] = (s.count > 0).then_some(s);
        }
    }

    fn swap(&mut self, i: usize, button: i8) {
        let target = match button {
            0..=8 => self.kind.hotbar(button as usize),
            40 if self.kind == Kind::Player => 45,
            _ => return,
        };
        if target == i {
            return;
        }
        if self.kind.craft_output() == Some(i) {
            if self.slots[target].is_none() {
                self.slots[target] = self.take_output();
            }
            return;
        }
        let (src, hot) = (self.slots[i], self.slots[target]);
        match (src, hot) {
            (None, None) => {}
            (Some(_), None) => {
                self.slots[target] = self.slots[i].take();
            }
            (_, Some(h)) => {
                if !self.may_place(i, &h) {
                    return;
                }
                let max = self.slot_max(i, h.item);
                if h.count <= max {
                    self.slots[i] = Some(h);
                    self.slots[target] = src;
                } else if src.is_none() {
                    self.slots[i] = split(&mut self.slots[target], max);
                }
            }
        }
    }

    fn pickup_all(&mut self, i: usize, button: i8) {
        let Some(mut c) = *self.carried else { return };
        if self.slots[i].is_some() {
            return;
        }
        let max = self.max(c.item);
        let order: Vec<usize> = if button == 0 {
            (0..self.slots.len()).collect()
        } else {
            (0..self.slots.len()).rev().collect()
        };
        for pass in 0..2 {
            for &j in &order {
                if c.count >= max {
                    break;
                }
                if self.kind.output() == Some(j) {
                    continue;
                }
                if let Some(s) = self.slots[j]
                    && same(&s, &c)
                    && !(pass == 0 && s.count >= self.max(s.item))
                {
                    let n = (max - c.count).min(s.count);
                    split(&mut self.slots[j], n);
                    c.count += n;
                }
            }
        }
        *self.carried = Some(c);
    }

    /// Перетаскивание: начало (0), добавление слота (1), конец (2).
    fn quick_craft(&mut self, index: Option<usize>, button: i8) {
        let phase = button & 3;
        let kind = (button >> 2) & 3;
        match phase {
            0 => {
                *self.drag = Drag::default();
                let ok = kind == 0 || kind == 1 || (kind == 2 && self.rules.creative);
                if ok && self.carried.is_some() {
                    self.drag.kind = Some(kind);
                }
            }
            1 => {
                let (Some(k), Some(i), Some(c)) = (self.drag.kind, index, *self.carried) else {
                    *self.drag = Drag::default();
                    return;
                };
                let fits = match self.slots[i] {
                    None => true,
                    Some(s) => same(&s, &c),
                };
                if fits
                    && self.may_place(i, &c)
                    && (k == 2 || c.count > self.drag.slots.len() as i32)
                    && !self.drag.slots.contains(&i)
                {
                    self.drag.slots.push(i);
                }
            }
            2 => {
                let drag = std::mem::take(self.drag);
                let (Some(k), Some(c)) = (drag.kind, *self.carried) else {
                    return;
                };
                if drag.slots.is_empty() {
                    return;
                }
                if drag.slots.len() == 1 {
                    self.pickup(drag.slots[0], k == 0);
                    return;
                }
                let per = match k {
                    0 => c.count / drag.slots.len() as i32,
                    1 => 1,
                    _ => self.max(c.item),
                };
                let mut left = c.count;
                for &i in &drag.slots {
                    let have = self.slots[i].map_or(0, |s| s.count);
                    let max = self.slot_max(i, c.item);
                    let put = (have + per).min(max);
                    if k != 2 {
                        left -= put - have;
                    }
                    self.slots[i] = Some(Stack { count: put, ..c });
                }
                *self.carried = (left > 0).then_some(Stack { count: left, ..c });
            }
            _ => *self.drag = Drag::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(item: i32, count: i32) -> Slot {
        Some(Stack {
            item,
            count,
            damage: 0,
            extra: 0,
        })
    }

    fn run(
        kind: Kind,
        slots: &mut [Slot],
        carried: &mut Slot,
        clicks: &[(i16, i8, i32)],
    ) -> Vec<Stack> {
        let max = |item: i32| if item == 9 { 1 } else { 64 };
        let equip = |item: i32| (item == 9).then_some("head");
        let fuel = |item: i32| item == 7;
        let rules = Rules {
            max_stack: &max,
            equip: &equip,
            smeltable: &|item| item == 1,
            fuel: &fuel,
            bucket: Some(8),
            lapis: Some(10),
            grindable: &|_| false,
            creative: false,
        };
        let mut drag = Drag::default();
        let mut craft = |_: &mut [Slot]| {};
        let mut m = Menu {
            kind,
            slots,
            carried,
            drag: &mut drag,
            rules: &rules,
            craft: &mut craft,
        };
        clicks
            .iter()
            .flat_map(|&(s, b, mode)| m.click(s, b, mode))
            .collect()
    }

    #[test]
    fn stonecutter_takes_one_input_per_result_and_shift_moves_all() {
        let mut slots = vec![None; Kind::Stonecutter.size()];
        slots[0] = st(1, 3);
        slots[1] = st(2, 2);
        let mut carried = None;
        let rules = Rules {
            max_stack: &|_| 64,
            equip: &|_| None,
            smeltable: &|item| item == 1,
            fuel: &|_| false,
            bucket: None,
            lapis: None,
            grindable: &|_| false,
            creative: false,
        };
        let mut drag = Drag::default();
        let mut craft = |slots: &mut [Slot]| {
            if let Some(input) = slots[0].as_mut() {
                input.count -= 1;
                if input.count == 0 {
                    slots[0] = None;
                }
            }
            slots[1] = slots[0].map(|_| Stack::new(2, 2));
        };
        let mut menu = Menu {
            kind: Kind::Stonecutter,
            slots: &mut slots,
            carried: &mut carried,
            drag: &mut drag,
            rules: &rules,
            craft: &mut craft,
        };
        menu.click(1, 0, PICKUP);
        assert_eq!(
            (menu.slots[0], menu.slots[1], *menu.carried),
            (st(1, 2), st(2, 2), st(2, 2))
        );
        *menu.carried = None;
        menu.click(1, 0, QUICK_MOVE);
        assert_eq!(
            (menu.slots[0], menu.slots[1], menu.slots[37]),
            (None, None, st(2, 4))
        );
        assert_eq!(*menu.carried, None);
    }

    #[test]
    fn stonecutter_shift_requires_room_for_whole_result() {
        let mut slots = vec![st(9, 64); Kind::Stonecutter.size()];
        slots[0] = st(1, 1);
        slots[1] = st(2, 2);
        let mut carried = None;
        run(
            Kind::Stonecutter,
            &mut slots,
            &mut carried,
            &[(1, 0, QUICK_MOVE)],
        );
        assert_eq!((slots[0], slots[1]), (st(1, 1), st(2, 2)));
        slots[2] = st(2, 63);
        run(
            Kind::Stonecutter,
            &mut slots,
            &mut carried,
            &[(1, 0, QUICK_MOVE)],
        );
        assert_eq!(
            (slots[0], slots[1], slots[2]),
            (st(1, 1), st(2, 2), st(2, 63))
        );
    }

    #[test]
    fn stonecutter_shift_merges_only_the_input_that_fits() {
        let mut slots = vec![None; Kind::Stonecutter.size()];
        slots[0] = st(1, 63);
        slots[2] = st(1, 5);
        let mut carried = None;
        run(
            Kind::Stonecutter,
            &mut slots,
            &mut carried,
            &[(2, 0, QUICK_MOVE)],
        );
        assert_eq!((slots[0], slots[2]), (st(1, 64), st(1, 4)));
        slots[3] = st(3, 5);
        run(
            Kind::Stonecutter,
            &mut slots,
            &mut carried,
            &[(3, 0, QUICK_MOVE)],
        );
        assert_eq!((slots[0], slots[3], slots[29]), (st(1, 64), None, st(3, 5)));
    }

    #[test]
    fn pickup_split_and_place() {
        let mut slots = vec![None; 46];
        slots[9] = st(1, 10);
        let mut carried = None;
        run(Kind::Player, &mut slots, &mut carried, &[(9, 1, PICKUP)]);
        assert_eq!((slots[9], carried), (st(1, 5), st(1, 5)));
        run(
            Kind::Player,
            &mut slots,
            &mut carried,
            &[(10, 1, PICKUP), (9, 0, PICKUP)],
        );
        assert_eq!((slots[9], slots[10], carried), (st(1, 9), st(1, 1), None));
        // Броня: камень в слот шлема не кладётся, шлем — кладётся.
        carried = st(1, 1);
        run(Kind::Player, &mut slots, &mut carried, &[(5, 0, PICKUP)]);
        assert_eq!(slots[5], None);
        carried = st(9, 1);
        run(Kind::Player, &mut slots, &mut carried, &[(5, 0, PICKUP)]);
        assert_eq!((slots[5], carried), (st(9, 1), None));
    }

    #[test]
    fn shift_click_and_throw() {
        let mut slots = vec![None; 46];
        slots[9] = st(1, 64);
        slots[36] = st(1, 60);
        slots[20] = st(9, 1);
        let mut carried = None;
        run(
            Kind::Player,
            &mut slots,
            &mut carried,
            &[(9, 0, QUICK_MOVE), (20, 0, QUICK_MOVE)],
        );
        assert_eq!(
            (slots[9], slots[36], slots[37]),
            (None, st(1, 64), st(1, 60))
        );
        assert_eq!((slots[20], slots[5]), (None, st(9, 1)));
        let thrown = run(
            Kind::Player,
            &mut slots,
            &mut carried,
            &[(36, 0, THROW), (37, 1, THROW)],
        );
        assert_eq!(thrown, vec![st(1, 1).unwrap(), st(1, 60).unwrap()]);
        // Сундук: из сундука — в инвентарь с конца.
        let mut chest = vec![None; 27 + 36];
        chest[0] = st(2, 5);
        run(
            Kind::Chest(27),
            &mut chest,
            &mut carried,
            &[(0, 0, QUICK_MOVE)],
        );
        assert_eq!(chest[62], st(2, 5));
    }

    #[test]
    fn furnace_shift_click() {
        let mut slots = vec![None; 39];
        slots[3] = st(1, 5);
        slots[4] = st(7, 2);
        slots[5] = st(2, 1);
        let mut carried = None;
        run(
            Kind::Furnace,
            &mut slots,
            &mut carried,
            &[(3, 0, QUICK_MOVE), (4, 0, QUICK_MOVE), (5, 0, QUICK_MOVE)],
        );
        assert_eq!(
            (slots[0], slots[1], slots[30]),
            (st(1, 5), st(7, 2), st(2, 1))
        );
        // В результат класть нельзя, а забирать половину — можно.
        slots[2] = st(9, 1);
        carried = st(2, 1);
        run(Kind::Furnace, &mut slots, &mut carried, &[(2, 0, PICKUP)]);
        assert_eq!(slots[2], st(9, 1));
        carried = None;
        slots[2] = st(3, 4);
        run(Kind::Furnace, &mut slots, &mut carried, &[(2, 1, PICKUP)]);
        assert_eq!((slots[2], carried), (st(3, 2), st(3, 2)));
    }

    #[test]
    fn drag_and_double_click() {
        let mut slots = vec![None; 46];
        let mut carried = st(1, 10);
        run(
            Kind::Player,
            &mut slots,
            &mut carried,
            &[
                (OUTSIDE, 0, QUICK_CRAFT),
                (9, 1, QUICK_CRAFT),
                (10, 1, QUICK_CRAFT),
                (11, 1, QUICK_CRAFT),
                (OUTSIDE, 2, QUICK_CRAFT),
            ],
        );
        assert_eq!(
            (slots[9], slots[10], slots[11], carried),
            (st(1, 3), st(1, 3), st(1, 3), st(1, 1))
        );
        run(
            Kind::Player,
            &mut slots,
            &mut carried,
            &[(12, 0, PICKUP_ALL)],
        );
        assert_eq!((slots[9], carried), (None, st(1, 10)));
        // Обмен с хотбаром.
        slots[9] = st(2, 1);
        run(Kind::Player, &mut slots, &mut carried, &[(9, 3, SWAP)]);
        assert_eq!((slots[9], slots[39]), (None, st(2, 1)));
    }
}
