//! Сборка встроенного архива ванильных данных.
//!
//! Порядок поиска дерева данных:
//!
//! 1. `CHUNKWAKE_BUNDLE` — готовый zip (позволяет собрать бинарь без Java).
//! 2. `CHUNKWAKE_DATA` — каталог кэша (`generated/26.3`).
//! 3. `<workspace>/generated/<версия>` — обычное место кэша при разработке.
//! 4. `CHUNKWAKE_JAR` или `server.jar` рядом с корнем workspace — из него
//!    собирается кэш тем же кодом, что и у сервера (`chunkwake-datacache`).
//!
//! Пусто везде — собирается пустой архив: `cargo test` и CI не требуют
//! данных игры, а бинарь на старте скажет, чего ему не хватает.
//!
//! Результат — `OUT_DIR/vanilla-data.bin`. Если список входов и их метки
//! времени не изменились, архив не переупаковывается.

use chunkwake_datacache::{ArchiveError, pack, version_json};
use std::path::{Path, PathBuf};

/// Версия, под которую собран сервер (`crates/server/src/lib.rs`).
const VERSION: &str = "26.3";

fn main() {
    for key in [
        "CHUNKWAKE_BUNDLE",
        "CHUNKWAKE_DATA",
        "CHUNKWAKE_JAR",
        "CHUNKWAKE_JAVA",
        "PROFILE",
        "TARGET",
        "CARGO_MANIFEST_DIR",
    ] {
        println!("cargo:rerun-if-env-changed={key}");
    }
    // Файлы данных лежат вне крейта: без `rerun-if-changed` Cargo не заметит
    // их правку, поэтому путь к ним регистрируется ниже.
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let target = out.join("vanilla-data.bin");
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let workspace = manifest
        .parent()
        .and_then(Path::parent)
        .unwrap_or(&manifest);

    let wanted = std::env::var("CHUNKWAKE_BUNDLE").unwrap_or_default();
    if wanted == "none" || wanted == "0" || wanted.eq_ignore_ascii_case("false") {
        empty(&target, "CHUNKWAKE_BUNDLE=none");
        return;
    }
    if !wanted.is_empty() {
        let path = PathBuf::from(&wanted);
        println!("cargo:rerun-if-changed={}", path.display());
        match std::fs::copy(&path, &target) {
            Ok(_) => {
                announce(&target, &format!("приготовленный архив {}", path.display()));
                return;
            }
            Err(error) => {
                println!("cargo:warning=CHUNKWAKE_BUNDLE={wanted}: {error}; ищу данные сам");
            }
        }
    }

    let Some(root) = locate(workspace) else {
        empty(
            &target,
            "нет ни generated/26.3, ни server.jar; бинарь без ванильных данных",
        );
        return;
    };
    // Правка любого файла кэша должна пересобрать архив.
    println!("cargo:rerun-if-changed={}", root.display());
    // Язык консоли и `version.json` лежат только в jar, а кэш мог быть собран
    // до того, как их начали туда класть. В архив они попадают в любом случае:
    // без `version.json` бинарь соберётся, но не узнает версию своих данных.
    let mut extra = lang_from_jar(workspace, &root);
    extra.extend(version_from_jar(workspace, &root));
    if let Some(size) = up_to_date(&root, &target, &extra) {
        println!(
            "cargo:warning=vanilla data: {} записей из {} (архив не менялся, {size} КиБ)",
            count_entries(&root),
            root.display(),
        );
        return;
    }
    let packed = if extra.is_empty() {
        pack(&root, &target)
    } else {
        chunkwake_datacache::pack_with(&root, &target, &extra)
    };
    match packed {
        Ok(n) => announce(&target, &format!("{n} записей из {}", root.display())),
        Err(error) => empty(
            &target,
            &format!("упаковка {} сорвалась: {error}", root.display()),
        ),
    }
}

/// `version.json` из jar, если в дереве его нет.
///
/// Кэш, собранный генератором раньше, версии не содержал: её дописывал сервер
/// уже после. Без неё `doctor` и запуск не узнают, что за данные внутри, —
/// поэтому сборка достаёт файл из jar сама.
fn version_from_jar(workspace: &Path, root: &Path) -> Vec<(String, Vec<u8>)> {
    if root.join("version.json").is_file() {
        return Vec::new();
    }
    let Some(jar) = jar_path(workspace) else {
        return Vec::new();
    };
    match chunkwake_datacache::version_json(&jar) {
        Ok(text) => vec![("version.json".to_owned(), text.into_bytes())],
        Err(error) => {
            println!(
                "cargo:warning=version.json из {}: {error}; бинарь не узнает версию данных",
                jar.display()
            );
            Vec::new()
        }
    }
}

/// `en_us.json` и остальные языки из jar, если в дереве их нет.
fn lang_from_jar(workspace: &Path, root: &Path) -> Vec<(String, Vec<u8>)> {
    if root.join("assets/minecraft/lang/en_us.json").is_file() {
        return Vec::new();
    }
    let Some(jar) = jar_path(workspace) else {
        return Vec::new();
    };
    match chunkwake_datacache::lang_entries(&jar) {
        Ok(entries) => entries,
        Err(error) => {
            println!(
                "cargo:warning=язык консоли из {}: {error}; сообщения останутся ключами",
                jar.display()
            );
            Vec::new()
        }
    }
}

