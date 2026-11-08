//! Руины порталов: выбор набора, шаблон, высота и пост-обработка (`netherrack`,
//! капельные столбы, лианы и листва).
//!
//! Сундуки в шаблонах нет, поэтому лут не съедается. Мобы не пишутся.

use super::structure::{Base, Columns};
use super::template::{self, Bounds, Rot, Template};
use super::{Bits, Heightmap, Legacy, Level, Palette, Worldgen};
use crate::templates::Mirror;
use std::sync::Arc;

/// Портал и его свойства из `setups` структуры.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Setup {
    pub placement: Placement,
    pub air_pocket: f32,
    pub mossiness: f32,
    pub overgrown: bool,
    pub vines: bool,
    pub can_be_cold: bool,
    /// `Properties.cold`: `can_be_cold` и холодный биом в `origin`.
    pub cold: bool,
    pub blackstone: bool,
    pub weight: f32,
}

/// `RuinedPortalPiece.VerticalPlacement`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Placement {
    OnLandSurface,
    PartlyBuried,
    OnOceanFloor,
    InMountain,
    Underground,
    InNether,
}

impl Placement {
    pub(crate) fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "on_land_surface" => Self::OnLandSurface,
            "partly_buried" => Self::PartlyBuried,
            "on_ocean_floor" => Self::OnOceanFloor,
            "in_mountain" => Self::InMountain,
            "underground" => Self::Underground,
            "in_nether" => Self::InNether,
            _ => return None,
        })
    }

    /// `getHeightMapType`: у дна океана своя карта.
    fn heightmap(self) -> Heightmap {
        if self == Self::OnOceanFloor {
            Heightmap::OceanFloorWg
        } else {
            Heightmap::WorldSurfaceWg
        }
    }
}

const PORTALS: [&str; 10] = [
    "minecraft:ruined_portal/portal_1",
    "minecraft:ruined_portal/portal_2",
    "minecraft:ruined_portal/portal_3",
    "minecraft:ruined_portal/portal_4",
    "minecraft:ruined_portal/portal_5",
    "minecraft:ruined_portal/portal_6",
    "minecraft:ruined_portal/portal_7",
    "minecraft:ruined_portal/portal_8",
    "minecraft:ruined_portal/portal_9",
    "minecraft:ruined_portal/portal_10",
];

const GIANT_PORTALS: [&str; 3] = [
    "minecraft:ruined_portal/giant_portal_1",
    "minecraft:ruined_portal/giant_portal_2",
    "minecraft:ruined_portal/giant_portal_3",
];

pub(crate) struct Loaded {
    pub templates: Vec<Arc<Template>>,
    pub giants: Vec<Arc<Template>>,
}

pub(crate) fn load(data_dir: &std::path::Path, palette: &Palette) -> Result<Loaded, String> {
    let mut templates = Vec::with_capacity(PORTALS.len());
    for id in PORTALS {
        templates.push(Arc::new(template::load_template(data_dir, palette, id)?));
    }
    let mut giants = Vec::with_capacity(GIANT_PORTALS.len());
    for id in GIANT_PORTALS {
        giants.push(Arc::new(template::load_template(data_dir, palette, id)?));
    }
    Ok(Loaded { templates, giants })
}

/// Один кусок портала: шаблон, мировое начало, зеркало и поворот.
#[derive(Clone, Debug)]
pub(crate) struct Piece {
    pub template: Arc<Template>,
    pub origin: [i32; 3],
    pub pivot: [i32; 3],
    pub mirror: Mirror,
    pub rot: Rot,
    pub min: [i32; 3],
    pub max: [i32; 3],
    pub placement: Placement,
    pub setup: Setup,
}

impl Piece {
    pub(crate) fn bounds(&self) -> ([i32; 3], [i32; 3]) {
        (self.min, self.max)
    }

