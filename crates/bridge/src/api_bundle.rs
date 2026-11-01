//! Встроенный API плагинов: `chunkwake-api.jar` и его библиотеки в бинаре.
//!
//! Paper-плагинам нужен каталог `api/` рядом с сервером: загрузчик кладёт
//! `chunkwake-api.jar` и всё из `api/libs` в classpath JVM. Раньше этот
//! каталог приходилось собирать руками (`java/gradlew -p java dist`) и
//! раскладывать рядом с бинарником, иначе плагины просто не включались.
//!
//! `build.rs` упаковывает `java/dist` в zip и кладёт его в `OUT_DIR`; здесь
//! архив встраивается в бинарь через `include_bytes!` и разворачивается в
//! `<каталог сервера>/api` при первом старте. Распакованное помечается
//! отпечатком архива: после пересборки API заменяется, а до тех пор повторный
//! запуск ничего не пишет.
//!
//! Сборка без готового `java/dist` (`CHUNKWAKE_API_BUNDLE=none` или его нет)
//! даёт пустой архив: бинарь собирается, но на старте ищет `api/` рядом с
//! собой, как раньше.

use std::path::{Path, PathBuf};

/// Архив, упакованный `build.rs`. Пустой файл — API не встроен.
const ARCHIVE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/plugin-api.bin"));

/// Подпись распакованного каталога: хранит отпечаток архива.
const MARKER: &str = ".chunkwake-api-bundle";

/// Метка недописанного каталога: такой `api/` считается негодным.
const PARTIAL: &str = ".chunkwake-api-partial";

/// Есть ли в бинаре встроенный API.
pub fn present() -> bool {
    !ARCHIVE.is_empty()
}

/// Отпечаток архива: им подписан распакованный каталог.
pub fn fingerprint() -> u64 {
    // FNV-1a: нужен признак «этот же архив», а не криптостойкость.
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in ARCHIVE {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash ^ ARCHIVE.len() as u64
}

/// Каталог `dir` уже распакован из этого архива.
fn extracted(dir: &Path) -> bool {
    if !present() {
        return false;
    }
    let Ok(text) = std::fs::read_to_string(dir.join(MARKER)) else {
        return false;
    };
    text.trim() == format!("{:016x}", fingerprint())
}

/// Разворачивает встроенный API в `dir` и возвращает его.
///
/// `Ok(None)` — API не встроен (сборка без `java/dist`): вызывающий ищет его
/// сам. Распаковка идёт во временный каталог рядом и переименовывается только
/// после успеха, поэтому оборванная запись не оставит полупустого `api/`.
pub fn ensure(dir: &Path) -> Result<Option<PathBuf>, String> {
    if !present() {
        return Ok(None);
    }
    if extracted(dir) {
        return Ok(Some(dir.to_path_buf()));
    }
    let io = |what: &str, p: &Path, e: std::io::Error| format!("{what} {}: {e}", p.display());
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent).map_err(|e| io("create", parent, e))?;
    }
    let tmp = dir.with_file_name(".api.bundle");
    if tmp.exists() {
        std::fs::remove_dir_all(&tmp).map_err(|e| io("remove", &tmp, e))?;
    }
    unpack(ARCHIVE, &tmp).map_err(|e| format!("unpack embedded plugin api: {e}"))?;
    std::fs::write(tmp.join(MARKER), format!("{:016x}\n", fingerprint()))
        .map_err(|e| io("write", &tmp.join(MARKER), e))?;
    // Старый `api/` заменяется целиком: собранный другой версией paper-api
    // набор библиотек рядом с новым ядром — источник необъяснимых падений JVM.
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|e| io("remove", dir, e))?;
    }
    match rename_with_retry(&tmp, dir) {
        Ok(()) => Ok(Some(dir.to_path_buf())),
        Err(error) => {
            // Windows: сканер или защита держат свежезаписанные файлы, и
            // переименование каталога падает. Раскладываем на месте, закрыв
            // каталог меткой: недописанное не должно сойти за готовое.
            tracing::debug!("plugin api: renaming failed ({error}), unpacking in place");
            let _ = std::fs::remove_dir_all(&tmp);
            std::fs::create_dir_all(dir).map_err(|e| io("create", dir, e))?;
            std::fs::write(dir.join(PARTIAL), b"").map_err(|e| io("write", dir, e))?;
            if let Err(error) = unpack(ARCHIVE, dir) {
                return Err(format!("unpack embedded plugin api: {error}"));
            }
            let _ = std::fs::remove_file(dir.join(PARTIAL));
            std::fs::write(dir.join(MARKER), format!("{:016x}\n", fingerprint()))
                .map_err(|e| io("write", dir, e))?;
            Ok(Some(dir.to_path_buf()))
        }
    }
}

/// Переименование каталога с повторами: сканер может держать файлы.
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

