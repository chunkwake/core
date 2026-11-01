//! Океанские руины: тёплые и холодные, одиночные и гроздью.
//!
//! Всё — шаблоны `.nbt` с процессорами `block_rot` и `block_ignore`. Высота
//! куска считается в `postProcess` по живому рельефу, поэтому `origin` тут —
//! только XZ, а Y выставляется при постановке.

use super::structure::Columns;
use super::template::{self, Bounds, Rot, Template};
use super::{Bits, Legacy, Level, Palette, Worldgen};
use crate::templates::Mirror;
use std::sync::Arc;

pub(crate) const WARM: [&str; 8] = [
    "minecraft:underwater_ruin/warm_1",
    "minecraft:underwater_ruin/warm_2",
    "minecraft:underwater_ruin/warm_3",
    "minecraft:underwater_ruin/warm_4",
    "minecraft:underwater_ruin/warm_5",
    "minecraft:underwater_ruin/warm_6",
    "minecraft:underwater_ruin/warm_7",
    "minecraft:underwater_ruin/warm_8",
];

pub(crate) const BRICK: [&str; 8] = [
    "minecraft:underwater_ruin/brick_1",
    "minecraft:underwater_ruin/brick_2",
    "minecraft:underwater_ruin/brick_3",
    "minecraft:underwater_ruin/brick_4",
    "minecraft:underwater_ruin/brick_5",
    "minecraft:underwater_ruin/brick_6",
    "minecraft:underwater_ruin/brick_7",
    "minecraft:underwater_ruin/brick_8",
];

pub(crate) const CRACKED: [&str; 8] = [
    "minecraft:underwater_ruin/cracked_1",
    "minecraft:underwater_ruin/cracked_2",
    "minecraft:underwater_ruin/cracked_3",
    "minecraft:underwater_ruin/cracked_4",
    "minecraft:underwater_ruin/cracked_5",
    "minecraft:underwater_ruin/cracked_6",
    "minecraft:underwater_ruin/cracked_7",
    "minecraft:underwater_ruin/cracked_8",
];

pub(crate) const MOSSY: [&str; 8] = [
    "minecraft:underwater_ruin/mossy_1",
    "minecraft:underwater_ruin/mossy_2",
    "minecraft:underwater_ruin/mossy_3",
    "minecraft:underwater_ruin/mossy_4",
    "minecraft:underwater_ruin/mossy_5",
    "minecraft:underwater_ruin/mossy_6",
    "minecraft:underwater_ruin/mossy_7",
    "minecraft:underwater_ruin/mossy_8",
];

pub(crate) const BIG_BRICK: [&str; 4] = [
    "minecraft:underwater_ruin/big_brick_1",
    "minecraft:underwater_ruin/big_brick_2",
    "minecraft:underwater_ruin/big_brick_3",
    "minecraft:underwater_ruin/big_brick_8",
];

pub(crate) const BIG_CRACKED: [&str; 4] = [
    "minecraft:underwater_ruin/big_cracked_1",
    "minecraft:underwater_ruin/big_cracked_2",
    "minecraft:underwater_ruin/big_cracked_3",
    "minecraft:underwater_ruin/big_cracked_8",
];

pub(crate) const BIG_MOSSY: [&str; 4] = [
    "minecraft:underwater_ruin/big_mossy_1",
    "minecraft:underwater_ruin/big_mossy_2",
    "minecraft:underwater_ruin/big_mossy_3",
    "minecraft:underwater_ruin/big_mossy_8",
];

pub(crate) const BIG_WARM: [&str; 4] = [
    "minecraft:underwater_ruin/big_warm_4",
    "minecraft:underwater_ruin/big_warm_5",
    "minecraft:underwater_ruin/big_warm_6",
    "minecraft:underwater_ruin/big_warm_7",
];

/// Все шаблоны руин по имени.
pub(crate) struct Loaded {
    templates: std::collections::HashMap<String, Arc<Template>>,
}

impl Loaded {
    fn get(&self, id: &str) -> Arc<Template> {
        Arc::clone(&self.templates[id])
    }
}