    pub(crate) fn place(
        &self,
        pal: &Palette,
        level: &mut dyn Level,
        rng: &mut Worldgen,
        window: Bounds,
    ) {
        // `postProcess` вызывается, только если центр коробки внутри чанка.
        let center = [
            self.min[0] + (self.max[0] - self.min[0] + 1) / 2,
            self.min[1] + (self.max[1] - self.min[1] + 1) / 2,
            self.min[2] + (self.max[2] - self.min[2] + 1) / 2,
        ];
        if !window.contains(center) {
            return;
        }
        let processors = portal_processors(pal, self.setup, self.placement);
        // `chunkBB.encapsulate(boundingBox)`: шаблон ставится целиком, броски
        // процессоров идут по всем его блокам.
        let window = Bounds {
            min: [0, 1, 2].map(|i| window.min[i].min(self.min[i])),
            max: [0, 1, 2].map(|i| window.max[i].max(self.max[i])),
        };
        template::place_with(
            pal,
            level,
            &self.template,
            self.origin,
            self.pivot,
            self.mirror,
            self.rot,
            &processors,
            rng,
            window,
        );
        let bbox = template::mirrored_bounds_pivot(
            self.origin,
            self.mirror,
            self.rot,
            self.template.size,
            self.pivot,
        );
        spread_netherrack(pal, level, rng, self.placement, self.setup, bbox);
        drip_columns(pal, level, rng, bbox, self.setup, self.placement);
        if self.setup.vines || self.setup.overgrown {
            for x in bbox.0[0]..=bbox.1[0] {
                for z in bbox.0[2]..=bbox.1[2] {
                    for y in bbox.0[1]..=bbox.1[1] {
                        if self.setup.vines {
                            maybe_vine(pal, level, rng, [x, y, z]);
                        }
                        if self.setup.overgrown {
                            maybe_leaves(pal, level, rng, [x, y, z]);
                        }
                    }
                }
            }
        }
    }
}

/// Процессоры куска в порядке ванильного `makeSettings`.
fn portal_processors(
    pal: &Palette,
    setup: Setup,
    placement: Placement,
) -> Vec<template::Processor> {
    let air = pal.defaults["minecraft:air"];
    let netherrack = pal.defaults["minecraft:netherrack"];
    let magma = pal.defaults["minecraft:magma_block"];
    let mut rules = Vec::new();
    // `gold_block` с шансом 0.3 → воздух.
    rules.push(template::ProcRule {
        pos: template::PosTest::Always,
        loot: None,
        input: template::RuleInput::Random {
            block: pal.blocks["minecraft:gold_block"].0,
            chance: 0.3,
        },
        at: template::RuleAt::Always,
        output: air,
    });
    // Лава: на дне океана магма, в холоде незеррак, иначе магма с шансом 0.2.
    let lava = pal.blocks["minecraft:lava"].0;
    if placement == Placement::OnOceanFloor {
        rules.push(template::ProcRule {
            pos: template::PosTest::Always,
            loot: None,
            input: template::RuleInput::Block(lava),
            at: template::RuleAt::Always,
            output: magma,
        });
    } else if setup_cold(setup) {
        rules.push(template::ProcRule {
            pos: template::PosTest::Always,
            loot: None,
            input: template::RuleInput::Block(lava),
            at: template::RuleAt::Always,
            output: netherrack,
        });
    } else {
        rules.push(template::ProcRule {
            pos: template::PosTest::Always,
            loot: None,
            input: template::RuleInput::Random {
                block: lava,
                chance: 0.2,
            },
            at: template::RuleAt::Always,
            output: magma,
        });
    }
    // Незеррак шаблона в тепле → магма с шансом 0.07.
    if !setup_cold(setup) {
        rules.push(template::ProcRule {
            pos: template::PosTest::Always,
            loot: None,
            input: template::RuleInput::Random {
                block: pal.blocks["minecraft:netherrack"].0,
                chance: 0.07,
            },
            at: template::RuleAt::Always,
            output: magma,
        });
    }
    let mut out = vec![template::Processor::IgnoreStructureBlock {
        air_pocket: setup.air_pocket,
    }];
    out.push(template::Processor::Rule(rules));
    out.push(template::Processor::Age(setup.mossiness));
    out.push(template::Processor::LavaSubmerged);
    if setup.blackstone {
        out.push(template::Processor::Blackstone);
    }
    out
}

/// `sample(random, airPocketProbability)`.
fn sample(rng: &mut impl Bits, limit: f32) -> bool {
    if limit == 0.0 {
        return false;
    }
    if limit == 1.0 {
        return true;
    }
    rng.next_float() < limit
}

/// `Properties.cold`, посчитанный при сборке старта.
fn setup_cold(setup: Setup) -> bool {
    setup.cold
}

