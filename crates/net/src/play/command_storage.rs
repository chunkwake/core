//! Командные хранилища (`/data … storage`).
//!
//! В ванили корень хранится по `namespace:path` в `CommandStorage` и пишется в
//! `data/command_storage_<namespace>.dat`. Здесь — та же карта в памяти:
//! отсутствующее хранилище читается как пустой compound и создаётся при записи.

use super::*;

/// `ResourceLocation` хранилища: без namespace — `minecraft:`.
pub(super) fn storage_id(raw: &str) -> Option<String> {
    let (namespace, path) = match raw.split_once(':') {
        Some((namespace, path)) => (namespace, path),
        None => ("minecraft", raw),
    };
    let valid = |part: &str| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_-./".contains(c))
    };
    (valid(namespace) && valid(path)).then(|| format!("{namespace}:{path}"))
}

/// Корень хранилища (пустой, если ещё не создан).
pub(super) fn command_storage_get(plan: &PlayPlan, id: &str) -> Compound {
    plan.command_storage
        .lock()
        .unwrap()
        .get(id)
        .cloned()
        .unwrap_or_default()
}

/// Записать корень; `false`, если он не изменился.
pub(super) fn command_storage_set(plan: &PlayPlan, id: &str, root: Compound) -> bool {
    let mut map = plan.command_storage.lock().unwrap();
    match map.get(id) {
        Some(old) if *old == root => false,
        _ => {
            map.insert(id.to_owned(), root);
            true
        }
    }
}

/// Слить `patch` в корень (`/data merge storage`); `false` — без изменений.
pub(super) fn command_storage_merge(plan: &PlayPlan, id: &str, patch: &Compound) -> bool {
    let mut map = plan.command_storage.lock().unwrap();
    let root = map.entry(id.to_owned()).or_default();
    let before = root.clone();
    crate::play::commands::snbt::merge_compounds(root, patch);
    *root != before
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_ids_match_resource_locations() {
        assert_eq!(storage_id("demo:test"), Some("demo:test".into()));
        assert_eq!(storage_id("test").as_deref(), Some("minecraft:test"));
        assert_eq!(
            storage_id("demo:dir/file-1").as_deref(),
            Some("demo:dir/file-1")
        );
        assert_eq!(storage_id(""), None);
        assert_eq!(storage_id("demo:"), None);
        assert_eq!(storage_id("Demo:test"), None);
        assert_eq!(storage_id("demo:тест"), None);
    }
}
