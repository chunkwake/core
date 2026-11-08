//! NBT-шаблоны `minecraft:template` и `minecraft:fossil`.
//!
//! Постановка повторяет `StructureTemplate.placeInWorld` 26.3: палитра
//! выбирается `nextInt`, блоки идут сплошные → прочие → с NBT, каждый
//! список отсортирован по y, x, z. `structure_void` и `jigsaw` не пишутся.
//! Воздух пишется и заменяет блок мира. Лут археологии — данные
//! блок-сущности, конвейер хранит только состояния.

use super::{Bits, Fluid, Heightmap, Level, Palette, Predicate, Worldgen};
use crate::random::Legacy;
use crate::templates::Mirror;
use chunkwake_world::nbt::{self, Compound, Tag};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

/// `Rotation`: порядок `values()` — none, по часовой 90 и 180, против часовой.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Rot {
    None,
    Cw90,
    Cw180,
    Ccw90,
}

pub(crate) const ROTS: [Rot; 4] = [Rot::None, Rot::Cw90, Rot::Cw180, Rot::Ccw90];

impl Rot {
    fn parse(name: &str) -> Result<Self, String> {
        match name {
            "none" => Ok(Self::None),
            "clockwise_90" => Ok(Self::Cw90),
            "clockwise_180" => Ok(Self::Cw180),
            "counterclockwise_90" => Ok(Self::Ccw90),
            other => Err(format!("rotation {other} is not supported")),
        }
    }
}

/// Один блок палитры. `pos` — локальные координаты шаблона.
#[derive(Clone, Debug)]
struct Piece {
    pos: [i32; 3],
    state: u32,
    /// Блочная сущность из шаблона (`setBlockEntity` ставит её как есть).
    nbt: Option<chunkwake_world::nbt::Compound>,
    /// `mode=DATA` у структурного блока. Сортировку куска не меняет.
    marker: Option<String>,
}

/// Jigsaw в порядке блоков файла, до сортировки постановки.
#[derive(Clone, Debug)]
pub(crate) struct StoredJig {
    pub pos: [i32; 3],
    /// Фронт и верх после каждого поворота в порядке [`ROTS`]: сборка jigsaw
    /// перебирает кандидатов по всем поворотам, и строки на каждый перебор
    /// съедали ~80% времени камер испытаний.
    pub turned: [(&'static str, &'static str); 4],
    pub rollable: bool,
    pub name: Arc<str>,
    pub target: Arc<str>,
    pub pool: Arc<str>,
    pub placement_priority: i32,
    pub selection_priority: i32,
    pub final_state: u32,
}

/// Jigsaw после поворота: позиция мировая, фронт уже повёрнут.
#[derive(Clone, Debug)]
pub(crate) struct JigInfo {
    pub pos: [i32; 3],
    pub front: &'static str,
    pub top: &'static str,
    pub rollable: bool,
    pub name: Option<Arc<str>>,
    pub target: Arc<str>,
    pub pool: Arc<str>,
    pub placement_priority: i32,
    pub selection_priority: i32,
}

/// Разобранный `.nbt`. Каждая палитра уже в порядке `buildInfoList`.
#[derive(Clone, Debug)]
pub(crate) struct Template {
    pub(crate) size: [i32; 3],
    palettes: Vec<Vec<Piece>>,
    pub(crate) jigsaws: Vec<Vec<StoredJig>>,
}

impl Template {
    /// Пустой шаблон `StructureTemplateManager.getOrCreate`.
    pub(crate) fn empty() -> Self {
        Self {
            size: [0; 3],
            palettes: vec![vec![]],
            jigsaws: vec![vec![]],
        }
    }
}

#[derive(Clone, Debug)]
struct Weighted {
    template: Arc<Template>,
    weight: i32,
    /// Пустой список — все четыре поворота, как `Rotation.values()`.
    rotations: Vec<Rot>,
}

/// `minecraft:template`.
#[derive(Clone, Debug)]
pub struct TemplateFeature {
    templates: Vec<Weighted>,
    processors: Vec<Processor>,
}

/// `minecraft:fossil`.
#[derive(Clone, Debug)]
pub struct Fossil {
    fossil: Vec<Arc<Template>>,
    overlay: Vec<Arc<Template>>,
    fossil_processors: Vec<Processor>,
    overlay_processors: Vec<Processor>,
    max_empty: i32,
}

#[derive(Clone, Debug)]
pub(crate) enum Processor {
    /// `block_rot`: `nextFloat() > integrity` оставляет блок мира.
    Rot {
        integrity: f32,
        rottable: Option<Arc<Vec<bool>>>,
    },
    /// `protected_blocks`: не заменять блоки из набора.
    Protected(Arc<Vec<bool>>),
    /// `rule`: первое совпадение заменяет состояние. `append_loot` состояние не меняет.
    Rule(Vec<ProcRule>),
    /// `block_age`: разрушение камня, ступеней, плит, стен и обсидиана.
    Age(f32),
    /// `lava_submerged_block`: неполный блок в лаве остаётся лавой.
    LavaSubmerged,
    /// `blackstone_replace`: камень и его производные меняются на чернокамень.
    Blackstone,
    /// `block_ignore`: структурные блоки (и воздух, если пещеры нет) не ставятся.
    IgnoreStructureBlock { air_pocket: f32 },
    /// `capped`: на блок ничего не делает; в `finalizeProcessing` делегат
    /// пробует случайные блоки всего куска, пока не изменит `limit` штук.
    Capped {
        delegate: Box<Processor>,
        limit: crate::carver::IntP,
    },
}

#[derive(Clone, Debug)]
pub(crate) enum RuleInput {
    /// `always_true`.
    Always,
    Block(u32),
    State(u32),
    Tag(Arc<Vec<bool>>),
    /// `random_block_match`: `nextFloat` только если блок совпал.
    Random {
        block: u32,
        chance: f32,
    },
}

#[derive(Clone, Debug)]
pub(crate) enum RuleAt {
    Pred(Predicate),
    Block(u32),
    /// `always_true`.
    Always,
}

/// `PosRuleTest`: от расстояния между блоком и опорной точкой старта.
#[derive(Clone, Copy, Debug)]
pub(crate) enum PosTest {
    /// `always_true`.
    Always,
    /// `linear_pos`: манхэттен; `axis_aligned_linear_pos`: по одной оси.
    Linear {
        axis: Option<usize>,
        min_chance: f32,
        max_chance: f32,
        min_dist: i32,
        max_dist: i32,
    },
}

impl PosTest {
    fn parse(v: &Value) -> Result<Self, String> {
        let kind = v["predicate_type"].as_str();
        let f = |k: &str| v[k].as_f64().unwrap_or(0.0) as f32;
        let i = |k: &str| v[k].as_i64().unwrap_or(0) as i32;
        let axis = match kind {
            None | Some("minecraft:always_true") => return Ok(Self::Always),
            Some("minecraft:linear_pos") => None,
            Some("minecraft:axis_aligned_linear_pos") => Some(match v["axis"].as_str().unwrap_or("y") {
                "x" => 0,
                "z" => 2,
                _ => 1,
            }),
            Some(other) => return Err(format!("position predicate {other} is not supported")),
        };
        Ok(Self::Linear {
            axis,
            min_chance: f("min_chance"),
            max_chance: f("max_chance"),
            min_dist: i("min_dist"),
            max_dist: i("max_dist"),
        })
    }

    /// `LinearPosTest` / `AxisAlignedLinearPosTest.test`: бросок `nextFloat`
    /// против `clampedLerp(inverseLerp(dist))` во float.
    fn holds(self, world: [i32; 3], reference: [i32; 3], rng: &mut Legacy) -> bool {
        let Self::Linear {
            axis,
            min_chance,
            max_chance,
            min_dist,
            max_dist,
        } = self
        else {
            return true;
        };
        let d = |i: usize| (world[i] - reference[i]).abs();
        let dist = match axis {
            Some(a) => d(a),
            None => d(0) + d(1) + d(2),
        };
        let rnd = rng.next_float();
        let t = (dist as f32 - min_dist as f32) / (max_dist as f32 - min_dist as f32);
        let chance = if t < 0.0 {
            min_chance
        } else if t > 1.0 {
            max_chance
        } else {
            min_chance + t * (max_chance - min_chance)
        };
        rnd <= chance
    }
}

#[derive(Clone, Debug)]
pub(super) struct ProcRule {
    pub(crate) input: RuleInput,
    pub(crate) at: RuleAt,
    pub(crate) pos: PosTest,
    pub(crate) output: u32,
    /// `block_entity_modifier` `append_loot`: таблица лута блок-сущности.
    pub(crate) loot: Option<String>,
}

/// Включающие границы, как `BoundingBox.isInside`.
#[derive(Clone, Copy)]
pub(crate) struct Bounds {
    pub min: [i32; 3],
    pub max: [i32; 3],
}

impl Bounds {
    pub(crate) fn contains(self, p: [i32; 3]) -> bool {
        (0..3).all(|i| self.min[i] <= p[i] && p[i] <= self.max[i])
    }
}

impl TemplateFeature {
    pub fn parse(
        v: &Value,
        read: &super::Read<'_>,
        palette: &mut Palette,
        data_dir: &Path,
    ) -> Result<Self, String> {
        let mut templates = Vec::new();
        for entry in v["templates"].as_array().ok_or("template: no templates")? {
            let data = &entry["data"];
            let id = data["id"].as_str().ok_or("template: no id")?;
            let rotations = match data["rotations"].as_array() {
                Some(list) => list
                    .iter()
                    .map(|r| Rot::parse(r.as_str().unwrap_or("")))
                    .collect::<Result<Vec<_>, _>>()?,
                None => Vec::new(),
            };
            templates.push(Weighted {
                template: Arc::new(load_template(data_dir, palette, id)?),
                weight: entry["weight"].as_i64().unwrap_or(1) as i32,
                rotations,
            });
        }
        if templates.is_empty() {
            return Err("template: empty list".into());
        }
        let processors = match v.get("processors") {
            Some(p) if !p.is_null() => parse_processors(p, read, palette)?,
            _ => Vec::new(),
        };
        Ok(Self {
            templates,
            processors,
        })
    }

    pub fn place(&self, pal: &Palette, level: &mut dyn Level, r: &mut Worldgen, origin: [i32; 3]) {
        let total: i32 = self.templates.iter().map(|e| e.weight).sum();
        if total <= 0 {
            return;
        }
        // `WeightedList`: `nextInt(totalWeight)`, затем вычитание весов.
        let mut roll = r.next_int_bound(total);
        let Some(entry) = self.templates.iter().find(|e| {
            roll -= e.weight;
            roll < 0
        }) else {
            return;
        };
        let rots: &[Rot] = if entry.rotations.is_empty() {
            &ROTS
        } else {
            &entry.rotations
        };
        let rot = rots[r.next_int_bound(rots.len() as i32) as usize];
        let (sx, _, sz) = (
            entry.template.size[0],
            entry.template.size[1],
            entry.template.size[2],
        );
        let (dx, dz) = center_offset(rot, sx, sz);
        let place = [origin[0] + dx, origin[1], origin[2] + dz];
        place_in_world(
            pal,
            level,
            &entry.template,
            rot,
            place,
            &self.processors,
            r,
            None,
        );
    }
}

impl Fossil {
    pub fn parse(
        v: &Value,
        read: &super::Read<'_>,
        palette: &mut Palette,
        data_dir: &Path,
    ) -> Result<Self, String> {
        let load_list = |key: &str| -> Result<Vec<Arc<Template>>, String> {
            v[key]
                .as_array()
                .ok_or_else(|| format!("fossil: no {key}"))?
                .iter()
                .map(|id| {
                    let id = id.as_str().ok_or("fossil: structure id")?;
                    load_template(data_dir, palette, id).map(Arc::new)
                })
                .collect()
        };
        let fossil = load_list("fossil_structures")?;
        let overlay = load_list("overlay_structures")?;
        if fossil.is_empty() || fossil.len() != overlay.len() {
            return Err("fossil: structure lists differ".into());
        }
        Ok(Self {
            fossil,
            overlay,
            fossil_processors: parse_processors(&v["fossil_processors"], read, palette)?,
            overlay_processors: parse_processors(&v["overlay_processors"], read, palette)?,
            max_empty: v["max_empty_corners_allowed"].as_i64().unwrap_or(0) as i32,
        })
    }

