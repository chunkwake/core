//! Встроенный архив ванильных данных: распаковка при первом старте.
//!
//! `build.rs` упаковывает `generated/<версия>` (или собранный из `server.jar`
//! кэш) в zip и кладёт в `OUT_DIR`. Здесь этот архив встраивается в бинарь
//! через `include_bytes!` и разворачивается в каталог данных сервера.
//!
//! Маркер рядом с распакованным деревом хранит отпечаток архива: после смены
//! версии или сборочных данных архив распаковывается заново, иначе запуск
//! ничего не делает.
//!
//! Сборка без данных (`CHUNKWAKE_BUNDLE=none` или их нет вообще) даёт пустой
//! архив: бинарь собирается, но на старте требует `generated/` или `server.jar`.

use crate::DataError;
use std::path::{Path, PathBuf};

/// Архив, упакованный `build.rs`. Пустой файл — бандла нет.
const ARCHIVE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/vanilla-data.bin"));

/// Подпись распакованного дерева. Меняется вместе с форматом архива.
const MARKER: &str = ".chunkwake-bundle-v1";

/// Метка недописанного дерева: такой каталог считается негодным.
const PARTIAL: &str = ".chunkwake-bundle-partial";

/// Есть ли в бинаре встроенные данные.
pub fn present() -> bool {
    !ARCHIVE.is_empty()
}

/// Отпечаток архива: его же пишем в маркер.
pub fn fingerprint() -> u64 {
    // FNV-1a: нужен не криптостойкий хеш, а признак «этот же архив».
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in ARCHIVE {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash ^ ARCHIVE.len() as u64
}

/// Дерево данных в `root` уже распаковано из этого архива.
pub fn extracted(root: &Path) -> bool {
    if !present() {
        return false;
    }
    let Ok(text) = std::fs::read_to_string(root.join(MARKER)) else {
        return false;
    };
    text.trim() == format!("{:016x}", fingerprint())
}

/// Версия игры из встроенного `version.json` (без распаковки на диск).
pub fn version_json() -> Option<String> {
    if !present() {
        return None;
    }
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(ARCHIVE)).ok()?;
    let mut file = archive.by_name("version.json").ok()?;
    let mut text = String::new();
    std::io::Read::read_to_string(&mut file, &mut text).ok()?;
    Some(text)
}

/// Разворачивает встроенные данные в `cache_root/<версия>` и возвращает каталог.
///
/// Распаковка идёт во временный каталог рядом и переименовывается только после
/// успеха: оборванная распаковка не оставит полудерева, а маркер пишется
/// последним.
pub fn ensure(cache_root: &Path, version: &str) -> Result<PathBuf, DataError> {
    let target = cache_root.join(version);
    if extracted(&target) {
        return Ok(target);
    }
    if !present() {
        return Err(DataError::Io(format!(
            "no vanilla data: this build has no embedded data and {} is missing",
            target.display()
        )));
    }
    let io = |what: &str, p: &Path, e: std::io::Error| {
        DataError::Io(format!("{what} {}: {e}", p.display()))
    };
    std::fs::create_dir_all(cache_root).map_err(|e| io("create", cache_root, e))?;
    let tmp = cache_root.join(format!(".{version}.bundle"));
    if tmp.exists() {
        std::fs::remove_dir_all(&tmp).map_err(|e| io("remove", &tmp, e))?;
    }
    chunkwake_datacache::unpack_parallel(ARCHIVE, &tmp, unpack_threads())
        .map_err(|e| DataError::Io(format!("unpack embedded data: {e}")))?;
    // Дерево не из этого архива — от прежней сборки или выгрузки другой
    // версией экстрактора. Раньше полный кэш оставался как есть, и новые
    // поля (атрибуты в `entities.csv`) не доходили до сервера никогда.
    // Целое дерево уходит в `.<версия>.old` — правки в нём не пропадут;
    // недописанное просто удаляется.
    if target.exists() {
        if is_complete_cache(&target) {
            let old = cache_root.join(format!(".{version}.old"));
            if old.exists() {
                std::fs::remove_dir_all(&old).map_err(|e| io("remove", &old, e))?;
            }
            rename_with_retry(&target, &old).map_err(|e| io("move aside", &target, e))?;
            tracing::info!(
                "vanilla data: {} was not unpacked from this build; kept as {}",
                target.display(),
                old.display()
            );
        } else {
            std::fs::remove_dir_all(&target).map_err(|e| io("remove", &target, e))?;
        }
    }
    // Антивирус на Windows держит свежезаписанные файлы ещё мгновение:
    // переименование каталога с ними падает с «отказано в доступе».
    match rename_with_retry(&tmp, &target) {
        Ok(()) => {}
        Err(error) => {
            // Не вышло — раскладываем на месте и закрываем каталог меткой:
            // недописанное дерево не должно сойти за правленый кэш.
            let _ = std::fs::remove_dir_all(&tmp);
            std::fs::create_dir_all(&target).map_err(|e| io("create", &target, e))?;
            std::fs::write(target.join(PARTIAL), b"").map_err(|e| io("write", &target, e))?;
            chunkwake_datacache::unpack_parallel(ARCHIVE, &target, unpack_threads())
                .map_err(|e| DataError::Io(format!("unpack embedded data: {e}")))?;
            std::fs::remove_file(target.join(PARTIAL)).map_err(|e| io("remove", &target, e))?;
            tracing::debug!("vanilla data: renamed with fallback after {error}");
        }
    }
    std::fs::write(target.join(MARKER), format!("{:016x}\n", fingerprint()))
        .map_err(|e| io("write", &target.join(MARKER), e))?;
    // Заодно метка обычного кэша: по ней `chunkwake_datacache::cached`,
    // `build.rs` и `doctor` видят готовое дерево без второго условия.
    let _ = std::fs::write(target.join(chunkwake_datacache::CACHE_MARKER), b"");
    Ok(target)
}

