//! Книга рецептов (`ServerRecipeBook`): загрузка рецептов, отображения
//! (`RecipeDisplay`), пакеты `recipe_book_add`/`remove`/`settings` и
//! раскладка ингредиентов по сетке (`ServerPlaceRecipe`).
//!
//! Рецепты читаются из `data/minecraft/recipe/**/*.json`; каждому рецепту
//! (у которого есть отображение) выдаётся `RecipeDisplayId` в порядке
//! сортировки id — так же, как `RecipeManager.unpackRecipeInfo` нумерует
//! `allDisplays`. Знания игрока хранятся в [`BookState`] и в `playerdata`
//! (`recipeBook.recipes`/`toBeDisplayed`/настройки).

use super::*;
use std::collections::HashSet;
use std::path::Path;

/// `SlotDisplay`: то, как клиент рисует предмет/тег/преобразование.
#[derive(Clone, Debug)]
pub enum SlotDisp {
    Empty,
    AnyFuel,
    /// `TagSlotDisplay` с прямым набором предметов.
    Items(Vec<i32>),
    Stack {
        item: i32,
        count: i32,
    },
    AnyPotion(Box<SlotDisp>),
    OnlyWithComponent {
        source: Box<SlotDisp>,
        component: i32,
    },
    Dyed {
        dye: Box<SlotDisp>,
        target: Box<SlotDisp>,
    },
    SmithingTrim {
        base: Box<SlotDisp>,
        material: Box<SlotDisp>,
        pattern: i32,
    },
    Composite(Vec<SlotDisp>),
}

impl SlotDisp {
    fn items(items: &[i32]) -> Self {
        if items.is_empty() {
            SlotDisp::Empty
        } else {
            SlotDisp::Items(items.to_vec())
        }
    }

    fn stack(item: i32, count: i32) -> Self {
        SlotDisp::Stack { item, count }
    }
}

/// Отображение рецепта (`RecipeDisplay`): тип и данные для пакета.
#[derive(Clone, Debug)]
pub enum Display {
    Shapeless {
        ingredients: Vec<SlotDisp>,
        result: SlotDisp,
        station: SlotDisp,
    },
    Shaped {
        width: usize,
        height: usize,
        ingredients: Vec<SlotDisp>,
        result: SlotDisp,
        station: SlotDisp,
    },
    Furnace {
        ingredient: SlotDisp,
        result: SlotDisp,
        station: SlotDisp,
        duration: i32,
        experience: f32,
    },
    Stonecutter {
        input: SlotDisp,
        result: SlotDisp,
        station: SlotDisp,
    },
    Smithing {
        template: SlotDisp,
        base: SlotDisp,
        addition: SlotDisp,
        result: SlotDisp,
        station: SlotDisp,
    },
}

/// Как раскладывать рецепт по сетке.
#[derive(Clone, Debug)]
pub enum Placement {
    Crafting {
        width: usize,
        height: usize,
        cells: Vec<Vec<i32>>,
        /// `true` — shapeless: клетки кладутся подряд.
        shapeless: bool,
    },
    Furnace {
        input: Vec<i32>,
    },
    Stonecutter {
        input: Vec<i32>,
        index: usize,
    },
    Smithing {
        template: Vec<i32>,
        base: Vec<i32>,
        addition: Vec<i32>,
    },
    None,
}

/// Особый крафт, который не выражается shaped/shapeless-сеткой.
#[derive(Clone, Debug)]
pub enum SpecialCraft {
    Transmute {
        input: Vec<i32>,
        material: Vec<i32>,
        result: Option<i32>,
        count: i32,
        add_material: bool,
        min: i32,
        max: i32,
    },
    Dye {
        target: Vec<i32>,
        dye: Vec<i32>,
        result: i32,
        count: i32,
    },
    Imbue {
        material: Vec<i32>,
        source: Vec<i32>,
        result: i32,
        count: i32,
    },
}

/// Результат особого крафта: предмет, количество и слот-источник
/// компонентов (для dye/transmute с сохранением входного стека).
#[derive(Clone, Copy, Debug)]
pub struct SpecialResult {
    pub item: i32,
    pub count: i32,
    pub copy_from: Option<usize>,
    /// Слот красителя (для `dyed_color`).
    pub dye_slot: Option<usize>,
    /// Слот зелья-источника (для `potion_contents`).
    pub potion_slot: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct RecipeEntry {
    pub id: String,
    pub display_id: i32,
    pub category: i32,
    pub group: Option<i32>,
    pub display: Display,
    /// `craftingRequirements`: по одному ингредиенту на позицию.
    pub requirements: Vec<Vec<i32>>,
    pub result: (i32, i32),
    pub placement: Placement,
    /// `showNotification` (флаг 1 в `recipe_book_add`).
    pub notification: bool,
    pub special: Option<SpecialCraft>,
}

#[derive(Clone, Debug)]
pub struct RecipeBookData {
    pub entries: Vec<RecipeEntry>,
    /// id рецепта → все его отображения (у transmute их несколько).
    pub by_id: FastMap<String, Vec<usize>>,
}

impl RecipeBookData {
    fn key(id: &str) -> String {
        if id.starts_with("minecraft:") {
            id.to_owned()
        } else {
            format!("minecraft:{id}")
        }
    }

    pub fn entry(&self, id: &str) -> Option<&RecipeEntry> {
        self.entries_for(id).first().copied()
    }

    /// Все отображения рецепта (обычно одно).
    pub fn entries_for(&self, id: &str) -> Vec<&RecipeEntry> {
        self.by_id
            .get(&Self::key(id))
            .map(|list| list.iter().filter_map(|&i| self.entries.get(i)).collect())
            .unwrap_or_default()
    }

    pub fn entry_by_display(&self, display_id: i32) -> Option<&RecipeEntry> {
        self.entries.iter().find(|e| e.display_id == display_id)
    }