    pub fn place(&self, pal: &Palette, level: &mut dyn Level, r: &mut Worldgen, origin: [i32; 3]) {
        let rot = ROTS[r.next_int_bound(ROTS.len() as i32) as usize];
        let index = r.next_int_bound(self.fossil.len() as i32) as usize;
        let fossil = &self.fossil[index];
        let overlay = &self.overlay[index];
        let size = rotated_size(fossil.size, rot);
        // `(-size) / 2` в Java для положительного размера совпадает с `-(size / 2)`.
        let corner = [origin[0] - size[0] / 2, origin[1], origin[2] - size[2] / 2];
        let mut floor = origin[1];
        for x in 0..size[0] {
            for z in 0..size[2] {
                let h = level.surface(Heightmap::OceanFloorWg, corner[0] + x, corner[2] + z);
                floor = floor.min(h);
            }
        }
        let place_y = (floor - 15 - r.next_int_bound(10)).max(level.min_y() + 10);
        let place = zero_position([corner[0], place_y, corner[2]], rot, fossil.size);
        let footprint = template_bounds(place, rot, fossil.size);
        if empty_corners(pal, level, footprint) > self.max_empty {
            return;
        }
        let window = chunk_window(level, origin);
        place_in_world(
            pal,
            level,
            fossil,
            rot,
            place,
            &self.fossil_processors,
            r,
            Some(window),
        );
        place_in_world(
            pal,
            level,
            overlay,
            rot,
            place,
            &self.overlay_processors,
            r,
            Some(window),
        );
    }
}

pub(crate) fn parse_processors(
    v: &Value,
    read: &super::Read<'_>,
    palette: &mut Palette,
) -> Result<Vec<Processor>, String> {
    let owned;
    let v = match v {
        Value::String(name) => {
            owned = read("processor_list", name)?;
            &owned
        }
        other => other,
    };
    let mut out = Vec::new();
    for p in v["processors"].as_array().into_iter().flatten() {
        match p["processor_type"].as_str().unwrap_or("") {
            "minecraft:block_rot" => {
                let rottable = match p.get("rottable_blocks") {
                    Some(blocks) if !blocks.is_null() => Some(super::block_set(blocks, palette)?),
                    _ => None,
                };
                out.push(Processor::Rot {
                    integrity: p["integrity"].as_f64().unwrap_or(1.0) as f32,
                    rottable,
                });
            }
            "minecraft:protected_blocks" => {
                out.push(Processor::Protected(super::block_set(
                    &p["value"],
                    palette,
                )?));
            }
            "minecraft:rule" => {
                let mut rules = Vec::new();
                for rule in p["rules"].as_array().into_iter().flatten() {
                    let input = &rule["input_predicate"];
                    let kind = predicate_kind(input);
                    let input = match kind {
                        "minecraft:always_true" => RuleInput::Always,
                        "minecraft:block_match" => {
                            RuleInput::Block(palette.block(input["block"].as_str().unwrap_or(""))?)
                        }
                        "minecraft:blockstate_match" => {
                            RuleInput::State(palette.state(&input["block_state"])?)
                        }
                        "minecraft:tag_match" => {
                            RuleInput::Tag(palette.tag(input["tag"].as_str().unwrap_or(""))?)
                        }
                        "minecraft:random_block_match" => RuleInput::Random {
                            block: palette.block(input["block"].as_str().unwrap_or(""))?,
                            chance: input["probability"].as_f64().unwrap_or(0.0) as f32,
                        },
                        other => {
                            return Err(format!("processor rule {other} is not supported"));
                        }
                    };
                    let loc = &rule["location_predicate"];
                    let at = if predicate_kind(loc) == "minecraft:block_match" {
                        RuleAt::Block(palette.block(loc["block"].as_str().unwrap_or(""))?)
                    } else if predicate_kind(loc) == "minecraft:always_true" {
                        RuleAt::Always
                    } else {
                        RuleAt::Pred(Predicate::parse(loc, palette)?)
                    };
                    let modifier = &rule["block_entity_modifier"];
                    let loot = match modifier["type"].as_str() {
                        None | Some("minecraft:passthrough") => None,
                        Some("minecraft:append_loot") => {
                            Some(modifier["loot_table"].as_str().unwrap_or("").to_owned())
                        }
                        Some(other) => {
                            return Err(format!("block entity modifier {other} is not supported"));
                        }
                    };
                    rules.push(ProcRule {
                        input,
                        at,
                        pos: PosTest::parse(&rule["position_predicate"])?,
                        output: palette.state(&rule["output_state"])?,
                        loot,
                    });
                }
                out.push(Processor::Rule(rules));
            }
            "minecraft:capped" => {
                let one = serde_json::json!({ "processors": [p["delegate"].clone()] });
                let delegate = parse_processors(&one, read, palette)?
                    .pop()
                    .ok_or("capped: no delegate")?;
                out.push(Processor::Capped {
                    delegate: Box::new(delegate),
                    limit: crate::carver::IntP::parse(&p["limit"])?,
                });
            }
            other => return Err(format!("processor {other} is not supported")),
        }
    }
    Ok(out)
}

pub(crate) fn load_template(
    data_dir: &Path,
    palette: &Palette,
    id: &str,
) -> Result<Template, String> {
    let path = id.strip_prefix("minecraft:").unwrap_or(id);
    let file = data_dir
        .join("data/minecraft/structure")
        .join(path)
        .with_extension("nbt");
    // `getOrCreate`: шаблона нет — пустой, без блоков и стыков. Так в 26.3
    // пул `ancient_city/walls/no_corners` ссылается на несуществующий файл.
    let Some(root) = nbt::read_gzip_file(&file)? else {
        return Ok(Template {
            size: [0; 3],
            palettes: vec![Vec::new()],
            jigsaws: vec![Vec::new()],
        });
    };
    let size = int3(root.get("size").ok_or_else(|| format!("{id}: no size"))?)
        .ok_or_else(|| format!("{id}: size"))?;
    let blocks = root
        .get("blocks")
        .and_then(Tag::as_list)
        .ok_or_else(|| format!("{id}: no blocks"))?;
    let palettes = palette_lists(&root).map_err(|e| format!("{id}: {e}"))?;
    let jigsaw = palette.blocks.get("minecraft:jigsaw").map(|b| b.0);
    let air = palette
        .state(&Value::String("minecraft:air".into()))
        .unwrap_or(0);
    let mut out = Vec::with_capacity(palettes.len());
    let mut jigsaws = Vec::with_capacity(palettes.len());
    for entries in &palettes {
        let mut raw = Vec::with_capacity(blocks.len());
        let mut jigs = Vec::new();
        for block in blocks {
            let block = block.as_compound().ok_or_else(|| format!("{id}: block"))?;
            let pos = int3(block.get("pos").ok_or_else(|| format!("{id}: pos"))?)
                .ok_or_else(|| format!("{id}: pos"))?;
            let index = block
                .get("state")
                .and_then(Tag::as_i64)
                .ok_or_else(|| format!("{id}: state"))? as usize;
            let entry = entries
                .get(index)
                .ok_or_else(|| format!("{id}: state {index}"))?;
            let state = state_of(palette, entry).map_err(|e| format!("{id}: {e}"))?;
            if jigsaw.is_some_and(|id| palette.block_index(state) == Some(id)) {
                jigs.push(
                    stored_jig(palette, block, pos, state, air)
                        .map_err(|e| format!("{id}: {e}"))?,
                );
            }
            raw.push(Piece {
                pos,
                state,
                nbt: block
                    .get("nbt")
                    .and_then(chunkwake_world::nbt::Tag::as_compound)
                    .cloned(),
                marker: data_marker(block),
            });
        }
        out.push(sort_pieces(palette, raw));
        jigsaws.push(jigs);
    }
    Ok(Template {
        size,
        palettes: out,
        jigsaws,
    })
}

/// Публичная загрузка шаблона для `/place template`.
pub(crate) fn load_public(
    data_dir: &Path,
    palette: &Palette,
    id: &str,
) -> Result<Template, String> {
    // `/place template` берёт `StructureTemplateManager.get`: нет файла — ошибка,
    // а не пустой шаблон `getOrCreate`, как у пулов генерации.
    let path = id.strip_prefix("minecraft:").unwrap_or(id);
    let file = data_dir
        .join("data/minecraft/structure")
        .join(path)
        .with_extension("nbt");
    if !file.is_file() {
        return Err(format!("{id}: no such template"));
    }
    load_template(data_dir, palette, id)
}

/// Размер шаблона до поворота.
pub(crate) fn size_public(
    data_dir: &Path,
    palette: &Palette,
    id: &str,
) -> Result<[i32; 3], String> {
    Ok(load_public(data_dir, palette, id)?.size)
}

/// Блоки шаблона в мировых координатах: без процессоров, маркеров и воды.
///
/// `origin` — угол постройки, как `pos` у `/place template`: поворот идёт
/// вокруг опоры `(0,0,0)`, поэтому при `clockwise_90` постройка уходит
/// в отрицательный Z, ровно как в ванили.
pub(crate) fn place_public(
    pal: &Palette,
    template: &Template,
    origin: [i32; 3],
    rotation: crate::templates::Rotation,
    mirror: crate::templates::Mirror,
    ignore_air: bool,
) -> Vec<crate::templates::Placed> {
    use crate::templates::{Mirror, Placed, Rotation};
    let rot = match rotation {
        Rotation::None => Rot::None,
        Rotation::Clockwise90 => Rot::Cw90,
        Rotation::Clockwise180 => Rot::Cw180,
        Rotation::CounterClockwise90 => Rot::Ccw90,
    };
    let air = pal.blocks.get("minecraft:air").map(|b| b.0);
    let mut out = Vec::new();
    // `break` в конце и раньше: цикл по палитрам по сути однократный.
    if let Some(pieces) = template.palettes.first() {
        for piece in pieces {
            let [x, y, z] = piece.pos;
            // `StructureTemplate.transform`: зеркало, затем поворот вокруг нуля.
            let (x, z) = match mirror {
                Mirror::None => (x, z),
                Mirror::LeftRight => (x, -z),
                Mirror::FrontBack => (-x, z),
            };
            let (x, z) = match rot {
                Rot::None => (x, z),
                Rot::Cw90 => (-z, x),
                Rot::Cw180 => (-x, -z),
                Rot::Ccw90 => (z, -x),
            };
            if ignore_air && air.is_some_and(|id| pal.block_index(piece.state) == Some(id)) {
                continue;
            }
            if skipped(pal, piece.state) {
                continue;
            }
            let state = mirror_state(pal, piece.state, mirror, rot);
            out.push(Placed {
                pos: [origin[0] + x, origin[1] + y, origin[2] + z],
                state,
                has_nbt: piece.nbt.is_some(),
            });
        }
    }
    out
}

/// `BlockState.mirror`, затем `BlockState.rotate`.
fn mirror_state(pal: &Palette, state: u32, mirror: crate::templates::Mirror, rot: Rot) -> u32 {
    use crate::templates::Mirror;
    let mut state = match mirror {
        Mirror::None => state,
        Mirror::LeftRight | Mirror::FrontBack => mirror_block(pal, state, mirror),
    };
    state = rotate_state(pal, state, rot);
    state
}

/// Зеркало блока: горизонтальный `facing` и `rotation`.
///
/// `LEFT_RIGHT` отражает Z (`north` ↔ `south`), `FRONT_BACK` — X
/// (`east` ↔ `west`); вертикальный `facing` и `axis` не меняются.
fn mirror_block(pal: &Palette, state: u32, mirror: crate::templates::Mirror) -> u32 {
    use crate::templates::Mirror;
    let mut state = state;
    let flipped = |value: &str| -> Option<&'static str> {
        match (mirror, value) {
            (Mirror::LeftRight, "north") => Some("south"),
            (Mirror::LeftRight, "south") => Some("north"),
            (Mirror::FrontBack, "east") => Some("west"),
            (Mirror::FrontBack, "west") => Some("east"),
            _ => None,
        }
    };
    for key in ["facing", "horizontal_facing"] {
        let Some(value) = pal.prop(state, key) else {
            continue;
        };
        if let Some(turned) = flipped(value)
            && let Some(next) = pal.with_prop(state, key, turned)
        {
            state = next;
        }
    }
    if let Some(value) = pal.prop(state, "rotation")
        && let Ok(n) = value.parse::<i32>()
    {
        let turned = ((16 - n) & 15).to_string();
        if let Some(next) = pal.with_prop(state, "rotation", &turned) {
            state = next;
        }
    }
    state
}

fn stored_jig(
    palette: &Palette,
    block: &Compound,
    pos: [i32; 3],
    state: u32,
    air: u32,
) -> Result<StoredJig, String> {
    let nbt = block.get("nbt").and_then(Tag::as_compound);
    let text = |key: &str, default: &str| {
        nbt.and_then(|n| n.get(key))
            .and_then(Tag::as_str)
            .unwrap_or(default)
            .to_owned()
    };
    let ident = |key: &str| mc_id(&text(key, "minecraft:empty"));
    let orientation = palette
        .prop(state, "orientation")
        .unwrap_or("north_up")
        .to_owned();
    let (front, _) = orientation.split_once('_').unwrap_or(("north", "up"));
    let horizontal = matches!(front, "north" | "south" | "west" | "east");
    let rollable = match nbt.and_then(|n| n.get("joint")).and_then(Tag::as_str) {
        Some("rollable") => true,
        Some("aligned") => false,
        // `getDefaultJointType`: горизонтальный фронт — aligned, иначе rollable.
        _ => !horizontal,
    };
    let final_state = match nbt.and_then(|n| n.get("final_state")) {
        Some(tag) => final_state(palette, tag)?,
        None => air,
    };
    let turned = ROTS.map(|rot| {
        let o = rotate_orientation(&orientation, rot).unwrap_or_else(|| orientation.clone());
        let (front, top) = o.split_once('_').unwrap_or(("north", "up"));
        (direction(front), direction(top))
    });
    Ok(StoredJig {
        pos,
        turned,
        rollable,
        name: ident("name").into(),
        target: ident("target").into(),
        pool: ident("pool").into(),
        placement_priority: nbt
            .and_then(|n| n.get("placement_priority"))
            .and_then(Tag::as_i64)
            .unwrap_or(0) as i32,
        selection_priority: nbt
            .and_then(|n| n.get("selection_priority"))
            .and_then(Tag::as_i64)
            .unwrap_or(0) as i32,
        final_state,
    })
}