pub(crate) fn load(data_dir: &std::path::Path, palette: &Palette) -> Result<Loaded, String> {
    let mut templates = std::collections::HashMap::new();
    for list in [
        &WARM[..],
        &BRICK[..],
        &CRACKED[..],
        &MOSSY[..],
        &BIG_BRICK[..],
        &BIG_CRACKED[..],
        &BIG_MOSSY[..],
        &BIG_WARM[..],
    ] {
        for id in list {
            let loaded = template::load_template(data_dir, palette, id)?;
            let key = id.strip_prefix("minecraft:").unwrap_or(id).to_owned();
            templates.insert(key, Arc::new(loaded));
        }
    }
    Ok(Loaded { templates })
}

/// Настройки вида: тёплая или холодная, доли крупной руины и грозди.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Cfg {
    pub warm: bool,
    pub large_probability: f32,
    pub cluster_probability: f32,
}

/// Один кусок: шаблон, XZ начала, поворот, целостность.
#[derive(Clone, Debug)]
pub(crate) struct Piece {
    pub template: Arc<Template>,
    /// XZ и Y=90: настоящая высота считается при постановке.
    pub origin: [i32; 3],
    pub rot: Rot,
    pub integrity: f32,
    pub min: [i32; 3],
    pub max: [i32; 3],
    pub warm: bool,
    pub large: bool,
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
        world_seed: i64,
    ) {
        let warm = self.warm;
        // `postProcess`: высота по `OCEAN_FLOOR_WG` в точке начала, затем по
        // живому рельефу под всей коробкой.
        let y = level.surface(
            super::Heightmap::OceanFloorWg,
            self.origin[0],
            self.origin[2],
        );
        let corner_rel = template::transform(
            self.rot,
            self.template.size[0] - 1,
            0,
            self.template.size[2] - 1,
        );
        let corner = [
            self.origin[0] + corner_rel[0],
            y,
            self.origin[2] + corner_rel[2],
        ];
        let start = [self.origin[0], y, self.origin[2]];
        let y = ruin_height(pal, level, start, corner);
        let origin = [start[0], y, start[2]];
        let markers = template::place_ruin(
            pal,
            level,
            &self.template,
            origin,
            self.rot,
            self.integrity,
            warm,
            window,
            world_seed,
        );
        let sea = level.sea_level();
        for (pos, meta) in markers {
            let Some(meta) = meta else { continue };
            if !window.contains(pos) {
                continue;
            }
            match meta.as_str() {
                "chest" => {
                    if !level.can_write(pos[0], pos[2]) {
                        continue;
                    }
                    // `ChestBlock.WATERLOGGED`: вода на клетке заливает сундук.
                    let mut state = pal.defaults["minecraft:chest"];
                    if pal.fluid(level.get(pos[0], pos[1], pos[2])) == super::Fluid::Water
                        && let Some(next) = pal.with_prop(state, "waterlogged", "true")
                    {
                        state = next;
                    }
                    level.set(pos[0], pos[1], pos[2], state);
                    // `setLootTable(UNDERWATER_RUIN_BIG|SMALL, nextLong())`.
                    let seed = rng.next_long();
                    let table = if self.large {
                        "minecraft:chests/underwater_ruin_big"
                    } else {
                        "minecraft:chests/underwater_ruin_small"
                    };
                    super::template::set_loot(level, pos, table, seed);
                }
                "drowned" => {
                    if !level.can_write(pos[0], pos[2]) {
                        continue;
                    }
                    let state = if pos[1] > sea {
                        pal.defaults["minecraft:air"]
                    } else {
                        pal.defaults["minecraft:water"]
                    };
                    level.set(pos[0], pos[1], pos[2], state);
                }
                _ => {}
            }
        }
    }
}

