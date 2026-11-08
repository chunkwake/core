//! Кэш ванильных данных: запуск генератора из `server.jar`, шаблоны структур,
//! упаковка готового дерева в архив и его распаковка.
//!
//! Крейт отдельный, потому что им пользуются двое: сервер (когда кэша нет, а
//! `server.jar` есть) и `build.rs` крейта `chunkwake-data`, который кладёт
//! готовый набор внутрь бинаря. Рантайму ничего из этого не нужно: он читает
//! уже распакованное дерево.

mod archive;

pub use archive::{ArchiveError, TOP_LEVEL, is_wanted, pack, pack_with, unpack, unpack_parallel};

use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Пишется последним; меняется при изменении состава кэша.
pub const CACHE_MARKER: &str = ".chunkwake-cache-v15";

/// Вложенные `.jar` версии лежат здесь (бандлер распаковывает сервер сюда).
const VERSION_JAR_PREFIX: &str = "META-INF/versions/";

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("{0}")]
    Io(String),
    #[error("data generator failed: {0}")]
    Generator(String),
    #[error("{file}: {msg}")]
    Format { file: String, msg: String },
}

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
) -> Result<PathBuf, CacheError> {
    ensure_generated_with(java, jar, version, cache_root, &|_| {})
}

/// То же, с сообщениями о ходе генерации (этапы и «ещё идёт» раз в несколько секунд).
pub fn ensure_generated_with(
    java: &Path,
    jar: &Path,
    version: &str,
    cache_root: &Path,
    progress: &dyn Fn(&str),
) -> Result<PathBuf, CacheError> {
    if let Some(target) = cached(cache_root, version) {
        return Ok(target);
    }
    let target = cache_root.join(version);
    let io = |what: &str, p: &Path, e: std::io::Error| {
        CacheError::Io(format!("{what} {}: {e}", p.display()))
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
    progress("[1/2] running the vanilla data generator (reports, registries, loot tables)");
    let mut cmd = Command::new(java);
    cmd.current_dir(&work)
        .arg("-DbundlerMainClass=net.minecraft.data.Main")
        .arg("-jar")
        .arg(&jar)
        .arg("--server")
        .arg("--reports")
        .arg("--output")
        .arg(&tmp_abs);
    let output = run(cmd, "[1/2] data generator", progress).map_err(|e| {
        CacheError::Generator(format!(
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
        return Err(CacheError::Generator(format!(
            "exit {}{hint}: {}",
            output.status,
            tail.into_iter().rev().collect::<Vec<_>>().join(" | ")
        )));
    }
    if !tmp_out.join("reports/packets.json").is_file() {
        return Err(CacheError::Generator(
            "no reports/packets.json produced".into(),
        ));
    }
    // Свет и климат биомов в отчётах генератора не лежат: их выгружают
    // маленькие классы на распакованных бандлером `versions/` и `libraries/`.
    let class_dir = work.join("chunkwake-extractor");
    let io = |what: &str, p: &Path, e: std::io::Error| {
        CacheError::Io(format!("{what} {}: {e}", p.display()))
    };
    std::fs::create_dir_all(&class_dir).map_err(|e| io("create", &class_dir, e))?;
    std::fs::write(class_dir.join("ChunkwakeLight.class"), LIGHT_EXTRACTOR)
        .map_err(|e| io("write", &class_dir.join("ChunkwakeLight.class"), e))?;
    std::fs::write(class_dir.join("ChunkwakeBiomes.class"), BIOME_EXTRACTOR)
        .map_err(|e| io("write", &class_dir.join("ChunkwakeBiomes.class"), e))?;
    std::fs::write(
        class_dir.join("ChunkwakeWorldgen.class"),
        WORLDGEN_EXTRACTOR,
    )
    .map_err(|e| io("write", &class_dir.join("ChunkwakeWorldgen.class"), e))?;
    std::fs::write(class_dir.join("ChunkwakeShapes.class"), SHAPES_EXTRACTOR)
        .map_err(|e| io("write", &class_dir.join("ChunkwakeShapes.class"), e))?;
    std::fs::write(
        class_dir.join("ChunkwakeInstruments.class"),
        INSTRUMENTS_EXTRACTOR,
    )
    .map_err(|e| io("write", &class_dir.join("ChunkwakeInstruments.class"), e))?;
    std::fs::write(
        class_dir.join("ChunkwakeMapColors.class"),
        MAP_COLORS_EXTRACTOR,
    )
    .map_err(|e| io("write", &class_dir.join("ChunkwakeMapColors.class"), e))?;
    std::fs::write(
        class_dir.join("ChunkwakeEntities.class"),
        ENTITIES_EXTRACTOR,
    )
    .map_err(|e| io("write", &class_dir.join("ChunkwakeEntities.class"), e))?;
    std::fs::write(
        class_dir.join("ChunkwakePathTypes.class"),
        PATH_TYPES_EXTRACTOR,
    )
    .map_err(|e| io("write", &class_dir.join("ChunkwakePathTypes.class"), e))?;

    let mut classpath =
        vec![std::path::absolute(&class_dir).map_err(|e| io("resolve", &class_dir, e))?];
    for dir in ["versions", "libraries"] {
        jars(&work.join(dir), &mut classpath);
    }
    let classpath = std::env::join_paths(&classpath)
        .map_err(|e| CacheError::Generator(format!("classpath: {e}")))?;

    progress("[2/3] extracting block light and hardness");
    run_extractor(
        java,
        &work,
        &classpath,
        "ChunkwakeLight",
        &tmp_abs.join("reports/light.csv"),
        "[2/3] light extractor",
        progress,
    )?;
    progress("[3/3] extracting overworld biome parameters");
    run_extractor(
        java,
        &work,
        &classpath,
        "ChunkwakeBiomes",
        &tmp_abs.join("reports/biomes.csv"),
        "[3/3] biome extractor",
        progress,
    )?;
    progress("[3/3] extracting block fluids and classes for worldgen");
    run_extractor(
        java,
        &work,
        &classpath,
        "ChunkwakeWorldgen",
        &tmp_abs.join("reports/worldgen_blocks.csv"),
        "[3/3] worldgen extractor",
        progress,
    )?;
    progress("[3/3] extracting block collision shapes");
    run_extractor(
        java,
        &work,
        &classpath,
        "ChunkwakeShapes",
        &tmp_abs.join("reports/shapes.csv"),
        "[3/3] shapes extractor",
        progress,
    )?;
    progress("[3/3] extracting note block instruments");
    run_extractor(
        java,
        &work,
        &classpath,
        "ChunkwakeInstruments",
        &tmp_abs.join("reports/instruments.csv"),
        "[3/3] instruments extractor",
        progress,
    )?;
    progress("[3/3] extracting block map colors");
    run_extractor(
        java,
        &work,
        &classpath,
        "ChunkwakeMapColors",
        &tmp_abs.join("reports/map_colors.csv"),
        "[3/3] map colors extractor",
        progress,
    )?;
    progress("[3/3] extracting entity types");
    run_extractor(
        java,
        &work,
        &classpath,
        "ChunkwakeEntities",
        &tmp_abs.join("reports/entities.csv"),
        "[3/3] entities extractor",
        progress,
    )?;
    progress("[3/3] extracting pathfinding cell types");
    run_extractor(
        java,
        &work,
        &classpath,
        "ChunkwakePathTypes",
        &tmp_abs.join("reports/path_types.csv"),
        "[3/3] path types extractor",
        progress,
    )?;
    // Шаблоны структур не входят в вывод генератора данных: они лежат
    // в jar версии. Пишем их в тот же временный каталог до маркера.
    progress("copying structure templates");
    copy_structures(&jar, &tmp_out)?;
    // Язык клиента нужен консоли команд: он тоже живёт только в jar.
    let _ = copy_lang(&jar, &tmp_out);
    // Версия игры — часть кэша, а не что-то, что дописывается потом: по ней
    // `cache::cached`, `doctor` и `build.rs` узнают готовое дерево, и она же
    // нужна встроенному архиву. Сервер раньше писал её сам после генерации,
    // а сборка (`build.rs`) шла этим же путём и упаковывала кэш без версии —
    // бинарь получался со встроенными данными, но без `version.json`.
    std::fs::write(tmp_out.join("version.json"), version_json(&jar)?)
        .map_err(|e| io("write", &tmp_out.join("version.json"), e))?;
    std::fs::write(tmp_out.join(CACHE_MARKER), "").map_err(|e| io("write", &tmp_out, e))?;
    std::fs::rename(&tmp_out, &target).map_err(|e| io("rename", &tmp_out, e))?;
    Ok(target)
}

/// Выгрузка свойств света (исходник рядом: `extractor/ChunkwakeLight.java`).
const LIGHT_EXTRACTOR: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/extractor/ChunkwakeLight.class"));

/// Выгрузка климатических параметров биомов (`extractor/ChunkwakeBiomes.java`).
const BIOME_EXTRACTOR: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/extractor/ChunkwakeBiomes.class"));

/// Выгрузка коллизии и трения блоков (`extractor/ChunkwakeShapes.java`).
const SHAPES_EXTRACTOR: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/extractor/ChunkwakeShapes.class"));

/// Выгрузка инструмента нотного блока (`extractor/ChunkwakeInstruments.java`).
const INSTRUMENTS_EXTRACTOR: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/extractor/ChunkwakeInstruments.class"
));

/// Выгрузка цвета блоков на карте (`extractor/ChunkwakeMapColors.java`).
const MAP_COLORS_EXTRACTOR: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/extractor/ChunkwakeMapColors.class"
));

