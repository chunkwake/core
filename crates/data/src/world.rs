//! Блоки (`reports/blocks.json`) и типы измерений (`data/minecraft/dimension_type`).

use crate::DataError;
use std::collections::HashMap;
use std::path::Path;

fn read_json(path: &Path) -> Result<serde_json::Value, DataError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| DataError::Io(format!("read {}: {e}", path.display())))?;
    serde_json::from_str(&text).map_err(|e| DataError::Format {
        file: path.display().to_string(),
        msg: e.to_string(),
    })
}

/// ID состояний блоков по умолчанию: `minecraft:stone` → сетевой ID.
#[derive(Debug, Default)]
pub struct Blocks {
    defaults: HashMap<String, i32>,
    max_id: i64,
}

impl Blocks {
    pub fn load(root: &Path) -> Result<Self, DataError> {
        let path = root.join("reports/blocks.json");
        Self::parse(&read_json(&path)?).map_err(|msg| DataError::Format {
            file: path.display().to_string(),
            msg,
        })
    }

    pub fn parse(json: &serde_json::Value) -> Result<Self, String> {
        let obj = json.as_object().ok_or("expected object")?;
        let mut defaults = HashMap::with_capacity(obj.len());
        let mut max_id = 0;
        for (name, block) in obj {
            for s in block["states"].as_array().into_iter().flatten() {
                max_id = max_id.max(s["id"].as_i64().unwrap_or(0));
            }
            let state = block["states"]
                .as_array()
                .and_then(|s| s.iter().find(|s| s["default"] == true))
                .ok_or_else(|| format!("{name}: no default state"))?;
            let id = state["id"]
                .as_i64()
                .ok_or_else(|| format!("{name}: no id"))?;
            defaults.insert(name.clone(), id as i32);
        }
        Ok(Self { defaults, max_id })
    }

    /// Общее число состояний (бит прямой палитры = ceil(log2(n))).
    pub fn state_count(&self) -> usize {
        self.max_id as usize + 1
    }

    pub fn default_state(&self, name: &str) -> Result<i32, DataError> {
        self.defaults
            .get(name)
            .copied()
            .ok_or_else(|| DataError::Io(format!("unknown block {name}")))
    }
}

/// Параметры измерения, нужные для кодирования чанков.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DimensionType {
    pub min_y: i32,
    pub height: i32,
    pub has_skylight: bool,
}

impl DimensionType {
    /// `name` — путь без namespace, например `overworld`.
    pub fn load(root: &Path, name: &str) -> Result<Self, DataError> {
        let path = root.join(format!("data/minecraft/dimension_type/{name}.json"));
        let json = read_json(&path)?;
        let int = |k: &str| {
            json[k]
                .as_i64()
                .map(|v| v as i32)
                .ok_or_else(|| DataError::Format {
                    file: path.display().to_string(),
                    msg: format!("no {k}"),
                })
        };
        Ok(Self {
            min_y: int("min_y")?,
            height: int("height")?,
            has_skylight: json["has_skylight"].as_bool().unwrap_or(false),
        })
    }

    pub fn sections(&self) -> usize {
        (self.height / 16) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_states() {
        let json = serde_json::json!({
            "minecraft:grass_block": { "states": [
                { "id": 8, "properties": { "snowy": "true" } },
                { "default": true, "id": 9, "properties": { "snowy": "false" } }
            ]}
        });
        let b = Blocks::parse(&json).unwrap();
        assert_eq!(b.default_state("minecraft:grass_block").unwrap(), 9);
        assert!(b.default_state("minecraft:nope").is_err());
    }
}
