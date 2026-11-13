//! Достижения (`Advancement`): загрузка данных, прогресс, `update_advancements`
//! и триггеры (`villager_trade`, `fishing_rod_hooked`, `consume_item`).
//!
//! Определения читаются из `data/minecraft/advancement/**/*.json` при старте.
//! Прогресс хранится по игроку (advancement → критерий → время получения) и
//! пишется в `advancements/<uuid>.json`.

use super::*;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct AdvancementCriterion {
    pub name: String,
    pub trigger: String,
    /// `conditions.item.items`: какие предметы закрывают критерий.
    pub items: Vec<String>,
    /// `conditions.entity.predicate.minecraft:entity_type`.
    pub entities: Vec<String>,
    /// `conditions.recipes` (для `recipe_unlocked`).
    pub recipes: Vec<String>,
    /// `conditions.location.*.blocks` (для `placed_block`).
    pub blocks: Vec<String>,
    /// `conditions.from`/`to` (для `changed_dimension`).
    pub from_dims: Vec<String>,
    pub to_dims: Vec<String>,
    /// `conditions.potion` (для `brewed_potion`).
    pub potions: Vec<String>,
    /// `conditions.distance` (для `used_ender_eye`, по квадрату расстояния).
    pub distance: Option<(f64, f64)>,
    /// `conditions.levels` (для `enchanted_item`).
    pub levels: Option<(i32, i32)>,
    /// `conditions.signal_strength` (для `target_hit`).
    pub signal_strength: Option<(i32, i32)>,
    /// `conditions.projectile.predicate.minecraft:distance.horizontal`.
    pub proj_distance: Option<(f64, f64)>,
    /// `conditions.damage.type.direct_entity.minecraft:entity_type`.
    pub direct_entities: Vec<String>,
    /// `conditions.damage.type.tags` с `expected: true`.
    pub damage_tags_req: Vec<String>,
    /// `conditions.damage.blocked`.
    pub blocked: Option<bool>,
    /// `conditions.victims[].predicate.minecraft:entity_type` (`channeled_lightning`).
    pub victims: Vec<String>,
    /// `conditions.distance.horizontal` (`nether_travel`).
    pub travel_distance: Option<(f64, f64)>,
    /// `conditions.child`/`parent`/`partner` (`bred_animals`).
    pub child_entities: Vec<String>,
    pub parent_entities: Vec<String>,
    pub partner_entities: Vec<String>,
    /// `conditions.ingredients` (`recipe_crafted`): список предикатов.
    pub ingredients: Vec<Vec<String>>,
}

#[derive(Clone, Debug)]
pub struct AdvancementDisplay {
    pub title: serde_json::Value,
    pub description: serde_json::Value,
    pub icon: String,
    /// `AdvancementType.id`: task=0, challenge=1, goal=2.
    pub frame: i32,
    pub background: Option<String>,
    pub show_toast: bool,
    pub announce: bool,
    pub hidden: bool,
}

#[derive(Clone, Debug)]
pub struct AdvancementDef {
    pub id: String,
    pub parent: Option<String>,
    pub display: Option<AdvancementDisplay>,
    pub requirements: Vec<Vec<String>>,
    pub criteria: Vec<AdvancementCriterion>,
    pub x: f32,
    pub y: f32,
}

fn short(id: &str) -> &str {
    id.trim_start_matches("minecraft:")
}

