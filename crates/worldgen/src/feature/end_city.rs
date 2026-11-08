//! Город Края: дерево шаблонов `end_city/*` (`EndCityPieces`).
//!
//! Шалкеры и рамка с элитрами — сущности генерации, их не пишем; сундуки
//! получают таблицу `end_city_treasure`.

use super::template::{self, Bounds, ROTS, Rot, Template};
use super::{Bits, Legacy, Level, Palette};
use crate::templates::Mirror;
use std::collections::HashMap;
use std::sync::Arc;

/// `EndCityPieces.MAX_GEN_DEPTH`.
const MAX_GEN_DEPTH: i32 = 8;

/// Один кусок города: шаблон, начало и поворот.
#[derive(Clone, Debug)]
pub(crate) struct Piece {
    /// Имя шаблона без `end_city/`: нужно сверке в тесте.
    #[cfg_attr(not(test), allow(dead_code))]
    pub name: String,
    pub template: Arc<Template>,
    pub origin: [i32; 3],
    pub rot: Rot,
    /// `overwrite`: `STRUCTURE_BLOCK`, иначе ещё и воздух не ставится.
    pub overwrite: bool,
    pub min: [i32; 3],
    pub max: [i32; 3],
    gen_depth: i32,
}

impl Piece {
    pub(crate) fn bounds(&self) -> ([i32; 3], [i32; 3]) {
        (self.min, self.max)
    }

    fn intersects(&self, other: &Piece) -> bool {
        (0..3).all(|i| self.max[i] >= other.min[i] && self.min[i] <= other.max[i])
    }

    /// `TemplateStructurePiece.postProcess` и `handleDataMarker`: маркер
    /// `Chest` отдаёт сундуку под собой таблицу лута.
    pub(crate) fn place(
        &self,
        pal: &Palette,
        level: &mut dyn Level,
        r: &mut super::Worldgen,
        window: Bounds,
    ) {
        let markers = template::place_mirrored(
            pal,
            level,
            &self.template,
            self.origin,
            Mirror::None,
            self.rot,
            window,
            r,
            !self.overwrite,
        );
        for (pos, meta) in markers {
            let Some(meta) = meta else { continue };
            if !meta.starts_with("Chest") {
                continue;
            }
            let chest = [pos[0], pos[1] - 1, pos[2]];
            if !window.contains(chest) || !level.can_write(chest[0], chest[2]) {
                continue;
            }
            // `RandomizableContainer.setBlockEntityLootTable`: только если
            // под маркером контейнер.
            let block = pal.block_name(level.get(chest[0], chest[1], chest[2]));
            if block != Some("minecraft:chest") {
                continue;
            }
            let seed = r.next_long();
            template::set_loot(level, chest, "minecraft:chests/end_city_treasure", seed);
        }
    }
}

/// `Rotation.getRotated`.
fn rotated(a: Rot, b: Rot) -> Rot {
    let i = |r: Rot| ROTS.iter().position(|x| *x == r).unwrap_or(0);
    ROTS[(i(a) + i(b)) % 4]
}

#[derive(Clone, Copy)]
enum Gen {
    HouseTower,
    Tower,
    TowerBridge,
    FatTower,
}

/// `TOWER_BRIDGES`: поворот и смещение от куска башни.
const TOWER_BRIDGES: [(Rot, [i32; 3]); 4] = [
    (Rot::None, [1, -1, 0]),
    (Rot::Cw90, [6, -1, 1]),
    (Rot::Ccw90, [0, -1, 5]),
    (Rot::Cw180, [5, -1, 6]),
];

/// `FAT_TOWER_BRIDGES`.
const FAT_TOWER_BRIDGES: [(Rot, [i32; 3]); 4] = [
    (Rot::None, [4, -1, 0]),
    (Rot::Cw90, [12, -1, 4]),
    (Rot::Ccw90, [0, -1, 8]),
    (Rot::Cw180, [8, -1, 12]),
];

struct Builder<'a> {
    templates: &'a HashMap<String, Arc<Template>>,
    /// Все созданные куски; списки ниже — индексы в нём.
    arena: Vec<Piece>,
    /// `TOWER_BRIDGE_GENERATOR.shipCreated`.
    ship_created: bool,
    missing: bool,
}