    /// Подобрать особый рецепт (transmute/dye/imbue) под сетку.
    pub fn special_craft(&self, grid: &[Option<i32>], size: usize) -> Option<SpecialResult> {
        let filled: Vec<(usize, i32)> = grid
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.map(|item| (i, item)))
            .collect();
        for e in &self.entries {
            let Some(special) = &e.special else {
                continue;
            };
            match special {
                SpecialCraft::Transmute {
                    input,
                    material,
                    result,
                    count,
                    add_material,
                    min,
                    max,
                } => {
                    if filled.len() < 2 {
                        continue;
                    }
                    let n = filled.len() as i32 - 1;
                    if n < *min || n > *max {
                        continue;
                    }
                    let mut found = None;
                    for (slot, item) in &filled {
                        if !input.contains(item) {
                            continue;
                        }
                        if filled
                            .iter()
                            .filter(|(s, _)| s != slot)
                            .all(|(_, i)| material.contains(i))
                        {
                            found = Some(*slot);
                            break;
                        }
                    }
                    let Some(slot) = found else {
                        continue;
                    };
                    let base = *count;
                    let total = if *add_material { n + base } else { base };
                    let (item, copy_from) = match result {
                        Some(item) => (*item, None),
                        None => (filled.iter().find(|(s, _)| *s == slot)?.1, Some(slot)),
                    };
                    return Some(SpecialResult {
                        item,
                        count: total,
                        copy_from,
                        dye_slot: None,
                        potion_slot: None,
                    });
                }
                SpecialCraft::Dye {
                    target,
                    dye,
                    result,
                    count,
                } => {
                    if filled.len() != 2 {
                        continue;
                    }
                    let ((a_slot, a), (b_slot, b)) = (filled[0], filled[1]);
                    let (slot, dye_slot, ok) = if target.contains(&a) && dye.contains(&b) {
                        (a_slot, b_slot, true)
                    } else if target.contains(&b) && dye.contains(&a) {
                        (b_slot, a_slot, true)
                    } else {
                        (0, 0, false)
                    };
                    if !ok {
                        continue;
                    }
                    return Some(SpecialResult {
                        item: *result,
                        count: *count,
                        copy_from: Some(slot),
                        dye_slot: Some(dye_slot),
                        potion_slot: None,
                    });
                }
                SpecialCraft::Imbue {
                    material,
                    source,
                    result,
                    count,
                } => {
                    if size != 3 || filled.len() != 9 {
                        continue;
                    }
                    let center = grid.get(4).copied().flatten();
                    if !center.is_some_and(|i| source.contains(&i)) {
                        continue;
                    }
                    if grid
                        .iter()
                        .enumerate()
                        .all(|(i, s)| i == 4 || s.is_some_and(|item| material.contains(&item)))
                    {
                        return Some(SpecialResult {
                            item: *result,
                            count: *count,
                            copy_from: None,
                            dye_slot: None,
                            potion_slot: Some(4),
                        });
                    }
                }
            }
        }
        None
    }

    /// `*`: все «обучаемые» рецепты (у нас — все с отображением).
    pub fn learnable(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for e in &self.entries {
            if out.last() != Some(&e.id) {
                out.push(e.id.clone());
            }
        }
        out
    }
}

/// Знания игрока (`ServerRecipeBook.known`/`highlight` + настройки).
#[derive(Clone, Debug, Default)]
pub struct BookState {
    pub known: HashSet<String>,
    pub highlight: HashSet<String>,
    /// `RecipeBookSettings`: crafting, furnace, blast furnace, smoker →
    /// `(open, filtering)`.
    pub settings: [[bool; 2]; 4],
}

fn item_of(name: &str, items: &HashMap<String, i32>) -> Option<i32> {
    items.get(name).copied()
}

fn expand(
    name: &str,
    items: &HashMap<String, i32>,
    tags: &HashMap<String, HashSet<String>>,
) -> Vec<i32> {
    if let Some(tag) = name.strip_prefix('#') {
        let mut out: Vec<i32> = tags
            .get(tag)
            .into_iter()
            .flatten()
            .filter_map(|n| item_of(n, items))
            .collect();
        out.sort_unstable();
        out.dedup();
        return out;
    }
    item_of(name, items).into_iter().collect()
}

/// Один ингредиент: строка, `#тег`, список или объект `{item|tag}`.
fn ingredient(
    v: &serde_json::Value,
    items: &HashMap<String, i32>,
    tags: &HashMap<String, HashSet<String>>,
) -> Vec<i32> {
    match v {
        serde_json::Value::String(s) => expand(s, items, tags),
        serde_json::Value::Array(a) => {
            let mut out = Vec::new();
            for e in a {
                out.extend(ingredient(e, items, tags));
            }
            out.sort_unstable();
            out.dedup();
            out
        }
        serde_json::Value::Object(o) => {
            if let Some(s) = o.get("item").and_then(|s| s.as_str()) {
                return expand(s, items, tags);
            }
            if let Some(s) = o.get("tag").and_then(|s| s.as_str()) {
                return expand(&format!("#{s}"), items, tags);
            }
            if let Some(a) = o.get("items").and_then(|a| a.as_array()) {
                let mut out = Vec::new();
                for e in a {
                    out.extend(ingredient(e, items, tags));
                }
                out.sort_unstable();
                out.dedup();
                out
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    }
}

fn result_of(v: &serde_json::Value, items: &HashMap<String, i32>) -> Option<(i32, i32)> {
    let id = v.get("id").and_then(|s| s.as_str())?;
    let item = item_of(id, items)?;
    let count = v.get("count").and_then(|c| c.as_i64()).unwrap_or(1) as i32;
    Some((item, count.max(1)))
}

fn crafting_category(name: &str, categories: &HashMap<String, i32>) -> i32 {
    let key = match name {
        "building" => "minecraft:crafting_building_blocks",
        "equipment" => "minecraft:crafting_equipment",
        "redstone" => "minecraft:crafting_redstone",
        _ => "minecraft:crafting_misc",
    };
    categories.get(key).copied().unwrap_or(0)
}

fn cooking_category(type_name: &str, category: &str, categories: &HashMap<String, i32>) -> i32 {
    let key = match (type_name, category) {
        ("smelting", "blocks") => "minecraft:furnace_blocks",
        ("smelting", "food") => "minecraft:furnace_food",
        ("smelting", _) => "minecraft:furnace_misc",
        ("blasting", "blocks") => "minecraft:blast_furnace_blocks",
        ("blasting", _) => "minecraft:blast_furnace_misc",
        ("smoking", _) => "minecraft:smoker_food",
        _ => "minecraft:campfire",
    };
    categories.get(key).copied().unwrap_or(0)
}

fn station_id(type_name: &str, items: &HashMap<String, i32>) -> i32 {
    let name = match type_name {
        "smelting" => "minecraft:furnace",
        "blasting" => "minecraft:blast_furnace",
        "smoking" => "minecraft:smoker",
        "campfire_cooking" => "minecraft:campfire",
        "stonecutting" => "minecraft:stonecutter",
        "smithing_transform" => "minecraft:smithing_table",
        _ => "minecraft:crafting_table",
    };
    item_of(name, items).unwrap_or(0)
}

fn collect(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|e| e == "json") {
            out.push(path);
        }
    }
}