/// Путь к jar с `net.minecraft.*` для classpath JVM.
///
/// Плагины под Paper зовут NMS напрямую (JourneyMap — `RegistryFriendlyByteBuf`),
/// а эти классы лежат во вложенном jar внутри `server.jar`:
/// `META-INF/versions/<версия>/server-<версия>.jar`. Внешний jar отдаёт только
/// 6 своих `net/minecraft` записей, поэтому внутренний надо распаковать рядом
/// с API — один раз, с тем же отпечатком, что у остальных распакованных данных.
///
/// `Ok(None)` — jar игры не найден или в нём нет вложенного: плагины, которым
/// NMS не нужен, работают и так.
pub fn game_classes(api_dir: &Path, game_jar: &Path) -> Result<Option<PathBuf>, String> {
    const NAME: &str = "game-classes.jar";
    let io = |what: &str, p: &Path, e: std::io::Error| format!("{what} {}: {e}", p.display());
    std::fs::create_dir_all(api_dir).map_err(|e| io("create", api_dir, e))?;
    let target = api_dir.join(NAME);
    // Отпечаток исходного jar: пересборка сервера обновит и классы.
    let stamp = target.with_extension("stamp");
    let meta = std::fs::metadata(game_jar).map_err(|e| io("stat", game_jar, e))?;
    let fingerprint = format!("{}\t{}\n", meta.len(), game_jar.display());
    let fresh = target.is_file() && std::fs::read_to_string(&stamp).is_ok_and(|s| s == fingerprint);
    let Ok(file) = std::fs::File::open(game_jar) else {
        return Ok(None);
    };
    let Ok(mut outer) = zip::ZipArchive::new(file) else {
        return Ok(None);
    };
    // Библиотеки jar нужны и когда классы уже распакованы: прошлые сборки
    // (и прошлый запуск) могли оставить `game-classes.jar` без `game-libs`.
    extract_game_libs(&mut outer, api_dir)?;
    if fresh {
        return Ok(Some(target));
    }
    // Вложенный jar версии: `META-INF/versions/<v>/server-<v>.jar`. Каталог
    // версии в пути есть всегда, поэтому проверяем именно `.jar` и один слэш.
    let nested = outer
        .file_names()
        .find(|n| {
            n.strip_prefix("META-INF/versions/")
                .is_some_and(|rest| rest.ends_with(".jar"))
        })
        .map(str::to_owned);
    let Some(nested) = nested else {
        return Ok(None);
    };
    let mut entry = outer
        .by_name(&nested)
        .map_err(|e| format!("{nested} in {}: {e}", game_jar.display()))?;
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    std::io::copy(&mut entry, &mut bytes).map_err(|e| format!("read {nested}: {e}"))?;
    drop(entry);
    std::fs::write(&target, &bytes).map_err(|e| io("write", &target, e))?;
    let _ = std::fs::write(&stamp, fingerprint);
    Ok(Some(target))
}

/// Распаковывает `META-INF/libraries/**/*.jar` из jar игры в `api/game-libs`.
///
/// Vanilla-бандлер держит зависимости внутри: без них классы `net.minecraft.*`
/// находят свои типы только наполовину — NMS компилировался против них.
fn extract_game_libs(
    outer: &mut zip::ZipArchive<std::fs::File>,
    api_dir: &Path,
) -> Result<(), String> {
    let dir = api_dir.join("game-libs");
    let names: Vec<String> = outer
        .file_names()
        .filter(|n| n.starts_with("META-INF/libraries/") && n.ends_with(".jar"))
        .map(str::to_owned)
        .collect();
    if names.is_empty() {
        return Ok(());
    }
    // Проверяем по файлам, а не по маркеру: недописанный каталог от прошлой
    // попытки не должен считаться готовым.
    let done = names.iter().all(|n| {
        let base = n.rsplit('/').next().unwrap_or(n);
        dir.join(base).is_file()
    });
    if done {
        return Ok(());
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    for name in &names {
        let base = name.rsplit('/').next().unwrap_or(name);
        let target = dir.join(base);
        if target.is_file() {
            continue;
        }
        let mut entry = outer.by_name(name).map_err(|e| format!("{name}: {e}"))?;
        let mut file = std::fs::File::create(&target)
            .map_err(|e| format!("create {}: {e}", target.display()))?;
        std::io::copy(&mut entry, &mut file)
            .map_err(|e| format!("write {}: {e}", target.display()))?;
    }
    Ok(())
}

fn unpack(archive: &[u8], dir: &Path) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(archive)).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|e| e.to_string())?;
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        let path = dir.join(&relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&path)
                .map_err(|e| format!("create {}: {e}", path.display()))?;
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create {}: {e}", parent.display()))?;
        }
        let mut file =
            std::fs::File::create(&path).map_err(|e| format!("create {}: {e}", path.display()))?;
        std::io::copy(&mut entry, &mut file)
            .map_err(|e| format!("write {}: {e}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_bundle_is_not_present() {
        // В обычном `cargo test` без `java/dist` архив пуст.
        if present() {
            return;
        }
        assert!(ensure(Path::new("api-test")).unwrap().is_none());
    }

    /// Архив содержит точку входа для classpath: без этого jar'а JVM не найдёт мост.
    #[test]
    fn bundle_has_entry_point() {
        if !present() {
            return;
        }
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(ARCHIVE)).expect("zip");
        assert!(
            archive.by_name("chunkwake-api.jar").is_ok(),
            "в архиве нет chunkwake-api.jar — плагины не включатся"
        );
        let has_paper = archive
            .file_names()
            .any(|n| n.starts_with("libs/") && n.contains("paper-api"));
        assert!(has_paper, "в архиве нет paper-api в libs/");
    }

    /// Встроенный API разворачивается и второй раз не трогается.
    #[test]
    fn embedded_bundle_round_trip() {
        if !present() {
            return;
        }
        let root = std::env::temp_dir().join(format!("chunkwake-api-{:016x}", fingerprint()));
        let dir = root.join("api");
        let ready = ensure(&dir).expect("unpack").expect("present");
        assert!(ready.join("chunkwake-api.jar").is_file());
        assert!(extracted(&dir));
        // Второй вызов ничего не переписывает.
        let marker = dir.join(MARKER);
        let before = marker.metadata().unwrap().modified().unwrap();
        ensure(&dir).expect("cached");
        assert_eq!(marker.metadata().unwrap().modified().unwrap(), before);
    }
}