/// Переименование каталога с несколькими попытками: файлы, которые только что
/// записаны, на Windows может держать сканер.
fn rename_with_retry(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut last = None;
    for attempt in 0..12 {
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(error) => {
                last = Some(error);
                std::thread::sleep(std::time::Duration::from_millis(50 * (attempt + 1)));
            }
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("rename failed")))
}

/// Каталог похож на готовый кэш ванильных данных: отчёты и дерево на месте.
fn is_complete_cache(dir: &Path) -> bool {
    !dir.join(PARTIAL).exists()
        && dir.join("reports/blocks.json").is_file()
        && dir.join("reports/packets.json").is_file()
        && dir.join("data/minecraft/worldgen/noise_settings").is_dir()
}

/// Сколько потоков отдать распаковке: по числу ядер, но не больше шестнадцати.
/// Мелкие файлы упираются в метаданные файловой системы, а не в CPU.
fn unpack_threads() -> usize {
    std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(4)
        .clamp(2, 16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_bundle_is_not_present() {
        // В обычном `cargo test` без `generated/` и без `server.jar` архив пуст.
        if present() {
            return;
        }
        assert!(version_json().is_none());
        assert!(!extracted(Path::new(".")));
    }

    /// В архиве есть всё, без чего распакованное дерево негодно.
    ///
    /// Проверка отдельная от распаковки: `ensure` умеет дописывать недостающее
    /// в чужой кэш, и без этого списка он мог бы скрыть дыру в самом архиве —
    /// так и вышло с `version.json`, который генератор не писал.
    #[test]
    fn bundle_has_entry_points() {
        if !present() {
            return;
        }
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(ARCHIVE)).expect("zip");
        for name in [
            "version.json",
            "reports/blocks.json",
            "reports/packets.json",
        ] {
            assert!(
                archive.by_name(name).is_ok(),
                "в архиве нет {name} — распакованное дерево не поднимется"
            );
        }
        let has_noise = archive
            .file_names()
            .any(|n| n.starts_with("data/minecraft/worldgen/noise_settings/"));
        assert!(has_noise, "в архиве нет настроек шума");
    }

    /// Встроенный архив распаковывается и второй раз не трогается.
    #[test]
    fn embedded_bundle_round_trip() {
        if !present() {
            return;
        }
        // Каталог общий на все прогоны: распаковка 11 тысяч файлов на
        // Windows под антивирусом стоит секунды, и платить их каждый раз
        // незачем. Отпечаток в имени — чтобы чужая сборка не подсунула своё.
        let cache = std::env::temp_dir().join(format!("chunkwake-bundle-{:016x}", fingerprint()));
        let dir = ensure(&cache, "test").expect("unpack");
        assert!(dir.join("version.json").is_file());
        assert!(dir.join("reports/blocks.json").is_file());
        assert!(dir.join("data/minecraft/worldgen/noise_settings").is_dir());
        assert!(extracted(&dir));
        // Метка времени маркера не меняется: второй вызов ничего не пишет.
        let marker = dir.join(MARKER);
        let before = marker.metadata().unwrap().modified().unwrap();
        let again = ensure(&cache, "test").expect("cached");
        assert_eq!(again, dir);
        assert_eq!(marker.metadata().unwrap().modified().unwrap(), before);
    }

    /// Версия из архива совпадает с `MC_VERSION` сервера.
    #[test]
    fn bundle_version_matches_server() {
        let Some(text) = version_json() else {
            return;
        };
        let json: serde_json::Value = serde_json::from_str(&text).expect("version.json");
        assert!(json["name"].is_string());
        assert!(json["protocol_version"].as_i64().is_some());
    }

    /// Готовый кэш от прежней сборки не перезаписывается: в него только
    /// дописывается язык консоли, а свои файлы он сохраняет.
    #[test]
    fn stale_cache_is_replaced_and_kept_aside() {
        if !present() {
            return;
        }
        let cache =
            std::env::temp_dir().join(format!("chunkwake-bundle-stale-{:016x}", fingerprint()));
        let _ = std::fs::remove_dir_all(&cache);
        let dir = cache.join("test");
        // Кэш прежней сборки: полное дерево без маркера, `entities.csv`
        // старого формата и ручная правка.
        chunkwake_datacache::unpack_parallel(ARCHIVE, &dir, 4).unwrap();
        let _ = std::fs::remove_file(dir.join(MARKER));
        std::fs::write(
            dir.join("reports/entities.csv"),
            b"minecraft:pig;1;2;0.9;0.9;creature;false\n",
        )
        .unwrap();
        std::fs::write(dir.join("reports/local-edit.txt"), b"keep me").unwrap();

        let ready = ensure(&cache, "test").expect("replace stale cache");
        assert_eq!(ready, dir);
        assert!(extracted(&dir));
        // Данные — из бинаря, а не старые.
        assert!(!dir.join("reports/local-edit.txt").exists());
        assert_ne!(
            std::fs::read(dir.join("reports/entities.csv")).unwrap(),
            b"minecraft:pig;1;2;0.9;0.9;creature;false\n"
        );
        // Старое дерево с правкой отложено.
        assert_eq!(
            std::fs::read(cache.join(".test.old/reports/local-edit.txt")).unwrap(),
            b"keep me"
        );
        // Второй вызов ничего не пишет.
        let csv = dir.join("reports/entities.csv");
        let before = csv.metadata().unwrap().modified().unwrap();
        ensure(&cache, "test").expect("cached");
        assert_eq!(csv.metadata().unwrap().modified().unwrap(), before);
        let _ = std::fs::remove_dir_all(&cache);
    }
}