/// `EndCityPieces.startHouseTower`. Пусто, если шаблона нет.
pub(crate) fn build(
    templates: &HashMap<String, Arc<Template>>,
    origin: [i32; 3],
    rot: Rot,
    rng: &mut Legacy,
) -> Vec<Piece> {
    let mut b = Builder {
        templates,
        arena: Vec::new(),
        ship_created: false,
        missing: false,
    };
    let mut pieces = Vec::new();
    let mut last = b.new_piece("base_floor", origin, rot, true);
    pieces.push(last);
    last = b.add(&mut pieces, last, [-1, 0, -1], "second_floor_1", rot, false);
    last = b.add(&mut pieces, last, [-1, 4, -1], "third_floor_1", rot, false);
    last = b.add(&mut pieces, last, [-1, 8, -1], "third_roof", rot, true);
    b.recursive(Gen::Tower, 1, last, None, &mut pieces, rng);
    if b.missing {
        return Vec::new();
    }
    let mut arena: Vec<Option<Piece>> = b.arena.into_iter().map(Some).collect();
    pieces.into_iter().filter_map(|i| arena[i].take()).collect()
}

impl Builder<'_> {
    fn new_piece(&mut self, name: &str, origin: [i32; 3], rot: Rot, overwrite: bool) -> usize {
        let template = match self.templates.get(&format!("end_city/{name}")) {
            Some(t) => Arc::clone(t),
            None => {
                self.missing = true;
                Arc::new(Template::empty())
            }
        };
        let (min, max) = template::mirrored_bounds(origin, Mirror::None, rot, template.size);
        self.arena.push(Piece {
            name: name.to_owned(),
            template,
            origin,
            rot,
            overwrite,
            min,
            max,
            gen_depth: 0,
        });
        self.arena.len() - 1
    }

    /// `addHelper(pieces, addPiece(parent, offset, name, rotation, overwrite))`:
    /// начало ребёнка — начало родителя плюс смещение в повороте родителя.
    fn add(
        &mut self,
        pieces: &mut Vec<usize>,
        parent: usize,
        offset: [i32; 3],
        name: &str,
        rot: Rot,
        overwrite: bool,
    ) -> usize {
        let p = &self.arena[parent];
        let d = template::transform(p.rot, offset[0], offset[1], offset[2]);
        let origin = [p.origin[0] + d[0], p.origin[1] + d[1], p.origin[2] + d[2]];
        let child = self.new_piece(name, origin, rot, overwrite);
        pieces.push(child);
        child
    }

    /// `EndCityPieces.recursiveChildren`.
    fn recursive(
        &mut self,
        generator: Gen,
        depth: i32,
        parent: usize,
        offset: Option<[i32; 3]>,
        pieces: &mut Vec<usize>,
        rng: &mut Legacy,
    ) -> bool {
        if depth > MAX_GEN_DEPTH {
            return false;
        }
        let mut children = Vec::new();
        if self.generate(generator, depth, parent, offset, &mut children, rng) {
            let tag = rng.next_int();
            let mut collision = false;
            for &c in &children {
                self.arena[c].gen_depth = tag;
                let hit = pieces
                    .iter()
                    .copied()
                    .find(|&p| self.arena[p].intersects(&self.arena[c]));
                if let Some(hit) = hit
                    && self.arena[hit].gen_depth != self.arena[parent].gen_depth
                {
                    collision = true;
                    break;
                }
            }
            if !collision {
                pieces.extend(children);
                return true;
            }
        }
        false
    }

    fn generate(
        &mut self,
        generator: Gen,
        depth: i32,
        parent: usize,
        offset: Option<[i32; 3]>,
        pieces: &mut Vec<usize>,
        rng: &mut Legacy,
    ) -> bool {
        let rot = self.arena[parent].rot;
        match generator {
            Gen::HouseTower => {
                if depth > MAX_GEN_DEPTH {
                    return false;
                }
                let offset = offset.unwrap_or([0; 3]);
                let mut last = self.add(pieces, parent, offset, "base_floor", rot, true);
                match rng.next_int_bound(3) {
                    0 => {
                        self.add(pieces, last, [-1, 4, -1], "base_roof", rot, true);
                    }
                    1 => {
                        last = self.add(pieces, last, [-1, 0, -1], "second_floor_2", rot, false);
                        last = self.add(pieces, last, [-1, 8, -1], "second_roof", rot, false);
                        self.recursive(Gen::Tower, depth + 1, last, None, pieces, rng);
                    }
                    _ => {
                        last = self.add(pieces, last, [-1, 0, -1], "second_floor_2", rot, false);
                        last = self.add(pieces, last, [-1, 4, -1], "third_floor_2", rot, false);
                        last = self.add(pieces, last, [-1, 8, -1], "third_roof", rot, true);
                        self.recursive(Gen::Tower, depth + 1, last, None, pieces, rng);
                    }
                }
                true
            }
            Gen::Tower => {
                let x = 3 + rng.next_int_bound(2);
                let z = 3 + rng.next_int_bound(2);
                let mut last = self.add(pieces, parent, [x, -3, z], "tower_base", rot, true);
                last = self.add(pieces, last, [0, 7, 0], "tower_piece", rot, true);
                let mut bridge = (rng.next_int_bound(3) == 0).then_some(last);
                let height = 1 + rng.next_int_bound(3);
                for i in 0..height {
                    last = self.add(pieces, last, [0, 4, 0], "tower_piece", rot, true);
                    if i < height - 1 && rng.next_bool() {
                        bridge = Some(last);
                    }
                }
                if let Some(bridge) = bridge {
                    for (brot, boff) in TOWER_BRIDGES {
                        if rng.next_bool() {
                            let start = self.add(
                                pieces,
                                bridge,
                                boff,
                                "bridge_end",
                                rotated(rot, brot),
                                true,
                            );
                            self.recursive(Gen::TowerBridge, depth + 1, start, None, pieces, rng);
                        }
                    }
                } else if depth != 7 {
                    return self.recursive(Gen::FatTower, depth + 1, last, None, pieces, rng);
                }
                self.add(pieces, last, [-1, 4, -1], "tower_top", rot, true);
                true
            }
            Gen::TowerBridge => {
                let length = rng.next_int_bound(4) + 1;
                let mut last = self.add(pieces, parent, [0, 0, -4], "bridge_piece", rot, true);
                self.arena[last].gen_depth = -1;
                let mut next_y = 0;
                for _ in 0..length {
                    if rng.next_bool() {
                        last = self.add(pieces, last, [0, next_y, -4], "bridge_piece", rot, true);
                        next_y = 0;
                    } else {
                        if rng.next_bool() {
                            last = self.add(
                                pieces,
                                last,
                                [0, next_y, -4],
                                "bridge_steep_stairs",
                                rot,
                                true,
                            );
                        } else {
                            last = self.add(
                                pieces,
                                last,
                                [0, next_y, -8],
                                "bridge_gentle_stairs",
                                rot,
                                true,
                            );
                        }
                        next_y = 4;
                    }
                }
                if !self.ship_created && rng.next_int_bound(10 - depth) == 0 {
                    let x = -8 + rng.next_int_bound(8);
                    let z = -70 + rng.next_int_bound(10);
                    self.add(pieces, last, [x, next_y, z], "ship", rot, true);
                    self.ship_created = true;
                } else if !self.recursive(
                    Gen::HouseTower,
                    depth + 1,
                    last,
                    Some([-3, next_y + 1, -11]),
                    pieces,
                    rng,
                ) {
                    return false;
                }
                let end = self.add(
                    pieces,
                    last,
                    [4, next_y, 0],
                    "bridge_end",
                    rotated(rot, Rot::Cw180),
                    true,
                );
                self.arena[end].gen_depth = -1;
                true
            }
            Gen::FatTower => {
                let mut last = self.add(pieces, parent, [-3, 4, -3], "fat_tower_base", rot, true);
                last = self.add(pieces, last, [0, 4, 0], "fat_tower_middle", rot, true);
                let mut i = 0;
                while i < 2 && rng.next_int_bound(3) != 0 {
                    last = self.add(pieces, last, [0, 8, 0], "fat_tower_middle", rot, true);
                    for (brot, boff) in FAT_TOWER_BRIDGES {
                        if rng.next_bool() {
                            let start = self.add(
                                pieces,
                                last,
                                boff,
                                "bridge_end",
                                rotated(rot, brot),
                                true,
                            );
                            self.recursive(Gen::TowerBridge, depth + 1, start, None, pieces, rng);
                        }
                    }
                    i += 1;
                }
                self.add(pieces, last, [-2, 8, -2], "fat_tower_top", rot, true);
                true
            }
        }
    }
}