fn json_string_list(v: &serde_json::Value) -> Vec<String> {
    match v {
        serde_json::Value::String(s) => vec![s.clone()],
        serde_json::Value::Array(a) => a
            .iter()
            .filter_map(|x| x.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}

/// Собрать `blocks` из дерева block predicate (`match_block`, `location_check`,
/// `all_of`/`any_of`).
fn collect_blocks(v: &serde_json::Value, out: &mut Vec<String>) {
    if let Some(b) = v.get("blocks") {
        out.extend(json_string_list(b));
    }
    if let Some(block) = v.get("predicate").and_then(|p| p.get("block")) {
        collect_blocks(block, out);
    }
    if let Some(terms) = v.get("terms").and_then(|t| t.as_array()) {
        for term in terms {
            collect_blocks(term, out);
        }
    }
}

/// `match_tool` внутри дерева `location`: `predicate.items`.
fn collect_location_items(v: &serde_json::Value, out: &mut Vec<String>) {
    if let Some(items) = v.get("predicate").and_then(|p| p.get("items")) {
        out.extend(json_string_list(items));
    }
    if let Some(terms) = v.get("terms").and_then(|t| t.as_array()) {
        for term in terms {
            collect_location_items(term, out);
        }
    }
}

/// Строка или список строк в JSON.
fn string_or_list(v: &serde_json::Value) -> Vec<String> {
    match v {
        serde_json::Value::String(s) => vec![s.clone()],
        serde_json::Value::Array(a) => a
            .iter()
            .filter_map(|x| x.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}

fn bounds_f64(v: &serde_json::Value) -> Option<(f64, f64)> {
    match v {
        serde_json::Value::Number(n) => {
            let x = n.as_f64()?;
            Some((x, x))
        }
        serde_json::Value::Object(o) => Some((
            o.get("min")
                .and_then(|x| x.as_f64())
                .unwrap_or(f64::NEG_INFINITY),
            o.get("max")
                .and_then(|x| x.as_f64())
                .unwrap_or(f64::INFINITY),
        )),
        _ => None,
    }
}

fn bounds_i32(v: &serde_json::Value) -> Option<(i32, i32)> {
    match v {
        serde_json::Value::Number(n) => {
            let x = n.as_i64()? as i32;
            Some((x, x))
        }
        serde_json::Value::Object(o) => Some((
            o.get("min")
                .and_then(|x| x.as_i64())
                .map_or(i32::MIN, |x| x as i32),
            o.get("max")
                .and_then(|x| x.as_i64())
                .map_or(i32::MAX, |x| x as i32),
        )),
        _ => None,
    }
}

fn parse_display(v: &serde_json::Value) -> Option<AdvancementDisplay> {
    let d = v.get("display")?;
    let icon = d
        .get("icon")
        .and_then(|i| i.get("id").or_else(|| i.get("item")))
        .and_then(|i| i.as_str())?
        .to_owned();
    let frame = match d.get("frame").and_then(|f| f.as_str()) {
        Some("challenge") => 1,
        Some("goal") => 2,
        _ => 0,
    };
    Some(AdvancementDisplay {
        title: d
            .get("title")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({"text": ""})),
        description: d
            .get("description")
            .cloned()
            .unwrap_or(serde_json::Value::String(String::new())),
        icon,
        frame,
        background: d
            .get("background")
            .and_then(|b| b.as_str())
            .map(str::to_owned),
        show_toast: d
            .get("show_toast")
            .and_then(|b| b.as_bool())
            .unwrap_or(true),
        announce: d
            .get("announce_to_chat")
            .and_then(|b| b.as_bool())
            .unwrap_or(true),
        hidden: d.get("hidden").and_then(|b| b.as_bool()).unwrap_or(false),
    })
}

fn parse_advancement(id: &str, json: &serde_json::Value) -> Option<AdvancementDef> {
    let parent = json
        .get("parent")
        .and_then(|p| p.as_str())
        .map(short)
        .map(str::to_owned);
    let display = parse_display(json);
    let mut criteria = Vec::new();
    if let Some(map) = json.get("criteria").and_then(|c| c.as_object()) {
        for (name, c) in map {
            let trigger = c
                .get("trigger")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_owned();
            let mut items = c
                .get("conditions")
                .and_then(|c| c.get("item"))
                .and_then(|i| i.get("items"))
                .map(json_string_list)
                .unwrap_or_default();
            // `inventory_changed`: `conditions.items` — список предикатов.
            if let Some(list) = c
                .get("conditions")
                .and_then(|c| c.get("items"))
                .and_then(|i| i.as_array())
            {
                for pred in list {
                    let names = pred
                        .get("items")
                        .map(json_string_list)
                        .or_else(|| {
                            pred.get("tag")
                                .and_then(|t| t.as_str())
                                .map(|t| vec![format!("#{t}")])
                        })
                        .unwrap_or_default();
                    items.extend(names);
                }
            }
            let entities = c
                .get("conditions")
                .and_then(|c| c.get("entity"))
                .and_then(|e| e.get("predicate"))
                .and_then(|p| p.get("minecraft:entity_type"))
                .map(json_string_list)
                .unwrap_or_default();
            let recipes = c
                .get("conditions")
                .and_then(|c| c.get("recipes"))
                .map(json_string_list)
                .unwrap_or_default();
            let mut blocks = Vec::new();
            // `enter_block`: `conditions.blocks` — блок или список блоков.
            if let Some(blocks_v) = c.get("conditions").and_then(|c| c.get("blocks")) {
                blocks.extend(string_or_list(blocks_v));
            }
            if let Some(location) = c.get("conditions").and_then(|c| c.get("location")) {
                collect_blocks(location, &mut blocks);
                // `match_tool.predicate.items` внутри location.
                collect_location_items(location, &mut items);
            }
            let cond = c.get("conditions");
            let dim = |key: &str| {
                cond.and_then(|c| c.get(key))
                    .map(string_or_list)
                    .unwrap_or_default()
            };
            let potions = cond
                .and_then(|c| c.get("potion"))
                .map(|v| match v {
                    serde_json::Value::Object(o) => {
                        o.get("potions").map(string_or_list).unwrap_or_default()
                    }
                    other => string_or_list(other),
                })
                .unwrap_or_default();
            let distance = cond.and_then(|c| c.get("distance")).and_then(bounds_f64);
            let levels = cond.and_then(|c| c.get("levels")).and_then(bounds_i32);
            let signal_strength = cond
                .and_then(|c| c.get("signal_strength"))
                .and_then(bounds_i32);
            let proj_distance = cond
                .and_then(|c| c.get("projectile"))
                .and_then(|p| p.get("predicate"))
                .and_then(|p| p.get("minecraft:distance"))
                .and_then(|d| d.get("horizontal"))
                .and_then(bounds_f64);
            let damage = cond.and_then(|c| c.get("damage"));
            let direct_entities = damage
                .and_then(|d| d.get("type"))
                .and_then(|t| t.get("direct_entity"))
                .and_then(|e| e.get("minecraft:entity_type"))
                .map(string_or_list)
                .unwrap_or_default();
            let mut damage_tags_req = Vec::new();
            if let Some(tags) = damage
                .and_then(|d| d.get("type"))
                .and_then(|t| t.get("tags"))
                .and_then(|t| t.as_array())
            {
                for tag in tags {
                    let expected = tag
                        .get("expected")
                        .and_then(|b| b.as_bool())
                        .unwrap_or(true);
                    if expected && let Some(id) = tag.get("id").and_then(|s| s.as_str()) {
                        damage_tags_req.push(id.trim_start_matches('#').to_owned());
                    }
                }
            }
            let blocked = damage
                .and_then(|d| d.get("blocked"))
                .and_then(|b| b.as_bool());
            let mut victims = Vec::new();
            if let Some(list) = cond
                .and_then(|c| c.get("victims"))
                .and_then(|v| v.as_array())
            {
                for victim in list {
                    let names = victim
                        .get("predicate")
                        .and_then(|p| p.get("minecraft:entity_type"))
                        .map(string_or_list)
                        .unwrap_or_default();
                    victims.extend(names);
                }
            }
            let travel_distance = cond
                .and_then(|c| c.get("distance"))
                .and_then(|d| d.get("horizontal"))
                .and_then(bounds_f64);
            let entity_type = |key: &str| {
                cond.and_then(|c| c.get(key))
                    .and_then(|e| e.get("predicate"))
                    .and_then(|p| p.get("minecraft:entity_type"))
                    .map(string_or_list)
                    .unwrap_or_default()
            };
            let (child_entities, parent_entities, partner_entities) = (
                entity_type("child"),
                entity_type("parent"),
                entity_type("partner"),
            );
            let ingredients = cond
                .and_then(|c| c.get("ingredients"))
                .and_then(|i| i.as_array())
                .map(|list| {
                    list.iter()
                        .map(|pred| {
                            pred.get("items")
                                .map(string_or_list)
                                .or_else(|| {
                                    pred.get("tag")
                                        .and_then(|t| t.as_str())
                                        .map(|t| vec![format!("#{t}")])
                                })
                                .unwrap_or_default()
                        })
                        .collect()
                })
                .unwrap_or_default();
            criteria.push(AdvancementCriterion {
                name: name.clone(),
                trigger,
                items,
                entities,
                recipes,
                blocks,
                from_dims: dim("from"),
                to_dims: dim("to"),
                potions,
                distance,
                levels,
                signal_strength,
                proj_distance,
                direct_entities,
                damage_tags_req,
                blocked,
                victims,
                travel_distance,
                child_entities,
                parent_entities,
                partner_entities,
                ingredients,
            });
        }
    }
    let requirements = json
        .get("requirements")
        .and_then(|r| r.as_array())
        .map(|groups| groups.iter().map(json_string_list).collect::<Vec<_>>())
        .filter(|g| !g.is_empty())
        .unwrap_or_else(|| vec![criteria.iter().map(|c| c.name.clone()).collect()]);
    Some(AdvancementDef {
        id: id.to_owned(),
        parent,
        display,
        requirements,
        criteria,
        x: 0.0,
        y: 0.0,
    })
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, serde_json::Value)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, out);
        } else if path.extension().is_some_and(|e| e == "json")
            && let Ok(bytes) = std::fs::read(&path)
            && let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes)
        {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .with_extension("")
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, json));
        }
    }
}

