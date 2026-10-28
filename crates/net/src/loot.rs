//! Дроп блоков по ванильным таблицам лута (`LootTable.getRandomItems`):
//! пулы, записи (`item`, `alternatives`, `group`, `sequence`), условия
//! (Шёлковое касание и ножницы, `table_bonus`, `match_block`, случайный шанс)
//! и функции (`set_count`, `apply_bonus` с Удачей, `limit_count`).
//! Для сущностей: вложенные таблицы (`loot_table`), `killed_by_player`,
//! свойства моба (`entity_properties`), Добыча (`enchanted_count_increase`)
//! и переплавка огнём (`furnace_smelt`).
//! Чего не понимаем (`dynamic`, теги, копирование компонентов) — пропускаем.

use chunkwake_data::world::BlockLoot;
use rand::Rng;
use serde_json::Value;

/// Что ломали и чем.
pub struct Ctx<'a> {
    /// Свойство сломанного блока (`age`, `type` и т. п.).
    pub prop: &'a dyn Fn(&str) -> Option<String>,
    pub block: &'a str,
    /// Имя предмета в руке.
    pub tool: Option<&'a str>,
    /// Уровень чар инструмента.
    pub level: &'a dyn Fn(&str) -> i32,
    /// Свойства убитой сущности (`color`, `sheared`, `on_fire`); `None` — ломают блок.
    pub entity: Option<&'a Props<'a>>,
    pub killed_by_player: bool,
    /// Результат переплавки предмета (`furnace_smelt`).
    pub smelt: &'a dyn Fn(&str) -> Option<String>,
}

/// Свойство сущности по ключу.
pub type Props<'a> = dyn Fn(&str) -> Option<String> + 'a;

/// Предметы (имя, количество) из таблицы блока `block`.
pub fn roll(loot: &BlockLoot, c: &Ctx, rng: &mut impl Rng) -> Vec<(String, i32)> {
    let mut out = Vec::new();
    roll_table(loot, c, c.block, rng, &mut out, 0);
    out
}

fn roll_table(
    loot: &BlockLoot,
    c: &Ctx,
    name: &str,
    rng: &mut impl Rng,
    out: &mut Vec<(String, i32)>,
    depth: u8,
) {
    let Some(table) = loot.tables.get(name) else {
        return;
    };
    let e = Eval { loot, c };
    for pool in list(table.get("pools")) {
        if !e.conditions(pool, rng) {
            continue;
        }
        let rolls = pool
            .get("rolls")
            .map_or(1, |r| e.number(r, rng).round() as i32);
        for _ in 0..rolls {
            // Все подходящие записи, затем одна по весу.
            let mut chosen = Vec::new();
            for entry in list(pool.get("entries")) {
                e.expand(entry, rng, &mut chosen);
            }
            let total: i64 = chosen.iter().map(|x| weight(x)).sum();
            if total <= 0 {
                continue;
            }
            let mut pick = rng.gen_range(0..total);
            let Some(item) = chosen.into_iter().find(|x| {
                pick -= weight(x);
                pick < 0
            }) else {
                continue;
            };
            // Вложенная таблица (шерсть овцы по цвету).
            if kind(item) == "minecraft:loot_table" {
                if let Some(sub) = item.get("value").and_then(Value::as_str)
                    && depth < 4
                {
                    roll_table(loot, c, sub, rng, out, depth + 1);
                }
                continue;
            }
            let Some(name) = item.get("name").and_then(Value::as_str) else {
                continue;
            };
            let mut name = name.to_owned();
            let mut count = 1;
            for f in list(item.get("modifier"))
                .chain(list(pool.get("modifier")))
                .chain(list(table.get("modifier")))
            {
                count = e.function(f, count, &mut name, rng);
            }
            if count > 0 {
                out.push((name, count));
            }
        }
    }
}

/// Значение или список значений как итератор.
fn list(v: Option<&Value>) -> impl Iterator<Item = &Value> {
    let (one, many) = match v {
        Some(Value::Array(a)) => (None, a.as_slice()),
        Some(v) => (Some(v), &[][..]),
        None => (None, &[][..]),
    };
    one.into_iter().chain(many)
}

