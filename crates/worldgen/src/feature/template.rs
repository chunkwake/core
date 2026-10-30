//! NBT-шаблоны `minecraft:template` и `minecraft:fossil`.
//!
//! Постановка повторяет `StructureTemplate.placeInWorld` 26.3: палитра
//! выбирается `nextInt`, блоки идут сплошные → прочие → с NBT, каждый
//! список отсортирован по y, x, z. `structure_void` и `jigsaw` не пишутся.
//! Воздух пишется и заменяет блок мира. Лут археологии — данные
//! блок-сущности, конвейер хранит только состояния.

use super::{Bits, Fluid, Heightmap, Level, Palette, Predicate, Worldgen};
use crate::random::Legacy;
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
    nbt: bool,
    /// `mode=DATA` у структурного блока. Сортировку куска не меняет.
    marker: Option<String>,
}

/// Jigsaw в порядке блоков файла, до сортировки постановки.
#[derive(Clone, Debug)]
pub(crate) struct StoredJig {
    pub pos: [i32; 3],
    pub orientation: String,
    pub rollable: bool,
    pub name: String,
    pub target: String,
    pub pool: String,
    pub placement_priority: i32,
    pub selection_priority: i32,
    pub final_state: u32,
}

/// Jigsaw после поворота: позиция мировая, фронт уже повёрнут.
#[derive(Clone, Debug)]
pub(crate) struct JigInfo {
    pub pos: [i32; 3],
    pub front: String,
    pub top: String,
    pub rollable: bool,
    pub name: Option<String>,
    pub target: String,
    pub pool: String,
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
}

#[derive(Clone, Debug)]
enum RuleInput {
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
enum RuleAt {
    Pred(Predicate),
    Block(u32),
}

#[derive(Clone, Debug)]
pub(super) struct ProcRule {
    input: RuleInput,
    at: RuleAt,
    output: u32,
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
                    } else {
                        RuleAt::Pred(Predicate::parse(loc, palette)?)
                    };
                    rules.push(ProcRule {
                        input,
                        at,
                        output: palette.state(&rule["output_state"])?,
                    });
                }
                out.push(Processor::Rule(rules));
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
    let root = nbt::read_gzip_file(&file)?.ok_or_else(|| format!("no template {id}"))?;
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
                nbt: block.get("nbt").is_some(),
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
    load_template(data_dir, palette, id)
}

/// Размер шаблона до поворота.
pub(crate) fn size_public(
    data_dir: &Path,
    palette: &Palette,
    id: &str,
) -> Result<[i32; 3], String> {
    Ok(load_template(data_dir, palette, id)?.size)
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
    for pieces in &template.palettes {
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
                has_nbt: piece.nbt,
            });
        }
        // Ванила берёт одну палитру по позиции; для команды первая — верная.
        break;
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
    Ok(StoredJig {
        pos,
        orientation,
        rollable,
        name: ident("name"),
        target: ident("target"),
        pool: ident("pool"),
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
        if piece.nbt {
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
    if let Some(values) = tag.as_int_array() {
        if values.len() >= 3 {
            return Some([values[0], values[1], values[2]]);
        }
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
        if piece.nbt && container(pal, piece.state) {
            r.next_long();
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
    for (name, (id, _)) in &pal.blocks {
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

fn container(pal: &Palette, state: u32) -> bool {
    let block = pal.block_index(state);
    [
        "minecraft:chest",
        "minecraft:trapped_chest",
        "minecraft:barrel",
    ]
    .into_iter()
    .any(|name| pal.blocks.get(name).is_some_and(|b| block == Some(b.0)))
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
            state = processor.apply(pal, level, current, world, r, false);
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
                    if rule.input.holds(pal, state, &mut rng) && rule.at.holds(pal, level, world) {
                        return Some(rule.output);
                    }
                }
                Some(state)
            }
        }
    }
}

impl RuleInput {
    fn holds(&self, pal: &Palette, state: u32, r: &mut impl Bits) -> bool {
        let block = pal.block_index(state);
        match self {
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
    for piece in blocks {
        let shifted = transform(rot, piece.pos[0], piece.pos[1], piece.pos[2]);
        let mut world = [
            place[0] + shifted[0],
            place[1] + shifted[1],
            place[2] + shifted[2],
        ];
        if !window.contains(world) {
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
            match processor.apply(pal, level, state, world, r, true) {
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
        if !window.contains(world) {
            continue;
        }
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
    }
    true
}

/// `getShuffledJigsawBlocks`: палитра от позиции, затем `Util.shuffle` и
/// стабильная сортировка по `selection_priority` по убыванию.
pub(crate) fn shuffled_jigsaws(
    template: &Template,
    rot: Rot,
    place: [i32; 3],
    r: &mut impl Bits,
) -> Vec<JigInfo> {
    if template.jigsaws.is_empty() {
        return Vec::new();
    }
    let index = palette_index(template.jigsaws.len(), place);
    let mut list: Vec<JigInfo> = template.jigsaws[index]
        .iter()
        .map(|jig| {
            let shifted = transform(rot, jig.pos[0], jig.pos[1], jig.pos[2]);
            let orient = rotate_orientation(&jig.orientation, rot)
                .unwrap_or_else(|| jig.orientation.clone());
            let (front, top) = orient.split_once('_').unwrap_or(("north", "up"));
            JigInfo {
                pos: [
                    place[0] + shifted[0],
                    place[1] + shifted[1],
                    place[2] + shifted[2],
                ],
                front: front.to_owned(),
                top: top.to_owned(),
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
    list.sort_by(|a, b| b.selection_priority.cmp(&a.selection_priority));
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

pub(super) fn rotate_state(pal: &Palette, state: u32, rot: Rot) -> u32 {
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
                        nbt: false,
                        marker: None,
                    },
                    Piece {
                        pos: [1, 0, 0],
                        state: sand,
                        nbt: false,
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
        let pools: Vec<&str> = hall.jigsaws[0].iter().map(|j| j.pool.as_str()).collect();
        assert!(
            pools.iter().any(|p| p.contains("village/plains")),
            "{pools:?}"
        );
    }
}