/// Прочитать все рецепты из `data/minecraft/recipe` (кроме brewing).
pub fn load(
    data_dir: &Path,
    items: &HashMap<String, i32>,
    tags: &HashMap<String, HashSet<String>>,
    categories: &HashMap<String, i32>,
    trim_patterns: &HashMap<String, i32>,
    components: &HashMap<String, i32>,
) -> RecipeBookData {
    let root = data_dir.join("data/minecraft/recipe");
    let mut paths = Vec::new();
    collect(&root, &mut paths);
    paths.retain(|p| {
        !p.strip_prefix(&root).is_ok_and(|r| {
            r.components()
                .next()
                .is_some_and(|c| c.as_os_str() == "brewing")
        })
    });
    paths.sort();
    let mut entries: Vec<RecipeEntry> = Vec::new();
    let mut groups: HashMap<String, i32> = HashMap::new();
    let mut stone_index = 0usize;
    let dye_component = components.get("minecraft:dye").copied().unwrap_or(0);
    for path in paths {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .with_extension("")
            .to_string_lossy()
            .replace('\\', "/");
        let id = format!("minecraft:{rel}");
        let type_name = json
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .trim_start_matches("minecraft:");
        let group = json
            .get("group")
            .and_then(|g| g.as_str())
            .filter(|g| !g.is_empty());
        let group_id = group.map(|g| {
            let next = groups.len() as i32;
            *groups.entry(g.to_owned()).or_insert(next)
        });
        let notification = json
            .get("show_notification")
            .and_then(|b| b.as_bool())
            .unwrap_or(true);
        let cat = |name: &str| crafting_category(name, categories);
        let item = |name: &str| item_of(name, items);
        let station = |name: &str| {
            let id = station_id(name, items);
            if id == 0 {
                SlotDisp::Empty
            } else {
                SlotDisp::Stack { item: id, count: 1 }
            }
        };
        let (displays, placement, requirements, result) = match type_name {
            "crafting_shaped" => {
                let key = json.get("key").and_then(|k| k.as_object());
                let pattern: Vec<&str> = json
                    .get("pattern")
                    .and_then(|p| p.as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|l| l.as_str())
                    .collect();
                let height = pattern.len();
                let width = pattern.iter().map(|l| l.chars().count()).max().unwrap_or(0);
                let Some(result) = json.get("result").and_then(|r| result_of(r, items)) else {
                    continue;
                };
                let mut cells = Vec::with_capacity(width * height);
                let mut reqs = Vec::new();
                for row in &pattern {
                    for x in 0..width {
                        let ch = row.chars().nth(x).unwrap_or(' ');
                        let ing = key
                            .and_then(|k| k.get(&ch.to_string()))
                            .map(|v| ingredient(v, items, tags))
                            .unwrap_or_default();
                        if !ing.is_empty() {
                            reqs.push(ing.clone());
                        }
                        cells.push(ing);
                    }
                }
                if width == 0 || height == 0 {
                    continue;
                }
                let display = Display::Shaped {
                    width,
                    height,
                    ingredients: cells.iter().map(|c| SlotDisp::items(c)).collect(),
                    result: SlotDisp::stack(result.0, result.1),
                    station: station("crafting"),
                };
                (
                    vec![display],
                    Placement::Crafting {
                        width,
                        height,
                        cells,
                        shapeless: false,
                    },
                    reqs,
                    result,
                )
            }
            "crafting_shapeless" => {
                let ings: Vec<Vec<i32>> = json
                    .get("ingredients")
                    .and_then(|i| i.as_array())
                    .into_iter()
                    .flatten()
                    .map(|v| ingredient(v, items, tags))
                    .filter(|v| !v.is_empty())
                    .collect();
                let Some(result) = json.get("result").and_then(|r| result_of(r, items)) else {
                    continue;
                };
                if ings.is_empty() {
                    continue;
                }
                let display = Display::Shapeless {
                    ingredients: ings.iter().map(|c| SlotDisp::items(c)).collect(),
                    result: SlotDisp::stack(result.0, result.1),
                    station: station("crafting"),
                };
                (
                    vec![display],
                    Placement::Crafting {
                        width: 1,
                        height: ings.len(),
                        cells: ings.clone(),
                        shapeless: true,
                    },
                    ings,
                    result,
                )
            }
            "crafting_transmute" => {
                let input = json
                    .get("input")
                    .map(|v| ingredient(v, items, tags))
                    .unwrap_or_default();
                let material = json
                    .get("material")
                    .map(|v| ingredient(v, items, tags))
                    .unwrap_or_default();
                if input.is_empty() || material.is_empty() {
                    continue;
                }
                let res = json.get("result");
                let base_item = res
                    .and_then(|r| r.get("id"))
                    .and_then(|s| s.as_str())
                    .and_then(item);
                let base_count = res
                    .and_then(|r| r.get("count"))
                    .and_then(|c| c.as_i64())
                    .unwrap_or(1) as i32;
                let add_count = json
                    .get("add_material_count_to_result")
                    .and_then(|b| b.as_bool())
                    .unwrap_or(false);
                let count = json.get("material_count");
                let (min, max) = match count {
                    Some(serde_json::Value::Number(n)) => {
                        let n = n.as_i64().unwrap_or(1) as i32;
                        (n, n)
                    }
                    Some(serde_json::Value::Object(o)) => (
                        o.get("min").and_then(|v| v.as_i64()).unwrap_or(1) as i32,
                        o.get("max").and_then(|v| v.as_i64()).unwrap_or(8) as i32,
                    ),
                    _ => (1, 1),
                };
                let mut displays = Vec::new();
                let mut reqs = Vec::new();
                let mut all_cells = Vec::new();
                for n in min..=max.max(min) {
                    let result_count = if add_count {
                        n + base_count
                    } else {
                        base_count
                    };
                    let result = match base_item {
                        Some(item) => SlotDisp::stack(item, result_count),
                        None => SlotDisp::Composite(
                            input
                                .iter()
                                .map(|&i| SlotDisp::stack(i, result_count))
                                .collect(),
                        ),
                    };
                    let mut ingredients = vec![SlotDisp::items(&input)];
                    ingredients.extend((0..n).map(|_| SlotDisp::items(&material)));
                    displays.push(Display::Shapeless {
                        ingredients,
                        result,
                        station: station("crafting"),
                    });
                    let mut cells = vec![input.clone()];
                    cells.extend((0..n).map(|_| material.clone()));
                    all_cells = cells;
                    let mut r = vec![input.clone()];
                    r.extend((0..n).map(|_| material.clone()));
                    reqs = r;
                }
                let result = base_item
                    .map(|i| (i, base_count))
                    .or_else(|| input.first().map(|&i| (i, base_count)))
                    .unwrap_or((0, 0));
                (
                    displays,
                    Placement::Crafting {
                        width: 1,
                        height: all_cells.len(),
                        cells: all_cells,
                        shapeless: true,
                    },
                    reqs,
                    result,
                )
            }
            "crafting_dye" => {
                let dye = json
                    .get("dye")
                    .map(|v| ingredient(v, items, tags))
                    .unwrap_or_default();
                let target = json
                    .get("target")
                    .map(|v| ingredient(v, items, tags))
                    .unwrap_or_default();
                let Some(result) = json.get("result").and_then(|r| result_of(r, items)) else {
                    continue;
                };
                if dye.is_empty() || target.is_empty() {
                    continue;
                }
                let dye_display = SlotDisp::OnlyWithComponent {
                    source: Box::new(SlotDisp::items(&dye)),
                    component: dye_component,
                };
                let target_display = SlotDisp::items(&target);
                let display = Display::Shapeless {
                    ingredients: vec![target_display.clone(), dye_display.clone()],
                    result: SlotDisp::Dyed {
                        dye: Box::new(dye_display),
                        target: Box::new(target_display),
                    },
                    station: station("crafting"),
                };
                (
                    vec![display],
                    Placement::Crafting {
                        width: 1,
                        height: 2,
                        cells: vec![target.clone(), dye.clone()],
                        shapeless: true,
                    },
                    vec![target, dye],
                    result,
                )
            }
            "crafting_imbue" => {
                let material = json
                    .get("material")
                    .map(|v| ingredient(v, items, tags))
                    .unwrap_or_default();
                let source = json
                    .get("source")
                    .map(|v| ingredient(v, items, tags))
                    .unwrap_or_default();
                let Some(result) = json.get("result").and_then(|r| result_of(r, items)) else {
                    continue;
                };
                if material.is_empty() || source.is_empty() {
                    continue;
                }
                let mat = SlotDisp::items(&material);
                let src = SlotDisp::AnyPotion(Box::new(SlotDisp::items(&source)));
                let mut ingredients = vec![mat.clone(); 9];
                ingredients[4] = src;
                let display = Display::Shaped {
                    width: 3,
                    height: 3,
                    ingredients,
                    result: SlotDisp::AnyPotion(Box::new(SlotDisp::stack(result.0, result.1))),
                    station: station("crafting"),
                };
                let mut cells = vec![material.clone(); 9];
                cells[4] = source.clone();
                (
                    vec![display],
                    Placement::Crafting {
                        width: 3,
                        height: 3,
                        cells,
                        shapeless: false,
                    },
                    {
                        let mut r = vec![material.clone(); 8];
                        r.push(source);
                        r
                    },
                    result,
                )
            }
            "smelting" | "blasting" | "smoking" | "campfire_cooking" => {
                let input = json
                    .get("ingredient")
                    .map(|v| ingredient(v, items, tags))
                    .unwrap_or_default();
                let Some(result) = json.get("result").and_then(|r| result_of(r, items)) else {
                    continue;
                };
                if input.is_empty() {
                    continue;
                }
                let duration = json.get("cookingtime").and_then(|c| c.as_i64()).unwrap_or(
                    if type_name == "campfire_cooking" {
                        600
                    } else {
                        200
                    },
                ) as i32;
                let experience = json
                    .get("experience")
                    .and_then(|e| e.as_f64())
                    .unwrap_or(0.0) as f32;
                let display = Display::Furnace {
                    ingredient: SlotDisp::items(&input),
                    result: SlotDisp::stack(result.0, result.1),
                    station: station(type_name),
                    duration,
                    experience,
                };
                (
                    vec![display],
                    Placement::Furnace {
                        input: input.clone(),
                    },
                    vec![input],
                    result,
                )
            }
            "stonecutting" => {
                let input = json
                    .get("ingredient")
                    .map(|v| ingredient(v, items, tags))
                    .unwrap_or_default();
                let Some(result) = json.get("result").and_then(|r| result_of(r, items)) else {
                    continue;
                };
                if input.is_empty() {
                    continue;
                }
                let index = stone_index;
                stone_index += 1;
                let display = Display::Stonecutter {
                    input: SlotDisp::items(&input),
                    result: SlotDisp::stack(result.0, result.1),
                    station: station("stonecutting"),
                };
                (
                    vec![display],
                    Placement::Stonecutter {
                        input: input.clone(),
                        index,
                    },
                    vec![input],
                    result,
                )
            }
            "smithing_transform" => {
                let template = json
                    .get("template")
                    .map(|v| ingredient(v, items, tags))
                    .unwrap_or_default();
                let base = json
                    .get("base")
                    .map(|v| ingredient(v, items, tags))
                    .unwrap_or_default();
                let addition = json
                    .get("addition")
                    .map(|v| ingredient(v, items, tags))
                    .unwrap_or_default();
                let Some(result) = json.get("result").and_then(|r| result_of(r, items)) else {
                    continue;
                };
                let display = Display::Smithing {
                    template: SlotDisp::items(&template),
                    base: SlotDisp::items(&base),
                    addition: SlotDisp::items(&addition),
                    result: SlotDisp::stack(result.0, result.1),
                    station: station("smithing_transform"),
                };
                (
                    vec![display],
                    Placement::Smithing {
                        template: template.clone(),
                        base: base.clone(),
                        addition: addition.clone(),
                    },
                    vec![template, base, addition],
                    result,
                )
            }
            "smithing_trim" => {
                let template = json
                    .get("template")
                    .map(|v| ingredient(v, items, tags))
                    .unwrap_or_default();
                let base = json
                    .get("base")
                    .map(|v| ingredient(v, items, tags))
                    .unwrap_or_default();
                let addition = json
                    .get("addition")
                    .map(|v| ingredient(v, items, tags))
                    .unwrap_or_default();
                let Some(pattern) = json
                    .get("pattern")
                    .and_then(|p| p.as_str())
                    .and_then(|p| trim_patterns.get(p).copied())
                else {
                    continue;
                };
                if template.is_empty() || base.is_empty() || addition.is_empty() {
                    continue;
                }
                let base_display = SlotDisp::items(&base);
                let addition_display = SlotDisp::items(&addition);
                let display = Display::Smithing {
                    template: SlotDisp::items(&template),
                    base: base_display.clone(),
                    addition: addition_display.clone(),
                    result: SlotDisp::SmithingTrim {
                        base: Box::new(base_display),
                        material: Box::new(addition_display),
                        pattern,
                    },
                    station: station("smithing_transform"),
                };
                (
                    vec![display],
                    Placement::Smithing {
                        template: template.clone(),
                        base: base.clone(),
                        addition: addition.clone(),
                    },
                    vec![template, base, addition],
                    (0, 0),
                )
            }
            _ => continue,
        };
        let category = match type_name {
            "smelting" | "blasting" | "smoking" | "campfire_cooking" => cooking_category(
                type_name,
                json.get("category")
                    .and_then(|c| c.as_str())
                    .unwrap_or("misc"),
                categories,
            ),
            "stonecutting" => categories
                .get("minecraft:stonecutter")
                .copied()
                .unwrap_or(0),
            "smithing_transform" | "smithing_trim" => {
                categories.get("minecraft:smithing").copied().unwrap_or(0)
            }
            _ => cat(json
                .get("category")
                .and_then(|c| c.as_str())
                .unwrap_or("misc")),
        };
        let special = special_of(type_name, &json, items, tags);
        for (di, display) in displays.into_iter().enumerate() {
            let display_id = entries.len() as i32;
            entries.push(RecipeEntry {
                id: id.clone(),
                display_id,
                category,
                group: group_id,
                display,
                requirements: requirements.clone(),
                result,
                placement: placement.clone(),
                notification,
                special: if di == 0 { special.clone() } else { None },
            });
        }
    }
    let mut by_id: FastMap<String, Vec<usize>> = FastMap::default();
    for (i, e) in entries.iter().enumerate() {
        by_id.entry(e.id.clone()).or_default().push(i);
    }
    RecipeBookData { entries, by_id }
}