fn weight(entry: &Value) -> i64 {
    entry.get("weight").and_then(Value::as_i64).unwrap_or(1)
}

fn kind(v: &Value) -> &str {
    v.get("type").and_then(Value::as_str).unwrap_or("")
}

struct Eval<'a> {
    loot: &'a BlockLoot,
    c: &'a Ctx<'a>,
}

impl Eval<'_> {
    /// Раскрывает запись в листья-предметы; `false` — условия записи не выполнены
    /// (для `alternatives` и `sequence`).
    fn expand<'v>(&self, e: &'v Value, rng: &mut impl Rng, out: &mut Vec<&'v Value>) -> bool {
        if !self.conditions(e, rng) {
            return false;
        }
        let children = || list(e.get("children"));
        match kind(e) {
            "minecraft:item" | "minecraft:loot_table" => out.push(e),
            "minecraft:alternatives" => {
                for ch in children() {
                    if self.expand(ch, rng, out) {
                        break;
                    }
                }
            }
            "minecraft:group" => {
                for ch in children() {
                    self.expand(ch, rng, out);
                }
            }
            "minecraft:sequence" => {
                for ch in children() {
                    if !self.expand(ch, rng, out) {
                        break;
                    }
                }
            }
            "minecraft:empty" => {}
            _ => return false,
        }
        true
    }

    /// Все условия `condition` объекта.
    fn conditions(&self, v: &Value, rng: &mut impl Rng) -> bool {
        list(v.get("condition")).all(|c| self.condition(c, rng))
    }

    fn condition(&self, c: &Value, rng: &mut impl Rng) -> bool {
        if let Value::String(id) = c {
            return self
                .loot
                .predicates
                .get(id)
                .is_some_and(|p| self.condition(p, rng));
        }
        if let Value::Array(a) = c {
            return a.iter().all(|c| self.condition(c, rng));
        }
        let terms = || list(c.get("terms"));
        match kind(c) {
            "minecraft:survives_explosion" => true,
            "minecraft:inverted" => c.get("term").is_some_and(|t| !self.condition(t, rng)),
            "minecraft:any_of" => terms().any(|t| self.condition(t, rng)),
            "minecraft:all_of" => terms().all(|t| self.condition(t, rng)),
            "minecraft:random_chance" => {
                let p = c.get("chance").map_or(0.0, |v| self.number(v, rng));
                rng.r#gen::<f64>() < p
            }
            "minecraft:table_bonus" => {
                let chances: Vec<f64> = list(c.get("chances")).filter_map(Value::as_f64).collect();
                let lvl = self.enchant(c.get("enchantment")) as usize;
                chances
                    .get(lvl.min(chances.len().saturating_sub(1)))
                    .is_some_and(|&p| rng.r#gen::<f64>() < p)
            }
            "minecraft:match_block" => {
                let names: Vec<&str> = list(c.get("blocks")).filter_map(Value::as_str).collect();
                (names.is_empty() || names.contains(&self.c.block))
                    && c.get("state")
                        .and_then(Value::as_object)
                        .is_none_or(|state| {
                            state.iter().all(|(k, want)| {
                                let have = (self.c.prop)(k);
                                match (want, have) {
                                    (Value::String(w), Some(h)) => *w == h,
                                    (Value::Object(r), Some(h)) => {
                                        let n: i64 = h.parse().unwrap_or(i64::MIN);
                                        let ok = |key, f: fn(i64, i64) -> bool| {
                                            r.get(key)
                                                .and_then(Value::as_i64)
                                                .is_none_or(|b| f(n, b))
                                        };
                                        ok("min", |n, b| n >= b) && ok("max", |n, b| n <= b)
                                    }
                                    _ => false,
                                }
                            })
                        })
            }
            "minecraft:match_tool" => self.tool_matches(c.get("predicate")),
            "minecraft:killed_by_player" => self.c.killed_by_player,
            "minecraft:entity_properties" => {
                let Some(p) = c.get("predicate").and_then(Value::as_object) else {
                    return false;
                };
                // Ломает игрок: пустой предикат.
                let Some(this) = self.c.entity else {
                    return p.is_empty();
                };
                let target = c.get("entity").and_then(Value::as_str).unwrap_or("this");
                p.iter().all(|(k, v)| match (target, k.as_str()) {
                    ("this", "minecraft:flags") => v.as_object().is_some_and(|f| {
                        f.iter().all(|(flag, want)| {
                            let have = match flag.as_str() {
                                "is_on_fire" => this("on_fire"),
                                _ => None,
                            };
                            want.as_bool() == Some(have.as_deref() == Some("true"))
                        })
                    }),
                    ("this", "minecraft:components") => v.as_object().is_some_and(|m| {
                        m.iter().all(|(comp, want)| {
                            let have = match comp.as_str() {
                                "minecraft:sheep/color" => this("color"),
                                _ => None,
                            };
                            want.as_str().is_some_and(|w| have.as_deref() == Some(w))
                        })
                    }),
                    ("this", "minecraft:type_specific/sheep") => {
                        v.get("sheared").and_then(Value::as_bool).is_none_or(|w| {
                            this("sheared").as_deref() == Some(if w { "true" } else { "false" })
                        })
                    }
                    (_, "minecraft:equipment") => v
                        .get("mainhand")
                        .is_some_and(|h| self.tool_matches(Some(h))),
                    _ => false,
                })
            }
            _ => false,
        }
    }

    /// Предикат предмета: `items` (без тегов) и чары с минимальным уровнем.
    fn tool_matches(&self, p: Option<&Value>) -> bool {
        let Some(p) = p else {
            return true;
        };
        if let Some(items) = p.get("items") {
            let names: Vec<&str> = list(Some(items)).filter_map(Value::as_str).collect();
            if !self.c.tool.is_some_and(|t| names.contains(&t)) {
                return false;
            }
        }
        let enchantments = p
            .get("predicates")
            .and_then(|x| x.get("minecraft:enchantments"));
        list(enchantments).all(|e| {
            let lvl = self.enchant(e.get("enchantments"));
            let min = e
                .get("levels")
                .and_then(|l| l.get("min"))
                .and_then(Value::as_i64)
                .unwrap_or(1);
            lvl as i64 >= min
        })
    }

    fn enchant(&self, name: Option<&Value>) -> i32 {
        name.and_then(Value::as_str).map_or(0, |n| match n {
            // Тег `smelts_loot` — Заговор огня.
            "#minecraft:smelts_loot" => (self.c.level)("minecraft:fire_aspect"),
            n => (self.c.level)(n),
        })
    }

    /// Число: константа, `uniform`, `binomial`, `constant`.
    fn number(&self, v: &Value, rng: &mut impl Rng) -> f64 {
        if let Some(n) = v.as_f64() {
            return n;
        }
        let mut f = |k| v.get(k).map_or(0.0, |x| self.number(x, rng));
        match kind(v) {
            "minecraft:uniform" => {
                let (lo, hi) = (f("min"), f("max"));
                if hi > lo { rng.gen_range(lo..=hi) } else { lo }
            }
            "minecraft:binomial" => {
                let (n, p) = (f("n") as i32, f("p"));
                (0..n).filter(|_| rng.r#gen::<f64>() < p).count() as f64
            }
            "minecraft:constant" => f("value"),
            _ => 0.0,
        }
    }

    /// Функция предмета над количеством.
    fn function(&self, f: &Value, count: i32, name: &mut String, rng: &mut impl Rng) -> i32 {
        if !self.conditions(f, rng) {
            return count;
        }
        match kind(f) {
            "minecraft:furnace_smelt" => {
                if let Some(cooked) = (self.c.smelt)(name) {
                    *name = cooked;
                }
                count
            }
            // `EnchantedCountIncreaseFunction`: + уровень × число, округлённо.
            "minecraft:enchanted_count_increase" => {
                let lvl = self.enchant(f.get("enchantment"));
                if lvl == 0 {
                    return count;
                }
                let add = f.get("count").map_or(0.0, |v| self.number(v, rng)) * lvl as f64;
                let n = count + add.round() as i32;
                match f.get("limit").and_then(Value::as_i64) {
                    Some(l) if l > 0 => n.min(l as i32),
                    _ => n,
                }
            }
            "minecraft:set_count" => {
                // Целые из `uniform` — как `IntRange`: равномерно по целым.
                let n = match f.get("count") {
                    Some(v) if kind(v) == "minecraft:uniform" => {
                        let lo = v.get("min").map_or(0.0, |x| self.number(x, rng)).floor() as i32;
                        let hi = v.get("max").map_or(0.0, |x| self.number(x, rng)).floor() as i32;
                        rng.gen_range(lo..=hi.max(lo))
                    }
                    Some(v) => self.number(v, rng).floor() as i32,
                    None => 1,
                };
                if f.get("add").and_then(Value::as_bool) == Some(true) {
                    count + n
                } else {
                    n
                }
            }
            "minecraft:apply_bonus" => {
                let lvl = self.enchant(f.get("enchantment"));
                let param = |k| {
                    f.get("parameters")
                        .and_then(|p| p.get(k))
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0)
                };
                match f.get("formula").and_then(Value::as_str).unwrap_or("") {
                    "minecraft:ore_drops" if lvl > 0 => {
                        let bonus = (rng.gen_range(0..lvl + 2) - 1).max(0);
                        count * (bonus + 1)
                    }
                    "minecraft:uniform_bonus_count" => {
                        count + rng.gen_range(0..=(param("bonusMultiplier") as i32 * lvl).max(0))
                    }
                    "minecraft:binomial_with_bonus_count" => {
                        let n = param("extra") as i32 + lvl;
                        count
                            + (0..n)
                                .filter(|_| rng.r#gen::<f64>() < param("probability"))
                                .count() as i32
                    }
                    _ => count,
                }
            }
            "minecraft:limit_count" => {
                let l = f.get("limit");
                let mut b = |k| l.and_then(|l| l.get(k)).map(|v| self.number(v, rng) as i32);
                let mut n = count;
                if let Some(lo) = b("min") {
                    n = n.max(lo);
                }
                if let Some(hi) = b("max") {
                    n = n.min(hi);
                }
                n
            }
            _ => count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use serde_json::json;

    fn loot() -> BlockLoot {
        let mut l = BlockLoot::default();
        l.predicates.insert(
            "minecraft:tool/can_silk_touch".into(),
            json!({"type": "minecraft:match_tool", "predicate": {"predicates": {"minecraft:enchantments": [
                {"enchantments": "minecraft:silk_touch", "levels": {"min": 1}}]}}}),
        );
        l.tables.insert(
            "minecraft:diamond_ore".into(),
            json!({"pools": [{"rolls": 1, "entries": [{"type": "minecraft:alternatives", "children": [
                {"type": "minecraft:item", "condition": "minecraft:tool/can_silk_touch", "name": "minecraft:diamond_ore"},
                {"type": "minecraft:item", "name": "minecraft:diamond", "modifier": [
                    {"type": "minecraft:apply_bonus", "enchantment": "minecraft:fortune", "formula": "minecraft:ore_drops"},
                    {"type": "minecraft:explosion_decay"}]}]}]}]}),
        );
        l.tables.insert(
            "minecraft:wheat".into(),
            json!({"pools": [{"rolls": 1, "entries": [{"type": "minecraft:alternatives", "children": [
                {"type": "minecraft:item", "name": "minecraft:wheat",
                 "condition": {"type": "minecraft:match_block", "blocks": "minecraft:wheat", "state": {"age": "7"}}},
                {"type": "minecraft:item", "name": "minecraft:wheat_seeds"}]}]}]}),
        );
        l
    }

    fn drops(
        block: &str,
        silk: i32,
        fortune: i32,
        age: &str,
        rng: &mut impl Rng,
    ) -> Vec<(String, i32)> {
        let level = |n: &str| match n {
            "minecraft:silk_touch" => silk,
            "minecraft:fortune" => fortune,
            _ => 0,
        };
        let prop = |k: &str| (k == "age").then(|| age.to_owned());
        roll(
            &loot(),
            &Ctx {
                prop: &prop,
                block,
                tool: Some("minecraft:diamond_pickaxe"),
                level: &level,
                entity: None,
                killed_by_player: true,
                smelt: &|_| None,
            },
            rng,
        )
    }

    #[test]
    fn entity_tables() {
        let mut l = BlockLoot::default();
        l.tables.insert(
            "minecraft:entities/sheep".into(),
            json!({"pools": [
                {"rolls": 1, "entries": [{"type": "minecraft:item", "name": "minecraft:mutton", "modifier": [
                    {"type": "minecraft:set_count", "count": 1},
                    {"type": "minecraft:furnace_smelt", "condition": {"type": "minecraft:entity_properties",
                        "entity": "direct_attacker", "predicate": {"minecraft:equipment": {"mainhand": {"predicates":
                        {"minecraft:enchantments": [{"enchantments": "#minecraft:smelts_loot"}]}}}}}},
                    {"type": "minecraft:enchanted_count_increase", "count": 1.0, "enchantment": "minecraft:looting"}]}]},
                {"rolls": 1, "entries": [{"type": "minecraft:alternatives", "children": [
                    {"type": "minecraft:loot_table", "value": "minecraft:entities/sheep/red", "condition": {
                        "type": "minecraft:entity_properties", "entity": "this", "predicate": {
                            "minecraft:components": {"minecraft:sheep/color": "red"},
                            "minecraft:type_specific/sheep": {"sheared": false}}}}]}]}
            ]}),
        );
        l.tables.insert(
            "minecraft:entities/sheep/red".into(),
            json!({"pools": [{"rolls": 1, "entries": [{"type": "minecraft:item", "name": "minecraft:red_wool"}]}]}),
        );
        let run = |sheared: &str, fire: i32, looting: i32| {
            let level = |n: &str| match n {
                "minecraft:fire_aspect" => fire,
                "minecraft:looting" => looting,
                _ => 0,
            };
            let props = |k: &str| match k {
                "color" => Some("red".to_owned()),
                "sheared" => Some(sheared.to_owned()),
                _ => None,
            };
            let smelt =
                |n: &str| (n == "minecraft:mutton").then(|| "minecraft:cooked_mutton".to_owned());
            roll(
                &l,
                &Ctx {
                    prop: &|_| None,
                    block: "minecraft:entities/sheep",
                    tool: None,
                    level: &level,
                    entity: Some(&props),
                    killed_by_player: true,
                    smelt: &smelt,
                },
                &mut rand::rngs::StdRng::seed_from_u64(3),
            )
        };
        assert_eq!(
            run("false", 0, 0),
            [
                ("minecraft:mutton".into(), 1),
                ("minecraft:red_wool".into(), 1)
            ]
        );
        assert_eq!(run("true", 1, 2), [("minecraft:cooked_mutton".into(), 3)]);
    }

    #[test]
    fn silk_touch_and_fortune() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(1);
        assert_eq!(
            drops("minecraft:diamond_ore", 0, 0, "", &mut rng),
            [("minecraft:diamond".into(), 1)]
        );
        assert_eq!(
            drops("minecraft:diamond_ore", 1, 3, "", &mut rng),
            [("minecraft:diamond_ore".into(), 1)]
        );
        // Удача III: от 1 до 4 алмазов, в среднем 2.2.
        let total: i32 = (0..1000)
            .map(|_| drops("minecraft:diamond_ore", 0, 3, "", &mut rng)[0].1)
            .sum();
        assert!((2000..2400).contains(&total), "{total}");
    }

    #[test]
    fn crop_age() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(2);
        assert_eq!(
            drops("minecraft:wheat", 0, 0, "7", &mut rng)[0].0,
            "minecraft:wheat"
        );
        assert_eq!(
            drops("minecraft:wheat", 0, 0, "3", &mut rng)[0].0,
            "minecraft:wheat_seeds"
        );
        assert!(drops("minecraft:stone", 0, 0, "", &mut rng).is_empty());
    }
}
