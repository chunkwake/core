//! Расчёт результата `AnvilMenu.createResult` для версии 26.3.

use chunkwake_data::enchanting::EnchantDef;
use chunkwake_protocol::components::{self, Extra};
use chunkwake_protocol::inventory::Slot;

pub struct Ctx<'a> {
    pub max_damage: &'a dyn Fn(i32) -> i32,
    pub repairable: &'a dyn Fn(i32, i32) -> bool,
    pub item_name: &'a dyn Fn(i32) -> &'a str,
    pub enchantments: &'a [EnchantDef],
    pub enchanted_book: i32,
    pub creative: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Result {
    pub output: Slot,
    pub cost: i32,
    pub materials: i32,
    pub only_renaming: bool,
}

fn plain_name(name: &str) -> String {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(name) else {
        return name.to_owned();
    };
    match json {
        serde_json::Value::String(s) => s,
        serde_json::Value::Object(o) => o
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or(name)
            .to_owned(),
        _ => name.to_owned(),
    }
}

fn default_name(name: &str) -> String {
    name.trim_start_matches("minecraft:")
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or(String::new(), |c| {
                c.to_uppercase().collect::<String>() + chars.as_str()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `name=None` до первого пакета переименования; пустое имя снимает прежнее имя.
pub fn compute(left: Slot, right: Slot, name: Option<&str>, ctx: &Ctx<'_>) -> Result {
    let Some(left) = left else {
        return Result::default();
    };
    let mut out = left;
    let right_extra = right.map(|s| components::get(s.extra));
    let left_extra = components::get(left.extra);
    if left_extra.raw.is_some() || right_extra.as_ref().is_some_and(|e| e.raw.is_some()) {
        return Result::default();
    }
    let mut extra: Extra = (*left_extra).clone();
    let mut work = 0i32;
    let mut materials = 0;
    let prior =
        left_extra.repair_cost as i64 + right_extra.as_ref().map_or(0, |e| e.repair_cost as i64);
    if let Some(right) = right {
        let other = right_extra.as_ref().unwrap();
        let max = (ctx.max_damage)(left.item);
        let book = !other.stored.is_empty();
        if max > 0 && (ctx.repairable)(left.item, right.item) {
            if left.damage <= 0 {
                return Result::default();
            }
            let quarter = max / 4;
            if quarter == 0 {
                return Result::default();
            }
            while materials < right.count && out.damage > 0 {
                out.damage = (out.damage - quarter).max(0);
                materials += 1;
                work = work.saturating_add(1);
            }
        } else {
            if !book && !(left.item == right.item && max > 0) {
                return Result::default();
            }
            if max > 0 && !book {
                let remaining =
                    (max - left.damage).max(0) + (max - right.damage).max(0) + max * 12 / 100;
                let damage = (max - remaining).max(0);
                if damage < out.damage {
                    out.damage = damage;
                    work = work.saturating_add(2);
                }
            }
            let source = if book {
                &other.stored
            } else {
                &other.enchantments
            };
            let dest = if left.item == ctx.enchanted_book {
                &mut extra.stored
            } else {
                &mut extra.enchantments
            };
            let mut accepted = false;
            let mut rejected = false;
            for &(id, level) in source {
                let Some(def) = ctx.enchantments.iter().find(|d| d.id == id) else {
                    continue;
                };
                let old = dest.iter().find(|e| e.0 == id).map_or(0, |e| e.1);
                let new = if old == level {
                    level.saturating_add(1)
                } else {
                    old.max(level)
                }
                .min(def.max_level);
                let mut allowed = ctx.creative
                    || left.item == ctx.enchanted_book
                    || def.supported.contains((ctx.item_name)(left.item));
                for &(existing, _) in dest.iter().filter(|e| e.0 != id) {
                    if let Some(other) = ctx.enchantments.iter().find(|d| d.id == existing) {
                        if !def.compatible(other) {
                            allowed = false;
                            work = work.saturating_add(1);
                        }
                    }
                }
                if !allowed {
                    rejected = true;
                    continue;
                }
                accepted = true;
                if let Some(e) = dest.iter_mut().find(|e| e.0 == id) {
                    e.1 = new;
                } else {
                    dest.push((id, new));
                }
                let cost = if book {
                    (def.anvil_cost / 2).max(1)
                } else {
                    def.anvil_cost.max(1)
                };
                work = work.saturating_add(cost.saturating_mul(new));
                if left.count > 1 {
                    work = 40;
                }
            }
            if rejected && !accepted {
                return Result::default();
            }
        }
    }
    let mut rename = 0;
    if let Some(name) = name {
        if name.trim().is_empty() {
            if extra.custom_name.is_some() {
                extra.custom_name = None;
                rename = 1;
                work = work.saturating_add(1);
            }
        } else {
            let hover = extra
                .custom_name
                .as_deref()
                .map(plain_name)
                .unwrap_or_else(|| default_name((ctx.item_name)(left.item)));
            if name != hover {
                extra.custom_name = Some(serde_json::Value::String(name.to_owned()).to_string());
                rename = 1;
                work = work.saturating_add(1);
            }
        }
    }
    let mut cost = if work > 0 {
        (prior + i64::from(work)).clamp(0, i32::MAX as i64) as i32
    } else {
        0
    };
    let only_renaming = rename > 0 && work == rename;
    if only_renaming {
        cost = cost.min(39);
    }
    if work == 0 || (cost >= 40 && !ctx.creative) {
        return Result {
            cost,
            only_renaming,
            ..Result::default()
        };
    }
    let highest = left_extra
        .repair_cost
        .max(right_extra.as_ref().map_or(0, |e| e.repair_cost));
    extra.repair_cost = if only_renaming {
        highest
    } else {
        highest.saturating_mul(2).saturating_add(1)
    };
    out.extra = components::intern(extra);
    Result {
        output: Some(out),
        cost,
        materials,
        only_renaming,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chunkwake_protocol::inventory::Stack;
    use std::collections::HashSet;

    fn sword(damage: i32, cost: i32) -> Stack {
        Stack {
            damage,
            extra: components::intern(Extra {
                repair_cost: cost,
                ..Default::default()
            }),
            ..Stack::new(1, 1)
        }
    }

    fn with_context(f: impl FnOnce(&Ctx<'_>)) {
        let defs = vec![EnchantDef {
            id: 3,
            name: "minecraft:sharpness".into(),
            max_level: 5,
            anvil_cost: 1,
            supported: HashSet::from(["minecraft:iron_sword".into()]),
            ..Default::default()
        }];
        let max = |item| if item == 1 { 100 } else { 0 };
        let repair = |item, material| item == 1 && material == 2;
        let name = |item| {
            if item == 1 {
                "minecraft:iron_sword"
            } else {
                "minecraft:diamond"
            }
        };
        f(&Ctx {
            max_damage: &max,
            repairable: &repair,
            item_name: &name,
            enchantments: &defs,
            enchanted_book: 7,
            creative: false,
        });
    }

    #[test]
    fn repairs_with_materials_and_combines_durability() {
        with_context(|ctx| {
            let a = compute(Some(sword(80, 0)), Some(Stack::new(2, 3)), None, ctx);
            assert_eq!((a.cost, a.materials, a.output.unwrap().damage), (3, 3, 5));
            assert_eq!(components::get(a.output.unwrap().extra).repair_cost, 1);
            let b = compute(Some(sword(60, 0)), Some(sword(50, 0)), None, ctx);
            assert_eq!((b.cost, b.materials, b.output.unwrap().damage), (2, 0, 0));
        });
    }

    #[test]
    fn book_merges_enchantments_and_prior_costs() {
        with_context(|ctx| {
            let mut left = sword(0, 3);
            left.extra = components::intern(Extra {
                enchantments: vec![(3, 2)],
                repair_cost: 3,
                ..Default::default()
            });
            let mut book = Stack::new(7, 1);
            book.extra = components::intern(Extra {
                stored: vec![(3, 2)],
                repair_cost: 1,
                ..Default::default()
            });
            let r = compute(Some(left), Some(book), None, ctx);
            assert_eq!(r.cost, 7);
            let e = components::get(r.output.unwrap().extra);
            assert_eq!(e.level(3), 3);
            assert_eq!(e.repair_cost, 7);
        });
    }

    #[test]
    fn rename_only_caps_cost_and_preserves_prior_penalty() {
        with_context(|ctx| {
            assert_eq!(
                compute(Some(sword(0, 0)), None, Some("Iron Sword"), ctx).cost,
                0
            );
            let r = compute(Some(sword(0, 80)), None, Some("New sword"), ctx);
            assert_eq!(r.cost, 39);
            assert!(r.only_renaming);
            let e = components::get(r.output.unwrap().extra);
            assert_eq!(e.repair_cost, 80);
            assert_eq!(e.custom_name.as_deref(), Some("\"New sword\""));
            assert!(
                compute(Some(sword(80, 80)), Some(Stack::new(2, 1)), None, ctx)
                    .output
                    .is_none()
            );
        });
    }
}