/// Прочитать все достижения из `data/minecraft/advancement`.
pub fn load_advancements(data_dir: &Path) -> Vec<AdvancementDef> {
    let mut raw = Vec::new();
    let root = data_dir.join("data/minecraft/advancement");
    walk(&root, &root, &mut raw);
    raw.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out: Vec<AdvancementDef> = raw
        .iter()
        .filter_map(|(id, json)| parse_advancement(id, json))
        .collect();
    // Раскладка дерева: корни в (0,0), дети справа со сдвигом по вертикали.
    let children: FastMap<String, Vec<String>> = {
        let mut map: FastMap<String, Vec<String>> = FastMap::default();
        for a in &out {
            if let Some(p) = &a.parent {
                map.entry(p.clone()).or_default().push(a.id.clone());
            }
        }
        map
    };
    let mut pos: FastMap<String, (f32, f32)> = FastMap::default();
    let roots: Vec<String> = out
        .iter()
        .filter(|a| a.parent.is_none())
        .map(|a| a.id.clone())
        .collect();
    let mut stack: Vec<(String, f32, f32, f32)> = roots
        .iter()
        .enumerate()
        .map(|(i, id)| (id.clone(), 0.0, i as f32 * 2.0, 1.0))
        .collect();
    while let Some((id, x, y, _)) = stack.pop() {
        pos.insert(id.clone(), (x, y));
        if let Some(kids) = children.get(&id) {
            let n = kids.len() as f32;
            for (i, kid) in kids.iter().enumerate() {
                stack.push((
                    kid.clone(),
                    x + 1.5,
                    y + (i as f32 - (n - 1.0) / 2.0) * 1.5,
                    0.0,
                ));
            }
        }
    }
    for a in &mut out {
        if let Some(&(x, y)) = pos.get(&a.id) {
            a.x = x;
            a.y = y;
        }
    }
    out
}