/// Переменная сборки: из окружения, иначе из `.env` в корне репозитория.
///
/// Сервер читает `.env` сам (`dotenvy` в `main`), а Cargo — нет: без этого
/// `CHUNKWAKE_JAVA` из `.env` работал при запуске и терялся при сборке.
fn local_var(workspace: &Path, key: &str) -> Option<std::ffi::OsString> {
    if let Some(v) = std::env::var_os(key) {
        return Some(v);
    }
    let env = workspace.join(".env");
    println!("cargo:rerun-if-changed={}", env.display());
    dotenvy::from_path_iter(&env)
        .ok()?
        .filter_map(Result::ok)
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.into())
}

/// `server.jar` для сборки: `CHUNKWAKE_JAR` или обычные места рядом с корнем.
fn jar_path(workspace: &Path) -> Option<PathBuf> {
    local_var(workspace, "CHUNKWAKE_JAR")
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .or_else(|| {
            [
                workspace.join("server.jar"),
                workspace.join("target/release/server.jar"),
            ]
            .into_iter()
            .find(|p| p.is_file())
        })
}

/// Пустой архив: бинарь собирается, данных в нём нет.
fn empty(target: &Path, why: &str) {
    if let Err(error) = std::fs::write(target, b"") {
        println!("cargo:warning=пустой архив не записался: {error}");
        return;
    }
    println!("cargo:warning=vanilla data: {why}");
}

fn announce(target: &Path, what: &str) {
    let size = target.metadata().map(|m| m.len()).unwrap_or(0);
    println!(
        "cargo:warning=vanilla data: {what} → {} КиБ в бинаре",
        size / 1024
    );
}

/// Дерево данных для упаковки: сначала готовый кэш, потом jar и Java.
fn locate(workspace: &Path) -> Option<PathBuf> {
    if let Some(dir) = local_var(workspace, "CHUNKWAKE_DATA") {
        let dir = PathBuf::from(dir);
        if dir.join("reports/packets.json").is_file() {
            return Some(dir);
        }
        println!(
            "cargo:warning=CHUNKWAKE_DATA={}: нет reports/packets.json",
            dir.display()
        );
    }
    let generated = workspace.join("generated").join(VERSION);
    if generated.join("reports/packets.json").is_file() {
        return Some(generated);
    }
    for dir in [workspace.to_path_buf(), workspace.join("target/release")] {
        let cached = chunkwake_datacache::cached(&dir.join("generated"), VERSION);
        if let Some(cached) = cached.filter(|d| d.join("reports/packets.json").is_file()) {
            return Some(cached);
        }
    }
    let jar = jar_path(workspace)?;
    let java = local_var(workspace, "CHUNKWAKE_JAVA")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            println!(
                "cargo:warning=CHUNKWAKE_JAVA не задан ни в окружении, ни в .env, беру `java` из PATH; для 26.3 нужна Java 25"
            );
            PathBuf::from("java")
        });
    let version = version_json(&jar).ok().and_then(|text| {
        serde_json::from_str::<serde_json::Value>(&text)
            .ok()?
            .get("name")?
            .as_str()
            .map(str::to_owned)
    });
    let version = version.unwrap_or_else(|| VERSION.to_owned());
    if version != VERSION {
        println!("cargo:warning=server.jar — версия {version}, а сервер собран под {VERSION}");
    }
    let cache = std::env::var_os("CHUNKWAKE_BUILD_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("target/vanilla-cache"));
    match chunkwake_datacache::ensure_generated_with(&java, &jar, &version, &cache, &|msg| {
        println!("cargo:warning={msg}");
    }) {
        Ok(dir) => Some(dir),
        Err(error) => {
            println!(
                "cargo:warning=генератор данных {}: {error}; ставлю CHUNKWAKE_DATA на готовый кэш",
                jar.display()
            );
            None
        }
    }
}

/// Признак неизменности входа: тот же набор файлов с теми же размерами.
///
/// Метки времени кэша после распаковки архива могут «дрожать» между
/// машинами, поэтому сравниваются пути и размеры — их достаточно, чтобы
/// заметить смену версии или докачку шаблонов.
fn up_to_date(root: &Path, target: &Path, extra: &[(String, Vec<u8>)]) -> Option<u64> {
    let stamp = target.with_extension("stamp");
    let mut current = listing(root);
    for (name, bytes) in extra {
        current.push_str(&format!("{name}\t{}\tjar\n", bytes.len()));
    }
    let Ok(previous) = std::fs::read_to_string(&stamp) else {
        let _ = std::fs::write(&stamp, &current);
        return None;
    };
    if previous != current || !target.is_file() {
        let _ = std::fs::write(&stamp, &current);
        return None;
    }
    // Пустой архив — след сборки с `CHUNKWAKE_BUNDLE=none`: его надо заменить
    // настоящим, даже если состав входов не менялся.
    let size = target.metadata().map(|m| m.len()).unwrap_or(0);
    if size == 0 {
        return None;
    }
    Some(size / 1024)
}

/// Список входов: путь и размер, по одному в строке.
fn listing(root: &Path) -> String {
    let mut out = String::new();
    for name in chunkwake_datacache::TOP_LEVEL {
        walk(&root.join(name), name, &mut out);
    }
    out
}

fn walk(path: &Path, relative: &str, out: &mut String) {
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    if meta.is_file() {
        out.push_str(&format!("{relative}\t{}\n", meta.len()));
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    let mut items: Vec<_> = entries.filter_map(Result::ok).collect();
    items.sort_by_key(std::fs::DirEntry::file_name);
    for entry in items {
        let name = entry.file_name();
        let name = name.to_string_lossy().replace('\\', "/");
        walk(&entry.path(), &format!("{relative}/{name}"), out);
    }
}

fn count_entries(root: &Path) -> usize {
    listing(root).lines().count()
}

#[allow(dead_code)]
fn _assert_error_type(_: ArchiveError) {}