/// Особый крафт из JSON (transmute/dye/imbue).
fn special_of(
    type_name: &str,
    json: &serde_json::Value,
    items: &HashMap<String, i32>,
    tags: &HashMap<String, HashSet<String>>,
) -> Option<SpecialCraft> {
    match type_name {
        "crafting_transmute" => {
            let input = json.get("input").map(|v| ingredient(v, items, tags))?;
            let material = json.get("material").map(|v| ingredient(v, items, tags))?;
            if input.is_empty() || material.is_empty() {
                return None;
            }
            let res = json.get("result");
            let result = res
                .and_then(|r| r.get("id"))
                .and_then(|s| s.as_str())
                .and_then(|s| items.get(s).copied());
            let count = res
                .and_then(|r| r.get("count"))
                .and_then(|c| c.as_i64())
                .unwrap_or(1) as i32;
            let add_material = json
                .get("add_material_count_to_result")
                .and_then(|b| b.as_bool())
                .unwrap_or(false);
            let (min, max) = match json.get("material_count") {
                Some(serde_json::Value::Number(n)) => {
                    let n = n.as_i64().unwrap_or(1) as i32;
                    (n, n)
                }
                Some(serde_json::Value::Object(o)) => (
                    o.get("min").and_then(|v| v.as_i64()).unwrap_or(1) as i32,
                    o.get("max").and_then(|v| v.as_i64()).unwrap_or(8) as i32,
                ),
                _ => (1, 1),
            };
            Some(SpecialCraft::Transmute {
                input,
                material,
                result,
                count,
                add_material,
                min,
                max,
            })
        }
        "crafting_dye" => {
            let target = json.get("target").map(|v| ingredient(v, items, tags))?;
            let dye = json.get("dye").map(|v| ingredient(v, items, tags))?;
            let result = json.get("result").and_then(|r| result_of(r, items))?.0;
            Some(SpecialCraft::Dye {
                target,
                dye,
                result,
                count: 1,
            })
        }
        "crafting_imbue" => {
            let material = json.get("material").map(|v| ingredient(v, items, tags))?;
            let source = json.get("source").map(|v| ingredient(v, items, tags))?;
            let result = json.get("result").and_then(|r| result_of(r, items))?;
            Some(SpecialCraft::Imbue {
                material,
                source,
                result: result.0,
                count: result.1,
            })
        }
        _ => None,
    }
}

