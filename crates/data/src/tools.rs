//! Компонент `minecraft:tool` предметов (`Tool`, `Tool.Rule`) и требование
//! инструмента для дропа: из них ванила считает скорость ломки и дроп.

use crate::DataError;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

/// Правило инструмента: блоки (развёрнутые теги), скорость, годится ли для дропа.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolRule {
    pub blocks: Arc<HashSet<String>>,
    pub speed: Option<f32>,
    pub correct_for_drops: Option<bool>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tool {
    pub default_speed: f32,
    /// Износ за сломанный блок (`damage_per_block`, у мечей 2).
    pub damage_per_block: i32,
    pub rules: Vec<ToolRule>,
}

impl Tool {
    /// `Tool.getMiningSpeed`: скорость первого подходящего правила со скоростью.
    pub fn speed(&self, block: &str) -> f32 {
        self.rules
            .iter()
            .find_map(|r| r.speed.filter(|_| r.blocks.contains(block)))
            .unwrap_or(self.default_speed)
    }

    /// `Tool.isCorrectForDrops`: первое подходящее правило с флагом, иначе нет.
    pub fn correct_for_drops(&self, block: &str) -> bool {
        self.rules
            .iter()
            .find_map(|r| r.correct_for_drops.filter(|_| r.blocks.contains(block)))
            .unwrap_or(false)
    }
}

/// Правило в сыром виде (как в отчёте): блоки — `#тег` или имена через запятую.
#[derive(Clone, Debug, PartialEq)]
struct RawRule {
    blocks: String,
    speed: Option<f32>,
    correct: Option<bool>,
}

#[derive(Clone, Debug, PartialEq)]
struct RawTool {
    default_speed: f32,
    damage_per_block: i32,
    rules: Vec<RawRule>,
}