impl PlayPlan {
    fn adv(&self, id: &str) -> Option<&AdvancementDef> {
        self.survival
            .advancement_ids
            .get(id)
            .and_then(|&i| self.survival.advancements.get(i))
    }

    pub(crate) fn advancement_index(&self, id: &str) -> Option<usize> {
        self.survival.advancement_ids.get(id).copied()
    }

    fn adv_done(progress: &FastMap<String, i64>, def: &AdvancementDef) -> bool {
        !def.requirements.is_empty()
            && def.requirements.iter().all(|group| {
                group
                    .iter()
                    .any(|name| progress.get(name).is_some_and(|&at| at > 0))
            })
    }

    fn adv_parent_done(&self, progress: &AdvProgress, parent: Option<&str>) -> bool {
        let Some(parent) = parent else {
            return true;
        };
        self.adv(parent).is_some_and(|def| {
            progress.get(parent).is_some_and(|p| Self::adv_done(p, def)) || def.criteria.is_empty()
        })
    }

    /// Добавить критерии по триггеру и отправить прогресс.
    pub(crate) fn advancement_trigger(&self, uuid: Uuid, trigger: &str, item: Option<&str>) {
        self.advancement_run(uuid, trigger, |c| {
            if !c.items.is_empty()
                && !item
                    .is_some_and(|name| c.items.iter().any(|want| self.item_matches(want, name)))
            {
                return false;
            }
            if !c.blocks.is_empty()
                && !item
                    .is_some_and(|name| c.blocks.iter().any(|want| self.block_matches(want, name)))
            {
                return false;
            }
            if !c.entities.is_empty()
                && !item.is_some_and(|name| c.entities.iter().any(|n| short(n) == short(name)))
            {
                return false;
            }
            if !c.recipes.is_empty()
                && !item.is_some_and(|name| c.recipes.iter().any(|n| short(n) == short(name)))
            {
                return false;
            }
            true
        });
    }

    /// `RecipeUnlockedTrigger` для набора рецептов: один проход вместо
    /// вызова `advancement_trigger` на каждый рецепт (`/recipe give *`).
    pub(crate) fn advancement_recipes_unlocked(&self, uuid: Uuid, ids: &[String]) {
        self.advancement_run(uuid, "minecraft:recipe_unlocked", |c| {
            c.recipes.is_empty()
                || c.recipes
                    .iter()
                    .any(|want| ids.iter().any(|id| short(want) == short(id)))
        });
    }

    /// `item_used_on_block`: предмет в руке и блок под прицелом.
    pub(crate) fn advancement_item_used_on_block(&self, uuid: Uuid, item: &str, block: &str) {
        self.advancement_run(uuid, "minecraft:item_used_on_block", |c| {
            (c.items.is_empty() || c.items.iter().any(|w| self.item_matches(w, item)))
                && (c.blocks.is_empty() || c.blocks.iter().any(|w| self.block_matches(w, block)))
        });
    }