/// `OceanRuinPiece.getHeight`: ровная площадка под руиной, если вокруг есть
/// заметная впадина.
fn ruin_height(pal: &Palette, level: &dyn Level, pos: [i32; 3], corner: [i32; 3]) -> i32 {
    let mut min_y = 512;
    let top = pos[1] - 1;
    let mut area = 0;
    for x in pos[0].min(corner[0])..=pos[0].max(corner[0]) {
        for z in pos[2].min(corner[2])..=pos[2].max(corner[2]) {
            let mut floor = pos[1] - 1;
            let mut state = level.get(x, floor, z);
            let mut fluid = pal.fluid(state);
            while (pal.is_air(state)
                || fluid == super::Fluid::Water
                || fluid == super::Fluid::FlowingWater
                || is_ice(pal, state))
                && floor > level.min_y() + 1
            {
                floor -= 1;
                state = level.get(x, floor, z);
                fluid = pal.fluid(state);
            }
            min_y = min_y.min(floor);
            if floor >= top - 2 {
                continue;
            }
            area += 1;
        }
    }
    let width = (pos[0] - corner[0]).abs();
    if top - min_y > 2 && area > width - 2 {
        min_y + 1
    } else {
        pos[1]
    }
}

fn is_ice(pal: &Palette, state: u32) -> bool {
    let Some(block) = pal.block_index(state) else {
        return false;
    };
    pal.blocks
        .get("minecraft:ice")
        .is_some_and(|b| b.0 == block)
}

/// `OceanRuinPieces.addPieces`.
pub(crate) fn build(loaded: &Loaded, cfg: Cfg, sx: i32, sz: i32, seed: i64) -> Vec<Piece> {
    let mut rng = Legacy::new(0);
    rng.set_large_feature_seed(seed, sx, sz);
    let large = rng.next_float() <= cfg.large_probability;
    let integrity = if large { 0.9 } else { 0.8 };
    let origin = [sx * 16, 90, sz * 16];
    let rot = template::ROTS[rng.next_int_bound(4) as usize];
    let mut out = Vec::new();
    add_piece(
        loaded, &mut out, &mut rng, cfg, origin, rot, large, integrity,
    );
    if large && rng.next_float() <= cfg.cluster_probability {
        cluster(loaded, &mut out, &mut rng, cfg, origin, rot);
    }
    out
}

/// `addPiece`: тёплая руина — один шаблон, холодная — три слоя.
#[allow(clippy::too_many_arguments)]
fn add_piece(
    loaded: &Loaded,
    out: &mut Vec<Piece>,
    rng: &mut Legacy,
    cfg: Cfg,
    origin: [i32; 3],
    rot: Rot,
    large: bool,
    integrity: f32,
) {
    if cfg.warm {
        let list = if large { &BIG_WARM[..] } else { &WARM[..] };
        let id = list[rng.next_int_bound(list.len() as i32) as usize];
        out.push(make(loaded, id, origin, rot, integrity, cfg.warm, large));
        return;
    }
    let bricks = if large { &BIG_BRICK[..] } else { &BRICK[..] };
    let cracked = if large {
        &BIG_CRACKED[..]
    } else {
        &CRACKED[..]
    };
    let mossy = if large { &BIG_MOSSY[..] } else { &MOSSY[..] };
    let idx = rng.next_int_bound(bricks.len() as i32) as usize;
    out.push(make(
        loaded,
        bricks[idx],
        origin,
        rot,
        integrity,
        cfg.warm,
        large,
    ));
    out.push(make(
        loaded,
        cracked[idx],
        origin,
        rot,
        0.7,
        cfg.warm,
        large,
    ));
    out.push(make(loaded, mossy[idx], origin, rot, 0.5, cfg.warm, large));
}

fn make(
    loaded: &Loaded,
    id: &str,
    origin: [i32; 3],
    rot: Rot,
    integrity: f32,
    warm: bool,
    large: bool,
) -> Piece {
    let key = id.strip_prefix("minecraft:").unwrap_or(id);
    let template = loaded.get(key);
    let (min, max) =
        template::mirrored_bounds_pivot(origin, Mirror::None, rot, template.size, [0, 0, 0]);
    Piece {
        template,
        origin,
        rot,
        integrity,
        min,
        max,
        warm,
        large,
    }
}