/// `findGenerationPoint`: выбор набора, шаблон, высота и начало.
pub(crate) fn build(
    loaded: &Loaded,
    setups: &[Setup],
    sx: i32,
    sz: i32,
    seed: i64,
    columns: &dyn Columns,
) -> Option<Piece> {
    let mut rng = Legacy::new(0);
    rng.set_large_feature_seed(seed, sx, sz);
    let setup = pick_setup(&mut rng, setups)?;
    let air_pocket = sample(&mut rng, setup.air_pocket);
    let giant = rng.next_float() < 0.05;
    let template = if giant {
        let index = rng.next_int_bound(GIANT_PORTALS.len() as i32) as usize;
        Arc::clone(&loaded.giants[index])
    } else {
        let index = rng.next_int_bound(PORTALS.len() as i32) as usize;
        Arc::clone(&loaded.templates[index])
    };
    let rot = template::ROTS[rng.next_int_bound(4) as usize];
    let mirror = if rng.next_float() < 0.5 {
        Mirror::None
    } else {
        Mirror::FrontBack
    };
    let pivot = [template.size[0] / 2, 0, template.size[2] / 2];
    let base = [sx * 16, 0, sz * 16];
    let (mut min, mut max) =
        template::mirrored_bounds_pivot(base, mirror, rot, template.size, pivot);
    let center = [
        min[0] + (max[0] - min[0] + 1) / 2,
        min[1] + (max[1] - min[1] + 1) / 2,
        min[2] + (max[2] - min[2] + 1) / 2,
    ];
    let surface = match setup.placement.heightmap() {
        Heightmap::OceanFloorWg => columns.ocean_floor(center[0], center[2]),
        _ => columns.base_height(center[0], center[2]),
    } - 1;
    let span = max[1] - min[1] + 1;
    let y = suitable_y(
        &mut rng,
        setup.placement,
        air_pocket,
        surface,
        span,
        (min, max),
        columns,
        setup,
    );
    let dy = y - min[1];
    min[1] += dy;
    max[1] += dy;
    Some(Piece {
        template,
        origin: [base[0], y, base[2]],
        pivot,
        mirror,
        rot,
        min,
        max,
        placement: setup.placement,
        setup,
    })
}

fn pick_setup(rng: &mut impl Bits, setups: &[Setup]) -> Option<Setup> {
    if setups.len() > 1 {
        let total: f32 = setups.iter().map(|s| s.weight).sum();
        let mut pick = rng.next_float();
        for setup in setups {
            pick -= setup.weight / total;
            if pick < 0.0 {
                return Some(*setup);
            }
        }
        None
    } else {
        setups.first().copied()
    }
}

/// `findSuitableY`: стартовая высота по типу размещения, затем спуск до трёх
/// углов на твёрдом блоке.
#[allow(clippy::too_many_arguments)]
fn suitable_y(
    rng: &mut Legacy,
    placement: Placement,
    air_pocket: bool,
    surface: i32,
    span: i32,
    bbox: ([i32; 3], [i32; 3]),
    columns: &dyn Columns,
    setup: Setup,
) -> i32 {
    let min_y = columns.min_y() + 15;
    let mut y = match placement {
        Placement::InNether => {
            if air_pocket {
                rng.between_inclusive(32, 100)
            } else if rng.next_float() < 0.5 {
                rng.between_inclusive(27, 29)
            } else {
                rng.between_inclusive(29, 100)
            }
        }
        Placement::InMountain => {
            let max_y = surface - span;
            random_within(rng, 70, max_y)
        }
        Placement::Underground => {
            let max_y = surface - span;
            random_within(rng, min_y, max_y)
        }
        Placement::PartlyBuried => surface - span + rng.between_inclusive(2, 8),
        _ => surface,
    };
    let floor = placement.heightmap() == Heightmap::OceanFloorWg;
    // `getBaseColumn` по четырём нижним углам: `WORLD_SURFACE_WG` — не воздух,
    // `OCEAN_FLOOR_WG` — блок по умолчанию (жидкость не держит).
    let corners = [
        [bbox.0[0], bbox.0[2]],
        [bbox.1[0], bbox.0[2]],
        [bbox.0[0], bbox.1[2]],
        [bbox.1[0], bbox.1[2]],
    ];
    let opaque = |x: i32, z: i32, yy: i32| -> bool {
        match columns.base_block(x, yy, z) {
            None | Some(Base::Air) => false,
            Some(Base::Fluid) => !floor,
            Some(Base::Solid) => true,
        }
    };
    let _ = setup;
    while y > min_y {
        let mut count = 0;
        for [x, z] in corners {
            if opaque(x, z, y) {
                count += 1;
                if count == 3 {
                    return y;
                }
            }
        }
        y -= 1;
    }
    y
}

/// `getRandomWithinInterval`: бросок только при `min < max`.
fn random_within(rng: &mut impl Bits, min: i32, max: i32) -> i32 {
    if min < max {
        rng.between_inclusive(min, max)
    } else {
        max
    }
}

const NETHERRACK_CHANCE: [f32; 14] = [
    1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.9, 0.9, 0.8, 0.7, 0.6, 0.4, 0.2,
];

