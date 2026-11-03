//! Стол зачарования (`EnchantmentMenu`, `EnchantmentHelper`): полки вокруг,
//! стоимость трёх вариантов, подсказки и сам набор чар. Случайность — от зерна
//! игрока, поэтому подсказка и итог совпадают.

use chunkwake_data::enchanting::{EnchantDef, Enchanting};

/// `LegacyRandomSource` (= `java.util.Random`): ванильный стол зачарования
/// сеет его зерном игрока, поэтому варианты совпадают с ванилью до бита.
pub struct JavaRandom(i64);

impl JavaRandom {
    const MULT: i64 = 0x5DEECE66D;
    const MASK: i64 = (1 << 48) - 1;

    pub fn new(seed: i64) -> Self {
        JavaRandom((seed ^ Self::MULT) & Self::MASK)
    }

    fn next(&mut self, bits: u32) -> i32 {
        self.0 = (self.0.wrapping_mul(Self::MULT).wrapping_add(0xB)) & Self::MASK;
        (self.0 >> (48 - bits)) as i32
    }

    pub fn next_int(&mut self) -> i32 {
        self.next(32)
    }

    /// `nextInt(bound)`, `bound > 0`.
    pub fn below(&mut self, bound: i32) -> i32 {
        if bound & (bound - 1) == 0 {
            return ((bound as i64 * self.next(31) as i64) >> 31) as i32;
        }
        loop {
            let u = self.next(31);
            let r = u % bound;
            if u.wrapping_sub(r).wrapping_add(bound - 1) >= 0 {
                return r;
            }
        }
    }

    pub fn next_float(&mut self) -> f32 {
        self.next(24) as f32 / (1 << 24) as f32
    }
}

pub const BOOK: &str = "minecraft:book";
pub const ENCHANTED_BOOK: &str = "minecraft:enchanted_book";
pub const LAPIS: &str = "minecraft:lapis_lazuli";

/// Сколько полок питают стол (`EnchantingTableBlock.isValidBookShelf`): кольцо
/// 5×5 на высоте 0–1 и проводник силы между полкой и столом. Не больше 15.
pub fn shelves(e: &Enchanting, name_at: impl Fn([i32; 3]) -> String) -> i32 {
    let mut n = 0;
    for y in 0..=1 {
        for x in -2..=2i32 {
            for z in -2..=2i32 {
                if x.abs() != 2 && z.abs() != 2 {
                    continue;
                }
                if e.providers.contains(&name_at([x, y, z]))
                    && e.transmitters.contains(&name_at([x / 2, y, z / 2]))
                {
                    n += 1;
                }
            }
        }
    }
    n.min(15)
}

/// Подсказки окна: стоимости, по одной чаре и её уровню на вариант (-1 — нет).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Offer {
    pub costs: [i32; 3],
    pub clue: [i32; 3],
    pub level: [i32; 3],
}

impl Offer {
    /// Свойства окна (`container_set_data` 0–9): стоимости, зерно, чары, уровни.
    pub fn data(&self, seed: i32) -> [i32; 10] {
        let [c0, c1, c2] = self.costs;
        let [e0, e1, e2] = self.clue;
        let [l0, l1, l2] = self.level;
        [c0, c1, c2, seed & -16, e0, e1, e2, l0, l1, l2]
    }
}

/// `slotsChanged`: варианты для предмета `item` при `shelves` полках.
pub fn offer(e: &Enchanting, item: &str, shelves: i32, seed: i32) -> Offer {
    let mut o = Offer {
        clue: [-1; 3],
        level: [-1; 3],
        ..Default::default()
    };
    if !e.enchantable.contains_key(item) {
        return o;
    }
    let shelves = shelves.min(15);
    let mut r = JavaRandom::new(seed as i64);
    for slot in 0..3 {
        // `getEnchantmentCost`.
        let i = r.below(8) + 1 + (shelves >> 1) + r.below(shelves + 1);
        let cost = match slot {
            0 => (i / 3).max(1),
            1 => i * 2 / 3 + 1,
            _ => i.max(shelves * 2),
        };
        o.costs[slot] = if cost < slot as i32 + 1 { 0 } else { cost };
    }
    for slot in 0..3 {
        if o.costs[slot] > 0 {
            // Как в ваниле: подсказку берёт тот же генератор после `getEnchantmentList`.
            let (list, mut r) = enchantments_rng(e, item, slot as i32, o.costs[slot], seed);
            if !list.is_empty() {
                let pick = list[r.below(list.len() as i32) as usize];
                o.clue[slot] = pick.0;
                o.level[slot] = pick.1;
            }
        }
    }
    o
}

/// `getEnchantmentList`: итог варианта `slot` стоимостью `cost` — (сетевой ID, уровень).
pub fn enchantments(
    e: &Enchanting,
    item: &str,
    slot: i32,
    cost: i32,
    seed: i32,
) -> Vec<(i32, i32)> {
    enchantments_rng(e, item, slot, cost, seed).0
}