/// `addClusterRuins`: до пяти руин вокруг крупной, не задевая её коробку.
fn cluster(
    loaded: &Loaded,
    out: &mut Vec<Piece>,
    rng: &mut Legacy,
    cfg: Cfg,
    p: [i32; 3],
    rot: Rot,
) {
    let parent = [p[0], 90, p[2]];
    let rel = template::transform(rot, 15, 0, 15);
    let parent_corner = [parent[0] + rel[0], parent[1], parent[2] + rel[2]];
    let parent_bb = corners(parent, parent_corner);
    let bottom_left = [
        parent[0].min(parent_corner[0]),
        parent[1],
        parent[2].min(parent_corner[2]),
    ];
    let mut positions = all_positions(rng, bottom_left);
    let ruins = rng.between_inclusive(4, 8);
    for _ in 0..ruins {
        if positions.is_empty() {
            continue;
        }
        let idx = rng.next_int_bound(positions.len() as i32) as usize;
        let pos = positions.remove(idx);
        let next_rot = template::ROTS[rng.next_int_bound(4) as usize];
        let rel = template::transform(next_rot, 5, 0, 6);
        let next_corner = [pos[0] + rel[0], pos[1], pos[2] + rel[2]];
        let next_bb = corners(pos, next_corner);
        if intersects(next_bb, parent_bb) {
            continue;
        }
        add_piece(loaded, out, rng, cfg, pos, next_rot, false, 0.8);
    }
}

fn corners(a: [i32; 3], b: [i32; 3]) -> ([i32; 3], [i32; 3]) {
    (
        [a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2])],
        [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])],
    )
}

fn intersects(a: ([i32; 3], [i32; 3]), b: ([i32; 3], [i32; 3])) -> bool {
    (0..3).all(|i| a.0[i] <= b.1[i] && b.0[i] <= a.1[i])
}

/// `allPositions`: восемь заранее размеченных мест вокруг крупной руины.
fn all_positions(rng: &mut Legacy, origin: [i32; 3]) -> Vec<[i32; 3]> {
    // Восемь мест; порядок важен, он задаёт броски `Legacy`.
    let mut out = vec![[
        origin[0] - 16 + rng.between_inclusive(1, 8),
        origin[1],
        origin[2] + 16 + rng.between_inclusive(1, 7),
    ]];
    out.push([
        origin[0] - 16 + rng.between_inclusive(1, 8),
        origin[1],
        origin[2] + rng.between_inclusive(1, 7),
    ]);
    out.push([
        origin[0] - 16 + rng.between_inclusive(1, 8),
        origin[1],
        origin[2] - 16 + rng.between_inclusive(4, 8),
    ]);
    out.push([
        origin[0] + rng.between_inclusive(1, 7),
        origin[1],
        origin[2] + 16 + rng.between_inclusive(1, 7),
    ]);
    out.push([
        origin[0] + rng.between_inclusive(1, 7),
        origin[1],
        origin[2] - 16 + rng.between_inclusive(4, 6),
    ]);
    out.push([
        origin[0] + 16 + rng.between_inclusive(1, 7),
        origin[1],
        origin[2] + 16 + rng.between_inclusive(3, 8),
    ]);
    out.push([
        origin[0] + 16 + rng.between_inclusive(1, 7),
        origin[1],
        origin[2] + rng.between_inclusive(1, 7),
    ]);
    out.push([
        origin[0] + 16 + rng.between_inclusive(1, 7),
        origin[1],
        origin[2] - 16 + rng.between_inclusive(4, 8),
    ]);
    out
}

/// `couldValidBiomeExistOnTopOfChunkCenter` + биом старта.
pub(crate) fn can_start(
    columns: &dyn Columns,
    sx: i32,
    sz: i32,
    biomes: &std::collections::HashSet<u32>,
) -> Option<()> {
    let x = sx * 16 + 8;
    let z = sz * 16 + 8;
    if !columns.column_has(x, z, biomes) {
        return None;
    }
    let y = columns.ocean_floor(x, z) - 1;
    let biome = columns.biome_at(x, y, z)?;
    biomes.contains(&biome).then_some(())
}
