//! `minecraft:lock` (`ItemPredicate`) и проверка ключа в главной руке.
//!
//! `LockCode.canUnlock`: наблюдатель открывает всё, остальные — только если
//! предмет в главной руке проходит предикат. Компонент хранится только на
//! сервере (в сети он не синхронизируется), поэтому разбор общий с NBT.

use chunkwake_protocol::components::{self, Extra};
use chunkwake_protocol::inventory::Stack;
use chunkwake_world::nbt::{Compound, Tag};
use std::collections::{HashMap, HashSet};

/// `ItemPredicate.test` для предмета в главной руке.
pub(crate) fn can_unlock(
    lock: &Compound,
    stack: Option<Stack>,
    item_name: &str,
    item_tags: &HashMap<String, HashSet<String>>,
) -> bool {
    let (count, extra) = match stack {
        Some(s) => (s.count, components::get(s.extra)),
        // `ItemStack.EMPTY`: count 0, все компоненты по умолчанию.
        None => (0, components::get(0)),
    };
    if let Some(items) = lock.get("items")
        && !items_match(items, item_name, item_tags)
    {
        return false;
    }
    if let Some(bounds) = lock.get("count")
        && !count_matches(bounds, count)
    {
        return false;
    }
    if let Some(expected) = lock.get("components")
        && !exact_components_match(expected, stack, &extra)
    {
        return false;
    }
    // Частичные предикаты (`predicates`) не поддержаны: не открываем, чтобы
    // не пустить игрока без ключа.
    if let Some(predicates) = lock.get("predicates").and_then(Tag::as_compound)
        && !predicates.is_empty()
    {
        return false;
    }
    true
}

fn normalize_id(id: &str) -> String {
    if id.contains(':') {
        id.to_owned()
    } else {
        format!("minecraft:{id}")
    }
}

fn items_match(spec: &Tag, item_name: &str, item_tags: &HashMap<String, HashSet<String>>) -> bool {
    match spec {
        Tag::String(id) => one_item_matches(id, item_name, item_tags),
        Tag::List(list) => list
            .iter()
            .filter_map(Tag::as_str)
            .any(|id| one_item_matches(id, item_name, item_tags)),
        // `HolderSet` допускает запись `{id: "...", required: ...}`.
        Tag::Compound(c) => c
            .get("id")
            .and_then(Tag::as_str)
            .is_some_and(|id| one_item_matches(id, item_name, item_tags)),
        _ => false,
    }
}

fn one_item_matches(
    id: &str,
    item_name: &str,
    item_tags: &HashMap<String, HashSet<String>>,
) -> bool {
    if item_name.is_empty() {
        return false;
    }
    match id.strip_prefix('#') {
        Some(tag) => item_tags
            .get(&normalize_id(tag))
            .is_some_and(|set| set.contains(item_name)),
        None => normalize_id(id) == item_name,
    }
}

fn count_matches(bounds: &Tag, count: i32) -> bool {
    match bounds {
        Tag::Compound(c) => {
            let min = c.get("min").and_then(Tag::as_i64);
            let max = c.get("max").and_then(Tag::as_i64);
            min.is_none_or(|v| count as i64 >= v) && max.is_none_or(|v| count as i64 <= v)
        }
        other => other.as_i64() == Some(count as i64),
    }
}