/// `SlotDisplay.STREAM_CODEC`.
fn write_slot(w: &mut Writer, slot: &SlotDisp) {
    match slot {
        SlotDisp::Empty => {
            w.var_int(0);
        }
        SlotDisp::AnyFuel => {
            w.var_int(1);
        }
        SlotDisp::Items(items) => {
            if items.is_empty() {
                w.var_int(0);
            } else {
                w.var_int(6).var_int(items.len() as i32 + 1);
                for &item in items {
                    w.var_int(item);
                }
            }
        }
        SlotDisp::Stack { item, count } => {
            w.var_int(5)
                .var_int(*item)
                .var_int(*count)
                .var_int(0)
                .var_int(0);
        }
        SlotDisp::AnyPotion(inner) => {
            w.var_int(2);
            write_slot(w, inner);
        }
        SlotDisp::OnlyWithComponent { source, component } => {
            w.var_int(3);
            write_slot(w, source);
            w.var_int(*component);
        }
        SlotDisp::Dyed { dye, target } => {
            w.var_int(7);
            write_slot(w, dye);
            write_slot(w, target);
        }
        SlotDisp::SmithingTrim {
            base,
            material,
            pattern,
        } => {
            w.var_int(8);
            write_slot(w, base);
            write_slot(w, material);
            w.var_int(*pattern);
        }
        SlotDisp::Composite(contents) => {
            w.var_int(10).var_int(contents.len() as i32);
            for c in contents {
                write_slot(w, c);
            }
        }
    }
}

