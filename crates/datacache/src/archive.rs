//! Архив ванильных данных: упаковка готового дерева и распаковка обратно.
//!
//! Формат — обычный zip: его видно и можно открыть чем угодно, а сжатие
//! берёт на себя `zip`. В архив попадают только нужные рантайму каталоги
//! (`TOP_LEVEL`), по тем же относительным путям, что и в кэше.

use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

/// Каталоги и файлы кэша, которые попадают в архив. Порядок — порядок записи.
pub const TOP_LEVEL: &[&str] = &["version.json", "assets", "data", "reports"];

#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error("{0}")]
    Io(String),
    #[error("{file}: {msg}")]
    Format { file: String, msg: String },
}

impl From<std::io::Error> for ArchiveError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

impl From<zip::result::ZipError> for ArchiveError {
    fn from(e: zip::result::ZipError) -> Self {
        Self::Format {
            file: "archive".into(),
            msg: e.to_string(),
        }
    }
}

/// Нужен ли путь из кэша в архиве.
pub fn is_wanted(relative: &str) -> bool {
    let first = relative.split(['/', '\\']).next().unwrap_or(relative);
    TOP_LEVEL.contains(&first)
}

/// Одна запись архива: файл дерева или байты, подставленные сборкой.
struct Entry<'a> {
    name: String,
    /// Файл на диске; `None` — содержимое в `inline`.
    path: Option<PathBuf>,
    inline: Option<&'a [u8]>,
}

/// Упаковывает дерево `root` в `out`. Каталоги верхнего уровня — только из
/// [`TOP_LEVEL`]; отсутствующие пропускаются. Возвращает число записей.
pub fn pack(root: &Path, out: &Path) -> Result<usize, ArchiveError> {
    pack_with(root, out, &[])
}

/// То же, но с записями, которых нет в дереве: `extra` — путь в архиве и
/// содержимое. Так попадает `en_us.json`, когда его достали прямо из jar:
/// кэш, собранный до появления языкового файла, остаётся без изменений.
pub fn pack_with(
    root: &Path,
    out: &Path,
    extra: &[(String, Vec<u8>)],
) -> Result<usize, ArchiveError> {
    let mut entries = Vec::new();
    for name in TOP_LEVEL {
        let path = root.join(name);
        if path.is_file() {
            entries.push((name.to_string(), path));
        } else if path.is_dir() {
            collect(&path, name, &mut entries)?;
        }
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    // Дополнительные записи перекрывают файлы дерева с тем же путём.
    let mut files: Vec<Entry<'_>> = Vec::new();
    for (name, path) in entries {
        let replaced = extra.iter().any(|(extra_name, _)| *extra_name == name);
        if !replaced {
            files.push(Entry {
                name,
                path: Some(path),
                inline: None,
            });
        }
    }
    for (name, bytes) in extra {
        if is_wanted(name) {
            files.push(Entry {
                name: name.clone(),
                path: None,
                inline: Some(bytes.as_slice()),
            });
        }
    }
    files.sort_by(|a, b| a.name.cmp(&b.name));
    if files.is_empty() {
        return Err(ArchiveError::Format {
            file: root.display().to_string(),
            msg: "no vanilla data to pack".into(),
        });
    }
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = out.with_extension("packing");
    {
        let file = std::fs::File::create(&tmp)?;
        let mut zip = zip::ZipWriter::new(std::io::BufWriter::new(file));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            // Ноль — без метки времени: архив одинаков при одинаковом входе.
            .last_modified_time(zip::DateTime::default())
            .unix_permissions(0o644);
        let mut buffer = Vec::new();
        for entry in &files {
            // Шаблоны структур уже сжаты (gzip внутри NBT) — их не жмём.
            let options = if entry.name.ends_with(".nbt") {
                options.compression_method(zip::CompressionMethod::Stored)
            } else {
                options
            };
            zip.start_file(entry.name.as_str(), options)?;
            match (&entry.path, entry.inline) {
                (Some(path), _) => {
                    buffer.clear();
                    std::io::Read::read_to_end(&mut std::fs::File::open(path)?, &mut buffer)?;
                    zip.write_all(&buffer)?;
                }
                (None, Some(bytes)) => zip.write_all(bytes)?,
                (None, None) => {}
            }
        }
        zip.finish()?;
    }
    std::fs::rename(&tmp, out)?;
    Ok(files.len())
}

fn collect(dir: &Path, prefix: &str, out: &mut Vec<(String, PathBuf)>) -> Result<(), ArchiveError> {
    let mut items: Vec<_> = std::fs::read_dir(dir)?.filter_map(Result::ok).collect();
    items.sort_by_key(std::fs::DirEntry::file_name);
    for entry in items {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy().replace('\\', "/");
        let relative = format!("{prefix}/{name}");
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect(&path, &relative, out)?;
        } else if file_type.is_file() {
            out.push((relative, path));
        }
    }
    Ok(())
}