    /// `used_ender_eye`: условие `distance` по квадрату расстояния.
    pub(crate) fn advancement_used_ender_eye(&self, uuid: Uuid, sqr_distance: f64) {
        self.advancement_run(uuid, "minecraft:used_ender_eye", |c| {
            c.distance
                .is_none_or(|(min, max)| sqr_distance >= min && sqr_distance <= max)
        });
    }

    /// `changed_dimension`: `from`/`to` измерий.
    pub(crate) fn advancement_changed_dimension(&self, uuid: Uuid, from: &str, to: &str) {
        self.advancement_run(uuid, "minecraft:changed_dimension", |c| {
            (c.from_dims.is_empty() || c.from_dims.iter().any(|w| short(w) == short(from)))
                && (c.to_dims.is_empty() || c.to_dims.iter().any(|w| short(w) == short(to)))
        });
    }

    /// `brewed_potion`: зелье из `PotionContents`.
    pub(crate) fn advancement_brewed_potion(&self, uuid: Uuid, potion: Option<&str>) {
        self.advancement_run(uuid, "minecraft:brewed_potion", |c| {
            c.potions.is_empty()
                || potion.is_some_and(|p| c.potions.iter().any(|w| short(w) == short(p)))
        });
    }

    /// `enchanted_item`: зачарованный предмет и стоимость в уровнях.
    pub(crate) fn advancement_enchanted_item(&self, uuid: Uuid, item: &str, levels: i32) {
        self.advancement_run(uuid, "minecraft:enchanted_item", |c| {
            (c.items.is_empty() || c.items.iter().any(|w| self.item_matches(w, item)))
                && c.levels
                    .is_none_or(|(min, max)| levels >= min && levels <= max)
        });
    }

    /// `recipe_crafted`: id рецепта и использованные ингредиенты.
    pub(crate) fn advancement_recipe_crafted(&self, uuid: Uuid, recipe: &str, used: &[String]) {
        self.advancement_run(uuid, "minecraft:recipe_crafted", |c| {
            if !c.recipes.is_empty() && !c.recipes.iter().any(|w| short(w) == short(recipe)) {
                return false;
            }
            // `RecipeCraftedTrigger.TriggerInstance.matches`: каждый предикат
            // съедает один подходящий ингредиент (мультимножество).
            let mut remaining: Vec<&String> = used.iter().collect();
            for pred in &c.ingredients {
                let Some(pos) = remaining
                    .iter()
                    .position(|name| pred.iter().any(|w| self.item_matches(w, name)))
                else {
                    return false;
                };
                remaining.remove(pos);
            }
            true
        });
    }

    /// `bred_animals`: ребёнок, родитель и партнёр.
    pub(crate) fn advancement_bred_animals(
        &self,
        uuid: Uuid,
        child: &str,
        parent: &str,
        partner: &str,
    ) {
        self.advancement_run(uuid, "minecraft:bred_animals", |c| {
            let ok = |want: &[String], name: &str| {
                want.is_empty() || want.iter().any(|w| self.entity_matches(w, name))
            };
            ok(&c.child_entities, child)
                && ok(&c.parent_entities, parent)
                && ok(&c.partner_entities, partner)
        });
    }

    /// `enter_block`: блок, в который вошёл игрок (`conditions.blocks`).
    pub(crate) fn advancement_enter_block(&self, uuid: Uuid, block: &str) {
        self.advancement_run(uuid, "minecraft:enter_block", |c| {
            c.blocks.is_empty() || c.blocks.iter().any(|w| self.block_matches(w, block))
        });
    }

    /// `shot_crossbow`: предмет, из которого стреляли (`conditions.item`).
    pub(crate) fn advancement_shot_crossbow(&self, uuid: Uuid, item: &str) {
        self.advancement_run(uuid, "minecraft:shot_crossbow", |c| {
            c.items.is_empty() || c.items.iter().any(|w| self.item_matches(w, item))
        });
    }

    /// `used_totem`: предмет тотема (`conditions.item`).
    pub(crate) fn advancement_used_totem(&self, uuid: Uuid, item: &str) {
        self.advancement_run(uuid, "minecraft:used_totem", |c| {
            c.items.is_empty() || c.items.iter().any(|w| self.item_matches(w, item))
        });
    }

    /// `nether_travel`: горизонталь от входа в Нижний мир.
    pub(crate) fn advancement_nether_travel(&self, uuid: Uuid, horizontal: f64) {
        self.advancement_run(uuid, "minecraft:nether_travel", |c| {
            c.travel_distance
                .is_none_or(|(min, max)| horizontal >= min && horizontal <= max)
        });
    }

