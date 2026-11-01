//! Сборка встроенного API плагинов.
//!
//! Берёт готовый каталог `java/dist` (его делает `java/gradlew -p java dist`)
//! и упаковывает в `OUT_DIR/plugin-api.bin`: `chunkwake-api.jar` в корне,
//! библиотеки — в `libs/`. Именно такую раскладку ждёт `find_api_dir`.
//!
//! Порядок поиска:
//!
//! 1. `CHUNKWAKE_API_BUNDLE` — готовый zip (собрать бинарь без Gradle).
//! 2. `<workspace>/java/dist` — обычное место после `gradlew dist`.
//!
//! `CHUNKWAKE_API_BUNDLE=none` или отсутствие каталога даёт пустой архив:
//! `cargo test` на чистой машине не требует Java и Gradle, а бинарь на старте
//! честно скажет, что API не встроен.

use std::path::{Path, PathBuf};

fn main() {
    for key in [
        "CHUNKWAKE_API_BUNDLE",
        "CARGO_MANIFEST_DIR",
        "OUT_DIR",
        "PROFILE",
    ] {
        println!("cargo:rerun-if-env-changed={key}");
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let target = out.join("plugin-api.bin");
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let workspace = manifest
        .parent()
        .and_then(Path::parent)
        .unwrap_or(&manifest);

    let wanted = std::env::var("CHUNKWAKE_API_BUNDLE").unwrap_or_default();
    if wanted == "none" || wanted == "0" || wanted.eq_ignore_ascii_case("false") {
        return empty(&target, "CHUNKWAKE_API_BUNDLE=none");
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
                println!("cargo:warning=CHUNKWAKE_API_BUNDLE={wanted}: {error}; ищу java/dist");
            }
        }
    }

    let dist = workspace.join("java/dist");
    if !dist.join("chunkwake-api.jar").is_file() {
        return empty(
            &target,
            "нет java/dist/chunkwake-api.jar; бинарь без встроенного API плагинов",
        );
    }
    // Правка любого jar в dist должна пересобрать архив.
    println!("cargo:rerun-if-changed={}", dist.display());
    println!(
        "cargo:rerun-if-changed={}",
        dist.join("chunkwake-api.jar").display()
    );

    match pack(&dist, &target) {
        Ok((jars, bytes)) => announce(
            &target,
            &format!("{jars} jar(s) из {} → {bytes} байт сжатого", dist.display()),
        ),
        Err(error) => empty(
            &target,
            &format!("упаковка {} сорвалась: {error}", dist.display()),
        ),
    }
}

/// Пустой архив: бинарь собирается, API в нём нет.
fn empty(target: &Path, why: &str) {
    if let Err(error) = std::fs::write(target, b"") {
        println!("cargo:warning=пустой архив API не записался: {error}");
        return;
    }
    println!("cargo:warning=plugin api: {why}");
}

fn announce(target: &Path, what: &str) {
    let size = target.metadata().map(|m| m.len()).unwrap_or(0);
    println!(
        "cargo:warning=plugin api: {what}, {} КиБ в бинаре",
        size / 1024
    );
}

/// Упаковывает `dist` в zip: `chunkwake-api.jar` и `libs/*.jar`.
///
/// Уровень сжатия максимальный: jar'ы уже дефлятированы внутри, выигрыша ждать
/// не стоит, но манифесты и мелкие записи сжимаются хорошо, а время сборки
/// здесь никого не ждёт. Возвращает число jar'ов и размер архива.
fn pack(dist: &Path, target: &Path) -> Result<(usize, u64), String> {
    let file = std::fs::File::create(target).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let options: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut jars = 0;
    let mut add =
        |zip: &mut zip::ZipWriter<std::fs::File>, name: &str, path: &Path| -> Result<(), String> {
            let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
            zip.start_file(name, options).map_err(|e| e.to_string())?;
            std::io::Write::write_all(zip, &bytes).map_err(|e| format!("{name}: {e}"))?;
            jars += 1;
            Ok(())
        };
    add(
        &mut zip,
        "chunkwake-api.jar",
        &dist.join("chunkwake-api.jar"),
    )?;
    let libs = dist.join("libs");
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&libs)
        .map_err(|e| format!("{}: {e}", libs.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "jar"))
        .collect();
    entries.sort();
    for path in entries {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        add(&mut zip, &format!("libs/{name}"), &path)?;
    }
    zip.finish().map_err(|e| e.to_string())?;
    let size = std::fs::metadata(target).map(|m| m.len()).unwrap_or(0);
    Ok((jars, size))
}