fn mc_id(name: &str) -> String {
    if name.contains(':') {
        name.to_owned()
    } else {
        format!("minecraft:{name}")
    }
}

fn final_state(palette: &Palette, tag: &Tag) -> Result<u32, String> {
    let text = tag.as_str().ok_or("jigsaw final_state")?;
    let props = match text.split_once('[') {
        Some((name, rest)) => {
            let rest = rest.trim_end_matches(']');
            let mut map = serde_json::Map::new();
            for part in rest.split(',') {
                let Some((k, v)) = part.split_once('=') else {
                    continue;
                };
                map.insert(k.to_owned(), Value::String(v.to_owned()));
            }
            let mut body = serde_json::Map::new();
            body.insert("id".into(), Value::String(name.to_owned()));
            body.insert("properties".into(), Value::Object(map));
            Value::Object(body)
        }
        None => Value::String(text.to_owned()),
    };
    palette.state(&props)
}

fn palette_lists(root: &Compound) -> Result<Vec<Vec<&Compound>>, String> {
    if let Some(Tag::List(groups)) = root.get("palettes") {
        let mut out = Vec::new();
        for group in groups {
            let mut entries = Vec::new();
            for entry in group.as_list().ok_or("palettes entry")? {
                entries.push(entry.as_compound().ok_or("palette entry")?);
            }
            out.push(entries);
        }
        if !out.is_empty() {
            return Ok(out);
        }
    }
    let list = root
        .get("palette")
        .and_then(Tag::as_list)
        .ok_or("no palette")?;
    let mut entries = Vec::new();
    for entry in list {
        entries.push(entry.as_compound().ok_or("palette entry")?);
    }
    Ok(vec![entries])
}

/// `id`/`Name` и `properties`/`Properties`, как `NbtUtils.readBlockState` в 26.3.
fn state_of(palette: &Palette, entry: &Compound) -> Result<u32, String> {
    let id = entry
        .get("id")
        .or_else(|| entry.get("Name"))
        .and_then(Tag::as_str)
        .ok_or("palette: no id")?;
    let props = entry
        .get("properties")
        .or_else(|| entry.get("Properties"))
        .and_then(Tag::as_compound);
    let Some(props) = props.filter(|p| !p.is_empty()) else {
        return palette.state(&Value::String(id.to_owned()));
    };
    let mut map = serde_json::Map::new();
    for (key, value) in props {
        let text = value
            .as_str()
            .map(str::to_owned)
            .or_else(|| value.as_i64().map(|n| n.to_string()))
            .ok_or_else(|| format!("property {key}"))?;
        map.insert(key.clone(), Value::String(text));
    }
    palette.state(&serde_json::json!({ "id": id, "properties": map }))
}

/// `addToLists` + `buildInfoList`: сплошные, затем прочие, затем с NBT.
/// Внутри группы порядок y, x, z. Полный куб коллизии — прокси
/// `isCollisionShapeFullBlock && !hasDynamicShape`: у блоков этих шаблонов
/// динамическая форма как раз не занимает куб.
fn sort_pieces(palette: &Palette, raw: Vec<Piece>) -> Vec<Piece> {
    let mut full = Vec::new();
    let mut other = Vec::new();
    let mut with_nbt = Vec::new();
    for piece in raw {
        if piece.nbt.is_some() {
            with_nbt.push(piece);
        } else if palette.is_full_cube(piece.state) {
            full.push(piece);
        } else {
            other.push(piece);
        }
    }
    let cmp = |a: &Piece, b: &Piece| {
        a.pos[1]
            .cmp(&b.pos[1])
            .then(a.pos[0].cmp(&b.pos[0]))
            .then(a.pos[2].cmp(&b.pos[2]))
    };
    full.sort_by(cmp);
    other.sort_by(cmp);
    with_nbt.sort_by(cmp);
    full.append(&mut other);
    full.append(&mut with_nbt);
    full
}

fn int3(tag: &Tag) -> Option<[i32; 3]> {
    if let Some(values) = tag.as_int_array()
        && values.len() >= 3
    {
        return Some([values[0], values[1], values[2]]);
    }
    let list = tag.as_list()?;
    if list.len() < 3 {
        return None;
    }
    Some([
        list[0].as_i64()? as i32,
        list[1].as_i64()? as i32,
        list[2].as_i64()? as i32,
    ])
}

/// `getRotatedOffset` по осям X и Z: отрицательное направление оси поворачивается
/// и умножается на `size / 2` неповёрнутого шаблона.
fn center_offset(rot: Rot, sx: i32, sz: i32) -> (i32, i32) {
    let hx = sx / 2;
    let hz = sz / 2;
    match rot {
        Rot::None => (-hx, -hz),
        Rot::Cw90 => (hz, -hx),
        Rot::Cw180 => (hx, hz),
        Rot::Ccw90 => (-hz, hx),
    }
}

fn rotated_size(size: [i32; 3], rot: Rot) -> [i32; 3] {
    match rot {
        Rot::Cw90 | Rot::Ccw90 => [size[2], size[1], size[0]],
        Rot::None | Rot::Cw180 => size,
    }
}

/// `getZeroPositionWithTransform` при зеркале `NONE`. Размеры — сырые, до поворота.
fn zero_position(corner: [i32; 3], rot: Rot, size: [i32; 3]) -> [i32; 3] {
    let sx = size[0] - 1;
    let sz = size[2] - 1;
    let (dx, dz) = match rot {
        Rot::None => (0, 0),
        Rot::Cw90 => (sz, 0),
        Rot::Cw180 => (sx, sz),
        Rot::Ccw90 => (0, sx),
    };
    [corner[0] + dx, corner[1], corner[2] + dz]
}

/// `StructureTemplate.transform` при опорной точке ноль и без зеркала.
pub(crate) fn transform(rot: Rot, x: i32, y: i32, z: i32) -> [i32; 3] {
    match rot {
        Rot::None => [x, y, z],
        Rot::Cw90 => [-z, y, x],
        Rot::Cw180 => [-x, y, -z],
        Rot::Ccw90 => [z, y, -x],
    }
}

pub(crate) fn template_bounds(place: [i32; 3], rot: Rot, size: [i32; 3]) -> Bounds {
    let end = transform(rot, size[0] - 1, size[1] - 1, size[2] - 1);
    let mut min = [0; 3];
    let mut max = [0; 3];
    for i in 0..3 {
        min[i] = place[i] + end[i].min(0);
        max[i] = place[i] + end[i].max(0);
    }
    Bounds { min, max }
}

/// `StructureTemplate.transform` без зеркала. Опора входит в формулу как есть.
pub(crate) fn transform_pivot(rot: Rot, x: i32, y: i32, z: i32, pivot: [i32; 3]) -> [i32; 3] {
    let (px, pz) = (pivot[0], pivot[2]);
    match rot {
        Rot::None => [x, y, z],
        Rot::Cw180 => [px + px - x, y, pz + pz - z],
        Rot::Ccw90 => [px - pz + z, y, px + pz - x],
        Rot::Cw90 => [px + pz - z, y, pz - px + x],
    }
}

/// Коробка от углов `(0,0,0)` и `size − 1` после поворота вокруг опоры.
pub(crate) fn pivoted_bounds(
    origin: [i32; 3],
    rot: Rot,
    pivot: [i32; 3],
    size: [i32; 3],
) -> ([i32; 3], [i32; 3]) {
    let a = transform_pivot(rot, 0, 0, 0, pivot);
    let b = transform_pivot(rot, size[0] - 1, size[1] - 1, size[2] - 1, pivot);
    let mut min = [0; 3];
    let mut max = [0; 3];
    for i in 0..3 {
        min[i] = origin[i] + a[i].min(b[i]);
        max[i] = origin[i] + a[i].max(b[i]);
    }
    (min, max)
}

/// Шаблон иглу или корабля. `ignore_air` — не затирать мир воздухом.
/// `waterlog_blocks` — `APPLY_WATERLOGGING`. Маркеры `DATA` возвращаются
/// все, в том числе вне окна: сундук внутри окна крутит `nextLong`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn place_anchored(
    pal: &Palette,
    level: &mut dyn Level,
    template: &Template,
    rot: Rot,
    pivot: [i32; 3],
    origin: [i32; 3],
    ignore_air: bool,
    waterlog_blocks: bool,
    r: &mut Worldgen,
    window: Bounds,
) -> Vec<(String, [i32; 3])> {
    if template.palettes.is_empty() {
        return Vec::new();
    }
    let index = palette_index(template.palettes.len(), origin);
    let Some(blocks) = template.palettes.get(index) else {
        return Vec::new();
    };
    let air = pal.blocks.get("minecraft:air").map(|b| b.0);
    let structure_block = pal.blocks.get("minecraft:structure_block").map(|b| b.0);
    let mut markers = Vec::new();
    let mut placed = Vec::new();
    let mut to_fill = Vec::new();
    let mut locked = HashSet::new();
    for piece in blocks {
        let shifted = transform_pivot(rot, piece.pos[0], piece.pos[1], piece.pos[2], pivot);
        let world = [
            origin[0] + shifted[0],
            origin[1] + shifted[1],
            origin[2] + shifted[2],
        ];
        if let Some(meta) = &piece.marker {
            markers.push((meta.clone(), world));
        }
        if !window.contains(world) || !level.can_write(world[0], world[2]) {
            continue;
        }
        let block = pal.block_index(piece.state);
        if ignore_air && air.is_some_and(|id| block == Some(id)) {
            continue;
        }
        if structure_block.is_some_and(|id| block == Some(id)) {
            continue;
        }
        if skipped(pal, piece.state) {
            continue;
        }
        let rotated = rotate_state(pal, piece.state, rot);
        let previous = level.get(world[0], world[1], world[2]);
        let mut state = rotated;
        let mut fill_later = false;
        if waterlog_blocks {
            let template_fluid = pal.fluid(rotated);
            if template_fluid == Fluid::Water || template_fluid == Fluid::Lava {
                locked.insert((world[0], world[1], world[2]));
            } else if pal.prop(rotated, "waterlogged").is_some() {
                // `LiquidBlockContainer.placeLiquid`: только источник воды.
                // Течение и воздух ждут соседний источник.
                if pal.fluid(previous) == Fluid::Water {
                    if let Some(next) = pal.with_prop(rotated, "waterlogged", "true") {
                        state = next;
                    }
                } else {
                    fill_later = true;
                }
            }
        }
        level.set(world[0], world[1], world[2], state);
        placed.push(world);
        if fill_later {
            to_fill.push(world);
        }
        // `placeInWorld`: NBT шаблона. У иглу и корабля сундук-мишень лова
        // помечен `structure_block`, а NBT-сундук здесь — обычный контейнер
        // с готовыми `Items`; `nextLong` при этом всё равно бросается.
        if let Some(nbt) = &piece.nbt {
            place_template_nbt(pal, level, world, state, nbt, r);
        }
    }
    if waterlog_blocks {
        spread_water(pal, level, &mut to_fill, &locked);
    }
    // `getKnownShape` у корабля и иглу выключен: форма лестницы и связи
    // забора считаются заново. В шаблоне лестница — заглушка `outer_right`.
    settle_stairs(pal, level, &placed);
    settle_fences(pal, level, &placed);
    markers
}

/// `StructureTemplate.transform`: зеркало, затем поворот вокруг опоры.
pub(crate) fn transform_mirror(
    mirror: Mirror,
    rot: Rot,
    x: i32,
    y: i32,
    z: i32,
    pivot: [i32; 3],
) -> [i32; 3] {
    let (x, z) = match mirror {
        Mirror::None => (x, z),
        Mirror::LeftRight => (x, -z),
        Mirror::FrontBack => (-x, z),
    };
    transform_pivot(rot, x, y, z, pivot)
}

/// `StructureTemplate.getBoundingBox` для шаблона с зеркалом и поворотом.
pub(crate) fn mirrored_bounds(
    origin: [i32; 3],
    mirror: Mirror,
    rot: Rot,
    size: [i32; 3],
) -> ([i32; 3], [i32; 3]) {
    let mut min = [0; 3];
    let mut max = [0; 3];
    let a = transform_mirror(mirror, rot, 0, 0, 0, [0, 0, 0]);
    let b = transform_mirror(
        mirror,
        rot,
        size[0] - 1,
        size[1] - 1,
        size[2] - 1,
        [0, 0, 0],
    );
    for i in 0..3 {
        let (lo, hi) = (a[i].min(b[i]), a[i].max(b[i]));
        min[i] = origin[i] + lo;
        max[i] = origin[i] + hi;
    }
    (min, max)
}

