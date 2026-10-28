//! Рецепты верстака (`crafting_shaped`, `crafting_shapeless`) из
//! `data/<ns>/recipe/*.json` и подбор рецепта по сетке.

use crate::DataError;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

/// Подходящие предметы (ID в реестре `item`).
pub type Ingredient = Arc<HashSet<i32>>;

#[derive(Clone, Debug)]
pub enum Recipe {
    Shaped {
        width: usize,
        height: usize,
        cells: Vec<Option<Ingredient>>,
        result: (i32, i32),
    },
    Shapeless {
        ingredients: Vec<Ingredient>,
        result: (i32, i32),
    },
}

/// Рецепт печи: `kind` — индекс в `world::FURNACES` (плавка, доменная, копчение).
#[derive(Clone, Debug)]
pub struct Cooking {
    pub kind: u8,
    pub ingredient: Ingredient,
    pub result: (i32, i32),
    pub time: i32,
    pub experience: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Recipes {
    list: Vec<Recipe>,
    cooking: Vec<Cooking>,
}

impl Recipes {
    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Рецепт печи `kind` для предмета (первый подходящий, как `RecipeManager`).
    pub fn cooking(&self, kind: u8, item: i32) -> Option<&Cooking> {
        self.cooking
            .iter()
            .find(|c| c.kind == kind && c.ingredient.contains(&item))
    }

    /// `(предмет, количество)` для квадратной сетки `size × size` (предметы по строкам).
    pub fn find(&self, grid: &[Option<i32>], size: usize) -> Option<(i32, i32)> {
        let filled: Vec<(usize, usize)> = (0..grid.len())
            .filter(|&i| grid[i].is_some())
            .map(|i| (i % size, i / size))
            .collect();
        if filled.is_empty() {
            return None;
        }
        let x0 = filled.iter().map(|p| p.0).min()?;
        let y0 = filled.iter().map(|p| p.1).min()?;
        let w = filled.iter().map(|p| p.0).max()? - x0 + 1;
        let h = filled.iter().map(|p| p.1).max()? - y0 + 1;
        let at = |x: usize, y: usize| grid[(y0 + y) * size + x0 + x];
        self.list.iter().find_map(|r| match r {
            Recipe::Shaped {
                width,
                height,
                cells,
                result,
            } if *width == w && *height == h => {
                let fits = |mirror: bool| {
                    (0..h).all(|y| {
                        (0..w).all(|x| {
                            let cell = &cells[y * w + if mirror { w - 1 - x } else { x }];
                            match (cell, at(x, y)) {
                                (None, None) => true,
                                (Some(ing), Some(item)) => ing.contains(&item),
                                _ => false,
                            }
                        })
                    })
                };
                (fits(false) || fits(true)).then_some(*result)
            }
            Recipe::Shapeless {
                ingredients,
                result,
            } if ingredients.len() == filled.len() => {
                let items: Vec<i32> = grid.iter().flatten().copied().collect();
                matches_all(ingredients, &items, &mut vec![false; items.len()]).then_some(*result)
            }
            _ => None,
        })
    }
}

/// Каждому ингредиенту — свой предмет (перебор с возвратом, ингредиентов ≤ 9).
fn matches_all(ings: &[Ingredient], items: &[i32], used: &mut [bool]) -> bool {
    let Some((first, rest)) = ings.split_first() else {
        return true;
    };
    for i in 0..items.len() {
        if !used[i] && first.contains(&items[i]) {
            used[i] = true;
            if matches_all(rest, items, used) {
                return true;
            }
            used[i] = false;
        }
    }
    false
}

/// Теги предметов `data/<ns>/tags/item/<path>.json` (с вложенными).
struct ItemTags<'a> {
    root: &'a Path,
    items: &'a HashMap<String, i32>,
    memo: HashMap<String, Ingredient>,
}

impl ItemTags<'_> {
    fn ingredient(&mut self, v: &serde_json::Value) -> Option<Ingredient> {
        let key = v.to_string();
        if let Some(i) = self.memo.get(&key) {
            return Some(i.clone());
        }
        let mut set = HashSet::new();
        match v {
            serde_json::Value::String(s) => self.add(s, &mut set, 0),
            serde_json::Value::Array(a) => {
                for e in a.iter().filter_map(|e| e.as_str()) {
                    self.add(e, &mut set, 0);
                }
            }
            _ => return None,
        }
        let ing = Arc::new(set);
        self.memo.insert(key, ing.clone());
        (!ing.is_empty()).then_some(ing)
    }

    fn add(&self, entry: &str, out: &mut HashSet<i32>, depth: u32) {
        let Some(tag) = entry.strip_prefix('#') else {
            if let Some(&id) = self.items.get(&with_namespace(entry)) {
                out.insert(id);
            }
            return;
        };
        if depth > 32 {
            return;
        }
        let tag = with_namespace(tag);
        let (ns, path) = tag.split_once(':').unwrap_or(("minecraft", &tag));
        let file = self.root.join(format!("data/{ns}/tags/item/{path}.json"));
        let Ok(json) = crate::world::read_json(&file) else {
            return;
        };
        for v in json["values"].as_array().into_iter().flatten() {
            let id = v.as_str().or_else(|| v["id"].as_str()).unwrap_or("");
            if !id.is_empty() {
                self.add(id, out, depth + 1);
            }
        }
    }
}

fn with_namespace(id: &str) -> String {
    if id.contains(':') {
        id.to_owned()
    } else {
        format!("minecraft:{id}")
    }
}

