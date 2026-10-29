//! Точило (`GrindstoneMenu` 26.3): снятие чар и починка двух одинаковых предметов.
//! Опыт — сферами при заборе результата, не полоской.

use chunkwake_protocol::components::{self, Extra};
use chunkwake_protocol::inventory::{Slot, Stack};

/// Предмет и чары, нужные расчёту. Проклятие — тег `#minecraft:curse`.
pub struct Ctx<'a> {
    pub enchanted_book: i32,
    pub book: i32,
    pub max_damage: &'a dyn Fn(i32) -> i32,
    pub max_stack: &'a dyn Fn(i32) -> i32,
    pub curse: &'a dyn Fn(i32) -> bool,
    /// `Enchantment.getMinCost`.
    pub min_cost: &'a dyn Fn(i32, i32) -> i32,
}

/// Можно ли положить стопку во вход. Сырой патч не принимаем: его нельзя
/// переписать, не потеряв отделку и прочие неизвестные компоненты.
pub fn acceptable(stack: &Stack, max_damage: i32) -> bool {
    let e = components::get(stack.extra);
    if e.raw.is_some() {
        return false;
    }
    max_damage > 0 || !e.enchantments.is_empty() || !e.stored.is_empty()
}

/// Результат двух входов. `None` — стрелка пустая.
pub fn compute(top: Slot, bottom: Slot, ctx: &Ctx<'_>) -> Slot {
    match (top, bottom) {
        (None, None) => None,
        (Some(a), Some(b)) => {
            if a.count > 1 || b.count > 1 || a.item != b.item {
                return None;
            }
            let ea = components::get(a.extra);
            let eb = components::get(b.extra);
            if ea.raw.is_some() || eb.raw.is_some() {
                return None;
            }
            merge(a, &ea, b, &eb, ctx)
        }
        (only, None) | (None, only) => {
            let s = only?;
            if s.count > 1 {
                return None;
            }
            let e = components::get(s.extra);
            if e.raw.is_some() {
                return None;
            }
            let list = crafting(s.item, &e, ctx);
            if list.is_empty() {
                return None;
            }
            Some(write(s, &e, curses_of(&list, ctx), ctx))
        }
    }
}

/// Сумма `min_cost` обычных чар (`getExperienceFromItem`).
pub fn experience_of(stack: Slot, ctx: &Ctx<'_>) -> i32 {
    let Some(s) = stack else {
        return 0;
    };
    let e = components::get(s.extra);
    if e.raw.is_some() {
        return 0;
    }
    crafting(s.item, &e, ctx)
        .into_iter()
        .filter(|(id, _)| !(ctx.curse)(*id))
        .map(|(id, lvl)| (ctx.min_cost)(id, lvl))
        .sum()
}

/// `getExperienceAmount`: `ceil(n/2) + random(0 .. ceil(n/2))`. `roll` — это случайное.
pub fn xp_amount(n: i32, roll: i32) -> i32 {
    if n <= 0 {
        return 0;
    }
    let half = (n + 1) / 2;
    half + roll.clamp(0, half - 1)
}

fn merge(a: Stack, ea: &Extra, b: Stack, eb: &Extra, ctx: &Ctx<'_>) -> Slot {
    let max_a = (ctx.max_damage)(a.item);
    let damageable = max_a > 0;
    let mut count = 1;
    if !damageable {
        if (ctx.max_stack)(a.item) < 2 || !a.same_kind(&b) {
            return None;
        }
        count = 2;
    }
    let mut out = Stack { count, ..a };
    if damageable {
        let max_b = (ctx.max_damage)(b.item);
        let durability = max_a.max(max_b);
        let remaining = (max_a - a.damage) + (max_b - b.damage) + durability * 5 / 100;
        out.damage = (durability - remaining).max(0);
    }
    let mut list = crafting(a.item, ea, ctx);
    // Проклятие, которое уже есть на верхнем, со второго не поднимается.
    for (id, lvl) in crafting(b.item, eb, ctx) {
        let have = list.iter().find(|e| e.0 == id).map(|e| e.1).unwrap_or(0);
        if (ctx.curse)(id) && have > 0 {
            continue;
        }
        if have == 0 {
            list.push((id, lvl));
        } else if lvl > have
            && let Some(slot) = list.iter_mut().find(|e| e.0 == id)
        {
            slot.1 = lvl;
        }
    }
    Some(write(out, ea, curses_of(&list, ctx), ctx))
}

/// Чары для крафта: у книги хранимые, у остальных — надетые.
fn crafting(item: i32, e: &Extra, ctx: &Ctx<'_>) -> Vec<(i32, i32)> {
    if item == ctx.enchanted_book {
        e.stored.clone()
    } else {
        e.enchantments.clone()
    }
}

