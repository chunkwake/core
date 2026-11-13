//! Результат кузнечного стола (`smithing_transform`, `smithing_trim`).

use chunkwake_data::recipes::{SmithingData, SmithingKind};
use chunkwake_protocol::components::{self, Extra};
use chunkwake_protocol::inventory::{Slot, Stack};

pub fn result(slots: &[Slot], data: &SmithingData) -> Slot {
    let [Some(template), Some(base), Some(addition), ..] = slots else {
        return None;
    };
    let recipe = data.find(template.item, base.item, addition.item)?;
    match recipe.kind {
        SmithingKind::Transform(item) => Some(Stack {
            item,
            count: 1,
            ..*base
        }),
        SmithingKind::Trim(pattern) => {
            let material = *data.materials.get(&addition.item)?;
            let mut extra: Extra = (*components::get(base.extra)).clone();
            if extra.trim == Some((material, pattern)) {
                return None;
            }
            extra.trim = Some((material, pattern));
            Some(Stack {
                count: 1,
                extra: components::intern(extra),
                ..*base
            })
        }
    }
}

pub fn accepts(data: &SmithingData, slot: usize, item: i32) -> bool {
    data.recipes.iter().any(|r| match slot {
        0 => r.template.as_ref().is_some_and(|i| i.contains(&item)),
        1 => r.base.contains(&item),
        2 => r.addition.as_ref().is_some_and(|i| i.contains(&item)),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chunkwake_data::recipes::{Smithing, SmithingKind};
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;

    fn ing(item: i32) -> Arc<HashSet<i32>> {
        Arc::new(HashSet::from([item]))
    }

    #[test]
    fn transform_keeps_base_components_and_trim_changes_only_trim() {
        let data = SmithingData {
            recipes: vec![
                Smithing {
                    id: "minecraft:transform".into(),
                    template: Some(ing(1)),
                    base: ing(2),
                    addition: Some(ing(3)),
                    kind: SmithingKind::Transform(4),
                },
                Smithing {
                    id: "minecraft:trim".into(),
                    template: Some(ing(5)),
                    base: ing(4),
                    addition: Some(ing(6)),
                    kind: SmithingKind::Trim(7),
                },
            ],
            materials: HashMap::from([(6, 8)]),
        };
        let extra = components::intern(Extra {
            custom_name: Some("\"Sword\"".into()),
            ..Default::default()
        });
        let base = Stack {
            count: 2,
            damage: 17,
            extra,
            ..Stack::new(2, 1)
        };
        let upgraded = result(
            &[Some(Stack::new(1, 1)), Some(base), Some(Stack::new(3, 1))],
            &data,
        )
        .unwrap();
        assert_eq!(
            (
                upgraded.item,
                upgraded.count,
                upgraded.damage,
                upgraded.extra
            ),
            (4, 1, 17, extra)
        );
        let trimmed = result(
            &[
                Some(Stack::new(5, 1)),
                Some(upgraded),
                Some(Stack::new(6, 1)),
            ],
            &data,
        )
        .unwrap();
        let components = components::get(trimmed.extra);
        assert_eq!(components.trim, Some((8, 7)));
        assert_eq!(components.custom_name.as_deref(), Some("\"Sword\""));
        assert!(
            result(
                &[
                    Some(Stack::new(5, 1)),
                    Some(trimmed),
                    Some(Stack::new(6, 1))
                ],
                &data
            )
            .is_none()
        );
        assert!(
            result(
                &[Some(Stack::new(1, 1)), Some(base), Some(Stack::new(6, 1))],
                &data
            )
            .is_none()
        );
    }
}