fn enchantments_rng(
    e: &Enchanting,
    item: &str,
    slot: i32,
    cost: i32,
    seed: i32,
) -> (Vec<(i32, i32)>, JavaRandom) {
    let mut r = JavaRandom::new(seed.wrapping_add(slot) as i64);
    let mut list = select(e, item, cost, &mut r);
    if item == BOOK && list.len() > 1 {
        list.remove(r.below(list.len() as i32) as usize);
    }
    (list, r)
}

/// `selectEnchantment`: уровень с разбросом от зачаряемости, затем чары по весу.
fn select(e: &Enchanting, item: &str, level: i32, r: &mut JavaRandom) -> Vec<(i32, i32)> {
    let Some(&value) = e.enchantable.get(item) else {
        return Vec::new();
    };
    let mut level = level + 1 + r.below(value / 4 + 1) + r.below(value / 4 + 1);
    let spread = (r.next_float() + r.next_float() - 1.0) * 0.15;
    // `Math.round(float)`: floor(x + 0.5).
    level = ((level as f32 + level as f32 * spread + 0.5).floor() as i32).max(1);
    let mut avail = available(e, item, level);
    let mut out: Vec<(&EnchantDef, i32)> = Vec::new();
    if let Some(first) = pick(&avail, r) {
        out.push(first);
        while r.below(50) <= level {
            let last = out.last().unwrap().0;
            avail.retain(|(d, _)| d.compatible(last));
            if avail.is_empty() {
                break;
            }
            let Some(next) = pick(&avail, r) else {
                break;
            };
            out.push(next);
            level /= 2;
        }
    }
    out.into_iter().map(|(d, l)| (d.id, l)).collect()
}

/// `getAvailableEnchantmentResults`: высший уровень каждой чары, чья стоимость в окне;
/// порядок — как в теге `in_enchanting_table`.
fn available<'a>(e: &'a Enchanting, item: &str, level: i32) -> Vec<(&'a EnchantDef, i32)> {
    e.table
        .iter()
        .filter_map(|&i| e.defs.get(i))
        .filter(|d| item == BOOK || d.primary.contains(item))
        .filter_map(|d| {
            (1..=d.max_level)
                .rev()
                .find(|&l| level >= d.min_cost(l) && level <= d.max_cost(l))
                .map(|l| (d, l))
        })
        .collect()
}

fn pick<'a>(avail: &[(&'a EnchantDef, i32)], r: &mut JavaRandom) -> Option<(&'a EnchantDef, i32)> {
    let total: i32 = avail.iter().map(|(d, _)| d.weight.max(0)).sum();
    if total <= 0 {
        return None;
    }
    let mut n = r.below(total);
    avail.iter().copied().find(|(d, _)| {
        n -= d.weight.max(0);
        n < 0
    })
}

/// `EnchantmentHelper.getDamageProtection`: сумма защиты от чар брони для
/// типа урона `damage_type`.
pub fn damage_protection(
    e: &Enchanting,
    equipment: &[(&str, &[(i32, i32)])],
    damage_type: &str,
) -> f32 {
    let mut sum = 0.0;
    for &(slot, list) in equipment {
        for &(id, level) in list {
            let Some(d) = e.defs.iter().find(|d| d.id == id) else {
                continue;
            };
            if !d.slots.iter().any(|g| in_group(g, slot)) {
                continue;
            }
            for (req, value) in &d.protection {
                if req
                    .iter()
                    .all(|(set, want)| set.contains(damage_type) == *want)
                {
                    sum += value.calc(level);
                }
            }
        }
    }
    sum
}

/// `CombatRules.getDamageAfterMagicAbsorb`: защита до 20, по 4% за единицу.
pub fn after_protection(amount: f32, protection: f32) -> f32 {
    amount * (1.0 - protection.clamp(0.0, 20.0) / 25.0)
}

/// Подходит ли слот снаряжения (`mainhand`, `head`…) группе из `slots` чар.
fn in_group(group: &str, slot: &str) -> bool {
    match group {
        "any" => true,
        "hand" => matches!(slot, "mainhand" | "offhand"),
        "armor" => matches!(slot, "head" | "chest" | "legs" | "feet"),
        g => g == slot,
    }
}

/// Базовые значения атрибутов, которые меняют чары (`Attributes`).
pub fn attribute_base(name: &str) -> f64 {
    match name {
        "minecraft:submerged_mining_speed" => 0.2,
        "minecraft:sneaking_speed" => 0.3,
        "minecraft:burning_time" => 1.0,
        _ => 0.0,
    }
}