/// `getZeroPositionWithTransform(zero, mirror, rotation, sizeX, sizeZ)`.
pub(crate) fn zero_with_transform(
    zero: [i32; 3],
    mirror: Mirror,
    rot: Rot,
    size_x: i32,
    size_z: i32,
) -> [i32; 3] {
    let size_x = size_x - 1;
    let size_z = size_z - 1;
    let dx = if mirror == Mirror::FrontBack {
        size_x
    } else {
        0
    };
    let dz = if mirror == Mirror::LeftRight {
        size_z
    } else {
        0
    };
    let (ox, oz) = match rot {
        Rot::None => (dx, dz),
        Rot::Cw90 => (size_z - dz, dx),
        Rot::Cw180 => (size_x - dx, size_z - dz),
        Rot::Ccw90 => (dz, size_x - dx),
    };
    [zero[0] + ox, zero[1], zero[2] + oz]
}

/// Куски океанской руины: `block_rot`, `block_ignore` и подозрительный блок.
#[allow(clippy::too_many_arguments)]
pub(crate) fn place_ruin(
    pal: &Palette,
    level: &mut dyn Level,
    template: &Template,
    origin: [i32; 3],
    rot: Rot,
    integrity: f32,
    warm: bool,
    window: Bounds,
    world_seed: i64,
) -> Vec<([i32; 3], Option<String>)> {
    if template.palettes.is_empty() {
        return Vec::new();
    }
    let index = palette_index(template.palettes.len(), origin);
    let Some(blocks) = template.palettes.get(index) else {
        return Vec::new();
    };
    let structure_block = pal.blocks.get("minecraft:structure_block").map(|b| b.0);
    // `CappedProcessor.finalizeProcessing`: `RandomSource.createThreadLocalInstance(seed)`
    // — это `SingleThreadedRandomSource`, то есть `Legacy`, а не Xoroshiro.
    // `forkPositional` у него даёт фабрику от одного `nextLong`.
    let mut cap = {
        let mut base = Legacy::new(world_seed);
        let positional = base.next_long();
        Legacy::new(palette_seed(origin[0], origin[1], origin[2]) ^ positional)
    };
    let from = if warm {
        pal.blocks["minecraft:sand"].0
    } else {
        pal.blocks["minecraft:gravel"].0
    };
    let to = if warm {
        pal.defaults["minecraft:suspicious_sand"]
    } else {
        pal.defaults["minecraft:suspicious_gravel"]
    };

    // Первый проход: `block_rot` и `block_ignore`. `CappedProcessor`
    // объявляет `evaluatesEntirePieceState`, поэтому `processBlockInfos`
    // прогоняет процессоры по всему куску, а не только по текущему чанку:
    // от этого зависят индексы в `finalizeProcessing`.
    struct Candidate {
        world: [i32; 3],
        state: u32,
    }
    // `BlockIgnoreProcessor` в `processBlockInfos` возвращает `null` для
    // структурных блоков: в список обработки они не попадают, иначе
    // разъехались бы индексы `finalizeProcessing`.
    let mut markers: Vec<([i32; 3], String)> = Vec::new();
    let mut kept: Vec<Candidate> = Vec::new();
    let mut placed = Vec::new();
    for piece in blocks {
        let shifted = transform(rot, piece.pos[0], piece.pos[1], piece.pos[2]);
        let world = [
            origin[0] + shifted[0],
            origin[1] + shifted[1],
            origin[2] + shifted[2],
        ];
        let block = pal.block_index(piece.state);
        if structure_block.is_some_and(|id| block == Some(id)) {
            if let Some(meta) = &piece.marker {
                markers.push((world, meta.clone()));
            }
            continue;
        }
        // `BlockIgnoreProcessor.STRUCTURE_AND_AIR`.
        if pal.is_air(piece.state) {
            continue;
        }
        // `BlockRotProcessor`: свой `Legacy` от мировой позиции.
        let roll = Legacy::new(palette_seed(world[0], world[1], world[2])).next_float();
        if roll > integrity {
            continue;
        }
        kept.push(Candidate {
            world,
            state: piece.state,
        });
    }

    // `CappedProcessor.finalizeProcessing`: перемешать индексы и заменить до пяти.
    let mut order: Vec<usize> = (0..kept.len()).collect();
    shuffle(&mut order, &mut cap);
    let table = if warm {
        "minecraft:archaeology/ocean_ruin_warm"
    } else {
        "minecraft:archaeology/ocean_ruin_cold"
    };
    let mut loot: Vec<Option<i64>> = vec![None; kept.len()];
    let mut replaced = 0;
    for &i in &order {
        if replaced >= 5 {
            break;
        }
        if pal.block_index(kept[i].state) != Some(from) {
            continue;
        }
        kept[i].state = to;
        // `AppendLoot.apply`: `LootTable` плюс `nextLong` от `RuleProcessor`,
        // у которого свой `RandomSource.create(Mth.getSeed(pos))` на блок.
        let mut rule_rng = Legacy::new(palette_seed(
            kept[i].world[0],
            kept[i].world[1],
            kept[i].world[2],
        ));
        loot[i] = Some(rule_rng.next_long());
        replaced += 1;
    }

    let mut out = Vec::new();
    for (i, item) in kept.into_iter().enumerate() {
        let inside = window.contains(item.world);
        if !inside || !level.can_write(item.world[0], item.world[2]) {
            continue;
        }
        let state = rotate_state(pal, item.state, rot);
        level.set(item.world[0], item.world[1], item.world[2], state);
        placed.push(item.world);
        if let Some(seed) = loot[i] {
            let mut nbt = chunkwake_world::nbt::Compound::new();
            nbt.insert(
                "LootTable".into(),
                chunkwake_world::nbt::Tag::String(table.to_owned()),
            );
            nbt.insert(
                "LootTableSeed".into(),
                chunkwake_world::nbt::Tag::Long(seed),
            );
            level.set_block_entity(
                item.world[0],
                item.world[1],
                item.world[2],
                "minecraft:brushable_block",
                nbt,
            );
        }
    }
    // Маркеры куска идут в `handleDataMarker`, а не в блоки.
    for (world, meta) in markers {
        if window.contains(world) {
            out.push((world, Some(meta)));
        }
    }
    settle_stairs(pal, level, &placed);
    settle_fences(pal, level, &placed);
    out
}

/// `StructureTemplate.placeInWorld` с зеркалом и процессорами.
#[allow(clippy::too_many_arguments)]
pub(crate) fn place_with(
    pal: &Palette,
    level: &mut dyn Level,
    template: &Template,
    origin: [i32; 3],
    pivot: [i32; 3],
    mirror: Mirror,
    rot: Rot,
    processors: &[Processor],
    r: &mut Worldgen,
    window: Bounds,
) {
    if template.palettes.is_empty() {
        return;
    }
    let index = palette_index(template.palettes.len(), origin);
    let Some(blocks) = template.palettes.get(index) else {
        return;
    };
    let mut placed = Vec::new();
    for piece in blocks {
        let shifted =
            transform_mirror(mirror, rot, piece.pos[0], piece.pos[1], piece.pos[2], pivot);
        let world = [
            origin[0] + shifted[0],
            origin[1] + shifted[1],
            origin[2] + shifted[2],
        ];
        if !window.contains(world) || !level.can_write(world[0], world[2]) {
            continue;
        }
        let mut state = Some(piece.state);
        for processor in processors {
            let Some(current) = state else { break };
            // Процессоры видят состояние до зеркала и поворота. Проверок по
            // опорной точке у этих списков нет.
            state = processor.apply(pal, level, current, world, world, r, true);
        }
        let Some(state) = state else { continue };
        let state = mirror_state(pal, state, mirror, rot);
        if skipped(pal, state) || !level.can_write(world[0], world[2]) {
            continue;
        }
        // LiquidSettings.APPLY_WATERLOGGING: вода мира заливает проходимые блоки.
        level.set(
            world[0],
            world[1],
            world[2],
            waterlog(pal, level, world, state),
        );
        placed.push(world);
        if let Some(block) = pal.block_index(state)
            && pal
                .blocks
                .iter()
                .any(|(name, (id, _))| *id == block && name.ends_with("_wall"))
        {
            level.mark_shape(world[0], world[1], world[2]);
        }
        // `placeInWorld`: NBT шаблона. У руин портала таблица приходит из
        // шаблона, а семя — общий `random.nextLong()` фичи.
        if let Some(nbt) = &piece.nbt {
            place_template_nbt(pal, level, world, state, nbt, r);
        }
    }
    settle_stairs(pal, level, &placed);
    settle_fences(pal, level, &placed);
}

/// `getBoundingBox(position, rotation, pivot, mirror, size)`.
pub(crate) fn mirrored_bounds_pivot(
    origin: [i32; 3],
    mirror: Mirror,
    rot: Rot,
    size: [i32; 3],
    pivot: [i32; 3],
) -> ([i32; 3], [i32; 3]) {
    let a = transform_mirror(mirror, rot, 0, 0, 0, pivot);
    let b = transform_mirror(mirror, rot, size[0] - 1, size[1] - 1, size[2] - 1, pivot);
    let mut min = [0; 3];
    let mut max = [0; 3];
    for i in 0..3 {
        min[i] = origin[i] + a[i].min(b[i]);
        max[i] = origin[i] + a[i].max(b[i]);
    }
    (min, max)
}

/// `StructureTemplate.placeInWorld` без процессоров, но с зеркалом.
///
/// `ignore_air` и `waterlog` — как у `BlockIgnoreProcessor.STRUCTURE_BLOCK`:
/// структурные блоки не ставятся, вода в шаблоне заменяет блок мира.
/// Возвращает маркеры (`mode=DATA`) и точки, где был сундук из шаблона:
/// ванила съедает `nextLong` лута на каждой такой клетке.
#[allow(clippy::too_many_arguments)]
pub(crate) fn place_mirrored(
    pal: &Palette,
    level: &mut dyn Level,
    template: &Template,
    origin: [i32; 3],
    mirror: Mirror,
    rot: Rot,
    window: Bounds,
    r: &mut Worldgen,
    ignore_air: bool,
) -> Vec<([i32; 3], Option<String>)> {
    let mut out = Vec::new();
    if template.palettes.is_empty() {
        return out;
    }
    let index = palette_index(template.palettes.len(), origin);
    let Some(blocks) = template.palettes.get(index) else {
        return out;
    };
    let structure_block = pal.blocks.get("minecraft:structure_block").map(|b| b.0);
    let air = pal.blocks.get("minecraft:air").map(|b| b.0);
    let mut placed = Vec::new();
    for piece in blocks {
        let shifted = transform_mirror(
            mirror,
            rot,
            piece.pos[0],
            piece.pos[1],
            piece.pos[2],
            [0, 0, 0],
        );
        let world = [
            origin[0] + shifted[0],
            origin[1] + shifted[1],
            origin[2] + shifted[2],
        ];
        let block = pal.block_index(piece.state);
        if structure_block.is_some_and(|id| block == Some(id)) {
            if let Some(meta) = &piece.marker {
                out.push((world, Some(meta.clone())));
            }
            continue;
        }
        // `BlockIgnoreProcessor.STRUCTURE_AND_AIR`.
        if ignore_air && air.is_some_and(|id| block == Some(id)) {
            continue;
        }
        if !window.contains(world) || !level.can_write(world[0], world[2]) {
            continue;
        }
        if skipped(pal, piece.state) {
            continue;
        }
        let state = mirror_state(pal, piece.state, mirror, rot);
        level.set(world[0], world[1], world[2], state);
        placed.push(world);
        // `WallBlock` пересобирает стороны в пост-обработке чанка.
        if let Some(block) = pal.block_index(state)
            && pal
                .blocks
                .iter()
                .any(|(name, (id, _))| *id == block && name.ends_with("_wall"))
        {
            level.mark_shape(world[0], world[1], world[2]);
        }
        if let Some(nbt) = &piece.nbt
            && place_template_nbt(pal, level, world, state, nbt, r)
        {
            // Сундук-маркер `Chest*` уже поставлен; блок-сущность пишется сюда.
            out.push((world, None));
        }
    }
    settle_stairs(pal, level, &placed);
    settle_fences(pal, level, &placed);
    out
}

