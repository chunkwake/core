//! Командные хранилища (`/data … storage`).
//!
//! В ванили корень хранится по `namespace:path` в `CommandStorage` и пишется в
//! `data/command_storage_<namespace>.dat` (gzip NBT: путь → compound). Здесь —
//! та же карта в памяти; первое чтение подтягивает файл, запись обновляет его.

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

fn storage_file(dir: &std::path::Path, namespace: &str) -> std::path::PathBuf {
    dir.join("data")
        .join(format!("command_storage_{namespace}.dat"))
}

fn read_namespace(dir: &std::path::Path, namespace: &str) -> Compound {
    chunkwake_world::nbt::read_gzip_file(&storage_file(dir, namespace))
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// Дописать записи в файл namespace, не теряя чужие пути.
fn write_namespace(dir: &std::path::Path, namespace: &str, entries: &[(String, Compound)]) {
    let mut root = read_namespace(dir, namespace);
    for (path, data) in entries {
        root.insert(path.clone(), Tag::Compound(data.clone()));
    }
    let file = storage_file(dir, namespace);
    if let Some(parent) = file.parent()
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        tracing::warn!("command storage {}: {error}", parent.display());
        return;
    }
    if let Err(error) = chunkwake_world::nbt::write_gzip_file(&file, &root) {
        tracing::warn!("command storage {namespace}: {error}");
    }
}

/// Сохранить все известные пути namespace после правки.
fn persist(plan: &PlayPlan, id: &str) {
    let Some(dir) = plan.world_dir.as_deref() else {
        return;
    };
    let Some((namespace, _)) = id.split_once(':') else {
        return;
    };
    let entries: Vec<(String, Compound)> = {
        let map = plan.command_storage.lock().unwrap();
        map.iter()
            .filter_map(|(key, root)| {
                let (ns, path) = key.split_once(':')?;
                (ns == namespace).then(|| (path.to_owned(), root.clone()))
            })
            .collect()
    };
    if !entries.is_empty() {
        write_namespace(dir, namespace, &entries);
    }
}

/// Корень хранилища (пустой, если ещё не создан); первый доступ читает файл.
pub(super) fn command_storage_get(plan: &PlayPlan, id: &str) -> Compound {
    let mut map = plan.command_storage.lock().unwrap();
    if let Some(root) = map.get(id) {
        return root.clone();
    }
    let root = plan
        .world_dir
        .as_deref()
        .zip(id.split_once(':'))
        .map_or_else(Compound::new, |(dir, (namespace, path))| {
            read_namespace(dir, namespace)
                .get(path)
                .and_then(Tag::as_compound)
                .cloned()
                .unwrap_or_default()
        });
    map.insert(id.to_owned(), root.clone());
    root
}

/// Записать корень; `false`, если он не изменился.
pub(super) fn command_storage_set(plan: &PlayPlan, id: &str, root: Compound) -> bool {
    {
        let mut map = plan.command_storage.lock().unwrap();
        match map.get(id) {
            Some(old) if *old == root => return false,
            _ => {
                map.insert(id.to_owned(), root);
            }
        }
    }
    persist(plan, id);
    true
}

/// Слить `patch` в корень (`/data merge storage`); `false` — без изменений.
pub(super) fn command_storage_merge(plan: &PlayPlan, id: &str, patch: &Compound) -> bool {
    {
        let mut map = plan.command_storage.lock().unwrap();
        let root = map.entry(id.to_owned()).or_default();
        let before = root.clone();
        crate::play::commands::snbt::merge_compounds(root, patch);
        if *root == before {
            return false;
        }
    }
    persist(plan, id);
    true
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

    #[test]
    fn namespace_file_keeps_other_paths() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "chunkwake-cmd-storage-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let mut first = Compound::new();
        first.insert("a".into(), Tag::Int(7));
        write_namespace(&dir, "demo", &[("one".into(), first.clone())]);
        let mut second = Compound::new();
        second.insert("b".into(), Tag::Byte(1));
        write_namespace(&dir, "demo", &[("two".into(), second)]);
        let loaded = read_namespace(&dir, "demo");
        assert_eq!(
            loaded.get("one").and_then(Tag::as_compound).cloned(),
            Some(first)
        );
        assert!(loaded.contains_key("two"));
        // Другой namespace — другой файл.
        assert!(!read_namespace(&dir, "other").contains_key("one"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