/// Выгрузка свойств типов сущностей (`extractor/ChunkwakeEntities.java`).
const ENTITIES_EXTRACTOR: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/extractor/ChunkwakeEntities.class"
));

/// Выгрузка типов клеток поиска пути (`extractor/ChunkwakePathTypes.java`).
const PATH_TYPES_EXTRACTOR: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/extractor/ChunkwakePathTypes.class"
));

/// Выгрузка жидкостей и классов блоков для фич (`extractor/ChunkwakeWorldgen.java`).
const WORLDGEN_EXTRACTOR: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/extractor/ChunkwakeWorldgen.class"
));

/// Один выгрузчик: класс на общем classpath, результат — файл `out`.
fn run_extractor(
    java: &Path,
    work: &Path,
    classpath: &std::ffi::OsStr,
    main: &str,
    out: &Path,
    step: &str,
    progress: &dyn Fn(&str),
) -> Result<(), CacheError> {
    let mut cmd = Command::new(java);
    cmd.current_dir(work)
        .arg("-cp")
        .arg(classpath)
        .arg(main)
        .arg(out);
    let output = run(cmd, step, progress)
        .map_err(|e| CacheError::Generator(format!("cannot start {}: {e}", java.display())))?;
    if !output.status.success() || !out.is_file() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail: Vec<&str> = stderr.lines().rev().take(5).collect();
        return Err(CacheError::Generator(format!(
            "{main} exit {}: {}",
            output.status,
            tail.into_iter().rev().collect::<Vec<_>>().join(" | ")
        )));
    }
    Ok(())
}