/// `EnchantmentHelper.forEachModifier`: модификаторы атрибутов от чар
/// снаряжения. `equipment` — (слот, чары предмета). Ключ — имя атрибута.
pub fn attribute_modifiers(
    e: &Enchanting,
    equipment: &[(&str, &[(i32, i32)])],
) -> std::collections::BTreeMap<String, Vec<(String, f64, i32)>> {
    let mut out: std::collections::BTreeMap<String, Vec<(String, f64, i32)>> = Default::default();
    for &(slot, list) in equipment {
        for &(id, level) in list {
            let Some(d) = e.defs.iter().find(|d| d.id == id) else {
                continue;
            };
            if !d.slots.iter().any(|g| in_group(g, slot)) {
                continue;
            }
            for a in &d.attributes {
                out.entry(a.attribute.clone()).or_default().push((
                    format!("{}/{slot}", a.id),
                    a.amount.calc(level) as f64,
                    a.op,
                ));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn data() -> Enchanting {
        let pick = |s: &str| HashSet::from([s.to_owned()]);
        let def = |id, name: &str, max, min_base, min_step, items: HashSet<String>| EnchantDef {
            id,
            name: name.into(),
            max_level: max,
            weight: 10,
            anvil_cost: 1,
            min_base,
            min_step,
            max_base: min_base + 50,
            max_step: min_step,
            supported: items.clone(),
            primary: items,
            exclusive: HashSet::new(),
            in_table: true,
            curse: false,
            slots: vec!["mainhand".into()],
            attributes: Vec::new(),
            protection: Vec::new(),
        };
        let mut e = Enchanting::default();
        e.defs.push(def(
            0,
            "minecraft:efficiency",
            5,
            1,
            10,
            pick("minecraft:diamond_pickaxe"),
        ));
        e.defs.push(def(
            1,
            "minecraft:unbreaking",
            3,
            5,
            8,
            pick("minecraft:diamond_pickaxe"),
        ));
        e.defs.push(def(
            2,
            "minecraft:sharpness",
            5,
            1,
            11,
            pick("minecraft:diamond_sword"),
        ));
        e.table = vec![0, 1, 2];
        e.enchantable.insert("minecraft:diamond_pickaxe".into(), 10);
        e.enchantable.insert(BOOK.into(), 1);
        e.providers = pick("minecraft:bookshelf");
        e.transmitters = pick("minecraft:air");
        e
    }

    #[test]
    fn efficiency_in_main_hand_only() {
        use chunkwake_data::enchanting::{AttrEffect, LevelValue};
        let mut e = data();
        e.defs[0].attributes.push(AttrEffect {
            attribute: "minecraft:mining_efficiency".into(),
            id: "minecraft:enchantment.efficiency".into(),
            op: 0,
            amount: LevelValue::LevelsSquared { added: 1.0 },
        });
        let m = attribute_modifiers(&e, &[("mainhand", &[(0, 5)]), ("head", &[(0, 3)])]);
        assert_eq!(
            m["minecraft:mining_efficiency"],
            vec![(
                "minecraft:enchantment.efficiency/mainhand".to_owned(),
                26.0,
                0
            )]
        );
    }

    #[test]
    fn java_random_matches_jdk() {
        // new java.util.Random(42): nextInt(), nextInt(10), nextFloat().
        assert_eq!(JavaRandom::new(42).next_int(), -1170105035);
        assert_eq!(JavaRandom::new(42).below(10), 0);
        assert!((JavaRandom::new(42).next_float() - 0.7275637).abs() < 1e-6);
    }

    #[test]
    fn full_ring_gives_fifteen() {
        let e = data();
        let ring = |p: [i32; 3]| {
            if p[0].abs() == 2 || p[2].abs() == 2 {
                "minecraft:bookshelf"
            } else {
                "minecraft:air"
            }
            .to_owned()
        };
        assert_eq!(shelves(&e, ring), 15);
        assert_eq!(shelves(&e, |_| "minecraft:air".to_owned()), 0);
    }

    #[test]
    fn offers_are_stable_and_match_result() {
        let e = data();
        for seed in 0..200 {
            let o = offer(&e, "minecraft:diamond_pickaxe", 15, seed);
            assert_eq!(o, offer(&e, "minecraft:diamond_pickaxe", 15, seed));
            // Полный набор полок: третий вариант 30 уровней (`max(i, 2 × 15)`, i ≤ 30).
            assert_eq!(o.costs[2], 30, "{o:?}");
            for slot in 0..3 {
                let list = enchantments(
                    &e,
                    "minecraft:diamond_pickaxe",
                    slot as i32,
                    o.costs[slot],
                    seed,
                );
                assert!(
                    list.contains(&(o.clue[slot], o.level[slot])),
                    "{o:?} {list:?}"
                );
                // Меча нет среди вариантов кирки.
                assert!(list.iter().all(|&(id, _)| id != 2));
            }
        }
        // Незачаряемое — без вариантов.
        assert_eq!(offer(&e, "minecraft:stone", 15, 1).costs, [0; 3]);
    }
}
