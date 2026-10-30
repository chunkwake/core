//! Иглу и кораблекрушение: один или несколько `TemplateStructurePiece`.
//! Сундуки остаются блоками, лут и житель из шаблона не пишутся.

use super::structure::Columns;
use super::template::{self, Bounds, ROTS, Rot, Template};
use super::{Bits, Heightmap, Legacy, Level, Palette, Worldgen};
use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};

const SHIP_PIVOT: [i32; 3] = [4, 0, 15];
const TOP_PIVOT: [i32; 3] = [3, 5, 5];
const MID_PIVOT: [i32; 3] = [1, 3, 1];
const BOT_PIVOT: [i32; 3] = [3, 6, 7];
const TOP_OFF: [i32; 3] = [0, 0, 0];
const MID_OFF: [i32; 3] = [2, -3, 4];
const BOT_OFF: [i32; 3] = [0, -3, -2];

const BEACHED: &[&str] = &[
    "minecraft:shipwreck/with_mast",
    "minecraft:shipwreck/sideways_full",
    "minecraft:shipwreck/sideways_fronthalf",
    "minecraft:shipwreck/sideways_backhalf",
    "minecraft:shipwreck/rightsideup_full",
    "minecraft:shipwreck/rightsideup_fronthalf",
    "minecraft:shipwreck/rightsideup_backhalf",
    "minecraft:shipwreck/with_mast_degraded",
    "minecraft:shipwreck/rightsideup_full_degraded",
    "minecraft:shipwreck/rightsideup_fronthalf_degraded",
    "minecraft:shipwreck/rightsideup_backhalf_degraded",
];

const OCEAN: &[&str] = &[
    "minecraft:shipwreck/with_mast",
    "minecraft:shipwreck/upsidedown_full",
    "minecraft:shipwreck/upsidedown_fronthalf",
    "minecraft:shipwreck/upsidedown_backhalf",
    "minecraft:shipwreck/sideways_full",
    "minecraft:shipwreck/sideways_fronthalf",
    "minecraft:shipwreck/sideways_backhalf",
    "minecraft:shipwreck/rightsideup_full",
    "minecraft:shipwreck/rightsideup_fronthalf",
    "minecraft:shipwreck/rightsideup_backhalf",
    "minecraft:shipwreck/with_mast_degraded",
    "minecraft:shipwreck/upsidedown_full_degraded",
    "minecraft:shipwreck/upsidedown_fronthalf_degraded",
    "minecraft:shipwreck/upsidedown_backhalf_degraded",
    "minecraft:shipwreck/sideways_full_degraded",
    "minecraft:shipwreck/sideways_fronthalf_degraded",
    "minecraft:shipwreck/sideways_backhalf_degraded",
    "minecraft:shipwreck/rightsideup_full_degraded",
    "minecraft:shipwreck/rightsideup_fronthalf_degraded",
    "minecraft:shipwreck/rightsideup_backhalf_degraded",
];

#[derive(Clone)]
pub(super) enum Body {
    Igloo {
        top: Arc<Template>,
        middle: Arc<Template>,
        bottom: Arc<Template>,
    },
    Ship {
        beached: bool,
        templates: Vec<Arc<Template>>,
    },
}

pub(super) fn load_igloo(data_dir: &Path, palette: &Palette) -> Result<Body, String> {
    Ok(Body::Igloo {
        top: load(data_dir, palette, "minecraft:igloo/top")?,
        middle: load(data_dir, palette, "minecraft:igloo/middle")?,
        bottom: load(data_dir, palette, "minecraft:igloo/bottom")?,
    })
}

pub(super) fn load_ship(data_dir: &Path, palette: &Palette, beached: bool) -> Result<Body, String> {
    let names = if beached { BEACHED } else { OCEAN };
    let mut templates = Vec::with_capacity(names.len());
    for name in names {
        templates.push(load(data_dir, palette, name)?);
    }
    Ok(Body::Ship { beached, templates })
}

fn load(data_dir: &Path, palette: &Palette, id: &str) -> Result<Arc<Template>, String> {
    Ok(Arc::new(template::load_template(data_dir, palette, id)?))
}

/// `isValidBiome` на занятой высоте середины чанка. Океан смотрит дно.
pub(super) fn surface_ok(
    columns: &dyn Columns,
    sx: i32,
    sz: i32,
    biomes: &HashSet<u32>,
    ocean: bool,
) -> Option<()> {
    let mid_x = sx * 16 + 8;
    let mid_z = sz * 16 + 8;
    let free = if ocean {
        columns.ocean_floor(mid_x, mid_z)
    } else {
        columns.base_height(mid_x, mid_z)
    };
    let biome = columns.biome_at(mid_x, free - 1, mid_z)?;
    biomes.contains(&biome).then_some(())
}