fn curses_of(list: &[(i32, i32)], ctx: &Ctx<'_>) -> Vec<(i32, i32)> {
    list.iter()
        .copied()
        .filter(|(id, _)| (ctx.curse)(*id))
        .collect()
}

fn write(mut stack: Stack, base: &Extra, curses: Vec<(i32, i32)>, ctx: &Ctx<'_>) -> Stack {
    let mut e = base.clone();
    if stack.item == ctx.enchanted_book {
        if curses.is_empty() {
            stack.item = ctx.book;
            e.stored.clear();
            e.enchantments.clear();
        } else {
            e.stored = curses;
            e.enchantments.clear();
        }
    } else {
        e.enchantments = curses;
    }
    stack.extra = components::intern(e);
    stack
}

#[cfg(test)]
mod tests {
    use super::*;

    const SWORD: i32 = 1;
    const BOOK: i32 = 2;
    const ENCH_BOOK: i32 = 3;
    const GEM: i32 = 4;
    const SHARP: i32 = 10;
    const BIND: i32 = 11;
    const VANISH: i32 = 12;

    fn ctx() -> Ctx<'static> {
        Ctx {
            enchanted_book: ENCH_BOOK,
            book: BOOK,
            max_damage: &|item| if item == SWORD { 200 } else { 0 },
            max_stack: &|item| if item == GEM { 16 } else { 1 },
            curse: &|id| id == BIND || id == VANISH,
            min_cost: &|id, lvl| {
                if id == SHARP { 1 + 11 * (lvl - 1) } else { 25 }
            },
        }
    }

    fn stack(item: i32, damage: i32, ench: Vec<(i32, i32)>, stored: Vec<(i32, i32)>) -> Stack {
        let mut s = Stack::new(item, 1);
        s.damage = damage;
        s.extra = components::intern(Extra {
            enchantments: ench,
            stored,
            custom_name: Some("\"Named\"".into()),
            ..Extra::default()
        });
        s
    }

    #[test]
    fn strips_normal_enchants_and_keeps_curse_and_damage() {
        let c = ctx();
        let sword = stack(SWORD, 40, vec![(SHARP, 5), (BIND, 1)], vec![]);
        let out = compute(Some(sword), None, &c).unwrap();
        assert_eq!(out.item, SWORD);
        assert_eq!(out.damage, 40);
        let e = components::get(out.extra);
        assert_eq!(e.enchantments, vec![(BIND, 1)]);
        assert_eq!(e.custom_name.as_deref(), Some("\"Named\""));
        assert!(!acceptable(&Stack::new(GEM, 1), 0));
        assert!(acceptable(&sword, 200));
    }

    #[test]
    fn enchanted_book_becomes_a_book() {
        let c = ctx();
        let book = stack(ENCH_BOOK, 0, vec![], vec![(SHARP, 1)]);
        let out = compute(None, Some(book), &c).unwrap();
        assert_eq!(out.item, BOOK);
        assert_eq!(components::get(out.extra).stored, vec![]);
    }

    #[test]
    fn two_swords_repair_and_keep_both_curses_without_stacking() {
        let c = ctx();
        // Остаток 20 + 30 + 200 * 5 / 100 = 60, износ 140.
        let top = stack(SWORD, 180, vec![(SHARP, 1), (BIND, 1)], vec![]);
        let bot = stack(SWORD, 170, vec![(BIND, 3), (VANISH, 1)], vec![]);
        let out = compute(Some(top), Some(bot), &c).unwrap();
        assert_eq!((out.item, out.count, out.damage), (SWORD, 1, 140));
        let mut curses = components::get(out.extra).enchantments.clone();
        curses.sort();
        assert_eq!(curses, vec![(BIND, 1), (VANISH, 1)]);
        assert!(compute(Some(top), Some(stack(GEM, 0, vec![], vec![])), &c).is_none());
        assert!(compute(Some(stack(SWORD, 0, vec![], vec![])), None, &c).is_none());
    }

    #[test]
    fn xp_stays_inside_the_vanilla_range() {
        let c = ctx();
        let sword = stack(SWORD, 0, vec![(SHARP, 5)], vec![]);
        let n = experience_of(Some(sword), &c);
        assert_eq!(n, 45);
        let half = (n + 1) / 2;
        for roll in 0..half {
            let got = xp_amount(n, roll);
            assert!((half..=half * 2 - 1).contains(&got), "{got}");
        }
        assert_eq!(xp_amount(0, 0), 0);
        let both = experience_of(Some(stack(SWORD, 0, vec![(SHARP, 1)], vec![])), &c)
            + experience_of(Some(stack(ENCH_BOOK, 0, vec![], vec![(SHARP, 1)])), &c);
        assert_eq!(both, 2);
        assert_eq!(xp_amount(both, 0), 1);
    }
}