/// Запускает процесс, собирая вывод, и раз в несколько секунд сообщает, что он ещё идёт.
fn run(
    mut cmd: Command,
    step: &str,
    progress: &dyn Fn(&str),
) -> std::io::Result<std::process::Output> {
    use std::io::Read;
    use std::process::Stdio;
    use std::time::{Duration, Instant};
    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    // Вывод читаем в потоках: иначе заполненный канал остановит процесс.
    let pipe = |r: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut r) = r {
                let _ = r.read_to_end(&mut buf);
            }
            buf
        })
    };
    let out = pipe(
        child
            .stdout
            .take()
            .map(|r| Box::new(r) as Box<dyn Read + Send>),
    );
    let err = pipe(
        child
            .stderr
            .take()
            .map(|r| Box::new(r) as Box<dyn Read + Send>),
    );
    let start = Instant::now();
    let mut next = Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() >= next {
            progress(&format!("{step}: still running ({}s)", next.as_secs()));
            next += Duration::from_secs(5);
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    progress(&format!(
        "{step}: done in {:.1}s",
        start.elapsed().as_secs_f64()
    ));
    Ok(std::process::Output {
        status,
        stdout: out.join().unwrap_or_default(),
        stderr: err.join().unwrap_or_default(),
    })
}

/// В кэше уже есть хотя бы один `.nbt` шаблон.
pub fn structures_present(data_dir: &Path) -> bool {
    fn any_nbt(dir: &Path) -> bool {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return false;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if any_nbt(&path) {
                    return true;
                }
            } else if path.extension().is_some_and(|e| e == "nbt") {
                return true;
            }
        }
        false
    }
    any_nbt(&data_dir.join("data/minecraft/structure"))
}