enum Role {
    Igloo {
        top: bool,
        offset: [i32; 3],
    },
    Ship {
        beached: bool,
        frozen: Mutex<Option<i32>>,
    },
}

pub(super) struct Piece {
    template: Arc<Template>,
    rot: Rot,
    pivot: [i32; 3],
    /// Позиция до сдвига по высоте. Иглу каждый раз считает от неё заново.
    origin: [i32; 3],
    min: [i32; 3],
    max: [i32; 3],
    role: Role,
}

impl Piece {
    pub(super) fn bounds(&self) -> ([i32; 3], [i32; 3]) {
        (self.min, self.max)
    }

    pub(super) fn place(
        &self,
        pal: &Palette,
        level: &mut dyn Level,
        decor: &mut Worldgen,
        window: Bounds,
    ) {
        let shifted = match &self.role {
            Role::Igloo { offset, .. } => {
                let at = entrance(self.origin, self.rot, self.pivot, *offset);
                let height = level.surface(Heightmap::WorldSurfaceWg, at[0], at[2]);
                [
                    self.origin[0],
                    self.origin[1] + height - 90 - 1,
                    self.origin[2],
                ]
            }
            Role::Ship { .. } => [self.origin[0], self.ship_y(level, decor), self.origin[2]],
        };
        let (ignore_air, waterlog) = match &self.role {
            Role::Igloo { .. } => (false, false),
            Role::Ship { .. } => (true, true),
        };
        let markers = template::place_anchored(
            pal,
            level,
            &self.template,
            self.rot,
            self.pivot,
            shifted,
            ignore_air,
            waterlog,
            decor,
            window,
        );
        let clear_chest = matches!(self.role, Role::Igloo { .. });
        for (meta, pos) in markers {
            if clear_chest
                && meta == "chest"
                && window.contains(pos)
                && level.can_write(pos[0], pos[2])
            {
                level.set(pos[0], pos[1], pos[2], block(pal, "minecraft:air"));
            }
            let loot = if clear_chest {
                meta == "chest"
            } else {
                matches!(
                    meta.as_str(),
                    "map_chest" | "treasure_chest" | "supply_chest"
                )
            };
            if !loot {
                continue;
            }
            let below = [pos[0], pos[1] - 1, pos[2]];
            if window.contains(below)
                && is_block(
                    pal,
                    level.get(below[0], below[1], below[2]),
                    "minecraft:chest",
                )
            {
                decor.next_long();
            }
        }
        let Role::Igloo { top: true, .. } = &self.role else {
            return;
        };
        let local = template::transform_pivot(self.rot, 3, 0, 5, self.pivot);
        let trap = [
            shifted[0] + local[0],
            shifted[1] + local[1],
            shifted[2] + local[2],
        ];
        let under = level.get(trap[0], trap[1] - 1, trap[2]);
        if !pal.is_air(under)
            && !is_block(pal, under, "minecraft:ladder")
            && window.contains(trap)
            && level.can_write(trap[0], trap[2])
        {
            level.set(
                trap[0],
                trap[1],
                trap[2],
                block(pal, "minecraft:snow_block"),
            );
        }
    }

    /// Первая запись замораживает Y. Берег крутит `nextInt(3)` один раз.
    fn ship_y(&self, level: &dyn Level, decor: &mut Worldgen) -> i32 {
        let Role::Ship { beached, frozen } = &self.role else {
            return self.origin[1];
        };
        let mut slot = frozen.lock().unwrap();
        if let Some(y) = *slot {
            return y;
        }
        let sx = self.template.size[0];
        let sz = self.template.size[2];
        let map = if *beached {
            Heightmap::WorldSurfaceWg
        } else {
            Heightmap::OceanFloorWg
        };
        let mut sum = 0i32;
        let mut lowest = i32::MAX;
        let base = sx * sz;
        if base == 0 {
            let y = level.surface(map, self.origin[0], self.origin[2]);
            *slot = Some(y);
            return y;
        }
        for dz in 0..sz {
            for dx in 0..sx {
                let h = level.surface(map, self.origin[0] + dx, self.origin[2] + dz);
                sum += h;
                lowest = lowest.min(h);
            }
        }
        let y = if *beached {
            lowest - self.template.size[1] / 2 - decor.next_int_bound(3)
        } else {
            sum / base
        };
        *slot = Some(y);
        y
    }
}