fn write_display(w: &mut Writer, display: &Display) {
    match display {
        Display::Shapeless {
            ingredients,
            result,
            station,
        } => {
            w.var_int(0).var_int(ingredients.len() as i32);
            for ing in ingredients {
                write_slot(w, ing);
            }
            write_slot(w, result);
            write_slot(w, station);
        }
        Display::Shaped {
            width,
            height,
            ingredients,
            result,
            station,
        } => {
            w.var_int(1)
                .var_int(*width as i32)
                .var_int(*height as i32)
                .var_int(ingredients.len() as i32);
            for ing in ingredients {
                write_slot(w, ing);
            }
            write_slot(w, result);
            write_slot(w, station);
        }
        Display::Furnace {
            ingredient,
            result,
            station,
            duration,
            experience,
        } => {
            w.var_int(2);
            write_slot(w, ingredient);
            w.var_int(1); // `AnyFuel`
            write_slot(w, result);
            write_slot(w, station);
            w.var_int(*duration).f32(*experience);
        }
        Display::Stonecutter {
            input,
            result,
            station,
        } => {
            w.var_int(3);
            write_slot(w, input);
            write_slot(w, result);
            write_slot(w, station);
        }
        Display::Smithing {
            template,
            base,
            addition,
            result,
            station,
        } => {
            w.var_int(4);
            write_slot(w, template);
            write_slot(w, base);
            write_slot(w, addition);
            write_slot(w, result);
            write_slot(w, station);
        }
    }
}

fn write_requirements(w: &mut Writer, requirements: &[Vec<i32>]) {
    w.var_int(requirements.len() as i32);
    for ing in requirements {
        if ing.is_empty() {
            w.var_int(0);
        } else {
            w.var_int(ing.len() as i32 + 1);
            for &item in ing {
                w.var_int(item);
            }
        }
    }
}

/// `ClientboundRecipeBookAddPacket`: список записей и флаг замены.
pub fn recipe_book_add(
    id: i32,
    entries: &[(i32, bool, bool, &RecipeEntry)],
    replace: bool,
) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(entries.len() as i32);
    for &(display_id, notification, highlight, entry) in entries {
        w.var_int(display_id);
        write_display(&mut w, &entry.display);
        w.bool(entry.group.is_some());
        if let Some(group) = entry.group {
            w.var_int(group);
        }
        w.var_int(entry.category);
        w.bool(!entry.requirements.is_empty());
        if !entry.requirements.is_empty() {
            write_requirements(&mut w, &entry.requirements);
        }
        let mut flags = 0u8;
        if notification {
            flags |= 1;
        }
        if highlight {
            flags |= 2;
        }
        w.u8(flags);
    }
    w.bool(replace);
    w.buf
}

/// `ClientboundRecipeBookRemovePacket`.
pub fn recipe_book_remove(id: i32, displays: &[i32]) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(displays.len() as i32);
    for &display in displays {
        w.var_int(display);
    }
    w.buf
}

/// `ClientboundRecipeBookSettingsPacket` (`RecipeBookSettings.STREAM_CODEC`).
pub fn recipe_book_settings(id: i32, settings: &[[bool; 2]; 4]) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id);
    for s in settings {
        w.bool(s[0]).bool(s[1]);
    }
    w.buf
}

/// `ClientboundPlaceGhostRecipePacket`.
pub fn place_ghost(id: i32, container: i32, entry: &RecipeEntry) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(container);
    write_display(&mut w, &entry.display);
    w.buf
}

impl PlayPlan {
    fn recipe_book(&self) -> &RecipeBookData {
        &self.survival.recipe_book
    }

