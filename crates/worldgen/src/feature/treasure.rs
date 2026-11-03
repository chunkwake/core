//! Зарытый клад: `BuriedTreasurePieces`. Сундук на первом камне или
//! песчанике под дном, вокруг — песок или блок снизу.

use super::stronghold::Draw;
use super::template::Bounds;
use super::{Heightmap, Level, Palette, Worldgen};

pub(crate) struct Piece {
    /// `(chunk.getBlockX(9), 90, chunk.getBlockZ(9))`.
    pub(crate) pos: [i32; 3],
}

const BASE: [&str; 5] = [
    "minecraft:sandstone",
    "minecraft:stone",
    "minecraft:andesite",
    "minecraft:granite",
    "minecraft:diorite",
];

/// `Direction.values()`: вниз, вверх, север, юг, запад, восток.
const DIRS: [[i32; 3]; 6] = [
    [0, -1, 0],
    [0, 1, 0],
    [0, 0, -1],
    [0, 0, 1],
    [-1, 0, 0],
    [1, 0, 0],
];

impl Piece {
    pub(crate) fn bounds(&self) -> ([i32; 3], [i32; 3]) {
        (self.pos, self.pos)
    }

    pub(crate) fn place(
        &self,
        pal: &Palette,
        level: &mut dyn Level,
        rng: &mut Worldgen,
        window: Bounds,
    ) {
        let [x, _, z] = self.pos;
        let id = |name: &str| pal.blocks.get(name).map(|b| b.0);
        let base = BASE.map(id);
        let water = id("minecraft:water");
        let lava = id("minecraft:lava");
        let sand = pal.defaults["minecraft:sand"];
        let gap = |state: u32| {
            let block = pal.block_index(state);
            pal.is_air(state) || block == water || block == lava
        };
        let mut y = level.surface(Heightmap::OceanFloorWg, x, z);
        while y > level.min_y() {
            let current = level.get(x, y, z);
            let below = level.get(x, y - 1, z);
            if base.contains(&pal.block_index(below)) {
                let soft = if gap(current) { sand } else { current };
                for (i, d) in DIRS.iter().enumerate() {
                    let at = [x + d[0], y + d[1], z + d[2]];
                    if !gap(level.get(at[0], at[1], at[2])) {
                        continue;
                    }
                    let under = level.get(at[0], at[1] - 1, at[2]);
                    let state = if gap(under) && i != 1 { below } else { soft };
                    level.set(at[0], at[1], at[2], state);
                }
                let mut draw = Draw::new(pal, level, rng, window, 2, [x, y, z], [x, y, z]);
                draw.chest_at(0, 0, 0, "minecraft:chests/buried_treasure");
                return;
            }
            y -= 1;
        }
    }
}