/// Соседи `placeInWorld`: вверх, затем север, восток, юг, запад. Вниз не смотрит.
const WATER_DONORS: [[i32; 3]; 5] = [[0, 1, 0], [0, 0, -1], [1, 0, 0], [0, 0, 1], [-1, 0, 0]];
/// Доливает источник с соседа в блоки, которые умеют быть затопленными.
fn spread_water(
    pal: &Palette,
    level: &mut dyn Level,
    to_fill: &mut Vec<[i32; 3]>,
    locked: &HashSet<(i32, i32, i32)>,
) {
    let mut guard = 0;
    while !to_fill.is_empty() && guard < 64 {
        guard += 1;
        let mut progress = false;
        let mut next = Vec::new();
        for pos in to_fill.drain(..) {
            let mut source = pal.fluid(level.get(pos[0], pos[1], pos[2])) == Fluid::Water;
            if !source {
                for delta in WATER_DONORS {
                    let n = [pos[0] + delta[0], pos[1] + delta[1], pos[2] + delta[2]];
                    if locked.contains(&(n[0], n[1], n[2])) {
                        continue;
                    }
                    if pal.fluid(level.get(n[0], n[1], n[2])) == Fluid::Water {
                        source = true;
                        break;
                    }
                }
            }
            if !source {
                next.push(pos);
                continue;
            }
            let state = level.get(pos[0], pos[1], pos[2]);
            if pal.prop(state, "waterlogged") == Some("false")
                && let Some(next_state) = pal.with_prop(state, "waterlogged", "true")
            {
                level.set(pos[0], pos[1], pos[2], next_state);
            }
            progress = true;
        }
        if !progress {
            break;
        }
        *to_fill = next;
    }
}

/// `StairBlock.getStairsShape` после постановки всего куска.
fn settle_stairs(pal: &Palette, level: &mut dyn Level, placed: &[[i32; 3]]) {
    let mut seen = HashSet::new();
    for pos in placed {
        seen.insert((pos[0], pos[1], pos[2]));
        apply_stair_shape(pal, level, *pos);
    }
    refresh_neighbors(pal, level, placed, &mut seen, apply_stair_shape);
}

/// `FenceBlock.updateShape`: сторона включается от такого же забора или прочной грани.
fn settle_fences(pal: &Palette, level: &mut dyn Level, placed: &[[i32; 3]]) {
    let kinds = fence_kinds(pal);
    if kinds.is_empty() {
        return;
    }
    let mut seen = HashSet::new();
    for pos in placed {
        seen.insert((pos[0], pos[1], pos[2]));
        apply_fence(pal, level, *pos, &kinds);
    }
    let around = [
        [0, 1, 0],
        [0, -1, 0],
        [0, 0, -1],
        [1, 0, 0],
        [0, 0, 1],
        [-1, 0, 0],
    ];
    for pos in placed {
        for delta in around {
            let n = [pos[0] + delta[0], pos[1] + delta[1], pos[2] + delta[2]];
            if !seen.insert((n[0], n[1], n[2])) || !level.can_write(n[0], n[2]) {
                continue;
            }
            apply_fence(pal, level, n, &kinds);
        }
    }
}

fn refresh_neighbors(
    pal: &Palette,
    level: &mut dyn Level,
    placed: &[[i32; 3]],
    seen: &mut HashSet<(i32, i32, i32)>,
    apply: fn(&Palette, &mut dyn Level, [i32; 3]),
) {
    let around = [
        [0, 1, 0],
        [0, -1, 0],
        [0, 0, -1],
        [1, 0, 0],
        [0, 0, 1],
        [-1, 0, 0],
    ];
    for pos in placed {
        for delta in around {
            let n = [pos[0] + delta[0], pos[1] + delta[1], pos[2] + delta[2]];
            if !seen.insert((n[0], n[1], n[2])) || !level.can_write(n[0], n[2]) {
                continue;
            }
            apply(pal, level, n);
        }
    }
}

/// `true` — деревянный забор: он связывается с любым деревянным, не с адским.
fn fence_kinds(pal: &Palette) -> HashMap<u32, bool> {
    let mut kinds = HashMap::new();
    for (name, (id, _)) in pal.blocks.iter() {
        if name.ends_with("_fence") {
            kinds.insert(*id, !name.contains("nether_brick"));
        }
    }
    kinds
}

fn apply_fence(pal: &Palette, level: &mut dyn Level, pos: [i32; 3], kinds: &HashMap<u32, bool>) {
    let state = level.get(pos[0], pos[1], pos[2]);
    if fence_wood(pal, state, kinds).is_none() {
        return;
    }
    let sides = [
        ("north", [0, 0, -1], 3usize),
        ("south", [0, 0, 1], 2),
        ("west", [-1, 0, 0], 5),
        ("east", [1, 0, 0], 4),
    ];
    let mut state = state;
    for (prop, delta, face) in sides {
        let neighbor = level.get(pos[0] + delta[0], pos[1] + delta[1], pos[2] + delta[2]);
        let connect = same_fence(pal, state, neighbor, kinds) || pal.sturdy(neighbor, face);
        let value = if connect { "true" } else { "false" };
        if let Some(next) = pal.with_prop(state, prop, value) {
            state = next;
        }
    }
    level.set(pos[0], pos[1], pos[2], state);
}

fn fence_wood(pal: &Palette, state: u32, kinds: &HashMap<u32, bool>) -> Option<bool> {
    let id = pal.block_of.get(state as usize).copied()?;
    kinds.get(&id).copied()
}

fn same_fence(pal: &Palette, a: u32, b: u32, kinds: &HashMap<u32, bool>) -> bool {
    match (fence_wood(pal, a, kinds), fence_wood(pal, b, kinds)) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

fn apply_stair_shape(pal: &Palette, level: &mut dyn Level, pos: [i32; 3]) {
    let state = level.get(pos[0], pos[1], pos[2]);
    let Some(shape) = stair_shape(pal, level, pos, state) else {
        return;
    };
    if pal.prop(state, "shape") == Some(shape) {
        return;
    }
    if let Some(next) = pal.with_prop(state, "shape", shape) {
        level.set(pos[0], pos[1], pos[2], next);
    }
}

/// Форма по соседям спереди и сзади. Ось и половина соседа должны совпасть.
fn stair_shape(
    pal: &Palette,
    level: &dyn Level,
    pos: [i32; 3],
    state: u32,
) -> Option<&'static str> {
    let facing = pal.prop(state, "facing")?;
    let half = pal.prop(state, "half")?;
    if !is_stair(pal, state) {
        return None;
    }
    let at = |dir: &str| {
        let [dx, dy, dz] = dir_delta(dir);
        level.get(pos[0] + dx, pos[1] + dy, pos[2] + dz)
    };
    let front = at(facing);
    if stair_aligned(pal, front, half)
        && let Some(front_facing) = pal.prop(front, "facing")
        && !same_horiz_axis(front_facing, facing)
        && can_take_shape(pal, level, pos, state, opposite(front_facing))
    {
        return Some(if front_facing == counter_clockwise(facing) {
            "outer_left"
        } else {
            "outer_right"
        });
    }
    let back = at(opposite(facing));
    if stair_aligned(pal, back, half)
        && let Some(back_facing) = pal.prop(back, "facing")
        && !same_horiz_axis(back_facing, facing)
        && can_take_shape(pal, level, pos, state, back_facing)
    {
        return Some(if back_facing == counter_clockwise(facing) {
            "inner_left"
        } else {
            "inner_right"
        });
    }
    Some("straight")
}

fn is_stair(pal: &Palette, state: u32) -> bool {
    matches!(
        pal.prop(state, "shape"),
        Some("straight" | "inner_left" | "inner_right" | "outer_left" | "outer_right")
    ) && pal.prop(state, "facing").is_some()
        && pal.prop(state, "half").is_some()
}

fn stair_aligned(pal: &Palette, state: u32, half: &str) -> bool {
    is_stair(pal, state) && pal.prop(state, "half") == Some(half)
}

fn can_take_shape(pal: &Palette, level: &dyn Level, pos: [i32; 3], state: u32, face: &str) -> bool {
    let [dx, dy, dz] = dir_delta(face);
    let other = level.get(pos[0] + dx, pos[1] + dy, pos[2] + dz);
    !is_stair(pal, other)
        || pal.prop(other, "facing") != pal.prop(state, "facing")
        || pal.prop(other, "half") != pal.prop(state, "half")
}

fn dir_delta(dir: &str) -> [i32; 3] {
    match dir {
        "north" => [0, 0, -1],
        "south" => [0, 0, 1],
        "west" => [-1, 0, 0],
        "east" => [1, 0, 0],
        "up" => [0, 1, 0],
        "down" => [0, -1, 0],
        _ => [0, 0, 0],
    }
}

fn opposite(dir: &str) -> &'static str {
    match dir {
        "north" => "south",
        "south" => "north",
        "west" => "east",
        "east" => "west",
        "up" => "down",
        "down" => "up",
        _ => "",
    }
}

fn counter_clockwise(dir: &str) -> &'static str {
    match dir {
        "north" => "west",
        "west" => "south",
        "south" => "east",
        "east" => "north",
        _ => "",
    }
}

fn same_horiz_axis(a: &str, b: &str) -> bool {
    matches!(
        (a, b),
        ("north" | "south", "north" | "south") | ("east" | "west", "east" | "west")
    )
}

fn data_marker(block: &Compound) -> Option<String> {
    let nbt = block.get("nbt").and_then(Tag::as_compound)?;
    if nbt.get("mode").and_then(Tag::as_str) != Some("DATA") {
        return None;
    }
    Some(
        nbt.get("metadata")
            .and_then(Tag::as_str)
            .unwrap_or("")
            .to_string(),
    )
}

/// `StructureTemplate.placeInWorld`: у блока с NBT ванила ставит `BARRIER`,
/// затем `loadWithComponents`, а `RandomizableContainer` получает
/// `LootTableSeed` из `random.nextLong()`. Готовые `Items` шаблона
/// переносятся как есть.
pub(crate) fn set_template_nbt(
    level: &mut dyn Level,
    world: [i32; 3],
    state: u32,
    nbt: &chunkwake_world::nbt::Compound,
    seed: Option<i64>,
    id: &str,
) {
    let _ = state;
    let mut nbt = nbt.clone();
    // `id` и координаты Anvil дописывает сам, они повторяются.
    nbt.remove("id");
    // `RandomizableContainerBlockEntity.trySaveLootTable`: семя пишется
    // только вместе с таблицей, иначе его нет.
    if let Some(seed) = seed {
        if nbt.contains_key("LootTable") {
            nbt.insert(
                "LootTableSeed".into(),
                chunkwake_world::nbt::Tag::Long(seed),
            );
        }
    } else {
        nbt.remove("LootTableSeed");
    }
    level.set_block_entity(world[0], world[1], world[2], id, nbt);
}

/// Постановка блока шаблона, у которого есть NBT: контейнеры получают
/// `LootTableSeed` из генератора фичи, прочие (`campfire`, `trial_spawner`,
/// вывески) — как есть. Возвращает `true`, если генератор сдвинулся.
pub(crate) fn place_template_nbt(
    pal: &Palette,
    level: &mut dyn Level,
    world: [i32; 3],
    state: u32,
    nbt: &chunkwake_world::nbt::Compound,
    r: &mut Worldgen,
) -> bool {
    let Some(name) = pal.block_name(state) else {
        return false;
    };
    let Some(id) = entity_id(name) else {
        return false;
    };
    // `placeInWorld`: `nextLong` бросается только у `RandomizableContainer`.
    let seed = randomizable(name).map(|_| r.next_long());
    set_template_nbt(level, world, state, nbt, seed, id);
    seed.is_some()
}

/// `RandomizableContainer.setLootTable(table, random.nextLong())`: ванила
/// пишет в сундук пару `LootTable` + `LootTableSeed`, а не готовые предметы.
/// Содержимое считается при первом открытии тем же сидом.
pub(crate) fn set_loot(level: &mut dyn Level, world: [i32; 3], table: &str, seed: i64) {
    let mut nbt = chunkwake_world::nbt::Compound::new();
    nbt.insert(
        "LootTable".into(),
        chunkwake_world::nbt::Tag::String(table.to_owned()),
    );
    nbt.insert(
        "LootTableSeed".into(),
        chunkwake_world::nbt::Tag::Long(seed),
    );
    level.set_block_entity(world[0], world[1], world[2], "minecraft:chest", nbt);
}

/// `RandomizableContainer` — единственные блоки, которым `placeInWorld`
/// дописывает `LootTableSeed`. Список сверен с классами 26.3:
/// `ChestBlockEntity` (сундуки, включая медные — у них тип `CHEST`),
/// `BarrelBlockEntity`, `DispenserBlockEntity`, `HopperBlockEntity`,
/// `CrafterBlockEntity`, `ShulkerBoxBlockEntity`, `DecoratedPotBlockEntity`.
/// Печь, коптильня, плавильня, варочная стойка и костёр — контейнеры, но не
/// `RandomizableContainer`: семя им не бросается.
fn randomizable(name: &str) -> Option<&'static str> {
    if name == "minecraft:chest"
        || name == "minecraft:trapped_chest"
        || (name.starts_with("minecraft:") && name.ends_with("_chest") && name.contains("copper"))
    {
        return Some("minecraft:chest");
    }
    Some(match name {
        "minecraft:barrel" => "minecraft:barrel",
        "minecraft:dispenser" => "minecraft:dispenser",
        "minecraft:dropper" => "minecraft:dropper",
        "minecraft:hopper" => "minecraft:hopper",
        "minecraft:crafter" => "minecraft:crafter",
        "minecraft:decorated_pot" => "minecraft:decorated_pot",
        _ if name.ends_with("_shulker_box") || name == "minecraft:shulker_box" => {
            "minecraft:shulker_box"
        }
        _ => return None,
    })
}

