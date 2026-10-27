//! Запуск ванильного генератора данных с кэшем по версии.
//!
//! `java -DbundlerMainClass=net.minecraft.data.Main -jar server.jar --reports --output <dir>`

use crate::DataError;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Пишется последним; меняется при изменении состава кэша.
const CACHE_MARKER: &str = ".chunkwake-cache-v3";

/// Выгрузка свойств света (исходник рядом: `extractor/ChunkwakeLight.java`).
const LIGHT_EXTRACTOR: &[u8] = include_bytes!("../extractor/ChunkwakeLight.class");

/// Готовый кэш версии, если он есть: тогда server.jar не нужен.
pub fn cached(cache_root: &Path, version: &str) -> Option<PathBuf> {
    let target = cache_root.join(version);
    target.join(CACHE_MARKER).is_file().then_some(target)
}

/// Возвращает каталог с `reports/` для версии, при необходимости запуская генератор.
///
/// Кэш: `<cache_root>/<version>/{data,reports}`. Генерация идёт во временный каталог
/// и переименовывается только после успеха, поэтому сбой не оставляет
/// полукэша.
pub fn ensure_generated(
    java: &Path,
    jar: &Path,
    version: &str,
    cache_root: &Path,
) -> Result<PathBuf, DataError> {
    if let Some(target) = cached(cache_root, version) {
        return Ok(target);
    }
    let target = cache_root.join(version);
    let io = |what: &str, p: &Path, e: std::io::Error| {
        DataError::Io(format!("{what} {}: {e}", p.display()))
    };
    // Кэш старого формата (без маркера) пересоздаём.
    if target.exists() {
        std::fs::remove_dir_all(&target).map_err(|e| io("remove", &target, e))?;
    }
    let work = cache_root.join(".work");
    let tmp_out = cache_root.join(format!(".{version}.tmp"));
    std::fs::create_dir_all(&work).map_err(|e| io("create", &work, e))?;
    if tmp_out.exists() {
        std::fs::remove_dir_all(&tmp_out).map_err(|e| io("remove", &tmp_out, e))?;
    }
    let jar = std::path::absolute(jar).map_err(|e| io("resolve", jar, e))?;
    let tmp_abs = std::path::absolute(&tmp_out).map_err(|e| io("resolve", &tmp_out, e))?;

    // Бандлер распаковывает библиотеки в текущий каталог — держим их в .work.
    let output = Command::new(java)
        .current_dir(&work)
        .arg("-DbundlerMainClass=net.minecraft.data.Main")
        .arg("-jar")
        .arg(&jar)
        .arg("--server")
        .arg("--reports")
        .arg("--output")
        .arg(&tmp_abs)
        .output()
        .map_err(|e| {
            DataError::Generator(format!(
                "cannot start {}: {e}. Set CHUNKWAKE_JAVA to a Java runtime required by this version",
                java.display()
            ))
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail: Vec<&str> = stderr.lines().rev().take(5).collect();
        let hint = if stderr.contains("UnsupportedClassVersionError") {
            " (Java is too old for this version; set CHUNKWAKE_JAVA)"
        } else {
            ""
        };
        return Err(DataError::Generator(format!(
            "exit {}{hint}: {}",
            output.status,
            tail.into_iter().rev().collect::<Vec<_>>().join(" | ")
        )));
    }
    if !tmp_out.join("reports/packets.json").is_file() {
        return Err(DataError::Generator(
            "no reports/packets.json produced".into(),
        ));
    }
    extract_light(java, &work, &tmp_abs.join("reports/light.csv"))?;
    std::fs::write(tmp_out.join(CACHE_MARKER), "").map_err(|e| io("write", &tmp_out, e))?;
    std::fs::rename(&tmp_out, &target).map_err(|e| io("rename", &tmp_out, e))?;
    Ok(target)
}

/// Свет состояний блоков: в отчётах генератора его нет, поэтому запускаем
/// маленький класс на распакованных бандлером `versions/` и `libraries/`.
fn extract_light(java: &Path, work: &Path, out: &Path) -> Result<(), DataError> {
    let io = |what: &str, p: &Path, e: std::io::Error| {
        DataError::Io(format!("{what} {}: {e}", p.display()))
    };
    let class_dir = work.join("chunkwake-extractor");
    std::fs::create_dir_all(&class_dir).map_err(|e| io("create", &class_dir, e))?;
    let class = class_dir.join("ChunkwakeLight.class");
    std::fs::write(&class, LIGHT_EXTRACTOR).map_err(|e| io("write", &class, e))?;

    let mut classpath =
        vec![std::path::absolute(&class_dir).map_err(|e| io("resolve", &class_dir, e))?];
    for dir in ["versions", "libraries"] {
        jars(&work.join(dir), &mut classpath);
    }
    let classpath = std::env::join_paths(&classpath)
        .map_err(|e| DataError::Generator(format!("classpath: {e}")))?;
    let output = Command::new(java)
        .current_dir(work)
        .arg("-cp")
        .arg(classpath)
        .arg("ChunkwakeLight")
        .arg(out)
        .output()
        .map_err(|e| DataError::Generator(format!("cannot start {}: {e}", java.display())))?;
    if !output.status.success() || !out.is_file() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail: Vec<&str> = stderr.lines().rev().take(5).collect();
        return Err(DataError::Generator(format!(
            "light extractor exit {}: {}",
            output.status,
            tail.into_iter().rev().collect::<Vec<_>>().join(" | ")
        )));
    }
    Ok(())
}

/// Все `.jar` в каталоге, рекурсивно.
fn jars(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            jars(&p, out);
        } else if p.extension().is_some_and(|e| e == "jar") {
            out.push(std::path::absolute(&p).unwrap_or(p));
        }
    }
}