    /// `channeled_lightning`: каждый `victims` должен найтись среди поражённых.
    pub(crate) fn advancement_channeled_lightning(&self, uuid: Uuid, struck: &[&str]) {
        self.advancement_run(uuid, "minecraft:channeled_lightning", |c| {
            c.victims
                .iter()
                .all(|want| struck.iter().any(|name| self.entity_matches(want, name)))
        });
    }

    /// `target_hit`: сила сигнала и горизонталь до точки попадания.
    pub(crate) fn advancement_target_hit(&self, uuid: Uuid, signal: i32, horizontal: f64) {
        self.advancement_run(uuid, "minecraft:target_hit", |c| {
            c.signal_strength
                .is_none_or(|(min, max)| signal >= min && signal <= max)
                && c.proj_distance
                    .is_none_or(|(min, max)| horizontal >= min && horizontal <= max)
        });
    }

    /// `player_hurt_entity`: цель, прямой источник и тип урона.
    pub(crate) fn advancement_player_hurt_entity(
        &self,
        uuid: Uuid,
        victim: &str,
        direct: &str,
        damage_type: &str,
    ) {
        self.advancement_run(uuid, "minecraft:player_hurt_entity", |c| {
            (c.entities.is_empty() || c.entities.iter().any(|w| self.entity_matches(w, victim)))
                && self.damage_matches(c, direct, damage_type)
        });
    }

    /// `entity_hurt_player`: игрок получил урон (и был ли он заблокирован).
    pub(crate) fn advancement_entity_hurt_player(
        &self,
        uuid: Uuid,
        direct: &str,
        damage_type: &str,
        blocked: bool,
    ) {
        self.advancement_run(uuid, "minecraft:entity_hurt_player", |c| {
            c.blocked.is_none_or(|b| b == blocked) && self.damage_matches(c, direct, damage_type)
        });
    }

    /// `DamagePredicate`: прямой источник и теги типа урона.
    fn damage_matches(&self, c: &AdvancementCriterion, direct: &str, damage_type: &str) -> bool {
        (c.direct_entities.is_empty()
            || c.direct_entities
                .iter()
                .any(|w| self.entity_matches(w, direct)))
            && c.damage_tags_req.iter().all(|tag| {
                self.survival
                    .damage_tags
                    .get(tag)
                    .is_some_and(|set| set.contains(damage_type))
            })
    }

    /// Тип сущности или `#тег` (`entity_type_tags`).
    fn entity_matches(&self, want: &str, name: &str) -> bool {
        if let Some(tag) = want.strip_prefix('#') {
            let key = if tag.contains(':') {
                tag.to_owned()
            } else {
                format!("minecraft:{tag}")
            };
            return self
                .survival
                .entity_type_tags
                .get(&key)
                .is_some_and(|set| set.contains(name));
        }
        short(want) == short(name)
    }