fn result(json: &serde_json::Value, items: &HashMap<String, i32>) -> Option<(i32, i32)> {
    let r = &json["result"];
    let id = *items.get(&with_namespace(r["id"].as_str()?))?;
    Some((id, r["count"].as_i64().unwrap_or(1) as i32))
}

fn parse(json: &serde_json::Value, tags: &mut ItemTags) -> Option<Recipe> {
    match json["type"].as_str()? {
        "minecraft:crafting_shaped" => {
            let pattern: Vec<&str> = json["pattern"]
                .as_array()?
                .iter()
                .filter_map(|l| l.as_str())
                .collect();
            let height = pattern.len();
            let width = pattern.iter().map(|l| l.chars().count()).max()?;
            let mut cells = Vec::with_capacity(width * height);
            for line in &pattern {
                let chars: Vec<char> = line.chars().collect();
                for x in 0..width {
                    cells.push(match chars.get(x) {
                        None | Some(' ') => None,
                        Some(c) => Some(tags.ingredient(&json["key"][c.to_string()])?),
                    });
                }
            }
            Some(Recipe::Shaped {
                width,
                height,
                cells,
                result: result(json, tags.items)?,
            })
        }
        "minecraft:crafting_shapeless" => Some(Recipe::Shapeless {
            ingredients: json["ingredients"]
                .as_array()?
                .iter()
                .map(|v| tags.ingredient(v))
                .collect::<Option<_>>()?,
            result: result(json, tags.items)?,
        }),
        _ => None,
    }
}

/// Все рецепты верстака из `data/minecraft/recipe`; остальные типы пропускаются.
pub fn load_recipes(root: &Path, items: &HashMap<String, i32>) -> Result<Recipes, DataError> {
    let dir = root.join("data/minecraft/recipe");
    let entries = std::fs::read_dir(&dir)
        .map_err(|e| DataError::Io(format!("read {}: {e}", dir.display())))?;
    let mut paths: Vec<_> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    paths.sort();
    let mut tags = ItemTags {
        root,
        items,
        memo: HashMap::new(),
    };
    let mut list = Vec::new();
    let mut cooking_list = Vec::new();
    for path in paths {
        let text = std::fs::read_to_string(&path)
            .map_err(|e| DataError::Io(format!("read {}: {e}", path.display())))?;
        let crafting = text.contains("\"minecraft:crafting_sha");
        let cooking = ["smelting", "blasting", "smoking"]
            .iter()
            .position(|t| text.contains(&format!("\"minecraft:{t}\"")));
        if !crafting && cooking.is_none() {
            continue;
        }
        let json: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| DataError::Format {
                file: path.display().to_string(),
                msg: e.to_string(),
            })?;
        if crafting {
            list.extend(parse(&json, &mut tags));
        } else if let Some(kind) = cooking
            && json["type"]
                .as_str()
                .is_some_and(|t| t.ends_with(["smelting", "blasting", "smoking"][kind]))
            && let (Some(ingredient), Some(result)) =
                (tags.ingredient(&json["ingredient"]), result(&json, items))
        {
            cooking_list.push(Cooking {
                kind: kind as u8,
                ingredient,
                result,
                time: json["cookingtime"].as_i64().unwrap_or(200) as i32,
                experience: json["experience"].as_f64().unwrap_or(0.0) as f32,
            });
        }
    }
    Ok(Recipes {
        list,
        cooking: cooking_list,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shaped_shapeless_and_mirror() {
        let items: HashMap<String, i32> = [
            ("minecraft:planks", 1),
            ("minecraft:stick", 2),
            ("minecraft:log", 3),
            ("minecraft:axe", 4),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect();
        let mut tags = ItemTags {
            root: Path::new("."),
            items: &items,
            memo: HashMap::new(),
        };
        let stick = serde_json::json!({"type": "minecraft:crafting_shaped", "key": {"#": "minecraft:planks"},
            "pattern": ["#", "#"], "result": {"id": "minecraft:stick", "count": 4}});
        let axe = serde_json::json!({"type": "minecraft:crafting_shaped",
            "key": {"#": "minecraft:planks", "|": "minecraft:stick"},
            "pattern": ["##", "#|", " |"], "result": {"id": "minecraft:axe"}});
        let planks = serde_json::json!({"type": "minecraft:crafting_shapeless", "ingredients": ["minecraft:log"],
            "result": {"id": "minecraft:planks", "count": 4}});
        let r = Recipes {
            list: [stick, axe, planks]
                .iter()
                .filter_map(|j| parse(j, &mut tags))
                .collect(),
            cooking: Vec::new(),
        };
        assert_eq!(r.len(), 3);
        // Палки в любом месте сетки 2×2.
        assert_eq!(r.find(&[None, Some(1), None, Some(1)], 2), Some((2, 4)));
        assert_eq!(r.find(&[None, None, None, Some(3)], 2), Some((1, 4)));
        // Топор и его зеркало на верстаке.
        let p = Some(1);
        let s = Some(2);
        assert_eq!(
            r.find(&[p, p, None, p, s, None, None, s, None], 3),
            Some((4, 1))
        );
        assert_eq!(
            r.find(&[p, p, None, s, p, None, s, None, None], 3),
            Some((4, 1))
        );
        assert_eq!(
            r.find(&[p, s, None, None, None, None, None, None, None], 3),
            None
        );
    }
}