/// Распаковывает архив в `dest`. Пути вне `dest` отклоняются.
/// Возвращает число записей.
pub fn unpack<R: Read + Seek>(reader: R, dest: &Path) -> Result<usize, ArchiveError> {
    let mut archive = zip::ZipArchive::new(reader)?;
    std::fs::create_dir_all(dest)?;
    let mut count = 0;
    let mut buffer = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let Some(relative) = entry.enclosed_name() else {
            return Err(ArchiveError::Format {
                file: dest.display().to_string(),
                msg: format!("unsafe path in archive: {}", entry.name()),
            });
        };
        if entry.is_dir() {
            std::fs::create_dir_all(dest.join(&relative))?;
            continue;
        }
        let path = dest.join(&relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        buffer.clear();
        entry.read_to_end(&mut buffer)?;
        std::fs::write(&path, &buffer)?;
        count += 1;
    }
    Ok(count)
}

/// То же из памяти и в несколько потоков: тысячи мелких файлов по одному
/// пишутся секундами, а первая распаковка при запуске должна быть быстрой.
///
/// Каталоги создаются заранее в один поток, файлы пишутся параллельно:
/// `create_dir_all` из нескольких потоков на Windows — источник гонок, а
/// запись в готовый каталог безопасна.
pub fn unpack_parallel(bytes: &[u8], dest: &Path, threads: usize) -> Result<usize, ArchiveError> {
    std::fs::create_dir_all(dest)?;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    let mut files = Vec::with_capacity(archive.len());
    let mut dirs: std::collections::HashSet<std::path::PathBuf> = std::collections::HashSet::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let Some(relative) = entry.enclosed_name() else {
            return Err(ArchiveError::Format {
                file: dest.display().to_string(),
                msg: format!("unsafe path in archive: {}", entry.name()),
            });
        };
        if entry.is_dir() {
            dirs.insert(dest.join(&relative));
        } else {
            let path = dest.join(&relative);
            if let Some(parent) = path.parent() {
                dirs.insert(parent.to_path_buf());
            }
            files.push((index, path));
        }
    }
    drop(archive);
    // Каталоги в один поток и в порядке вложенности: `create_dir_all` из
    // нескольких потоков на Windows — источник гонок, а он же создаёт
    // родительские каталоги, поэтому достаточно самого глубокого.
    let mut dirs: Vec<_> = dirs.into_iter().collect();
    dirs.sort();
    for dir in &dirs {
        std::fs::create_dir_all(dir)?;
    }
    let threads = threads.clamp(1, 16).min(files.len().max(1));
    if threads == 1 {
        return write_files(bytes, &files);
    }
    let per = files.len().div_ceil(threads);
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for chunk in files.chunks(per) {
            handles.push(scope.spawn(move || write_files(bytes, chunk)));
        }
        let mut count = 0;
        for handle in handles {
            count += handle
                .join()
                .map_err(|_| ArchiveError::Io("unpack worker panicked".into()))??;
        }
        Ok(count)
    })
}