/// Тип блок-сущности по имени блока (`Block.getBlockEntityType`) для блоков,
/// что встречаются в шаблонах структур. `None` — блок-сущности нет.
fn entity_id(name: &str) -> Option<&'static str> {
    if let Some(id) = randomizable(name) {
        return Some(id);
    }
    Some(match name {
        "minecraft:campfire" | "minecraft:soul_campfire" => "minecraft:campfire",
        "minecraft:bell" => "minecraft:bell",
        "minecraft:furnace" => "minecraft:furnace",
        "minecraft:blast_furnace" => "minecraft:blast_furnace",
        "minecraft:smoker" => "minecraft:smoker",
        "minecraft:brewing_stand" => "minecraft:brewing_stand",
        "minecraft:lectern" => "minecraft:lectern",
        "minecraft:comparator" => "minecraft:comparator",
        "minecraft:sculk_sensor" => "minecraft:sculk_sensor",
        "minecraft:calibrated_sculk_sensor" => "minecraft:calibrated_sculk_sensor",
        "minecraft:sculk_catalyst" => "minecraft:sculk_catalyst",
        "minecraft:sculk_shrieker" => "minecraft:sculk_shrieker",
        "minecraft:trial_spawner" => "minecraft:trial_spawner",
        "minecraft:vault" => "minecraft:vault",
        "minecraft:creaking_heart" => "minecraft:creaking_heart",
        "minecraft:suspicious_sand" | "minecraft:suspicious_gravel" => "minecraft:brushable_block",
        "minecraft:spawner" => "minecraft:mob_spawner",
        "minecraft:ender_chest" => "minecraft:ender_chest",
        "minecraft:structure_block" => "minecraft:structure_block",
        "minecraft:jigsaw" => "minecraft:jigsaw",
        "minecraft:chiseled_bookshelf" => "minecraft:chiseled_bookshelf",
        _ if name.starts_with("minecraft:")
            && name.ends_with("_chest")
            && name.contains("copper") =>
        {
            "minecraft:chest"
        }
        // Вывески и баннеры: `SignBlock` и `BannerBlock` дают свои типы.
        _ if name.ends_with("_wall_sign")
            || name.ends_with("_sign")
            || name.ends_with("_hanging_sign") =>
        {
            "minecraft:sign"
        }
        _ if name.ends_with("_wall_banner") || name.ends_with("_banner") => "minecraft:banner",
        _ if name.ends_with("_wall_head")
            || name.ends_with("_head")
            || name.ends_with("_skull") =>
        {
            "minecraft:skull"
        }
        _ if name.ends_with("_golem_statue") => "minecraft:copper_golem_statue",
        _ => return None,
    })
}

/// Чанк точки, расширенный на 16 по x/z. `maxY` — `getMaxY()`, верх невключительной
/// высоты мира, но граница коробки включающая.
fn chunk_window(level: &dyn Level, origin: [i32; 3]) -> Bounds {
    let cx = origin[0].div_euclid(16);
    let cz = origin[2].div_euclid(16);
    let min_x = cx * 16;
    let min_z = cz * 16;
    Bounds {
        min: [min_x - 16, level.min_y(), min_z - 16],
        max: [min_x + 31, level.min_y() + level.height(), min_z + 31],
    }
}

fn empty_corners(pal: &Palette, level: &dyn Level, box_: Bounds) -> i32 {
    let mut n = 0;
    for x in [box_.min[0], box_.max[0]] {
        for y in [box_.min[1], box_.max[1]] {
            for z in [box_.min[2], box_.max[2]] {
                let state = level.get(x, y, z);
                if pal.is_air(state) || pal.is_liquid(state) {
                    n += 1;
                }
            }
        }
    }
    n
}

#[allow(clippy::too_many_arguments)]
fn place_in_world(
    pal: &Palette,
    level: &mut dyn Level,
    template: &Template,
    rot: Rot,
    place: [i32; 3],
    processors: &[Processor],
    r: &mut Worldgen,
    window: Option<Bounds>,
) -> bool {
    if template.palettes.is_empty() {
        return false;
    }
    // `getRandomPalette` бросает `nextInt` даже при одной палитре: это тот же
    // генератор фичи, его поставил `setRandom`.
    let index = r.next_int_bound(template.palettes.len() as i32) as usize;
    if template.size[0] < 1 || template.size[1] < 1 || template.size[2] < 1 {
        return false;
    }
    let blocks = &template.palettes[index];
    if blocks.is_empty() {
        return false;
    }
    for piece in blocks {
        let shifted = transform(rot, piece.pos[0], piece.pos[1], piece.pos[2]);
        let world = [
            place[0] + shifted[0],
            place[1] + shifted[1],
            place[2] + shifted[2],
        ];
        if window.is_some_and(|box_| !box_.contains(world)) {
            continue;
        }
        let mut state = Some(piece.state);
        for processor in processors {
            let Some(current) = state else { break };
            state = processor.apply(pal, level, current, world, world, r, false);
        }
        let Some(state) = state else { continue };
        let state = rotate_state(pal, state, rot);
        if skipped(pal, state) || !level.can_write(world[0], world[2]) {
            continue;
        }
        level.set(
            world[0],
            world[1],
            world[2],
            waterlog(pal, level, world, state),
        );
        // `placeInWorld`: свой NBT шаблона плюс `LootTableSeed`.
        if let Some(nbt) = &piece.nbt {
            place_template_nbt(pal, level, world, state, nbt, r);
        }
    }
    true
}

impl Processor {
    fn apply(
        &self,
        pal: &Palette,
        level: &dyn Level,
        state: u32,
        world: [i32; 3],
        reference: [i32; 3],
        r: &mut Worldgen,
        positional_rot: bool,
    ) -> Option<u32> {
        match self {
            Self::Rot {
                integrity,
                rottable,
            } => {
                if rottable
                    .as_ref()
                    .is_some_and(|mask| !mask.get(state as usize).copied().unwrap_or(false))
                {
                    return Some(state);
                }
                // У джигсо `settings.random` пустой: `getRandom` — `Legacy` от
                // позиции. Фича (`fossil`) кладёт в настройки свой генератор.
                let roll = if positional_rot {
                    Legacy::new(palette_seed(world[0], world[1], world[2])).next_float()
                } else {
                    r.next_float()
                };
                if roll > *integrity { None } else { Some(state) }
            }
            Self::Protected(mask) => {
                let world_state = level.get(world[0], world[1], world[2]);
                if mask.get(world_state as usize).copied().unwrap_or(false) {
                    None
                } else {
                    Some(state)
                }
            }
            Self::Rule(rules) => {
                // `RuleProcessor`: свой `LegacyRandomSource` от `Mth.getSeed` мировой
                // позиции, заново на каждый блок. Генератор декорации не двигается.
                let mut rng = Legacy::new(palette_seed(world[0], world[1], world[2]));
                for rule in rules {
                    if rule.input.holds(pal, state, &mut rng)
                        && rule.at.holds(pal, level, world)
                        && rule.pos.holds(world, reference, &mut rng)
                    {
                        return Some(rule.output);
                    }
                }
                Some(state)
            }
            Self::Age(mossiness) => {
                // `BlockAgeProcessor`: свой `Legacy` от позиции; порядок вызовов
                // внутри ветки важен так же, как значения.
                let mut rng = Legacy::new(palette_seed(world[0], world[1], world[2]));
                Some(age_block(pal, state, *mossiness, &mut rng))
            }
            Self::LavaSubmerged => {
                // `LavaSubmergedBlockProcessor`: лава под неполным блоком остаётся.
                let was_lava = pal
                    .block_index(level.get(world[0], world[1], world[2]))
                    .is_some_and(|id| pal.blocks.get("minecraft:lava").is_some_and(|b| b.0 == id));
                if was_lava && !pal.is_full_cube(state) {
                    return Some(pal.defaults["minecraft:lava"]);
                }
                Some(state)
            }
            Self::Blackstone => Some(blackstone(pal, state)),
            Self::Capped { .. } => Some(state),
            Self::IgnoreStructureBlock { air_pocket } => {
                // `BlockIgnoreProcessor`: структурный блок всегда, воздух — если
                // пещеры нет (иначе шаблон вырезает её сам).
                let Some(block) = pal.block_index(state) else {
                    return Some(state);
                };
                let is_structure = pal
                    .blocks
                    .get("minecraft:structure_block")
                    .is_some_and(|b| b.0 == block);
                if is_structure {
                    return None;
                }
                if *air_pocket == 0.0 && pal.is_air(state) {
                    return None;
                }
                Some(state)
            }
        }
    }
}

/// `BlockAgeProcessor.processBlock`: только камень, ступени, плиты, стены и
/// обсидиан. Порядок бросков повторяет байткод.
fn age_block(pal: &Palette, state: u32, mossiness: f32, rng: &mut Legacy) -> u32 {
    let name = |id: u32| {
        pal.blocks
            .iter()
            .find(|(_, (b, _))| *b == id)
            .map(|(n, _)| n.clone())
    };
    let Some(block) = pal.block_index(state) else {
        return state;
    };
    let Some(name) = name(block) else {
        return state;
    };
    let short = name.strip_prefix("minecraft:").unwrap_or(&name);
    if matches!(short, "stone_bricks" | "stone" | "chiseled_stone_bricks") {
        if rng.next_float() >= 0.5 {
            return state;
        }
        let cracked = pal.defaults["minecraft:cracked_stone_bricks"];
        let stairs = random_facing_stairs(pal, "minecraft:stone_brick_stairs", rng);
        let mossy = pal.defaults["minecraft:mossy_stone_bricks"];
        let mossy_stairs = random_facing_stairs(pal, "minecraft:mossy_stone_brick_stairs", rng);
        if rng.next_float() < mossiness {
            return if rng.next_int_bound(2) == 0 {
                mossy
            } else {
                mossy_stairs
            };
        }
        return if rng.next_int_bound(2) == 0 {
            cracked
        } else {
            stairs
        };
    }
    if short.ends_with("_stairs") {
        if rng.next_float() >= 0.5 {
            return state;
        }
        let non_mossy = [
            pal.defaults["minecraft:stone_slab"],
            pal.defaults["minecraft:stone_brick_slab"],
        ];
        let mossy_stairs = with_props(
            pal,
            pal.defaults["minecraft:mossy_stone_brick_stairs"],
            state,
        );
        let mossy_slab = pal.defaults["minecraft:mossy_stone_brick_slab"];
        if rng.next_float() < mossiness {
            return if rng.next_int_bound(2) == 0 {
                mossy_stairs
            } else {
                mossy_slab
            };
        }
        return non_mossy[rng.next_int_bound(2) as usize];
    }
    if short.ends_with("_slab") {
        if rng.next_float() < mossiness {
            return with_props(pal, pal.defaults["minecraft:mossy_stone_brick_slab"], state);
        }
        return state;
    }
    if short.ends_with("_wall") {
        if rng.next_float() < mossiness {
            return with_props(pal, pal.defaults["minecraft:mossy_stone_brick_wall"], state);
        }
        return state;
    }
    if short == "obsidian" && rng.next_float() < 0.15 {
        return pal.defaults["minecraft:crying_obsidian"];
    }
    state
}

/// `getRandomFacingStairs`: `nextInt(4)` на сторону, потом `getRandom(Half)`.
fn random_facing_stairs(pal: &Palette, name: &str, rng: &mut Legacy) -> u32 {
    const FACINGS: [&str; 4] = ["north", "east", "south", "west"];
    let mut state = pal.defaults[name];
    let dir = FACINGS[rng.next_int_bound(4) as usize];
    if let Some(next) = pal.with_prop(state, "facing", dir) {
        state = next;
    }
    let half = if rng.next_int_bound(2) == 0 {
        "bottom"
    } else {
        "top"
    };
    if let Some(next) = pal.with_prop(state, "half", half) {
        state = next;
    }
    state
}

/// `withPropertiesOf`: переносит совпадающие по имени свойства.
fn with_props(pal: &Palette, target: u32, source: u32) -> u32 {
    let mut state = target;
    for key in [
        "facing", "half", "type", "north", "east", "south", "west", "up",
    ] {
        let Some(value) = pal.prop(source, key).map(str::to_owned) else {
            continue;
        };
        if let Some(next) = pal.with_prop(state, key, &value) {
            state = next;
        }
    }
    state
}