    /// `ServerRecipeBook.sendInitialRecipeBook`: настройки и все известные
    /// рецепты (с заменой книги).
    pub(crate) fn recipe_book_initial(&self, uuid: Uuid) -> Vec<Vec<u8>> {
        let state = self.recipe_books.lock().unwrap().get(&uuid).cloned();
        let state = state.unwrap_or_default();
        let mut out = vec![recipe_book_settings(
            self.ids.recipe_book_settings,
            &state.settings,
        )];
        let rows: Vec<(i32, bool, bool, &RecipeEntry)> = state
            .known
            .iter()
            .flat_map(|id| {
                self.recipe_book()
                    .entries_for(id)
                    .into_iter()
                    .map(|e| (e.display_id, false, state.highlight.contains(&e.id), e))
            })
            .collect();
        out.push(recipe_book_add(self.ids.recipe_book_add, &rows, true));
        out
    }

    /// Загруженные знания книги после входа.
    pub(crate) fn load_recipe_book(
        &self,
        uuid: Uuid,
        known: Vec<String>,
        highlight: Vec<String>,
        settings: [[bool; 2]; 4],
    ) {
        let mut all = self.recipe_books.lock().unwrap();
        all.insert(
            uuid,
            BookState {
                known: known.into_iter().collect(),
                highlight: highlight.into_iter().collect(),
                settings,
            },
        );
    }

    /// Данные для сохранения в `playerdata`.
    pub(crate) fn collect_recipe_book(
        &self,
        uuid: Uuid,
    ) -> (Vec<String>, Vec<String>, [[bool; 2]; 4]) {
        let all = self.recipe_books.lock().unwrap();
        let Some(state) = all.get(&uuid) else {
            return (Vec::new(), Vec::new(), [[false; 2]; 4]);
        };
        let mut known: Vec<String> = state.known.iter().cloned().collect();
        let mut highlight: Vec<String> = state.highlight.iter().cloned().collect();
        known.sort();
        highlight.sort();
        (known, highlight, state.settings)
    }

    pub(crate) fn forget_recipe_book(&self, uuid: Uuid) {
        self.recipe_books.lock().unwrap().remove(&uuid);
    }

    /// `/recipe give`: выдать рецепты и отправить `recipe_book_add`.
    pub(crate) fn recipe_give(&self, uuid: Uuid, ids: &[String]) -> usize {
        let mut rows = Vec::new();
        let mut unlocked: Vec<String> = Vec::new();
        let mut added = 0;
        let mut all = self.recipe_books.lock().unwrap();
        let state = all.entry(uuid).or_default();
        for id in ids {
            let list = self.recipe_book().entries_for(id);
            if list.is_empty() {
                continue;
            }
            let canonical = list[0].id.clone();
            if state.known.contains(&canonical) {
                continue;
            }
            for &entry in &list {
                rows.push((entry.display_id, entry.notification, true, entry));
            }
            unlocked.push(canonical.clone());
            state.known.insert(canonical.clone());
            state.highlight.insert(canonical);
            added += 1;
        }
        drop(all);
        if !rows.is_empty() {
            let packet = recipe_book_add(self.ids.recipe_book_add, &rows, false);
            self.send_to(uuid, packet);
        }
        if !unlocked.is_empty() {
            self.advancement_recipes_unlocked(uuid, &unlocked);
        }
        added
    }

    /// `/recipe take`: убрать рецепты и отправить `recipe_book_remove`.
    pub(crate) fn recipe_take(&self, uuid: Uuid, ids: &[String]) -> usize {
        let mut displays = Vec::new();
        let mut removed = 0;
        let mut all = self.recipe_books.lock().unwrap();
        let state = all.entry(uuid).or_default();
        for id in ids {
            let list = self.recipe_book().entries_for(id);
            if list.is_empty() {
                continue;
            }
            let canonical = list[0].id.clone();
            if !state.known.remove(&canonical) {
                continue;
            }
            state.highlight.remove(&canonical);
            for entry in &list {
                displays.push(entry.display_id);
            }
            removed += 1;
        }
        drop(all);
        if !displays.is_empty() {
            let packet = recipe_book_remove(self.ids.recipe_book_remove, &displays);
            self.send_to(uuid, packet);
        }
        removed
    }

    pub(crate) fn recipe_book_change_settings(
        &self,
        uuid: Uuid,
        kind: i32,
        open: bool,
        filtering: bool,
    ) {
        if !(0..4).contains(&kind) {
            return;
        }
        let mut all = self.recipe_books.lock().unwrap();
        all.entry(uuid).or_default().settings[kind as usize] = [open, filtering];
    }

    pub(crate) fn recipe_book_seen(&self, uuid: Uuid, display_id: i32) {
        let Some(entry) = self.recipe_book().entry_by_display(display_id) else {
            return;
        };
        let mut all = self.recipe_books.lock().unwrap();
        all.entry(uuid).or_default().highlight.remove(&entry.id);
    }

    /// `*` для `/recipe give|take`.
    pub(crate) fn recipe_book_ids(&self) -> Vec<String> {
        self.recipe_book().learnable()
    }

    /// Рецепт по id (без namespace).
    pub(crate) fn recipe_book_entry(&self, id: &str) -> Option<&RecipeEntry> {
        self.recipe_book().entry(id)
    }

    /// Проверка, выдан ли игроку рецепт.
    pub(crate) fn recipe_known(&self, uuid: Uuid, id: &str) -> bool {
        self.recipe_books
            .lock()
            .unwrap()
            .get(&uuid)
            .is_some_and(|s| s.known.contains(id))
    }

