//! Offline snapshots and guarded recovery of server state.

use crate::config::Properties;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MARKER: &str = ".chunkwake-restore.json";
const LOCK: &str = ".chunkwake.lock";

pub struct ServerLock(File);

pub fn lock(dir: &Path) -> Result<ServerLock, String> {
    if !dir.is_dir() {
        return Err(format!("server directory {} does not exist", dir.display()));
    }
    let path = dir.join(LOCK);
    let file = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    file.try_lock()
        .map_err(|e| format!("server directory {} is already in use: {e}", dir.display()))?;
    Ok(ServerLock(file))
}

impl Drop for ServerLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

pub fn incomplete(dir: &Path) -> Option<String> {
    let path = dir.join(MARKER);
    path.exists().then(|| format!(
        "unfinished restore at {}; run `chunkwake backup resume --dir {}` or `chunkwake backup rollback --dir {}`",
        path.display(), dir.display(), dir.display()
    ))
}

fn one_component(value: &str) -> Result<&str, String> {
    let path = Path::new(value);
    if path.components().count() == 1
        && matches!(path.components().next(), Some(Component::Normal(_)))
    {
        Ok(value)
    } else {
        Err(format!(
            "level-name={value:?}: use one directory name without separators"
        ))
    }
}

fn roots(dir: &Path) -> Result<Vec<String>, String> {
    let props = Properties::load(&dir.join("server.properties"))?;
    let world = one_component(&props.level_name)?.to_owned();
    if [
        "backups",
        "plugins",
        ".chunkwake-restore-stage",
        ".chunkwake-restore-old",
    ]
    .contains(&world.as_str())
    {
        return Err("level-name conflicts with a reserved server directory".into());
    }
    Ok(vec![
        "server.properties".into(),
        "ops.json".into(),
        world,
        "plugins".into(),
    ])
}

fn backup_dir(dir: &Path) -> PathBuf {
    dir.join("backups")
}

fn id() -> String {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("{n}-{}-{}", std::process::id(), rand::random::<u32>())
}

fn walk(source: &Path, relative: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let meta = fs::symlink_metadata(source).map_err(|e| format!("{}: {e}", source.display()))?;
    if meta.file_type().is_symlink() {
        return Err(format!(
            "{}: symbolic links are not supported in backups",
            source.display()
        ));
    }
    if meta.is_file() {
        files.push(relative.to_owned());
    } else if meta.is_dir() {
        let mut entries = fs::read_dir(source)
            .map_err(|e| format!("{}: {e}", source.display()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            walk(&entry.path(), &relative.join(entry.file_name()), files)?;
        }
    } else {
        return Err(format!("{}: unsupported file type", source.display()));
    }
    Ok(())
}

fn hash(path: &Path) -> Result<(String, u64), String> {
    let mut file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut digest = Sha256::new();
    let mut size = 0;
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
        size += n as u64;
    }
    Ok((format!("{:x}", digest.finalize()), size))
}

fn safe_relative(path: &str) -> Result<PathBuf, String> {
    let p = Path::new(path);
    if p.components().all(|c| matches!(c, Component::Normal(_))) && p.components().next().is_some()
    {
        Ok(p.to_owned())
    } else {
        Err(format!("unsafe backup path {path:?}"))
    }
}

fn reject_symlink_chain(base: &Path, relative: &Path) -> Result<(), String> {
    let mut current = base.to_owned();
    for part in relative.components() {
        current.push(part.as_os_str());
        let meta =
            fs::symlink_metadata(&current).map_err(|e| format!("{}: {e}", current.display()))?;
        if meta.file_type().is_symlink() {
            return Err(format!("{}: symbolic link in backup", current.display()));
        }
    }
    Ok(())
}

fn copy_file(src: &Path, dst: &Path) -> Result<(), String> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut from = File::open(src).map_err(|e| format!("{}: {e}", src.display()))?;
    let mut to = File::create(dst).map_err(|e| format!("{}: {e}", dst.display()))?;
    io::copy(&mut from, &mut to)
        .map_err(|e| format!("{} -> {}: {e}", src.display(), dst.display()))?;
    to.sync_all()
        .map_err(|e| format!("sync {}: {e}", dst.display()))?;
    Ok(())
}