/// Дописывает `data/minecraft/structure/**/*.nbt` из `server.jar` (или из
/// вложенного jar версии) в готовый кэш. Каталог с шаблонами не трогает
/// и `reports/` не пересоздаёт. Возвращает число скопированных файлов;
/// ноль — шаблоны уже были.
pub fn copy_structures(jar: &Path, data_dir: &Path) -> Result<usize, CacheError> {
    if structures_present(data_dir) {
        return Ok(0);
    }
    let io = |what: &str, p: &Path, e: std::io::Error| {
        CacheError::Io(format!("{what} {}: {e}", p.display()))
    };
    let file = std::fs::File::open(jar).map_err(|e| io("open", jar, e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| CacheError::Format {
        file: jar.display().to_string(),
        msg: e.to_string(),
    })?;
    let staging = data_dir.join("data/minecraft/.structure-partial");
    if staging.exists() {
        std::fs::remove_dir_all(&staging).map_err(|e| io("remove", &staging, e))?;
    }
    let mut count = write_prefix(
        &mut archive,
        "data/minecraft/structure/",
        ".nbt",
        &staging,
        jar,
    )?;
    if count == 0 {
        let nested = nested_version_jar(&mut archive, jar)?;
        let mut inner =
            zip::ZipArchive::new(std::io::Cursor::new(nested)).map_err(|e| CacheError::Format {
                file: jar.display().to_string(),
                msg: e.to_string(),
            })?;
        count = write_prefix(
            &mut inner,
            "data/minecraft/structure/",
            ".nbt",
            &staging,
            jar,
        )?;
    }
    if count == 0 {
        return Err(CacheError::Format {
            file: jar.display().to_string(),
            msg: "no data/minecraft/structure/*.nbt".into(),
        });
    }
    let dest = data_dir.join("data/minecraft/structure");
    if dest.exists() {
        std::fs::remove_dir_all(&dest).map_err(|e| io("remove", &dest, e))?;
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| io("create", parent, e))?;
    }
    std::fs::rename(&staging, &dest).map_err(|e| io("rename", &staging, e))?;
    Ok(count)
}