/// `BlackstoneReplaceProcessor`: камень и его производные на чернокамень.
fn blackstone(pal: &Palette, state: u32) -> u32 {
    const MAP: [(&str, &str); 23] = [
        ("cobblestone", "blackstone"),
        ("mossy_cobblestone", "blackstone"),
        ("stone", "polished_blackstone"),
        ("stone_bricks", "polished_blackstone_bricks"),
        ("mossy_stone_bricks", "polished_blackstone_bricks"),
        ("cobblestone_stairs", "blackstone_stairs"),
        ("mossy_cobblestone_stairs", "blackstone_stairs"),
        ("stone_stairs", "polished_blackstone_stairs"),
        ("stone_brick_stairs", "polished_blackstone_brick_stairs"),
        (
            "mossy_stone_brick_stairs",
            "polished_blackstone_brick_stairs",
        ),
        ("cobblestone_slab", "blackstone_slab"),
        ("mossy_cobblestone_slab", "blackstone_slab"),
        ("smooth_stone_slab", "polished_blackstone_slab"),
        ("stone_slab", "polished_blackstone_slab"),
        ("stone_brick_slab", "polished_blackstone_brick_slab"),
        ("mossy_stone_brick_slab", "polished_blackstone_brick_slab"),
        ("stone_brick_wall", "polished_blackstone_brick_wall"),
        ("mossy_stone_brick_wall", "polished_blackstone_brick_wall"),
        ("cobblestone_wall", "blackstone_wall"),
        ("mossy_cobblestone_wall", "blackstone_wall"),
        ("chiseled_stone_bricks", "chiseled_polished_blackstone"),
        ("cracked_stone_bricks", "cracked_polished_blackstone_bricks"),
        ("iron_bars", "iron_chain"),
    ];
    let Some(block) = pal.block_index(state) else {
        return state;
    };
    for (from, to) in MAP {
        let from_id = pal.blocks.get(&format!("minecraft:{from}")).map(|b| b.0);
        if from_id != Some(block) {
            continue;
        }
        let Some(target) = pal.blocks.get(&format!("minecraft:{to}")) else {
            return state;
        };
        return with_props(pal, target.1.first().copied().unwrap_or(target.0), state);
    }
    state
}

impl RuleInput {
    fn holds(&self, pal: &Palette, state: u32, r: &mut impl Bits) -> bool {
        let block = pal.block_index(state);
        match self {
            Self::Always => true,
            Self::Block(id) => block == Some(*id),
            Self::State(want) => state == *want,
            Self::Tag(mask) => mask.get(state as usize).copied().unwrap_or(false),
            Self::Random { block: id, chance } => {
                if block != Some(*id) {
                    return false;
                }
                r.next_float() < *chance
            }
        }
    }
}

impl RuleAt {
    fn holds(&self, pal: &Palette, level: &dyn Level, world: [i32; 3]) -> bool {
        match self {
            Self::Pred(pred) => pred.test(level, pal, world),
            Self::Block(id) => {
                pal.block_index(level.get(world[0], world[1], world[2])) == Some(*id)
            }
            Self::Always => true,
        }
    }
}

/// Палитра `getRandomPalette`, когда у настроек нет своего генератора:
/// `LegacyRandomSource` от `Mth.getSeed` позиции (`lshr`, не арифметический сдвиг).
fn palette_index(n: usize, at: [i32; 3]) -> usize {
    if n <= 1 {
        return 0;
    }
    let mut rng = Legacy::new(palette_seed(at[0], at[1], at[2]));
    rng.next_int_bound(n as i32) as usize
}

fn palette_seed(x: i32, y: i32, z: i32) -> i64 {
    let s = (x.wrapping_mul(3129871) as i64) ^ (z as i64).wrapping_mul(116129781) ^ y as i64;
    let s = s
        .wrapping_mul(s)
        .wrapping_mul(42317861)
        .wrapping_add(s.wrapping_mul(11));
    ((s as u64) >> 16) as i64
}

/// Постановка куска пула. Палитра не трогает генератор декорации.
/// `ignore_air` — `legacy_single_pool_element`. `gravity` — `terrain_matching`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn place_pool(
    pal: &Palette,
    level: &mut dyn Level,
    template: &Template,
    rot: Rot,
    place: [i32; 3],
    processors: &[Processor],
    ignore_air: bool,
    gravity: bool,
    r: &mut Worldgen,
    window: Bounds,
    world_seed: i64,
    reference: [i32; 3],
) -> bool {
    if template.palettes.is_empty()
        || template.size[0] < 1
        || template.size[1] < 1
        || template.size[2] < 1
    {
        return false;
    }
    let index = palette_index(template.palettes.len(), place);
    let blocks = &template.palettes[index];
    if blocks.is_empty() {
        return false;
    }
    let air = pal.blocks.get("minecraft:air").map(|b| b.0);
    let structure_block = pal.blocks.get("minecraft:structure_block").map(|b| b.0);
    let finals: HashMap<[i32; 3], u32> = template
        .jigsaws
        .get(index)
        .into_iter()
        .flatten()
        .map(|j| (j.pos, j.final_state))
        .collect();
    // `processBlockInfos`: при `evaluatesEntirePieceState` обрабатывается
    // весь кусок, иначе только блоки окна чанка.
    let whole = processors
        .iter()
        .any(|p| matches!(p, Processor::Capped { .. }));
    struct Info<'t> {
        world: [i32; 3],
        state: u32,
        piece: &'t Piece,
        loot: Option<(String, i64)>,
    }
    let mut infos: Vec<Info<'_>> = Vec::new();
    for piece in blocks {
        let shifted = transform(rot, piece.pos[0], piece.pos[1], piece.pos[2]);
        let mut world = [
            place[0] + shifted[0],
            place[1] + shifted[1],
            place[2] + shifted[2],
        ];
        if !whole && !window.contains(world) {
            continue;
        }
        let block = pal.block_index(piece.state);
        if ignore_air && air.is_some_and(|id| block == Some(id)) {
            continue;
        }
        if structure_block.is_some_and(|id| block == Some(id)) {
            continue;
        }
        let mut state = finals.get(&piece.pos).copied().unwrap_or(piece.state);
        let mut keep = true;
        for processor in processors {
            match processor.apply(pal, level, state, world, reference, r, true) {
                Some(next) => state = next,
                None => {
                    keep = false;
                    break;
                }
            }
        }
        if !keep {
            continue;
        }
        if gravity {
            let surface = level.surface(Heightmap::WorldSurfaceWg, world[0], world[2]);
            world[1] = surface - 1 + piece.pos[1];
        }
        infos.push(Info {
            world,
            state,
            piece,
            loot: None,
        });
    }
    for processor in processors {
        let Processor::Capped { delegate, limit } = processor else {
            continue;
        };
        if infos.is_empty() {
            continue;
        }
        // `RandomSource.createThreadLocalInstance(seed).forkPositional().at(position)`.
        let mut rng = {
            let factory = Legacy::new(world_seed).next_long();
            Legacy::new(palette_seed(place[0], place[1], place[2]) ^ factory)
        };
        let most = limit.sample(&mut rng).min(infos.len() as i32);
        if most < 1 {
            continue;
        }
        let mut order: Vec<usize> = (0..infos.len()).collect();
        shuffle(&mut order, &mut rng);
        let mut replaced = 0;
        for i in order {
            if replaced >= most {
                break;
            }
            let info = &mut infos[i];
            if let Processor::Rule(rules) = delegate.as_ref() {
                if let Some((state, loot)) = rule_hit(pal, &*level, rules, info.state, info.world, reference) {
                    info.state = state;
                    if loot.is_some() {
                        info.loot = loot;
                    }
                    replaced += 1;
                }
            } else if let Some(next) = delegate.apply(pal, &*level, info.state, info.world, reference, r, true) {
                if next != info.state {
                    info.state = next;
                    replaced += 1;
                }
            }
        }
    }
    for info in infos {
        let world = info.world;
        if !window.contains(world) {
            continue;
        }
        let state = rotate_state(pal, info.state, rot);
        if skipped(pal, state) || !level.can_write(world[0], world[2]) {
            continue;
        }
        level.set(
            world[0],
            world[1],
            world[2],
            waterlog(pal, level, world, state),
        );
        if let Some((table, seed)) = info.loot {
            let mut nbt = chunkwake_world::nbt::Compound::new();
            nbt.insert("LootTable".into(), chunkwake_world::nbt::Tag::String(table));
            nbt.insert("LootTableSeed".into(), chunkwake_world::nbt::Tag::Long(seed));
            level.set_block_entity(world[0], world[1], world[2], "minecraft:brushable_block", nbt);
            continue;
        }
        // `placeInWorld`: свой NBT шаблона плюс `LootTableSeed`.
        if let Some(nbt) = &info.piece.nbt {
            place_template_nbt(pal, level, world, state, nbt, r);
        }
    }
    true
}

/// `RuleProcessor.processBlock`: первое совпавшее правило и его
/// `append_loot` (`nextLong` того же генератора после проверок).
fn rule_hit(
    pal: &Palette,
    level: &dyn Level,
    rules: &[ProcRule],
    state: u32,
    world: [i32; 3],
    reference: [i32; 3],
) -> Option<(u32, Option<(String, i64)>)> {
    let mut rng = Legacy::new(palette_seed(world[0], world[1], world[2]));
    for rule in rules {
        if rule.input.holds(pal, state, &mut rng)
            && rule.at.holds(pal, level, world)
            && rule.pos.holds(world, reference, &mut rng)
        {
            let loot = rule.loot.as_ref().map(|t| (t.clone(), rng.next_long()));
            return Some((rule.output, loot));
        }
    }
    None
}

/// `getShuffledJigsawBlocks`: палитра от позиции, затем `Util.shuffle` и
/// стабильная сортировка по `selection_priority` по убыванию.
pub(crate) fn shuffled_jigsaws(
    template: &Template,
    rot: Rot,
    place: [i32; 3],
    r: &mut impl Bits,
) -> Vec<JigInfo> {
    let t_jig = std::time::Instant::now();
    if template.jigsaws.is_empty() {
        return Vec::new();
    }
    let index = palette_index(template.jigsaws.len(), place);
    let mut list: Vec<JigInfo> = template.jigsaws[index]
        .iter()
        .map(|jig| {
            let shifted = transform(rot, jig.pos[0], jig.pos[1], jig.pos[2]);
            let (front, top) = jig.turned[rot_index(rot)];
            JigInfo {
                pos: [
                    place[0] + shifted[0],
                    place[1] + shifted[1],
                    place[2] + shifted[2],
                ],
                front,
                top,
                rollable: jig.rollable,
                name: Some(jig.name.clone()),
                target: jig.target.clone(),
                pool: jig.pool.clone(),
                placement_priority: jig.placement_priority,
                selection_priority: jig.selection_priority,
            }
        })
        .collect();
    shuffle(&mut list, r);
    list.sort_by_key(|b| std::cmp::Reverse(b.selection_priority));
    JIG.with(|c| c.set(c.get() + t_jig.elapsed().as_secs_f64() * 1000.0));
    list
}

/// `Util.shuffle`: `swap(j - 1, nextInt(j))`, пока `j > 1`.
pub(crate) fn shuffle<T>(list: &mut [T], r: &mut impl Bits) {
    let mut j = list.len();
    while j > 1 {
        let k = r.next_int_bound(j as i32) as usize;
        j -= 1;
        list.swap(j, k);
    }
}

fn skipped(pal: &Palette, state: u32) -> bool {
    let Some(block) = pal.block_index(state) else {
        return false;
    };
    ["minecraft:structure_void", "minecraft:jigsaw"]
        .into_iter()
        .any(|name| pal.blocks.get(name).is_some_and(|b| b.0 == block))
}

fn waterlog(pal: &Palette, level: &dyn Level, world: [i32; 3], state: u32) -> u32 {
    let existing = level.get(world[0], world[1], world[2]);
    if !pal.fluid(existing).is_water() || pal.fluid(state).is_water() {
        return state;
    }
    pal.with_prop(state, "waterlogged", "true").unwrap_or(state)
}

pub(crate) fn rotate_state(pal: &Palette, state: u32, rot: Rot) -> u32 {
    if rot == Rot::None {
        return state;
    }
    let mut state = state;
    for key in ["facing", "horizontal_facing"] {
        let Some(value) = pal.prop(state, key) else {
            continue;
        };
        let Some(turned) = rotate_horizontal(value, rot) else {
            continue;
        };
        if let Some(next) = pal.with_prop(state, key, turned) {
            state = next;
        }
    }
    if let Some(axis) = pal.prop(state, "axis") {
        let turned = rotate_axis(axis, rot);
        if turned != axis
            && let Some(next) = pal.with_prop(state, "axis", turned)
        {
            state = next;
        }
    }
    if let Some(value) = pal.prop(state, "rotation")
        && let Ok(n) = value.parse::<i32>()
    {
        let add = match rot {
            Rot::None => 0,
            Rot::Cw90 => 4,
            Rot::Cw180 => 8,
            Rot::Ccw90 => 12,
        };
        let turned = ((n + add) & 15).to_string();
        if let Some(next) = pal.with_prop(state, "rotation", &turned) {
            state = next;
        }
    }
    if let Some(value) = pal.prop(state, "orientation")
        && let Some(turned) = rotate_orientation(value, rot)
        && let Some(next) = pal.with_prop(state, "orientation", &turned)
    {
        state = next;
    }
    rotate_cross(pal, state, rot)
}