    /// `ServerPlaceRecipe`: разложить ингредиенты по сетке.
    ///
    /// `container` — id окна, к которому относится запрос:
    /// [`inv::PLAYER_WINDOW`] для 2×2 в инвентаре или id открытого окна.
    /// `Ok(true)` — разложено, `Ok(false)` — рецепт не помещается,
    /// `Err(())` — окно не подходит.
    pub(super) fn place_recipe(
        &self,
        inv: &mut Inventory,
        window: &mut Option<Window>,
        container: i32,
        entry: &RecipeEntry,
        use_max: bool,
    ) -> Result<bool, ()> {
        if container == inv::PLAYER_WINDOW {
            let mut grid: Vec<Slot> = inv.slots[1..5].to_vec();
            let ok = self.fill_grid(inv, &mut grid, 2, entry, use_max);
            inv.slots[1..5].copy_from_slice(&grid);
            inv.slots[0] = if ok {
                self.craft_result(&inv.slots[1..5], 2)
            } else {
                None
            };
            return Ok(ok);
        }
        let window = window.as_mut().filter(|w| w.id == container).ok_or(())?;
        match window.kind {
            Kind::Crafting => {
                let ok = self.fill_grid(inv, &mut window.slots[1..10], 3, entry, use_max);
                window.slots[0] = if ok {
                    self.craft_result(&window.slots[1..10], 3)
                } else {
                    None
                };
                Ok(ok)
            }
            Kind::Furnace => {
                if let Placement::Furnace { input } = &entry.placement {
                    return Ok(self.put_ingredient(inv, &mut window.slots[0], input, 1));
                }
                Ok(false)
            }
            Kind::Stonecutter => {
                if let Placement::Stonecutter { input, index } = &entry.placement {
                    self.put_ingredient(inv, &mut window.slots[0], input, 1);
                    window.stone_selection = Some(*index);
                    window.slots[1] = self.stone_result(window.slots[0], window.stone_selection);
                    return Ok(true);
                }
                Ok(false)
            }
            Kind::Smithing => {
                if let Placement::Smithing {
                    template,
                    base,
                    addition,
                } = &entry.placement
                {
                    self.put_ingredient(inv, &mut window.slots[0], template, 1);
                    self.put_ingredient(inv, &mut window.slots[1], base, 1);
                    self.put_ingredient(inv, &mut window.slots[2], addition, 1);
                    window.slots[3] =
                        crate::smithing::result(&window.slots, &self.survival.smithing);
                    return Ok(window.slots[3].is_some());
                }
                Ok(false)
            }
            _ => Ok(false),
        }
    }

    /// Найти в инвентаре и переложить `count` предмета из `input`.
    fn put_ingredient(
        &self,
        inv: &mut Inventory,
        target: &mut Slot,
        input: &[i32],
        count: i32,
    ) -> bool {
        let Some((slot, item)) = input.iter().find_map(|&item| {
            Inventory::storage_order()
                .find(|&i| inv.slots[i].is_some_and(|s| s.item == item && s.count > 0))
                .map(|i| (i, item))
        }) else {
            return false;
        };
        let take = count.min(inv.slots[slot].map_or(0, |s| s.count));
        if take <= 0 {
            return false;
        }
        let stack = inv.slots[slot].take().unwrap();
        let left = Stack {
            count: stack.count - take,
            ..stack
        };
        if left.count > 0 {
            inv.slots[slot] = Some(left);
        }
        match target {
            Some(s) if s.item == item && s.damage == stack.damage && s.extra == stack.extra => {
                s.count += take;
            }
            _ => {
                *target = Some(Stack {
                    count: take,
                    ..stack
                });
            }
        }
        true
    }

    /// Очистить сетку и разложить `entry`; `true` — разложено.
    fn fill_grid(
        &self,
        inv: &mut Inventory,
        grid: &mut [Slot],
        size: usize,
        entry: &RecipeEntry,
        use_max: bool,
    ) -> bool {
        let Placement::Crafting {
            width,
            height,
            cells,
            shapeless,
        } = &entry.placement
        else {
            return false;
        };
        if !shapeless && (*width > size || *height > size) {
            self.clear_grid(inv, grid);
            return false;
        }
        if *shapeless && cells.len() > size * size {
            self.clear_grid(inv, grid);
            return false;
        }
        self.clear_grid(inv, grid);
        let mut targets: Vec<(usize, &Vec<i32>)> = Vec::new();
        if *shapeless {
            for (i, cell) in cells.iter().enumerate() {
                if !cell.is_empty() {
                    targets.push((i, cell));
                }
            }
        } else {
            for (i, cell) in cells.iter().enumerate() {
                if cell.is_empty() {
                    continue;
                }
                let (x, y) = (i % width, i / width);
                targets.push((y * size + x, cell));
            }
        }
        let mut max_crafts = 1;
        if use_max {
            for (_, cell) in &targets {
                let have: i32 = cell
                    .iter()
                    .map(|&item| {
                        Inventory::storage_order()
                            .filter(|&i| inv.slots[i].is_some_and(|s| s.item == item))
                            .map(|i| inv.slots[i].map_or(0, |s| s.count))
                            .sum::<i32>()
                    })
                    .max()
                    .unwrap_or(0);
                max_crafts = max_crafts.min(have);
            }
            max_crafts = max_crafts.max(1);
        }
        let mut placed = false;
        for (index, cell) in targets {
            let Some(target) = grid.get_mut(index) else {
                continue;
            };
            if !self.put_ingredient(inv, target, cell, max_crafts) {
                self.clear_grid(inv, grid);
                return false;
            }
            placed = true;
        }
        placed
    }

    /// `clearGrid`: вернуть содержимое сетки в инвентарь.
    fn clear_grid(&self, inv: &mut Inventory, grid: &mut [Slot]) {
        for slot in grid {
            if let Some(stack) = slot.take() {
                let max = self.max_stack(stack.item);
                if let Some(left) = inv.add_stack(stack, max)
                    && left.count > 0
                {
                    inv.carried = Some(left);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_display_item_encoding() {
        let mut w = Writer::new();
        write_slot(&mut w, &SlotDisp::Items(vec![7, 9]));
        assert_eq!(w.buf, vec![6, 3, 7, 9]);
        let mut w = Writer::new();
        write_slot(&mut w, &SlotDisp::Stack { item: 12, count: 3 });
        assert_eq!(w.buf, vec![5, 12, 3, 0, 0]);
    }
}