fn write_files(bytes: &[u8], files: &[(usize, std::path::PathBuf)]) -> Result<usize, ArchiveError> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    let mut buffer = Vec::new();
    for (index, path) in files {
        let mut entry = archive.by_index(*index)?;
        buffer.clear();
        entry.read_to_end(&mut buffer)?;
        std::fs::write(path, &buffer)?;
    }
    Ok(files.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("chunkwake-archive-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn round_trip_keeps_relative_paths() {
        let root = temp("root");
        std::fs::create_dir_all(root.join("reports")).unwrap();
        std::fs::create_dir_all(root.join("data/minecraft/structure/igloo")).unwrap();
        std::fs::create_dir_all(root.join("assets/minecraft/lang")).unwrap();
        std::fs::write(root.join("version.json"), b"{\"name\":\"26.3\"}").unwrap();
        std::fs::write(root.join("reports/packets.json"), b"{}").unwrap();
        std::fs::write(root.join("data/minecraft/structure/igloo/top.nbt"), b"nbt").unwrap();
        std::fs::write(root.join("assets/minecraft/lang/en_us.json"), b"{}").unwrap();
        // Производные кэши и обрезанная копия генератора в архив не идут.
        std::fs::create_dir_all(root.join("chunkwake-derived-v1")).unwrap();
        std::fs::write(root.join("chunkwake-derived-v1/boot.v1.bin"), b"x").unwrap();
        std::fs::write(root.join(".chunkwake-cache-v10"), b"").unwrap();

        let archive = root.join("bundle.bin");
        let packed = pack(&root, &archive).unwrap();
        assert_eq!(packed, 4);

        let out = temp("out");
        let file = std::fs::File::open(&archive).unwrap();
        let count = unpack(file, &out).unwrap();
        assert_eq!(count, 4);
        assert_eq!(
            std::fs::read(out.join("version.json")).unwrap(),
            b"{\"name\":\"26.3\"}"
        );
        assert!(out.join("data/minecraft/structure/igloo/top.nbt").is_file());
        assert!(out.join("assets/minecraft/lang/en_us.json").is_file());
        assert!(!out.join("chunkwake-derived-v1").exists());
        assert!(!out.join(".chunkwake-cache-v10").exists());
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn empty_tree_is_an_error() {
        let root = temp("empty");
        assert!(pack(&root, &root.join("bundle.bin")).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Параллельная распаковка даёт то же дерево, что и последовательная.
    #[test]
    fn parallel_unpack_matches_sequential() {
        let root = temp("par-root");
        std::fs::create_dir_all(root.join("reports")).unwrap();
        for i in 0..64 {
            let dir = root.join(format!("data/minecraft/tags/{i}"));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("tag.json"), format!("{{\"i\":{i}}}")).unwrap();
        }
        std::fs::write(root.join("version.json"), b"{}").unwrap();
        let archive = root.join("bundle.bin");
        let packed = pack(&root, &archive).unwrap();
        assert_eq!(packed, 65);
        let bytes = std::fs::read(&archive).unwrap();

        let serial = temp("par-serial");
        assert_eq!(unpack(std::io::Cursor::new(&bytes), &serial).unwrap(), 65);
        let parallel = temp("par-parallel");
        assert_eq!(unpack_parallel(&bytes, &parallel, 8).unwrap(), 65);
        for i in 0..64 {
            let rel = format!("data/minecraft/tags/{i}/tag.json");
            assert_eq!(
                std::fs::read(serial.join(&rel)).unwrap(),
                std::fs::read(parallel.join(&rel)).unwrap(),
                "{rel}"
            );
        }
        // Один поток — тот же результат.
        let single = temp("par-single");
        assert_eq!(unpack_parallel(&bytes, &single, 1).unwrap(), 65);
        assert!(single.join("data/minecraft/tags/63/tag.json").is_file());
        for dir in [root, serial, parallel, single] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn wanted_filters_top_level() {
        assert!(is_wanted("reports/packets.json"));
        assert!(is_wanted("data/minecraft/tags/block/logs.json"));
        assert!(is_wanted("version.json"));
        assert!(!is_wanted("chunkwake-derived-v1/boot.v1.bin"));
        assert!(!is_wanted(".work/libraries/x.jar"));
    }
}