    fn advancement_run(
        &self,
        uuid: Uuid,
        trigger: &str,
        accept: impl Fn(&AdvancementCriterion) -> bool,
    ) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64);
        let mut changed = Vec::new();
        let mut completed = Vec::new();
        {
            let mut all = self.advancement_progress.lock().unwrap();
            let progress = all.entry(uuid).or_default();
            // Событие может закрыть и родителя, и ребёнка (первое размножение:
            // `breed_an_animal` + критерий `minecraft:cow` в `bred_all_animals`).
            // Поэтому проходим, пока появляются новые критерии: только что
            // выполненный родитель открывает своих детей в этом же событии.
            loop {
                let mut pass = false;
                for def in &self.survival.advancements {
                    if !self.adv_parent_done(progress, def.parent.as_deref()) {
                        continue;
                    }
                    let was = progress
                        .get(&def.id)
                        .is_some_and(|p| Self::adv_done(p, def));
                    let mut touched = false;
                    {
                        let entry = progress.entry(def.id.clone()).or_default();
                        for c in &def.criteria {
                            if c.trigger != trigger || !accept(c) {
                                continue;
                            }
                            if !entry.contains_key(&c.name) {
                                entry.insert(c.name.clone(), now);
                                touched = true;
                            }
                        }
                    }
                    if touched {
                        pass = true;
                        changed.push(def.id.clone());
                        let entry = &progress[&def.id];
                        if !was && Self::adv_done(entry, def) {
                            completed.push(def.id.clone());
                        }
                    }
                }
                if !pass {
                    break;
                }
            }
        }
        changed.sort();
        changed.dedup();
        completed.sort();
        completed.dedup();
        if !changed.is_empty() {
            self.send_advancement_progress(uuid, &changed);
        }
        for id in completed {
            self.announce_advancement(uuid, &id);
        }
    }

    /// `ItemPredicate.test` по имени: предмет или `#тег`.
    fn item_matches(&self, want: &str, name: &str) -> bool {
        if let Some(tag) = want.strip_prefix('#') {
            let key = if tag.contains(':') {
                tag.to_owned()
            } else {
                format!("minecraft:{tag}")
            };
            return self
                .survival
                .item_tags
                .get(&key)
                .is_some_and(|set| set.contains(name));
        }
        short(want) == short(name)
    }

    /// `BlockPredicate` по имени блока: блок или `#тег`.
    fn block_matches(&self, want: &str, name: &str) -> bool {
        if let Some(tag) = want.strip_prefix('#') {
            let key = if tag.contains(':') {
                tag.to_owned()
            } else {
                format!("minecraft:{tag}")
            };
            return self
                .survival
                .block_tags
                .get(&key)
                .is_some_and(|set| set.contains(name));
        }
        short(want) == short(name)
    }

    fn announce_advancement(&self, uuid: Uuid, id: &str) {
        let Some(def) = self.adv(id) else {
            return;
        };
        let Some(display) = &def.display else {
            return;
        };
        if !display.announce || display.hidden {
            return;
        }
        let name = self
            .hub
            .players
            .lock()
            .unwrap()
            .get(&uuid)
            .map(|p| p.entry.name.clone())
            .unwrap_or_default();
        let kind = match display.frame {
            1 => "challenge",
            2 => "goal",
            _ => "task",
        };
        let message = serde_json::json!({
            "translate": format!("chat.type.advancement.{kind}"),
            "with": [{"text": name}, display.title],
        });
        self.hub.send(
            None,
            play::system_chat(self.ids.system_chat, &message.to_string(), false),
        );
    }

    fn adv_body(w: &mut Writer, def: &AdvancementDef, icon_id: Option<i32>) {
        // parent: optional identifier.
        w.bool(def.parent.is_some());
        if let Some(parent) = &def.parent {
            w.string(&format!("minecraft:{parent}"));
        }
        // display: optional.
        w.bool(def.display.is_some());
        if let Some(d) = &def.display {
            text::write_component(w, &d.title);
            text::write_component(w, &d.description);
            // icon: ItemStackTemplate (item, count, пустой patch).
            w.var_int(icon_id.unwrap_or(0))
                .var_int(1)
                .var_int(0)
                .var_int(0);
            w.var_int(d.frame);
            let mut flags = 0i32;
            if d.background.is_some() {
                flags |= 1;
            }
            if d.show_toast {
                flags |= 2;
            }
            if d.hidden {
                flags |= 4;
            }
            w.i32(flags);
            if let Some(bg) = &d.background {
                w.string(bg);
            }
        }
        // requirements: list<list<string>>.
        w.var_int(def.requirements.len() as i32);
        for group in &def.requirements {
            w.var_int(group.len() as i32);
            for name in group {
                w.string(name);
            }
        }
        w.bool(true);
    }

    fn advancement_packets(
        &self,
        reset: bool,
        added: &[&AdvancementDef],
        progress: &[(String, FastMap<String, i64>)],
    ) -> Vec<u8> {
        let mut w = Writer::new();
        w.var_int(self.ids.update_advancements).bool(reset);
        w.var_int(added.len() as i32);
        for def in added {
            w.string(&format!("minecraft:{}", def.id));
            let icon = def.display.as_ref().and_then(|d| self.item_id(&d.icon));
            Self::adv_body(&mut w, def, icon);
            w.f32(def.x).f32(def.y);
        }
        w.var_int(0); // removed
        w.var_int(progress.len() as i32);
        for (id, criteria) in progress {
            w.string(&format!("minecraft:{id}"));
            w.var_int(criteria.len() as i32);
            for (name, at) in criteria {
                w.string(name).bool(*at > 0).i64(*at);
            }
        }
        w.bool(true);
        w.buf
    }

    /// Полный пакет при входе: все определения и накопленный прогресс.
    pub(crate) fn full_advancement_packet(&self, uuid: Uuid) -> Option<Vec<u8>> {
        if self.survival.advancements.is_empty() {
            return None;
        }
        let all: Vec<&AdvancementDef> = self.survival.advancements.iter().collect();
        let progress = self
            .advancement_progress
            .lock()
            .unwrap()
            .get(&uuid)
            .map(|p| {
                p.iter()
                    .map(|(id, c)| (id.clone(), c.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Some(self.advancement_packets(true, &all, &progress))
    }

    fn send_advancement_progress(&self, uuid: Uuid, ids: &[String]) {
        if ids.is_empty() {
            return;
        }
        let all = self.advancement_progress.lock().unwrap();
        let progress: Vec<(String, FastMap<String, i64>)> = ids
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    all.get(&uuid)
                        .and_then(|p| p.get(id))
                        .cloned()
                        .unwrap_or_default(),
                )
            })
            .collect();
        drop(all);
        let packet = self.advancement_packets(false, &[], &progress);
        self.send_to(uuid, packet);
    }

    /// Загруженный с диска прогресс.
    pub(crate) fn load_advancement_progress(
        &self,
        uuid: Uuid,
        data: Vec<(String, Vec<(String, i64)>)>,
    ) {
        let map: AdvProgress = data
            .into_iter()
            .map(|(id, criteria)| (short(&id).to_owned(), criteria.into_iter().collect()))
            .collect();
        self.advancement_progress.lock().unwrap().insert(uuid, map);
    }

    /// Прогресс для `advancements/<uuid>.json`.
    pub(crate) fn collect_advancement_progress(
        &self,
        uuid: Uuid,
    ) -> Vec<(String, Vec<(String, i64)>)> {
        self.advancement_progress
            .lock()
            .unwrap()
            .get(&uuid)
            .map(|p| {
                p.iter()
                    .filter(|(_, c)| !c.is_empty())
                    .map(|(id, c)| (id.clone(), c.iter().map(|(k, &v)| (k.clone(), v)).collect()))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(crate) fn forget_advancements(&self, uuid: Uuid) {
        self.advancement_progress.lock().unwrap().remove(&uuid);
    }

    /// `/advancement grant|revoke`: список id для `everything` и потомков.
    pub(crate) fn advancement_ids(&self) -> Vec<String> {
        self.survival
            .advancements
            .iter()
            .map(|a| a.id.clone())
            .collect()
    }

    fn adv_descendants(&self, root: &str) -> Vec<String> {
        let mut out = vec![root.to_owned()];
        let mut i = 0;
        while i < out.len() {
            let parent = out[i].clone();
            for def in &self.survival.advancements {
                if def.parent.as_deref() == Some(parent.as_str()) {
                    out.push(def.id.clone());
                }
            }
            i += 1;
        }
        out
    }

    fn adv_ancestors(&self, id: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut cur = self.adv(id).and_then(|d| d.parent.clone());
        while let Some(p) = cur {
            cur = self.adv(&p).and_then(|d| d.parent.clone());
            out.push(p);
        }
        out
    }

    /// Выдать критерии достижения (`criterion = None` — все).
    pub(crate) fn advancement_grant(&self, uuid: Uuid, id: &str, criterion: Option<&str>) {
        let Some(def) = self.adv(id) else {
            return;
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64);
        let mut changed = vec![id.to_owned()];
        {
            let mut all = self.advancement_progress.lock().unwrap();
            let entry = all
                .entry(uuid)
                .or_default()
                .entry(id.to_owned())
                .or_default();
            let names: Vec<String> = match criterion {
                Some(name) => def
                    .criteria
                    .iter()
                    .filter(|c| c.name == name)
                    .map(|c| c.name.clone())
                    .collect(),
                None => def.criteria.iter().map(|c| c.name.clone()).collect(),
            };
            for name in names {
                entry.entry(name).or_insert(now);
            }
        }
        // Родительские достижения тоже нужны для видимости.
        for p in self.adv_ancestors(id) {
            changed.push(p.clone());
        }
        self.send_advancement_progress(uuid, &changed);
    }

    pub(crate) fn advancement_revoke(&self, uuid: Uuid, id: &str, criterion: Option<&str>) {
        let mut changed = vec![id.to_owned()];
        {
            let mut all = self.advancement_progress.lock().unwrap();
            let progress = all.entry(uuid).or_default();
            match criterion {
                Some(name) => {
                    if let Some(entry) = progress.get_mut(id) {
                        entry.remove(name);
                        if entry.is_empty() {
                            progress.remove(id);
                        }
                    }
                }
                None => {
                    progress.remove(id);
                }
            }
        }
        for p in self.adv_ancestors(id) {
            changed.push(p);
        }
        self.send_advancement_progress(uuid, &changed);
    }

    /// `everything`: id всех достижений.
    pub(crate) fn advancement_all(&self) -> Vec<String> {
        self.advancement_ids()
    }

    /// `from`: достижение и потомки; `through`/`until`: предки.
    pub(crate) fn advancement_from(&self, id: &str) -> Vec<String> {
        self.adv_descendants(id)
    }

    pub(crate) fn advancement_through(&self, id: &str, include: bool) -> Vec<String> {
        let mut out = self.adv_ancestors(id);
        if include {
            out.insert(0, id.to_owned());
        }
        out
    }
}
