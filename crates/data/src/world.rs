//! Блоки (`reports/blocks.json`) и типы измерений (`data/minecraft/dimension_type`).

use crate::DataError;
use std::collections::HashMap;
use std::path::Path;

/// `reports/light.csv` (выгрузка `ChunkwakeLight`): по id состояния
/// `(излучение, затухание, небесный свет проходит вниз)`.
pub fn load_light(root: &Path) -> Result<Vec<(u8, u8, bool)>, DataError> {
    let path = root.join("reports/light.csv");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| DataError::Io(format!("read {}: {e}", path.display())))?;
    text.lines()
        .enumerate()
        .map(|(i, line)| {
            let mut f = line.split(',').map(|v| v.trim().parse::<u8>().ok());
            match (f.next().flatten(), f.next().flatten(), f.next().flatten()) {
                (Some(e), Some(d), Some(s)) => Ok((e, d, s != 0)),
                _ => Err(DataError::Format {
                    file: path.display().to_string(),
                    msg: format!("line {}: {line:?}", i + 1),
                }),
            }
        })
        .collect()
}

fn read_json(path: &Path) -> Result<serde_json::Value, DataError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| DataError::Io(format!("read {}: {e}", path.display())))?;
    serde_json::from_str(&text).map_err(|e| DataError::Format {
        file: path.display().to_string(),
        msg: e.to_string(),
    })
}

/// Свойства состояния блока: пары `(ключ, значение)`, отсортированные по ключу.
pub type Properties = Vec<(String, String)>;

/// ID состояний блоков по умолчанию: `minecraft:stone` → сетевой ID.
#[derive(Debug, Default)]
pub struct Blocks {
    defaults: HashMap<String, i32>,
    max_id: i64,
    /// ID состояния → имя блока и свойства (отсортированы по ключу).
    states: Vec<Option<(String, Properties)>>,
    /// Обратный индекс: `name[k=v,...]` → ID.
    by_key: HashMap<String, u32>,
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
        let mut states = Vec::new();
        let mut by_key = HashMap::new();
        for (name, block) in obj {
            for s in block["states"].as_array().into_iter().flatten() {
                let id = s["id"]
                    .as_i64()
                    .ok_or_else(|| format!("{name}: state without id"))?;
                max_id = max_id.max(id);
                let mut props: Vec<(String, String)> = s["properties"]
                    .as_object()
                    .into_iter()
                    .flatten()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_owned()))
                    .collect();
                props.sort();
                by_key.insert(state_key(name, &props), id as u32);
                if states.len() <= id as usize {
                    states.resize(id as usize + 1, None);
                }
                states[id as usize] = Some((name.clone(), props));
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
        Ok(Self {
            defaults,
            max_id,
            states,
            by_key,
        })
    }

    /// Общее число состояний (бит прямой палитры = ceil(log2(n))).
    pub fn state_count(&self) -> usize {
        self.max_id as usize + 1
    }

    /// Имя блока и его свойства для ID состояния.
    pub fn state(&self, id: u32) -> Option<(&str, &[(String, String)])> {
        self.states
            .get(id as usize)?
            .as_ref()
            .map(|(n, p)| (n.as_str(), p.as_slice()))
    }

    /// ID состояния по имени и свойствам (порядок свойств не важен).
    /// Недостающие свойства не подставляются: нужен полный набор.
    pub fn state_id(&self, name: &str, props: &[(String, String)]) -> Option<u32> {
        let mut props = props.to_vec();
        props.sort();
        self.by_key.get(&state_key(name, &props)).copied()
    }

    pub fn default_state(&self, name: &str) -> Result<i32, DataError> {
        self.defaults
            .get(name)
            .copied()
            .ok_or_else(|| DataError::Io(format!("unknown block {name}")))
    }
}

fn state_key(name: &str, props: &[(String, String)]) -> String {
    let mut key = name.to_owned();
    key.push('[');
    for (i, (k, v)) in props.iter().enumerate() {
        if i > 0 {
            key.push(',');
        }
        key.push_str(k);
        key.push('=');
        key.push_str(v);
    }
    key.push(']');
    key
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
        assert_eq!(
            b.state(8),
            Some((
                "minecraft:grass_block",
                &[("snowy".to_owned(), "true".to_owned())][..]
            ))
        );
        assert_eq!(
            b.state_id("minecraft:grass_block", &[("snowy".into(), "false".into())]),
            Some(9)
        );
        assert!(b.default_state("minecraft:nope").is_err());
    }
}