/// Пишет `assets/minecraft/lang/*.json` из `server.jar` в кэш: консоль команд
/// берёт оттуда `en_us.json`. Нет файла в кэше — не ошибка, ключи останутся
/// ключами.
pub fn copy_lang(jar: &Path, data_dir: &Path) -> Result<usize, CacheError> {
    let dest = data_dir.join("assets/minecraft/lang");
    if dest.is_dir() {
        return Ok(0);
    }
    let file = std::fs::File::open(jar)
        .map_err(|e| CacheError::Io(format!("open {}: {e}", jar.display())))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| CacheError::Format {
        file: jar.display().to_string(),
        msg: e.to_string(),
    })?;
    let mut count = write_prefix(&mut archive, "assets/minecraft/lang/", ".json", &dest, jar)?;
    if count == 0 {
        let nested = nested_version_jar(&mut archive, jar)?;
        let mut inner =
            zip::ZipArchive::new(std::io::Cursor::new(nested)).map_err(|e| CacheError::Format {
                file: jar.display().to_string(),
                msg: e.to_string(),
            })?;
        count = write_prefix(&mut inner, "assets/minecraft/lang/", ".json", &dest, jar)?;
    }
    Ok(count)
}

/// Языковые файлы клиента из jar, не распаковывая их на диск:
/// `("assets/minecraft/lang/en_us.json", байты)`. Нужны сборке, когда кэш
/// собран раньше появления языкового файла: архив всё равно должен его нести.
pub fn lang_entries(jar: &Path) -> Result<Vec<(String, Vec<u8>)>, CacheError> {
    let file = std::fs::File::open(jar)
        .map_err(|e| CacheError::Io(format!("open {}: {e}", jar.display())))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| CacheError::Format {
        file: jar.display().to_string(),
        msg: e.to_string(),
    })?;
    let mut out = Vec::new();
    let names = lang_names(&archive);
    if !names.is_empty() {
        for name in names {
            out.push((name.clone(), read_bytes(&mut archive, &name, jar)?));
        }
        return Ok(out);
    }
    let nested = nested_version_jar(&mut archive, jar)?;
    let mut inner =
        zip::ZipArchive::new(std::io::Cursor::new(nested)).map_err(|e| CacheError::Format {
            file: jar.display().to_string(),
            msg: e.to_string(),
        })?;
    for name in lang_names(&inner) {
        out.push((name.clone(), read_bytes(&mut inner, &name, jar)?));
    }
    Ok(out)
}

fn lang_names<R: Read + Seek>(archive: &zip::ZipArchive<R>) -> Vec<String> {
    archive
        .file_names()
        .filter(|name| {
            let name = name.trim_start_matches('/');
            name.starts_with("assets/minecraft/lang/") && name.ends_with(".json")
        })
        .map(|name| name.trim_start_matches('/').to_owned())
        .collect()
}

fn read_bytes<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
    jar: &Path,
) -> Result<Vec<u8>, CacheError> {
    let mut file = archive.by_name(name).map_err(|e| CacheError::Format {
        file: jar.display().to_string(),
        msg: format!("{name}: {e}"),
    })?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|e| CacheError::Format {
            file: jar.display().to_string(),
            msg: format!("{name}: {e}"),
        })?;
    Ok(bytes)
}
/// сохраняя путь после префикса. Возвращает число файлов.
fn write_prefix<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    prefix: &str,
    suffix: &str,
    dest: &Path,
    jar: &Path,
) -> Result<usize, CacheError> {
    let names: Vec<String> = archive
        .file_names()
        .filter(|name| {
            let name = name.trim_start_matches('/');
            name.starts_with(prefix) && name.ends_with(suffix)
        })
        .map(|name| name.trim_start_matches('/').to_owned())
        .collect();
    let io = |what: &str, p: &Path, e: std::io::Error| {
        CacheError::Io(format!("{what} {}: {e}", p.display()))
    };
    for name in &names {
        let mut file = archive.by_name(name).map_err(|e| CacheError::Format {
            file: jar.display().to_string(),
            msg: format!("{name}: {e}"),
        })?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|e| CacheError::Format {
                file: jar.display().to_string(),
                msg: format!("{name}: {e}"),
            })?;
        let rel = name.trim_start_matches(prefix);
        let path = dest.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| io("create", parent, e))?;
        }
        std::fs::write(&path, bytes).map_err(|e| io("write", &path, e))?;
    }
    Ok(names.len())
}