/// `DataComponentMatchers.exact`: перечисленные компоненты должны совпадать.
fn exact_components_match(expected: &Tag, stack: Option<Stack>, extra: &Extra) -> bool {
    let Some(map) = expected.as_compound() else {
        return false;
    };
    let damage = stack.map_or(0, |s| s.damage);
    for (key, want) in map {
        let actual = match key.as_str() {
            "minecraft:custom_name" => extra
                .custom_name
                .as_deref()
                .map(|n| crate::stack_nbt::json_to_tag(&crate::stack_nbt::text_json(n))),
            "minecraft:item_name" => extra
                .item_name
                .as_deref()
                .map(|n| crate::stack_nbt::json_to_tag(&crate::stack_nbt::text_json(n))),
            "minecraft:damage" => Some(Tag::Int(damage)),
            "minecraft:custom_data" => Some(Tag::Compound(
                extra
                    .custom_data
                    .as_deref()
                    .and_then(crate::stack_nbt::net_to_compound)
                    .unwrap_or_default(),
            )),
            "minecraft:lore" => Some(Tag::List(
                extra
                    .lore
                    .iter()
                    .map(|l| crate::stack_nbt::json_to_tag(&crate::stack_nbt::text_json(l)))
                    .collect(),
            )),
            "minecraft:unbreakable" => extra.unbreakable.then(|| Tag::Compound(Compound::new())),
            _ => return false,
        };
        if actual.as_ref() != Some(want) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compound(text: &str) -> Compound {
        crate::play::commands::snbt::parse_value(text)
            .and_then(|t| t.as_compound().cloned())
            .unwrap()
    }

    fn paper() -> Stack {
        Stack::new(1, 1)
    }

    fn tags() -> HashMap<String, HashSet<String>> {
        HashMap::from([(
            "minecraft:keys".into(),
            HashSet::from(["minecraft:paper".into()]),
        )])
    }

    #[test]
    fn items_predicate_checks_main_hand() {
        let lock = compound(r#"{items:"minecraft:paper"}"#);
        assert!(can_unlock(&lock, Some(paper()), "minecraft:paper", &tags()));
        assert!(!can_unlock(
            &lock,
            Some(paper()),
            "minecraft:stone",
            &tags()
        ));
        assert!(!can_unlock(&lock, None, "", &tags()));
    }

    #[test]
    fn items_predicate_accepts_lists_tags_and_shorthand() {
        let lock = compound(r#"{items:["minecraft:stone","minecraft:paper"]}"#);
        assert!(can_unlock(&lock, Some(paper()), "minecraft:paper", &tags()));
        let lock = compound(r##"{items:"#keys"}"##);
        assert!(can_unlock(&lock, Some(paper()), "minecraft:paper", &tags()));
        let lock = compound(r##"{items:"#minecraft:keys"}"##);
        assert!(can_unlock(&lock, Some(paper()), "minecraft:paper", &tags()));
        let lock = compound(r#"{items:"paper"}"#);
        assert!(can_unlock(&lock, Some(paper()), "minecraft:paper", &tags()));
    }

    #[test]
    fn count_bounds_match_item_count() {
        let lock = compound(r#"{items:"minecraft:paper",count:{min:2}}"#);
        assert!(!can_unlock(
            &lock,
            Some(paper()),
            "minecraft:paper",
            &tags()
        ));
        let two = Stack::new(1, 2);
        assert!(can_unlock(&lock, Some(two), "minecraft:paper", &tags()));
        let exact = compound(r#"{items:"minecraft:paper",count:1}"#);
        assert!(can_unlock(
            &exact,
            Some(paper()),
            "minecraft:paper",
            &tags()
        ));
    }

    #[test]
    fn exact_custom_name_unlocks_only_the_named_key() {
        let lock = compound(r#"{components:{"minecraft:custom_name":"Key"}}"#);
        let named = Stack {
            extra: components::intern(Extra {
                custom_name: Some(r#""Key""#.into()),
                ..Default::default()
            }),
            ..paper()
        };
        assert!(can_unlock(&lock, Some(named), "minecraft:paper", &tags()));
        let other = Stack {
            extra: components::intern(Extra {
                custom_name: Some(r#""Other""#.into()),
                ..Default::default()
            }),
            ..paper()
        };
        assert!(!can_unlock(&lock, Some(other), "minecraft:paper", &tags()));
    }

    #[test]
    fn unsupported_predicates_fail_closed() {
        let lock = compound(r#"{predicates:{"minecraft:custom_data":{a:1b}}}"#);
        assert!(!can_unlock(
            &lock,
            Some(paper()),
            "minecraft:paper",
            &tags()
        ));
        let lock = compound(r#"{components:{"minecraft:enchantments":{}}}"#);
        assert!(!can_unlock(
            &lock,
            Some(paper()),
            "minecraft:paper",
            &tags()
        ));
    }

    #[test]
    fn empty_lock_matches_any_item_including_empty_hand() {
        let lock = compound("{}");
        assert!(can_unlock(&lock, Some(paper()), "minecraft:paper", &tags()));
        assert!(can_unlock(&lock, None, "", &tags()));
    }
}