/// `CrossCollisionBlock.rotate`: стороны north/east/south/west крутятся вокруг Y.
fn rotate_cross(pal: &Palette, state: u32, rot: Rot) -> u32 {
    let (Some(n), Some(e), Some(s), Some(w)) = (
        pal.prop(state, "north").map(str::to_owned),
        pal.prop(state, "east").map(str::to_owned),
        pal.prop(state, "south").map(str::to_owned),
        pal.prop(state, "west").map(str::to_owned),
    ) else {
        return state;
    };
    let (n, e, s, w) = match rot {
        Rot::None => return state,
        Rot::Cw180 => (s, w, n, e),
        Rot::Ccw90 => (e, s, w, n),
        Rot::Cw90 => (w, n, e, s),
    };
    let mut state = state;
    for (key, value) in [("north", n), ("east", e), ("south", s), ("west", w)] {
        if let Some(next) = pal.with_prop(state, key, &value) {
            state = next;
        }
    }
    state
}

/// Номер поворота в [`ROTS`].
fn rot_index(rot: Rot) -> usize {
    match rot {
        Rot::None => 0,
        Rot::Cw90 => 1,
        Rot::Cw180 => 2,
        Rot::Ccw90 => 3,
    }
}

/// Направление `FrontAndTop` как статическая строка.
fn direction(dir: &str) -> &'static str {
    match dir {
        "down" => "down",
        "up" => "up",
        "south" => "south",
        "west" => "west",
        "east" => "east",
        _ => "north",
    }
}

fn rotate_horizontal(dir: &str, rot: Rot) -> Option<&'static str> {
    const H: [&str; 4] = ["north", "east", "south", "west"];
    let i = H.iter().position(|d| *d == dir)?;
    let shift = match rot {
        Rot::None => 0,
        Rot::Cw90 => 1,
        Rot::Cw180 => 2,
        Rot::Ccw90 => 3,
    };
    Some(H[(i + shift) % 4])
}

fn rotate_axis(axis: &str, rot: Rot) -> &str {
    match rot {
        Rot::Cw90 | Rot::Ccw90 => match axis {
            "x" => "z",
            "z" => "x",
            other => other,
        },
        Rot::None | Rot::Cw180 => axis,
    }
}

/// `FrontAndTop`: горизонтальная сторона крутится вокруг Y, вертикальная остаётся.
pub(crate) fn rotate_orientation(value: &str, rot: Rot) -> Option<String> {
    let (front, top) = value.split_once('_')?;
    let turn = |dir: &str| rotate_horizontal(dir, rot).unwrap_or(dir).to_owned();
    Some(format!("{}_{}", turn(front), turn(top)))
}

fn predicate_kind(v: &Value) -> &str {
    v["type"]
        .as_str()
        .or_else(|| v["predicate_type"].as_str())
        .unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::{
        Bits, Heightmap, Level, Palette, Piece, Processor, Rot, Template, TemplateFeature,
        Weighted, Worldgen, load_template, place_in_world, rotate_state, sort_pieces, transform,
    };
    use serde_json::Value;
    use std::sync::Arc;

    fn palette() -> Option<(std::path::PathBuf, Palette)> {
        let data = std::env::var_os("CHUNKWAKE_DATA")?;
        let data = std::path::PathBuf::from(data);
        let blocks = chunkwake_data::world::Blocks::load(&data).unwrap();
        let state = |n: &str| blocks.default_state(n).ok().map(|s| s as u32);
        Some((data.clone(), Palette::new(&data, &blocks.states(), &state)))
    }

    struct Grid {
        origin: [i32; 3],
        size: [i32; 3],
        blocks: Vec<u32>,
        air: u32,
        min_y: i32,
        height: i32,
        floor: i32,
        /// `Level.setBlockEntity`: так же, как у чанка конвейера.
        entities: Vec<([i32; 3], String, chunkwake_world::nbt::Compound)>,
    }

    impl Grid {
        fn new(origin: [i32; 3], size: [i32; 3], fill: u32, air: u32) -> Self {
            Self {
                origin,
                size,
                blocks: vec![fill; (size[0] * size[1] * size[2]) as usize],
                air,
                min_y: origin[1],
                height: size[1],
                floor: origin[1] + size[1] / 2,
                entities: Vec::new(),
            }
        }

        fn index(&self, x: i32, y: i32, z: i32) -> Option<usize> {
            let (dx, dy, dz) = (x - self.origin[0], y - self.origin[1], z - self.origin[2]);
            if dx < 0
                || dy < 0
                || dz < 0
                || dx >= self.size[0]
                || dy >= self.size[1]
                || dz >= self.size[2]
            {
                return None;
            }
            Some(((dy * self.size[2] + dz) * self.size[0] + dx) as usize)
        }
    }

    impl Level for Grid {
        fn min_y(&self) -> i32 {
            self.min_y
        }
        fn height(&self) -> i32 {
            self.height
        }
        fn get(&self, x: i32, y: i32, z: i32) -> u32 {
            self.index(x, y, z)
                .map(|i| self.blocks[i])
                .unwrap_or(self.air)
        }
        fn set(&mut self, x: i32, y: i32, z: i32, state: u32) {
            if let Some(i) = self.index(x, y, z) {
                self.blocks[i] = state;
            }
        }
        fn can_write(&self, x: i32, z: i32) -> bool {
            self.index(x, self.origin[1], z).is_some()
        }
        fn surface(&self, _: Heightmap, _: i32, _: i32) -> i32 {
            self.floor
        }
        fn sea_level(&self) -> i32 {
            63
        }
        fn biome(&self, _: i32, _: i32, _: i32) -> u32 {
            0
        }
        fn set_block_entity(
            &mut self,
            x: i32,
            y: i32,
            z: i32,
            id: &str,
            nbt: chunkwake_world::nbt::Compound,
        ) {
            self.entities.push(([x, y, z], id.to_owned(), nbt));
        }
    }

    /// Размер колодца, угол без поворота, `structure_void` не пишется,
    /// лестница и ось костяного блока поворачиваются, `block_rot` оставляет
    /// заранее посчитанное число блоков.
    #[test]
    fn well_geometry_and_rot() {
        let Some((data, pal)) = palette() else {
            return;
        };
        let well = load_template(&data, &pal, "minecraft:desert_well/well").unwrap();
        assert_eq!(well.size, [5, 7, 5]);
        let stone = pal.state(&Value::String("minecraft:stone".into())).unwrap();
        let air = pal.state(&Value::String("minecraft:air".into())).unwrap();
        // Один поворот `none`: угол шаблона в `origin + (-sx/2, 0, -sz/2)`.
        let placed = TemplateFeature {
            templates: vec![Weighted {
                template: Arc::new(well.clone()),
                weight: 1,
                rotations: vec![Rot::None],
            }],
            processors: Vec::new(),
        };
        let mut grid = Grid::new([-8, 0, -8], [24, 24, 24], stone, air);
        placed.place(&pal, &mut grid, &mut Worldgen::new(3), [8, 4, 8]);
        let corner = grid.get(6, 4, 6);
        assert_eq!(
            pal.block_index(corner),
            pal.blocks.get("minecraft:sandstone").map(|b| b.0)
        );

        let void_state = pal
            .state(&Value::String("minecraft:structure_void".into()))
            .unwrap();
        let sand = pal
            .state(&Value::String("minecraft:sandstone".into()))
            .unwrap();
        let synthetic = Template {
            size: [2, 1, 1],
            jigsaws: Vec::new(),
            palettes: vec![sort_pieces(
                &pal,
                vec![
                    Piece {
                        pos: [0, 0, 0],
                        state: void_state,
                        nbt: None,
                        marker: None,
                    },
                    Piece {
                        pos: [1, 0, 0],
                        state: sand,
                        nbt: None,
                        marker: None,
                    },
                ],
            )],
        };
        let mut grid = Grid::new([0, 0, 0], [4, 2, 2], stone, air);
        place_in_world(
            &pal,
            &mut grid,
            &synthetic,
            Rot::None,
            [0, 0, 0],
            &[],
            &mut Worldgen::new(1),
            None,
        );
        assert_eq!(grid.get(0, 0, 0), stone);
        assert_eq!(grid.get(1, 0, 0), sand);
        assert!(grid.blocks.iter().all(|&s| s != void_state));

        let stair = pal
            .state(&serde_json::json!({
                "id": "minecraft:sandstone_stairs",
                "properties": {
                    "facing": "north",
                    "half": "bottom",
                    "shape": "straight",
                    "waterlogged": "false"
                }
            }))
            .unwrap();
        let turned = rotate_state(&pal, stair, Rot::Cw90);
        assert_eq!(pal.prop(turned, "facing"), Some("east"));
        assert_eq!(pal.prop(turned, "shape"), Some("straight"));
        assert_eq!(
            pal.prop(rotate_state(&pal, stair, Rot::Cw180), "facing"),
            Some("south")
        );

        let skull = load_template(&data, &pal, "minecraft:fossil/skull_1").unwrap();
        let bone = skull.palettes[0]
            .iter()
            .find(|p| pal.prop(p.state, "axis") == Some("x"))
            .unwrap();
        let mut grid = Grid::new([-16, 0, -16], [40, 16, 40], stone, air);
        place_in_world(
            &pal,
            &mut grid,
            &skull,
            Rot::Cw90,
            [0, 2, 0],
            &[],
            &mut Worldgen::new(1),
            None,
        );
        let at = transform(Rot::Cw90, bone.pos[0], bone.pos[1], bone.pos[2]);
        assert_eq!(
            pal.prop(grid.get(at[0], 2 + at[1], at[2]), "axis"),
            Some("z")
        );

        // Сид 7: один `nextInt(1)` палитры, затем `nextFloat` на каждый блок.
        // 0.9 оставляет 116 из 127. Число снято с этого же генератора и
        // закреплено, чтобы лишний бросок сдвинул тест.
        const SEED: i64 = 7;
        const KEPT: usize = 116;
        let mut sim = Worldgen::new(SEED);
        assert_eq!(sim.next_int_bound(1), 0);
        let mut expect = 0;
        for _ in &well.palettes[0] {
            if sim.next_float() <= 0.9 {
                expect += 1;
            }
        }
        assert_eq!(expect, KEPT);
        let mut grid = Grid::new([-8, 0, -8], [24, 24, 24], stone, air);
        let rot = Processor::Rot {
            integrity: 0.9,
            rottable: None,
        };
        place_in_world(
            &pal,
            &mut grid,
            &well,
            Rot::None,
            [2, 4, 2],
            &[rot],
            &mut Worldgen::new(SEED),
            None,
        );
        let written = grid.blocks.iter().filter(|&&s| s != stone).count();
        assert_eq!(written, KEPT);
    }

    /// Лут контейнеров шаблона: ванила пишет `LootTable` из шаблона и семя
    /// `random.nextLong()` каждой `RandomizableContainer`, а не готовые
    /// предметы. Проверяем и то, и то на настоящих шаблонах структур.
    #[test]
    fn template_containers_get_loot_table_and_seed() {
        let Some((data, pal)) = palette() else {
            return;
        };
        let stone = pal.state(&Value::String("minecraft:stone".into())).unwrap();
        let air = pal.state(&Value::String("minecraft:air".into())).unwrap();
        let cases = [
            (
                "minecraft:ruined_portal/portal_1",
                "minecraft:chests/ruined_portal",
            ),
            (
                "minecraft:village/plains/houses/plains_small_house_1",
                "minecraft:chests/village/village_plains_house",
            ),
        ];
        for (name, table) in cases {
            let Ok(template) = load_template(&data, &pal, name) else {
                continue;
            };
            let mut grid = Grid::new([-24, 0, -24], [64, 32, 64], stone, air);
            place_in_world(
                &pal,
                &mut grid,
                &template,
                Rot::None,
                [0, 2, 0],
                &[],
                &mut Worldgen::new(31),
                None,
            );
            let chests: Vec<_> = template.palettes[0]
                .iter()
                .filter(|p| {
                    pal.block_index(p.state) == pal.blocks.get("minecraft:chest").map(|b| b.0)
                })
                .collect();
            if chests.is_empty() {
                continue;
            }
            for piece in &chests {
                let world = [piece.pos[0], piece.pos[1] + 2, piece.pos[2]];
                let hit = grid
                    .entities
                    .iter()
                    .find(|(pos, _, _)| *pos == world)
                    .unwrap_or_else(|| panic!("{name}: нет блок-сущности в {world:?}"));
                assert_eq!(hit.1, "minecraft:chest", "{name}");
                let tag = hit.2.get("LootTable").and_then(|t| t.as_str());
                assert_eq!(tag, Some(table), "{name}: таблица");
                let seed = hit.2.get("LootTableSeed").and_then(|t| t.as_i64());
                assert!(seed.is_some_and(|s| s != 0), "{name}: семя {seed:?}");
            }
            assert_eq!(grid.entities.len(), chests.len(), "{name}: лишние NBT");
        }
    }

    #[test]
    fn meeting_point_has_jigsaw_pools() {
        let Some((data, pal)) = palette() else {
            return;
        };
        let hall = load_template(
            &data,
            &pal,
            "minecraft:village/plains/town_centers/plains_meeting_point_1",
        )
        .unwrap();
        let pools: Vec<&str> = hall.jigsaws[0].iter().map(|j| &*j.pool).collect();
        assert!(
            pools.iter().any(|p| p.contains("village/plains")),
            "{pools:?}"
        );
    }
}

thread_local! {
    /// PROF: время `shuffled_jigsaws`, мс.
    pub(crate) static JIG: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}