pub fn create(dir: &Path) -> Result<String, String> {
    if let Some(error) = incomplete(dir) {
        return Err(error);
    }
    let problems = scan_data(dir)?;
    if !problems.is_empty() {
        return Err(format!(
            "cannot back up damaged world data:\n{}",
            problems.join("\n")
        ));
    }
    let roots = roots(dir)?;
    create_from_roots(dir, roots, true)
}

fn create_from_roots(
    dir: &Path,
    roots: Vec<String>,
    require_config: bool,
) -> Result<String, String> {
    for root in &roots {
        one_component(root)?;
        if [
            "backups",
            ".chunkwake-restore-stage",
            ".chunkwake-restore-old",
            LOCK,
            MARKER,
        ]
        .contains(&root.as_str())
        {
            return Err(format!("reserved backup root {root}"));
        }
    }
    let mut files = Vec::new();
    for root in &roots {
        let path = dir.join(root);
        if path.exists() {
            walk(&path, Path::new(root), &mut files)?;
        } else if root == "server.properties" && require_config {
            return Err("server.properties is missing; run setup".into());
        }
    }
    let id = id();
    let base = backup_dir(dir);
    fs::create_dir_all(&base).map_err(|e| e.to_string())?;
    let staging = base.join(format!(".{id}.tmp"));
    let final_dir = base.join(&id);
    fs::create_dir(&staging).map_err(|e| e.to_string())?;
    let result = (|| {
        let mut entries = Vec::new();
        for relative in files {
            let path = relative.to_string_lossy().replace('\\', "/");
            copy_file(&dir.join(&relative), &staging.join(&relative))?;
            let (sha256, size) = hash(&staging.join(&relative))?;
            entries.push(serde_json::json!({"path": path, "sha256": sha256, "size": size}));
        }
        let manifest = serde_json::json!({"format": 1, "game_version": crate::MC_VERSION, "roots": roots, "files": entries});
        let manifest_path = staging.join("manifest.json");
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        File::options()
            .write(true)
            .open(&manifest_path)
            .and_then(|file| file.sync_all())
            .map_err(|e| e.to_string())?;
        fs::rename(&staging, &final_dir).map_err(|e| format!("publish backup: {e}"))?;
        Ok::<(), String>(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result.map(|()| id)
}

fn backup_id(id: &str) -> Result<&str, String> {
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit() || b == b'-') {
        Err("backup id must contain only digits and hyphens".into())
    } else {
        Ok(id)
    }
}

fn manifest(dir: &Path, id: &str) -> Result<serde_json::Value, String> {
    let id = backup_id(id)?;
    let path = backup_dir(dir).join(id).join("manifest.json");
    let bytes = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    if value["format"] != 1 || !value["files"].is_array() || !value["roots"].is_array() {
        return Err(format!(
            "{}: unsupported or invalid manifest",
            path.display()
        ));
    }
    Ok(value)
}

pub fn verify(dir: &Path, id: &str) -> Result<usize, String> {
    let count = verify_bytes(dir, id)?;
    let base = backup_dir(dir).join(id);
    let problems = scan_data(&base)?;
    if !problems.is_empty() {
        return Err(format!(
            "backup {id} contains damaged world data:\n{}",
            problems.join("\n")
        ));
    }
    Ok(count)
}

fn verify_bytes(dir: &Path, id: &str) -> Result<usize, String> {
    let value = manifest(dir, id)?;
    if value["game_version"] != crate::MC_VERSION {
        return Err(format!("backup {id} is for a different Minecraft version"));
    }
    let base = backup_dir(dir).join(id);
    let mut count = 0;
    for entry in value["files"].as_array().unwrap() {
        let name = entry["path"]
            .as_str()
            .ok_or("manifest file path is missing")?;
        let relative = safe_relative(name)?;
        let expected = entry["sha256"]
            .as_str()
            .ok_or("manifest checksum is missing")?;
        let size = entry["size"].as_u64().ok_or("manifest size is missing")?;
        let path = base.join(&relative);
        reject_symlink_chain(&base, &relative)?;
        let meta = fs::symlink_metadata(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        if !meta.is_file() || meta.file_type().is_symlink() {
            return Err(format!("{}: not a regular file", path.display()));
        }
        let (actual, actual_size) = hash(&path)?;
        if actual != expected || actual_size != size {
            return Err(format!("{}: checksum mismatch", path.display()));
        }
        count += 1;
    }
    Ok(count)
}

pub fn list(dir: &Path) -> Result<Vec<String>, String> {
    let base = backup_dir(dir);
    if !base.exists() {
        return Ok(Vec::new());
    }
    let mut ids = Vec::new();
    for entry in fs::read_dir(base).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let id = entry.file_name().to_string_lossy().into_owned();
        if backup_id(&id).is_ok() && entry.path().join("manifest.json").is_file() {
            ids.push(id);
        }
    }
    ids.sort();
    ids.reverse();
    Ok(ids)
}

fn marker(dir: &Path) -> Result<(String, String), String> {
    let path = dir.join(MARKER);
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let target = value["target"]
        .as_str()
        .ok_or("restore marker has no target")?;
    let safety = value["safety"]
        .as_str()
        .ok_or("restore marker has no safety backup")?;
    Ok((backup_id(target)?.into(), backup_id(safety)?.into()))
}

fn replace_from(dir: &Path, id: &str, allow_damaged: bool) -> Result<(), String> {
    if allow_damaged {
        verify_bytes(dir, id)?;
    } else {
        verify(dir, id)?;
    }
    let value = manifest(dir, id)?;
    let roots = value["roots"].as_array().unwrap();
    let stage = dir.join(".chunkwake-restore-stage");
    if stage.exists() {
        fs::remove_dir_all(&stage).map_err(|e| e.to_string())?;
    }
    fs::create_dir(&stage).map_err(|e| e.to_string())?;
    for entry in value["files"].as_array().unwrap() {
        let path = safe_relative(entry["path"].as_str().ok_or("missing path")?)?;
        copy_file(&backup_dir(dir).join(id).join(&path), &stage.join(&path))?;
    }
    let old = dir.join(".chunkwake-restore-old");
    if old.exists() {
        fs::remove_dir_all(&old).map_err(|e| e.to_string())?;
    }
    fs::create_dir(&old).map_err(|e| e.to_string())?;
    for root in roots {
        let name = root.as_str().ok_or("invalid root")?;
        one_component(name)?;
        if [
            "backups",
            ".chunkwake-restore-stage",
            ".chunkwake-restore-old",
            LOCK,
        ]
        .contains(&name)
        {
            return Err("backup contains a reserved root".into());
        }
        let current = dir.join(name);
        if current.exists() {
            fs::rename(&current, old.join(name))
                .map_err(|e| format!("move {}: {e}", current.display()))?;
        }
        let replacement = stage.join(name);
        if replacement.exists() {
            fs::rename(&replacement, &current)
                .map_err(|e| format!("restore {}: {e}", current.display()))?;
        }
    }
    fs::remove_dir_all(&stage).map_err(|e| e.to_string())?;
    fs::remove_dir_all(&old).map_err(|e| e.to_string())?;
    Ok(())
}

/// `backup restore ID`: подтверждение вводит человек (`RESTORE <id>`).
///
/// Неинтерактивный вариант — [`restore_ask`] с `yes = true`.
#[allow(dead_code)]
pub fn restore(dir: &Path, id: &str) -> Result<String, String> {
    restore_ask(dir, id, false)
}

/// `backup restore ID --yes`: подтверждение приходит флагом, а не со stdin.
///
/// Без TTY (служба, CI, контейнер) чтение `RESTORE <id>` из stdin либо
/// вернуло бы пустую строку, либо ждало вечно — восстановление было
/// недоступно там, где оно нужнее всего.
pub fn restore_ask(dir: &Path, id: &str, yes: bool) -> Result<String, String> {
    if let Some(error) = incomplete(dir) {
        return Err(error);
    }
    verify(dir, id)?;
    let value = manifest(dir, id)?;
    println!("Restore {id} over {}", dir.display());
    println!(
        "Replace: {}",
        value["roots"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    if !yes {
        print!("Type RESTORE {id} to continue: ");
        io::stdout().flush().map_err(|e| e.to_string())?;
        let mut answer = String::new();
        io::stdin()
            .read_line(&mut answer)
            .map_err(|e| e.to_string())?;
        if answer.trim() != format!("RESTORE {id}") {
            return Err("restore cancelled".into());
        }
    }
    restore_confirmed(dir, id)
}

fn restore_confirmed(dir: &Path, id: &str) -> Result<String, String> {
    verify(dir, id)?;
    let roots = manifest(dir, id)?["roots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().ok_or("invalid backup root").map(str::to_owned))
        .collect::<Result<Vec<_>, _>>()?;
    let safety = create_from_roots(dir, roots, false)?;
    let marker_path = dir.join(MARKER);
    fs::write(
        &marker_path,
        serde_json::to_vec(&serde_json::json!({"target": id, "safety": safety})).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    File::options()
        .write(true)
        .open(&marker_path)
        .and_then(|file| file.sync_all())
        .map_err(|e| e.to_string())?;
    replace_from(dir, id, false)?;
    fs::remove_file(&marker_path).map_err(|e| e.to_string())?;
    Ok(safety)
}

pub fn continue_restore(dir: &Path, rollback: bool) -> Result<(), String> {
    let (target, safety) = marker(dir)?;
    replace_from(dir, if rollback { &safety } else { &target }, rollback)?;
    fs::remove_file(dir.join(MARKER)).map_err(|e| e.to_string())
}

pub fn latest_valid(dir: &Path) -> Option<String> {
    list(dir)
        .ok()?
        .into_iter()
        .find(|id| verify(dir, id).is_ok())
}

/// Проверяет сохранённые NBT без запуска игрового мира.
pub fn scan_data(dir: &Path) -> Result<Vec<String>, String> {
    let mut problems = Vec::new();
    let ops = dir.join("ops.json");
    if ops.exists() {
        match fs::read(&ops).map_err(|e| e.to_string()).and_then(|bytes| {
            serde_json::from_slice::<serde_json::Value>(&bytes).map_err(|e| e.to_string())
        }) {
            Ok(value) if value.is_array() => {}
            Ok(_) => problems.push(format!("{}: expected a JSON array", ops.display())),
            Err(error) => problems.push(format!("{}: {error}", ops.display())),
        }
    }
    let world = dir.join(one_component(
        &Properties::load(&dir.join("server.properties"))?.level_name,
    )?);
    if world.exists() && !world.is_dir() {
        problems.push(format!("{}: expected a world directory", world.display()));
        return Ok(problems);
    }
    let region_dir = world.join("region");
    if region_dir.exists() && !region_dir.is_dir() {
        problems.push(format!("{}: expected a directory", region_dir.display()));
    }
    if region_dir.is_dir() {
        let store = chunkwake_world::anvil::RegionStore::new(region_dir.clone());
        for entry in fs::read_dir(&region_dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let parts: Vec<&str> = name.split('.').collect();
            if parts.len() == 4
                && parts[0] == "r"
                && parts[3] == "mca"
                && let (Ok(x), Ok(z)) = (parts[1].parse(), parts[2].parse())
                && let Err(error) = store.load_region(x, z)
            {
                problems.push(format!("{}: {error}", entry.path().display()));
            }
        }
    }
    let players = world.join("playerdata");
    if players.exists() && !players.is_dir() {
        problems.push(format!("{}: expected a directory", players.display()));
    }
    if players.is_dir() {
        for entry in fs::read_dir(&players).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.path().extension().is_some_and(|e| e == "dat")
                && let Err(error) = chunkwake_world::nbt::read_gzip_file(&entry.path())
            {
                problems.push(format!("{}: {error}", entry.path().display()));
            }
        }
    }
    Ok(problems)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    fn fixture() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "chunkwake-backup-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(dir.join("world/playerdata")).unwrap();
        fs::create_dir_all(dir.join("plugins/TestPlugin")).unwrap();
        fs::write(
            dir.join("server.properties"),
            "level-name=world\nserver-port=25565\n",
        )
        .unwrap();
        fs::write(dir.join("ops.json"), "[]").unwrap();
        chunkwake_world::nbt::write_gzip_file(
            &dir.join("world/playerdata/test.dat"),
            &chunkwake_world::nbt::Compound::new(),
        )
        .unwrap();
        fs::write(dir.join("plugins/TestPlugin/config.yml"), b"enabled: true").unwrap();
        fs::write(dir.join("server.jar"), b"excluded").unwrap();
        fs::create_dir_all(dir.join("generated")).unwrap();
        fs::write(dir.join("generated/data"), b"excluded").unwrap();
        dir
    }

    #[test]
    fn create_verify_and_restore_all_state() {
        let dir = fixture();
        let _lock = lock(&dir).unwrap();
        let id = create(&dir).unwrap();
        assert_eq!(verify(&dir, &id).unwrap(), 4);
        assert_eq!(list(&dir).unwrap(), vec![id.clone()]);
        assert!(!dir.join("backups").join(&id).join("server.jar").exists());
        assert!(!dir.join("backups").join(&id).join("generated").exists());
        let original_player = fs::read(dir.join("world/playerdata/test.dat")).unwrap();
        chunkwake_world::nbt::write_gzip_file(
            &dir.join("world/playerdata/test.dat"),
            &chunkwake_world::nbt::Compound::from([(
                "changed".into(),
                chunkwake_world::nbt::Tag::Int(1),
            )]),
        )
        .unwrap();
        let changed_player = fs::read(dir.join("world/playerdata/test.dat")).unwrap();
        fs::write(dir.join("plugins/TestPlugin/config.yml"), b"changed").unwrap();
        let safety = restore_confirmed(&dir, &id).unwrap();
        assert_eq!(
            fs::read(dir.join("world/playerdata/test.dat")).unwrap(),
            original_player
        );
        assert_eq!(
            fs::read(dir.join("plugins/TestPlugin/config.yml")).unwrap(),
            b"enabled: true"
        );
        assert_eq!(
            fs::read(
                dir.join("backups")
                    .join(safety)
                    .join("world/playerdata/test.dat")
            )
            .unwrap(),
            changed_player
        );
        assert!(incomplete(&dir).is_none());
        drop(_lock);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn damaged_or_partial_backup_is_not_restored() {
        let dir = fixture();
        let id = create(&dir).unwrap();
        fs::create_dir(dir.join("backups/.partial.tmp")).unwrap();
        assert_eq!(list(&dir).unwrap(), vec![id.clone()]);
        fs::write(dir.join("backups").join(&id).join("ops.json"), "corrupt").unwrap();
        assert!(verify(&dir, &id).unwrap_err().contains("checksum mismatch"));
        assert!(restore_confirmed(&dir, &id).is_err());
        assert_eq!(fs::read(dir.join("ops.json")).unwrap(), b"[]");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn interrupted_restore_can_resume_or_rollback() {
        let dir = fixture();
        let id = create(&dir).unwrap();
        fs::write(dir.join("ops.json"), "[1]").unwrap();
        let safety = create(&dir).unwrap();
        fs::write(
            dir.join(MARKER),
            serde_json::to_vec(&serde_json::json!({"target": id, "safety": safety})).unwrap(),
        )
        .unwrap();
        assert!(incomplete(&dir).is_some());
        fs::create_dir(dir.join(".chunkwake-restore-old")).unwrap();
        fs::rename(
            dir.join("ops.json"),
            dir.join(".chunkwake-restore-old/ops.json"),
        )
        .unwrap();
        continue_restore(&dir, false).unwrap();
        assert_eq!(fs::read(dir.join("ops.json")).unwrap(), b"[]");
        fs::write(
            dir.join(MARKER),
            serde_json::to_vec(&serde_json::json!({"target": id, "safety": safety})).unwrap(),
        )
        .unwrap();
        continue_restore(&dir, true).unwrap();
        assert_eq!(fs::read(dir.join("ops.json")).unwrap(), b"[1]");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn lock_rejects_second_owner() {
        let dir = fixture();
        let first = lock(&dir).unwrap();
        assert!(lock(&dir).is_err());
        drop(first);
        assert!(lock(&dir).is_ok());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn doctor_scan_reports_corrupt_region_and_finds_valid_backup() {
        let dir = fixture();
        let id = create(&dir).unwrap();
        fs::create_dir_all(dir.join("world/region")).unwrap();
        fs::write(dir.join("world/region/r.0.0.mca"), b"truncated").unwrap();
        let problems = scan_data(&dir).unwrap();
        assert!(
            problems
                .iter()
                .any(|p| p.contains("truncated region header"))
        );
        assert_eq!(latest_valid(&dir), Some(id));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn restore_recovers_damaged_live_data_and_keeps_safety_copy() {
        let dir = fixture();
        let id = create(&dir).unwrap();
        fs::write(dir.join("world/playerdata/test.dat"), b"damaged").unwrap();
        assert!(!scan_data(&dir).unwrap().is_empty());
        let safety = restore_confirmed(&dir, &id).unwrap();
        assert!(scan_data(&dir).unwrap().is_empty());
        assert!(verify_bytes(&dir, &safety).is_ok());
        assert!(verify(&dir, &safety).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
