//! Операторы сервера: `ops.json` в формате ванилы (`ServerOpList`).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use uuid::Uuid;

/// Уровень прав по умолчанию для `/op` (`op-permission-level` ванилы).
pub const DEFAULT_LEVEL: i32 = 4;
/// Уровни, с которых доступны команды (`Commands.LEVEL_*`).
pub const LEVEL_GAMEMASTERS: i32 = 2;
pub const LEVEL_ADMINS: i32 = 3;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpEntry {
    pub uuid: Uuid,
    pub name: String,
    pub level: i32,
    #[serde(default)]
    pub bypasses_player_limit: bool,
}

pub struct OpList {
    path: Option<PathBuf>,
    entries: Mutex<Vec<OpEntry>>,
}

impl OpList {
    /// Пустой список без файла (тесты, сервер без каталога).
    pub fn empty() -> Self {
        Self {
            path: None,
            entries: Mutex::new(Vec::new()),
        }
    }

    /// Читает `ops.json`; нет файла — пустой список.
    pub fn load(path: PathBuf) -> Result<Self, String> {
        let entries = match std::fs::read_to_string(&path) {
            Ok(text) if text.trim().is_empty() => Vec::new(),
            Ok(text) => {
                serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(format!("read {}: {e}", path.display())),
        };
        Ok(Self {
            path: Some(path),
            entries: Mutex::new(entries),
        })
    }

    /// Уровень прав игрока; 0 — не оператор.
    pub fn level(&self, uuid: Uuid) -> i32 {
        self.entries
            .lock()
            .unwrap()
            .iter()
            .find(|e| e.uuid == uuid)
            .map_or(0, |e| e.level)
    }

    /// Делает оператором; `false` — уже был.
    pub fn op(&self, uuid: Uuid, name: &str) -> bool {
        let mut entries = self.entries.lock().unwrap();
        if entries.iter().any(|e| e.uuid == uuid) {
            return false;
        }
        entries.push(OpEntry {
            uuid,
            name: name.to_owned(),
            level: DEFAULT_LEVEL,
            bypasses_player_limit: false,
        });
        drop(entries);
        self.save();
        true
    }

    /// Снимает права; `false` — не был оператором.
    pub fn deop(&self, uuid: Uuid) -> bool {
        let mut entries = self.entries.lock().unwrap();
        let before = entries.len();
        entries.retain(|e| e.uuid != uuid);
        let changed = entries.len() != before;
        drop(entries);
        if changed {
            self.save();
        }
        changed
    }

    /// Оператор по имени (для `/deop` офлайн-игрока).
    pub fn find_name(&self, name: &str) -> Option<OpEntry> {
        self.entries
            .lock()
            .unwrap()
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(name))
            .cloned()
    }

    fn save(&self) {
        let Some(path) = &self.path else { return };
        let text = serde_json::to_string_pretty(&*self.entries.lock().unwrap()).unwrap_or_default();
        if let Err(e) = std::fs::write(path, text) {
            tracing::error!("write {}: {e}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vanilla_format_round_trip() {
        let dir = std::env::temp_dir().join(format!("chunkwake-ops-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ops.json");
        std::fs::write(
            &path,
            r#"[{"uuid":"069a79f4-44e9-4726-a5be-fca90e38aaf5","name":"Notch","level":4,"bypassesPlayerLimit":false}]"#,
        )
        .unwrap();
        let ops = OpList::load(path.clone()).unwrap();
        let notch: Uuid = "069a79f4-44e9-4726-a5be-fca90e38aaf5".parse().unwrap();
        assert_eq!(ops.level(notch), 4);
        let other = Uuid::from_u128(5);
        assert!(ops.op(other, "Other"));
        assert!(!ops.op(other, "Other"));
        assert_eq!(
            OpList::load(path.clone()).unwrap().level(other),
            DEFAULT_LEVEL
        );
        assert!(ops.deop(notch));
        assert_eq!(OpList::load(path).unwrap().level(notch), 0);
        let _ = std::fs::remove_dir_all(dir);
    }
}