/// `spreadNetherrack`: пятно вокруг центра коробки.
fn spread_netherrack(
    pal: &Palette,
    level: &mut dyn Level,
    rng: &mut Worldgen,
    placement: Placement,
    setup: Setup,
    bbox: ([i32; 3], [i32; 3]),
) {
    let follow = matches!(
        placement,
        Placement::OnLandSurface | Placement::OnOceanFloor
    );
    let center = [
        bbox.0[0] + (bbox.1[0] - bbox.0[0] + 1) / 2,
        bbox.0[1] + (bbox.1[1] - bbox.0[1] + 1) / 2,
        bbox.0[2] + (bbox.1[2] - bbox.0[2] + 1) / 2,
    ];
    let max_distance = NETHERRACK_CHANCE.len() as i32;
    let x_span = bbox.1[0] - bbox.0[0] + 1;
    let z_span = bbox.1[2] - bbox.0[2] + 1;
    let average_width = (x_span + z_span) / 2;
    let adjust = rng.next_int_bound((8 - average_width / 2).max(1));
    for x in center[0] - max_distance..=center[0] + max_distance {
        for z in center[2] - max_distance..=center[2] + max_distance {
            let distance = (x - center[0]).abs() + (z - center[2]).abs();
            let adjusted = (distance + adjust).max(0);
            if adjusted >= max_distance {
                continue;
            }
            let chance = NETHERRACK_CHANCE[adjusted as usize];
            if rng.next_double() >= chance as f64 {
                continue;
            }
            let surface = level.surface(placement.heightmap(), x, z) - 1;
            let y = if follow {
                surface
            } else {
                bbox.0[1].min(surface)
            };
            let pos = [x, y, z];
            if (y - bbox.0[1]).abs() > 3 || !replaceable_as_netherrack(pal, level, pos, placement) {
                continue;
            }
            place_netherrack(pal, level, rng, pos, setup);
            if setup.overgrown {
                maybe_leaves(pal, level, rng, pos);
            }
            drip_column(
                pal,
                level,
                rng,
                [pos[0], pos[1] - 1, pos[2]],
                setup,
                placement,
            );
        }
    }
}

fn replaceable_as_netherrack(
    pal: &Palette,
    level: &dyn Level,
    pos: [i32; 3],
    placement: Placement,
) -> bool {
    let state = level.get(pos[0], pos[1], pos[2]);
    let is = |name: &str| pal.blocks.get(name).map(|b| b.0) == pal.block_index(state);
    // `!state.is(Blocks.AIR)`: пещерный воздух карвера заменяется.
    if is("minecraft:air") || is("minecraft:obsidian") {
        return false;
    }
    if placement != Placement::InNether && is("minecraft:lava") {
        return false;
    }
    !protected_state(pal, state)
}

/// `#minecraft:features_cannot_replace`.
fn protected_state(pal: &Palette, state: u32) -> bool {
    static MASK: std::sync::OnceLock<Arc<Vec<bool>>> = std::sync::OnceLock::new();
    let _ = MASK;
    let _ = state;
    PROTECTED.with(|mask| {
        let mut slot = mask.borrow_mut();
        if slot.is_none() {
            *slot = Some(build_protected(pal));
        }
        slot.as_ref()
            .is_some_and(|m| m.get(state as usize).copied().unwrap_or(false))
    })
}

thread_local! {
    static PROTECTED: std::cell::RefCell<Option<Arc<Vec<bool>>>> =
        const { std::cell::RefCell::new(None) };
}

fn build_protected(pal: &Palette) -> Arc<Vec<bool>> {
    let mut set = std::collections::HashSet::new();
    for name in [
        "minecraft:bedrock",
        "minecraft:spawner",
        "minecraft:chest",
        "minecraft:end_portal",
        "minecraft:end_portal_frame",
        "minecraft:nether_portal",
        "minecraft:obsidian",
        "minecraft:crying_obsidian",
        "minecraft:reinforced_deepslate",
        "minecraft:barrier",
        "minecraft:structure_block",
        "minecraft:jigsaw",
        "minecraft:command_block",
        "minecraft:chain_command_block",
        "minecraft:repeating_command_block",
        "minecraft:structure_void",
        "minecraft:light",
    ] {
        if let Some((id, _)) = pal.blocks.get(name) {
            set.insert(*id);
        }
    }
    Arc::new(pal.block_of.iter().map(|b| set.contains(b)).collect())
}