fn parse_tool(c: &serde_json::Value) -> Option<RawTool> {
    let t = c.get("minecraft:tool")?;
    let rules = t
        .get("rules")
        .and_then(|r| r.as_array())
        .map(|rules| {
            rules
                .iter()
                .filter_map(|r| {
                    let blocks = match r.get("blocks")? {
                        serde_json::Value::String(s) => s.clone(),
                        serde_json::Value::Array(a) => a
                            .iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                            .join(","),
                        _ => return None,
                    };
                    Some(RawRule {
                        blocks,
                        speed: r.get("speed").and_then(|v| v.as_f64()).map(|v| v as f32),
                        correct: r.get("correct_for_drops").and_then(|v| v.as_bool()),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Some(RawTool {
        default_speed: t
            .get("default_mining_speed")
            .and_then(|v| v.as_f64())
            .map_or(1.0, |v| v as f32),
        damage_per_block: t
            .get("damage_per_block")
            .and_then(|v| v.as_i64())
            .map_or(1, |v| v as i32),
        rules,
    })
}

/// Строка сводки: `item \t default \t damage_per_block \t blocks|speed|correct \t …`
/// (пусто — нет значения).
fn tool_line(name: &str, t: &RawTool) -> String {
    let mut line = format!("{name}\t{}\t{}", t.default_speed, t.damage_per_block);
    for r in &t.rules {
        let speed = r.speed.map(|s| s.to_string()).unwrap_or_default();
        let correct = r.correct.map(|c| c.to_string()).unwrap_or_default();
        line += &format!("\t{}|{speed}|{correct}", r.blocks);
    }
    line
}

fn parse_tool_line(line: &str) -> Option<(&str, RawTool)> {
    let mut f = line.split('\t');
    let name = f.next()?;
    let default_speed = f.next()?.parse().ok()?;
    let damage_per_block = f.next()?.parse().ok()?;
    let rules = f
        .map(|r| {
            let mut p = r.split('|');
            let blocks = p.next()?.to_owned();
            let speed = match p.next()? {
                "" => None,
                s => Some(s.parse().ok()?),
            };
            let correct = match p.next()? {
                "" => None,
                s => Some(s.parse().ok()?),
            };
            Some(RawRule {
                blocks,
                speed,
                correct,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some((
        name,
        RawTool {
            default_speed,
            damage_per_block,
            rules,
        },
    ))
}

/// Разворачивает теги блоков `data/<ns>/tags/block/<path>.json` (с вложенными тегами).
struct Tags<'a> {
    root: &'a Path,
    memo: HashMap<String, Arc<HashSet<String>>>,
}

impl Tags<'_> {
    fn resolve(&mut self, spec: &str) -> Result<Arc<HashSet<String>>, DataError> {
        if let Some(set) = self.memo.get(spec) {
            return Ok(set.clone());
        }
        let mut out = HashSet::new();
        for part in spec.split(',').filter(|p| !p.is_empty()) {
            self.add(part, &mut out, 0)?;
        }
        let set = Arc::new(out);
        self.memo.insert(spec.to_owned(), set.clone());
        Ok(set)
    }

    fn add(&mut self, entry: &str, out: &mut HashSet<String>, depth: u32) -> Result<(), DataError> {
        let Some(tag) = entry.strip_prefix('#') else {
            out.insert(with_namespace(entry));
            return Ok(());
        };
        if depth > 32 {
            return Err(DataError::Format {
                file: tag.to_owned(),
                msg: "tag nesting too deep".into(),
            });
        }
        let tag = with_namespace(tag);
        let (ns, path) = tag.split_once(':').unwrap_or(("minecraft", &tag));
        let file = self.root.join(format!("data/{ns}/tags/block/{path}.json"));
        let json = crate::world::read_json(&file)?;
        for v in json["values"].as_array().into_iter().flatten() {
            // Запись — строка или `{"id": …, "required": false}`.
            let id = v.as_str().or_else(|| v["id"].as_str()).unwrap_or("");
            if !id.is_empty() {
                self.add(id, out, depth + 1)?;
            }
        }
        Ok(())
    }
}

fn with_namespace(id: &str) -> String {
    if id.contains(':') {
        id.to_owned()
    } else {
        format!("minecraft:{id}")
    }
}

/// Инструменты по ID предмета; сырые правила — через сводку `tools.tsv`,
/// теги разворачиваются при каждом запуске (их немного).
pub fn load_tools_cached(
    root: &Path,
    items: &HashMap<String, i32>,
) -> Result<Vec<Option<Tool>>, DataError> {
    let path = crate::world::derived(root, "tools.tsv");
    let raw: Vec<(String, RawTool)> = match std::fs::read_to_string(&path) {
        Ok(text) => text
            .lines()
            .filter_map(parse_tool_line)
            .map(|(n, t)| (n.to_owned(), t))
            .collect(),
        Err(_) => {
            let raw = load_raw_tools(root, items)?;
            let mut lines: Vec<String> = raw.iter().map(|(n, t)| tool_line(n, t)).collect();
            lines.sort();
            crate::world::write_derived(&path, &(lines.join("\n") + "\n"))?;
            raw
        }
    };
    let mut tags = Tags {
        root,
        memo: HashMap::new(),
    };
    let mut out = vec![None; items.values().max().map_or(0, |&m| m as usize + 1)];
    for (name, t) in raw {
        let Some(&id) = items.get(&name) else {
            continue;
        };
        let rules = t
            .rules
            .iter()
            .map(|r| {
                Ok(ToolRule {
                    blocks: tags.resolve(&r.blocks)?,
                    speed: r.speed,
                    correct_for_drops: r.correct,
                })
            })
            .collect::<Result<_, DataError>>()?;
        out[id as usize] = Some(Tool {
            default_speed: t.default_speed,
            damage_per_block: t.damage_per_block,
            rules,
        });
    }
    Ok(out)
}

fn load_raw_tools(
    root: &Path,
    items: &HashMap<String, i32>,
) -> Result<Vec<(String, RawTool)>, DataError> {
    let dir = root.join("reports/minecraft/components/item");
    let mut out = Vec::new();
    for name in items.keys() {
        let file = dir.join(format!("{}.json", name.trim_start_matches("minecraft:")));
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        if !text.contains("\"minecraft:tool\"") {
            continue;
        }
        let json: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| DataError::Format {
                file: file.display().to_string(),
                msg: e.to_string(),
            })?;
        if let Some(t) = parse_tool(&json["components"]) {
            out.push((name.clone(), t));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pickaxe_rules_like_vanilla() {
        let json = serde_json::json!({"minecraft:tool": {"rules": [
            {"blocks": "#minecraft:incorrect_for_iron_tool", "correct_for_drops": false},
            {"blocks": "#minecraft:mineable/pickaxe", "correct_for_drops": true, "speed": 6.0}
        ]}});
        let raw = parse_tool(&json).unwrap();
        assert_eq!(raw.default_speed, 1.0);
        let line = tool_line("minecraft:iron_pickaxe", &raw);
        assert_eq!(
            parse_tool_line(&line),
            Some(("minecraft:iron_pickaxe", raw))
        );

        let set = |names: &[&str]| Arc::new(names.iter().map(|s| s.to_string()).collect());
        let tool = Tool {
            default_speed: 1.0,
            damage_per_block: 1,
            rules: vec![
                ToolRule {
                    blocks: set(&["minecraft:obsidian"]),
                    speed: None,
                    correct_for_drops: Some(false),
                },
                ToolRule {
                    blocks: set(&["minecraft:obsidian", "minecraft:stone"]),
                    speed: Some(6.0),
                    correct_for_drops: Some(true),
                },
            ],
        };
        // Обсидиан: скорость кирки, но дроп железной кирке не положен.
        assert_eq!(tool.speed("minecraft:obsidian"), 6.0);
        assert!(!tool.correct_for_drops("minecraft:obsidian"));
        assert!(tool.correct_for_drops("minecraft:stone"));
        assert_eq!(tool.speed("minecraft:dirt"), 1.0);
        assert!(!tool.correct_for_drops("minecraft:dirt"));
    }

    #[test]
    fn list_blocks_and_empty_fields() {
        let json = serde_json::json!({"minecraft:tool": {"default_mining_speed": 2.0, "rules": [
            {"blocks": ["minecraft:cobweb", "vine"], "speed": 15.0}
        ]}});
        let raw = parse_tool(&json).unwrap();
        let line = tool_line("minecraft:x", &raw);
        assert_eq!(line, "minecraft:x\t2\t1\tminecraft:cobweb,vine|15|");
        assert_eq!(parse_tool_line(&line).unwrap().1, raw);
        let mut tags = Tags {
            root: Path::new("."),
            memo: HashMap::new(),
        };
        let set = tags.resolve(&raw.rules[0].blocks).unwrap();
        assert!(set.contains("minecraft:vine") && set.contains("minecraft:cobweb"));
    }
}