/// Вложенный jar версии из бандлера (`META-INF/versions/<версия>/*.jar`).
pub fn nested_version_jar<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    jar: &Path,
) -> Result<Vec<u8>, CacheError> {
    let name = {
        let mut found = None;
        for entry in archive.file_names() {
            if entry.starts_with(VERSION_JAR_PREFIX) && entry.ends_with(".jar") {
                found = Some(entry.to_owned());
                break;
            }
        }
        found
    };
    let Some(name) = name else {
        return Err(CacheError::Format {
            file: jar.display().to_string(),
            msg: "no version jar and no structure templates".into(),
        });
    };
    let mut file = archive.by_name(&name).map_err(|e| CacheError::Format {
        file: jar.display().to_string(),
        msg: format!("{name}: {e}"),
    })?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|e| CacheError::Format {
            file: jar.display().to_string(),
            msg: format!("{name}: {e}"),
        })?;
    Ok(bytes)
}

/// `version.json` из jar: имя версии и номер протокола. Читается и из внешнего
/// jar, и из вложенного jar версии.
pub fn version_json(jar: &Path) -> Result<String, CacheError> {
    let io = |e: std::io::Error| CacheError::Io(format!("open {}: {e}", jar.display()));
    let file = std::fs::File::open(jar).map_err(io)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| CacheError::Format {
        file: jar.display().to_string(),
        msg: e.to_string(),
    })?;
    if let Ok(text) = read_entry(&mut archive, "version.json", jar) {
        return Ok(text);
    }
    let nested = nested_version_jar(&mut archive, jar)?;
    let mut inner =
        zip::ZipArchive::new(std::io::Cursor::new(nested)).map_err(|e| CacheError::Format {
            file: jar.display().to_string(),
            msg: e.to_string(),
        })?;
    read_entry(&mut inner, "version.json", jar)
}

fn read_entry<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
    jar: &Path,
) -> Result<String, CacheError> {
    let mut file = archive.by_name(name).map_err(|e| CacheError::Format {
        file: jar.display().to_string(),
        msg: format!("{name}: {e}"),
    })?;
    let mut text = String::new();
    file.read_to_string(&mut text)
        .map_err(|e| CacheError::Format {
            file: jar.display().to_string(),
            msg: format!("{name}: {e}"),
        })?;
    Ok(text)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Дописывает шаблоны в уже собранный кэш, не снося `reports/`.
    #[test]
    fn structures_fill_existing_cache() {
        let Some(data) = std::env::var_os("CHUNKWAKE_DATA").map(PathBuf::from) else {
            return;
        };
        let marker = data.join(CACHE_MARKER);
        if !marker.is_file() {
            return;
        }
        if structures_present(&data) {
            return;
        }
        let jar = std::env::var_os("CHUNKWAKE_JAR")
            .map(PathBuf::from)
            .filter(|p| p.is_file())
            .or_else(|| {
                let mut dir = data.clone();
                for _ in 0..4 {
                    if !dir.pop() {
                        break;
                    }
                    let jar = dir.join("server.jar");
                    if jar.is_file() {
                        return Some(jar);
                    }
                }
                None
            });
        let Some(jar) = jar else {
            panic!("structure templates are missing and server.jar was not found");
        };
        let reports = data.join("reports/packets.json");
        let before = reports.metadata().unwrap().modified().unwrap();
        let n = copy_structures(&jar, &data).unwrap();
        assert!(n > 0);
        assert!(structures_present(&data));
        assert!(
            data.join("data/minecraft/structure/desert_well/well.nbt")
                .is_file()
        );
        assert_eq!(reports.metadata().unwrap().modified().unwrap(), before);
    }
}