fn place_netherrack(
    pal: &Palette,
    level: &mut dyn Level,
    rng: &mut Worldgen,
    pos: [i32; 3],
    setup: Setup,
) {
    if !replaceable_as_netherrack(pal, level, pos, setup.placement) {
        return;
    }
    let state = if !setup_cold(setup) && rng.next_float() < 0.07 {
        pal.defaults["minecraft:magma_block"]
    } else {
        pal.defaults["minecraft:netherrack"]
    };
    level.set(pos[0], pos[1], pos[2], state);
}

/// `addNetherrackDripColumnsBelowPortal`: столбцы под незерраком на полу.
fn drip_columns(
    pal: &Palette,
    level: &mut dyn Level,
    rng: &mut Worldgen,
    bbox: ([i32; 3], [i32; 3]),
    setup: Setup,
    placement: Placement,
) {
    let netherrack = pal.defaults["minecraft:netherrack"];
    for x in bbox.0[0] + 1..bbox.1[0] {
        for z in bbox.0[2] + 1..bbox.1[2] {
            let pos = [x, bbox.0[1], z];
            if level.get(pos[0], pos[1], pos[2]) != netherrack {
                continue;
            }
            drip_column(pal, level, rng, [x, bbox.0[1] - 1, z], setup, placement);
        }
    }
}

fn drip_column(
    pal: &Palette,
    level: &mut dyn Level,
    rng: &mut Worldgen,
    pos: [i32; 3],
    setup: Setup,
    placement: Placement,
) {
    place_netherrack_at(pal, level, rng, pos, setup, placement);
    let mut y = pos[1];
    let mut cap = 8;
    while cap > 0 && rng.next_float() < 0.5 {
        y -= 1;
        place_netherrack_at(pal, level, rng, [pos[0], y, pos[2]], setup, placement);
        cap -= 1;
    }
}

fn place_netherrack_at(
    pal: &Palette,
    level: &mut dyn Level,
    rng: &mut Worldgen,
    pos: [i32; 3],
    setup: Setup,
    placement: Placement,
) {
    if !replaceable_as_netherrack(pal, level, pos, placement) {
        return;
    }
    let state = if !setup_cold(setup) && rng.next_float() < 0.07 {
        pal.defaults["minecraft:magma_block"]
    } else {
        pal.defaults["minecraft:netherrack"]
    };
    level.set(pos[0], pos[1], pos[2], state);
}

/// `maybeAddVines`: лиана в пустом соседе при полной грани.
fn maybe_vine(pal: &Palette, level: &mut dyn Level, rng: &mut Worldgen, pos: [i32; 3]) {
    let state = level.get(pos[0], pos[1], pos[2]);
    if pal.is_air(state) || is_block(pal, state, "minecraft:vine") {
        return;
    }
    // `getRandomHorizontalDirection`: север, восток, юг, запад.
    let dirs = [
        ("north", [0, 0, -1]),
        ("east", [1, 0, 0]),
        ("south", [0, 0, 1]),
        ("west", [-1, 0, 0]),
    ];
    let (_, delta) = dirs[rng.next_int_bound(4) as usize];
    let neighbour = [pos[0] + delta[0], pos[1], pos[2] + delta[2]];
    if !pal.is_air(level.get(neighbour[0], neighbour[1], neighbour[2])) {
        return;
    }
    if !pal.is_full_cube(state) {
        return;
    }
    let face = match delta {
        [0, 0, -1] => "south",
        [1, 0, 0] => "west",
        [0, 0, 1] => "north",
        _ => "east",
    };
    if let Some(next) = pal.with_prop(pal.defaults["minecraft:vine"], face, "true") {
        level.set(neighbour[0], neighbour[1], neighbour[2], next);
    }
}

/// `maybeAddLeavesAbove`: листва над незерраком с шансом 0.5.
fn maybe_leaves(pal: &Palette, level: &mut dyn Level, rng: &mut Worldgen, pos: [i32; 3]) {
    if rng.next_float() >= 0.5 {
        return;
    }
    if !is_block(
        pal,
        level.get(pos[0], pos[1], pos[2]),
        "minecraft:netherrack",
    ) {
        return;
    }
    let above = [pos[0], pos[1] + 1, pos[2]];
    if !pal.is_air(level.get(above[0], above[1], above[2])) {
        return;
    }
    let mut state = pal.defaults["minecraft:jungle_leaves"];
    if let Some(next) = pal.with_prop(state, "persistent", "true") {
        state = next;
    }
    level.set(above[0], above[1], above[2], state);
}

fn is_block(pal: &Palette, state: u32, name: &str) -> bool {
    pal.blocks.get(name).map(|b| b.0) == pal.block_index(state)
}