/// Поворот — первый `nextInt(4)`. Провал биома сюда не доходит.
pub(super) fn build(
    body: &Body,
    sx: i32,
    sz: i32,
    rng: &mut Legacy,
    columns: &dyn Columns,
) -> Vec<Piece> {
    let rot = ROTS[rng.next_int_bound(4) as usize];
    match body {
        Body::Igloo {
            top,
            middle,
            bottom,
        } => {
            let mut out = Vec::new();
            if rng.next_double() < 0.5 {
                let depth = rng.next_int_bound(8) + 4;
                out.push(igloo(
                    Arc::clone(bottom),
                    rot,
                    sx,
                    sz,
                    BOT_OFF,
                    BOT_PIVOT,
                    depth * 3,
                    false,
                ));
                for i in 0..depth - 1 {
                    out.push(igloo(
                        Arc::clone(middle),
                        rot,
                        sx,
                        sz,
                        MID_OFF,
                        MID_PIVOT,
                        i * 3,
                        false,
                    ));
                }
            }
            out.push(igloo(
                Arc::clone(top),
                rot,
                sx,
                sz,
                TOP_OFF,
                TOP_PIVOT,
                0,
                true,
            ));
            out
        }
        Body::Ship { beached, templates } => {
            let index = rng.next_int_bound(templates.len() as i32) as usize;
            let template = Arc::clone(&templates[index]);
            let origin = [sx * 16, 90, sz * 16];
            let (min, max) = template::pivoted_bounds(origin, rot, SHIP_PIVOT, template.size);
            let frozen = if template.size[0] > 32 || template.size[1] > 32 {
                Some(oversized(
                    *beached,
                    min,
                    max,
                    template.size[1],
                    rng,
                    columns,
                ))
            } else {
                None
            };
            vec![Piece {
                template,
                rot,
                pivot: SHIP_PIVOT,
                origin,
                min,
                max,
                role: Role::Ship {
                    beached: *beached,
                    frozen: Mutex::new(frozen),
                },
            }]
        }
    }
}

fn igloo(
    template: Arc<Template>,
    rot: Rot,
    sx: i32,
    sz: i32,
    offset: [i32; 3],
    pivot: [i32; 3],
    depth: i32,
    top: bool,
) -> Piece {
    let origin = [
        sx * 16 + offset[0],
        90 + offset[1] - depth,
        sz * 16 + offset[2],
    ];
    let (min, max) = template::pivoted_bounds(origin, rot, pivot, template.size);
    Piece {
        template,
        rot,
        pivot,
        origin,
        min,
        max,
        role: Role::Igloo { top, offset },
    }
}

fn entrance(origin: [i32; 3], rot: Rot, pivot: [i32; 3], offset: [i32; 3]) -> [i32; 3] {
    let local = template::transform_pivot(rot, 3 - offset[0], 0, -offset[2], pivot);
    [
        origin[0] + local[0],
        origin[1] + local[1],
        origin[2] + local[2],
    ]
}

/// Мёртвая ветка текущих шаблонов: X или Y больше 32. Углы — занятая
/// `WORLD_SURFACE_WG`, на один блок дальше дальнего края.
fn oversized(
    beached: bool,
    min: [i32; 3],
    max: [i32; 3],
    size_y: i32,
    rng: &mut Legacy,
    columns: &dyn Columns,
) -> i32 {
    let span_x = max[0] - min[0] + 1;
    let span_z = max[2] - min[2] + 1;
    let corners = [
        (min[0], min[2]),
        (min[0], min[2] + span_z),
        (min[0] + span_x, min[2]),
        (min[0] + span_x, min[2] + span_z),
    ];
    let occupied: [i32; 4] =
        core::array::from_fn(|i| columns.base_height(corners[i].0, corners[i].1) - 1);
    if beached {
        let lowest = occupied.into_iter().min().unwrap_or(0);
        lowest - size_y / 2 - rng.next_int_bound(3)
    } else {
        occupied.into_iter().sum::<i32>() / 4
    }
}

fn block(pal: &Palette, name: &str) -> u32 {
    pal.defaults.get(name).copied().unwrap_or(0)
}

fn is_block(pal: &Palette, state: u32, name: &str) -> bool {
    pal.blocks
        .get(name)
        .is_some_and(|(id, _)| pal.block_index(state) == Some(*id))
}
