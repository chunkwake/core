//! Поиск пути наземных мобов — перенос `net.minecraft.world.level.pathfinder`
//! 26.3 (`PathFinder`, `WalkNodeEvaluator`, `BinaryHeap`, `Node`, `Target`,
//! `Path`, `PathType`) и следования по пути (`GroundPathNavigation`,
//! `MoveControl`).
//!
//! Числа, порядок обхода соседей, порядок кучи и все сравнения `float`
//! повторяют байткод: от них зависит, какой из равных по цене путей выберет A*,
//! а значит и по каким клеткам моб пойдёт.

use std::collections::HashMap;

use super::{Mob, World, colliders, wrap};

/// Тип клетки для поиска пути (`PathType`), в порядке объявления: по нему
/// же идёт обход `EnumSet` в [`Search::type_of_mob`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PathType {
    Blocked,
    Open,
    Walkable,
    WalkableDoor,
    Trapdoor,
    PowderSnow,
    OnTopOfPowderSnow,
    Fence,
    Lava,
    Water,
    WaterBorder,
    Rail,
    UnpassableRail,
    FireInNeighbor,
    Fire,
    DamagingInNeighbor,
    Damaging,
    DoorOpen,
    DoorWoodClosed,
    DoorIronClosed,
    Breach,
    Leaves,
    StickyHoney,
    Cocoa,
    DamageCautious,
    OnTopOfTrapdoor,
    BigMobsCloseToDanger,
}

impl PathType {
    pub const ALL: [PathType; 27] = [
        PathType::Blocked,
        PathType::Open,
        PathType::Walkable,
        PathType::WalkableDoor,
        PathType::Trapdoor,
        PathType::PowderSnow,
        PathType::OnTopOfPowderSnow,
        PathType::Fence,
        PathType::Lava,
        PathType::Water,
        PathType::WaterBorder,
        PathType::Rail,
        PathType::UnpassableRail,
        PathType::FireInNeighbor,
        PathType::Fire,
        PathType::DamagingInNeighbor,
        PathType::Damaging,
        PathType::DoorOpen,
        PathType::DoorWoodClosed,
        PathType::DoorIronClosed,
        PathType::Breach,
        PathType::Leaves,
        PathType::StickyHoney,
        PathType::Cocoa,
        PathType::DamageCautious,
        PathType::OnTopOfTrapdoor,
        PathType::BigMobsCloseToDanger,
    ];

    const NAMES: [&'static str; 27] = [
        "BLOCKED",
        "OPEN",
        "WALKABLE",
        "WALKABLE_DOOR",
        "TRAPDOOR",
        "POWDER_SNOW",
        "ON_TOP_OF_POWDER_SNOW",
        "FENCE",
        "LAVA",
        "WATER",
        "WATER_BORDER",
        "RAIL",
        "UNPASSABLE_RAIL",
        "FIRE_IN_NEIGHBOR",
        "FIRE",
        "DAMAGING_IN_NEIGHBOR",
        "DAMAGING",
        "DOOR_OPEN",
        "DOOR_WOOD_CLOSED",
        "DOOR_IRON_CLOSED",
        "BREACH",
        "LEAVES",
        "STICKY_HONEY",
        "COCOA",
        "DAMAGE_CAUTIOUS",
        "ON_TOP_OF_TRAPDOOR",
        "BIG_MOBS_CLOSE_TO_DANGER",
    ];

    /// Имя константы в `PathType`.
    pub fn name(self) -> &'static str {
        Self::NAMES[self as usize]
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::NAMES
            .iter()
            .position(|n| *n == name)
            .map(|i| Self::ALL[i])
    }

    /// Штраф по умолчанию (`PathType.getMalus`); < 0 — непроходимо.
    pub fn malus(self) -> f32 {
        use PathType::*;
        match self {
            Blocked | PowderSnow | Fence | Lava | UnpassableRail | Damaging | DoorWoodClosed
            | DoorIronClosed | Leaves => -1.0,
            Water | WaterBorder | FireInNeighbor | DamagingInNeighbor | StickyHoney => 8.0,
            Fire => 16.0,
            Breach | BigMobsCloseToDanger => 4.0,
            _ => 0.0,
        }
    }
}

/// Клетка мира для поиска пути: тип по одному состоянию блока
/// (`WalkNodeEvaluator.getPathTypeFromState`) и флаги состояния.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub kind: PathType,
    pub flags: u8,
}

impl Cell {
    /// `isAir() || isPathfindable(LAND)`.
    pub const LAND: u8 = 1;
    /// `isAir()`.
    pub const AIR: u8 = 2;
    /// `isSolid()`.
    pub const SOLID: u8 = 4;
    /// Тег `cauldrons` (`PathNavigation.trimPath`).
    pub const CAULDRON: u8 = 8;
    /// Тег `doors` (`MoveControl`: в двери не прыгают).
    pub const DOOR: u8 = 16;
    /// Тег `fences` (`MoveControl`).
    pub const FENCE: u8 = 32;

    /// Воздух.
    pub const AIR_CELL: Cell = Cell {
        kind: PathType::Open,
        flags: Self::LAND | Self::AIR,
    };

    pub fn has(self, flag: u8) -> bool {
        self.flags & flag != 0
    }
}

/// Клетка мира: из таблицы состояний, а без неё (тесты) — по форме и воде:
/// пустое — воздух, вода — вода, остальное — непроходимый твёрдый блок.
pub fn cell(w: &World, p: [i32; 3]) -> Cell {
    if let Some(c) = w.cell {
        return c(p);
    }
    if (w.water)(p) {
        Cell {
            kind: PathType::Water,
            flags: Cell::LAND,
        }
    } else if (w.shape)(p).is_empty() {
        Cell::AIR_CELL
    } else {
        Cell {
            kind: PathType::Blocked,
            flags: Cell::SOLID,
        }
    }
}

/// Таблица клеток по id состояния из `reports/path_types.csv`.
///
/// Выгрузка идёт сразу после `Bootstrap`, когда теги ещё не привязаны, и
/// проверки по тегам в `getPathTypeFromState` там ложны. Здесь они
/// повторяются по файлам тегов в том же порядке, что в байткоде: `trapdoors`,
/// `speleothems`, лава, `fire`, `fences`/`walls`, вода.
pub fn load_cells(
    data_dir: &std::path::Path,
    pl: &crate::placement::Placement,
    tags: &std::collections::HashMap<String, std::collections::HashSet<String>>,
) -> Option<Vec<Cell>> {
    let text = std::fs::read_to_string(data_dir.join("reports/path_types.csv")).ok()?;
    let tag = |name: &str, block: &str| tags.get(name).is_some_and(|t| t.contains(block));
    let mut out = Vec::new();
    for (id, line) in text.lines().enumerate() {
        let mut f = line.split(',');
        let mut kind = PathType::from_name(f.next()?)?;
        let mut bit = |b: u8| (f.next() == Some("1")).then_some(b).unwrap_or(0);
        let mut flags = bit(Cell::LAND) | bit(Cell::AIR) | bit(Cell::SOLID);
        let s = id as u32;
        let name = pl.name(s).unwrap_or("");
        let fluid = crate::fluids::state_fluid(pl, s).map(|f| f.kind);
        let lava = fluid == Some(crate::fluids::Kind::Lava);
        let air = flags & Cell::AIR != 0;
        if lava {
            // `LiquidBlock.isPathfindable`: не по лаве.
            flags &= !Cell::LAND;
        }
        if !air {
            if tag("minecraft:trapdoors", name) {
                kind = PathType::Trapdoor;
            } else if matches!(
                kind,
                PathType::Trapdoor
                    | PathType::PowderSnow
                    | PathType::Damaging
                    | PathType::StickyHoney
                    | PathType::Cocoa
                    | PathType::DamageCautious
            ) {
                // Решено до тегов: кувшинка, листья-платформы, снег, кактус, мёд.
            } else if tag("minecraft:speleothems", name) {
                kind = PathType::DamageCautious;
            } else if lava {
                kind = PathType::Lava;
            } else if tag("minecraft:fire", name) {
                kind = PathType::Fire;
            } else if matches!(
                kind,
                PathType::Fire
                    | PathType::DoorOpen
                    | PathType::DoorWoodClosed
                    | PathType::DoorIronClosed
                    | PathType::Rail
                    | PathType::Leaves
            ) {
            } else if tag("minecraft:fences", name) || tag("minecraft:walls", name) {
                kind = PathType::Fence;
            } else if kind == PathType::Open && fluid == Some(crate::fluids::Kind::Water) {
                kind = PathType::Water;
            }
        }
        if tag("minecraft:cauldrons", name) {
            flags |= Cell::CAULDRON;
        }
        if tag("minecraft:doors", name) {
            flags |= Cell::DOOR;
        }
        if tag("minecraft:fences", name) {
            flags |= Cell::FENCE;
        }
        out.push(Cell { kind, flags });
    }
    Some(out)
}

/// Моб глазами поиска пути: рамка, шаг, падение и умения навигации.
#[derive(Clone, Copy, Debug)]
pub struct Walker {
    pub pos: [f64; 3],
    /// `getBbWidth`, `getBbHeight`.
    pub width: f64,
    pub height: f64,
    pub on_ground: bool,
    pub in_water: bool,
    /// `maxUpStep` (атрибут `step_height`).
    pub step: f32,
    /// `getMaxFallDistance`.
    pub max_fall: i32,
    /// `getSpeed` — текущая скорость от `MoveControl`.
    pub speed: f32,
    pub can_float: bool,
    pub can_open_doors: bool,
    pub can_pass_doors: bool,
    pub can_walk_over_fences: bool,
    /// Свои штрафы (`Mob.setPathfindingMalus`) поверх `PathType.getMalus`.
    pub malus: &'static [(PathType, f32)],
    /// Нижняя граница мира (`getMinY`).
    pub min_y: i32,
}

/// `Animal`: огонь непроходим, соседство с огнём дорого.
pub const ANIMAL_MALUS: &[(PathType, f32)] =
    &[(PathType::FireInNeighbor, 16.0), (PathType::Fire, -1.0)];

impl Walker {
    pub fn malus(&self, t: PathType) -> f32 {
        self.malus
            .iter()
            .find(|(k, _)| *k == t)
            .map_or(t.malus(), |m| m.1)
    }

    fn block(&self) -> [i32; 3] {
        [
            self.pos[0].floor() as i32,
            self.pos[1].floor() as i32,
            self.pos[2].floor() as i32,
        ]
    }

    /// `getBoundingBox`.
    fn bb(&self) -> ([f64; 3], [f64; 3]) {
        let h = self.width / 2.0;
        (
            [self.pos[0] - h, self.pos[1], self.pos[2] - h],
            [self.pos[0] + h, self.pos[1] + self.height, self.pos[2] + h],
        )
    }
}

/// `Mob.getMaxFallDistance`: без цели 3, с целью — запас здоровья сверх трети
/// минус `(3 − сложность) × 4`, плюс безопасные 3.
pub fn max_fall_distance(has_target: bool, health: f32, max_health: f32, difficulty: u8) -> i32 {
    if !has_target {
        return 3;
    }
    let mut i = (health - max_health * 0.33) as i32;
    i -= (3 - i32::from(difficulty)) * 4;
    if i < 0 {
        i = 0;
    }
    (i as f32 + 3.0).floor() as i32
}

#[derive(Clone, Debug)]
struct Node {
    p: [i32; 3],
    heap: i32,
    g: f32,
    h: f32,
    f: f32,
    from: Option<u32>,
    closed: bool,
    walked: f32,
    malus: f32,
    kind: PathType,
}

impl Node {
    fn new(p: [i32; 3]) -> Self {
        Self {
            p,
            heap: -1,
            g: 0.0,
            h: 0.0,
            f: 0.0,
            from: None,
            closed: false,
            walked: 0.0,
            malus: 0.0,
            kind: PathType::Blocked,
        }
    }
}

/// `Node.distanceTo`.
fn dist(a: [i32; 3], b: [i32; 3]) -> f32 {
    let dx = (b[0] - a[0]) as f32;
    let dy = (b[1] - a[1]) as f32;
    let dz = (b[2] - a[2]) as f32;
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// `Node.distanceManhattan`.
fn manhattan(a: [i32; 3], b: [i32; 3]) -> f32 {
    let dx = (b[0] - a[0]).abs() as f32;
    let dy = (b[1] - a[1]).abs() as f32;
    let dz = (b[2] - a[2]).abs() as f32;
    dx + dy + dz
}

/// `Direction.Plane.HORIZONTAL`: север, восток, юг, запад; шаг (x, z).
const HORIZONTAL: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

/// Цель поиска (`Target`): лучший узел по расстоянию.
struct Target {
    p: [i32; 3],
    best: f32,
    best_node: Option<u32>,
    reached: bool,
}

/// Один расчёт пути: узлы, куча и кэши `WalkNodeEvaluator`.
struct Search<'q, 'a, 's, 'r> {
    w: &'q World<'a, 's, 'r>,
    m: &'q Walker,
    nodes: Vec<Node>,
    index: HashMap<[i32; 3], u32>,
    types: HashMap<[i32; 3], PathType>,
    collisions: HashMap<[u64; 6], bool>,
    heap: Vec<u32>,
    /// `entityWidth`, `entityHeight`, `entityDepth`.
    ew: i32,
    eh: i32,
    ed: i32,
}

impl<'q, 'a, 's, 'r> Search<'q, 'a, 's, 'r> {
    fn new(w: &'q World<'a, 's, 'r>, m: &'q Walker) -> Self {
        Self {
            w,
            m,
            nodes: Vec::new(),
            index: HashMap::new(),
            types: HashMap::new(),
            collisions: HashMap::new(),
            heap: Vec::new(),
            ew: (m.width as f32 + 1.0).floor() as i32,
            eh: (m.height as f32 + 1.0).floor() as i32,
            ed: (m.width as f32 + 1.0).floor() as i32,
        }
    }

    // --- Узлы и куча (`BinaryHeap`) ---------------------------------------

    fn node(&mut self, p: [i32; 3]) -> u32 {
        if let Some(&i) = self.index.get(&p) {
            return i;
        }
        let i = self.nodes.len() as u32;
        self.nodes.push(Node::new(p));
        self.index.insert(p, i);
        i
    }

    fn n(&self, i: u32) -> &Node {
        &self.nodes[i as usize]
    }

    fn nm(&mut self, i: u32) -> &mut Node {
        &mut self.nodes[i as usize]
    }

    fn insert(&mut self, i: u32) {
        let at = self.heap.len();
        self.heap.push(i);
        self.nm(i).heap = at as i32;
        self.up_heap(at);
    }

    fn up_heap(&mut self, mut idx: usize) {
        let node = self.heap[idx];
        let f = self.n(node).f;
        while idx > 0 {
            let parent = (idx - 1) >> 1;
            let pn = self.heap[parent];
            if f >= self.n(pn).f {
                break;
            }
            self.heap[idx] = pn;
            self.nm(pn).heap = idx as i32;
            idx = parent;
        }
        self.heap[idx] = node;
        self.nm(node).heap = idx as i32;
    }

    fn down_heap(&mut self, mut idx: usize) {
        let node = self.heap[idx];
        let f = self.n(node).f;
        let size = self.heap.len();
        loop {
            let left = 1 + (idx << 1);
            let right = left + 1;
            if left >= size {
                break;
            }
            let ln = self.heap[left];
            let lf = self.n(ln).f;
            let (rn, rf) = if right >= size {
                (None, f32::INFINITY)
            } else {
                let r = self.heap[right];
                (Some(r), self.n(r).f)
            };
            if lf < rf {
                if lf >= f {
                    break;
                }
                self.heap[idx] = ln;
                self.nm(ln).heap = idx as i32;
                idx = left;
            } else {
                let Some(rn) = rn else { break };
                if rf >= f {
                    break;
                }
                self.heap[idx] = rn;
                self.nm(rn).heap = idx as i32;
                idx = right;
            }
        }
        self.heap[idx] = node;
        self.nm(node).heap = idx as i32;
    }

    fn pop(&mut self) -> u32 {
        let top = self.heap[0];
        let last = self.heap.pop().expect("непустая куча");
        if !self.heap.is_empty() {
            self.heap[0] = last;
            self.down_heap(0);
        }
        self.nm(top).heap = -1;
        top
    }

    fn change_cost(&mut self, i: u32, f: f32) {
        let old = self.n(i).f;
        self.nm(i).f = f;
        let at = self.n(i).heap as usize;
        if f < old {
            self.up_heap(at);
        } else {
            self.down_heap(at);
        }
    }

    // --- Типы клеток ------------------------------------------------------

    fn from_state(&self, p: [i32; 3]) -> PathType {
        cell(self.w, p).kind
    }

    /// `getPathTypeStatic`: воздух над чем-то — по блоку снизу и соседям.
    fn type_static(&self, p: [i32; 3]) -> PathType {
        let t = self.from_state(p);
        if t != PathType::Open || p[1] < self.m.min_y + 1 {
            return t;
        }
        use PathType::*;
        match self.from_state([p[0], p[1] - 1, p[2]]) {
            Open | Water | Lava | Walkable => Open,
            Fire => Fire,
            Damaging => Damaging,
            StickyHoney => StickyHoney,
            PowderSnow => OnTopOfPowderSnow,
            DamageCautious => DamageCautious,
            Trapdoor => OnTopOfTrapdoor,
            _ => self.check_neighbours(p, Walkable),
        }
    }

    /// `checkNeighbourBlocks`: опасность в соседях 3×3×3 без своего столбца.
    fn check_neighbours(&self, p: [i32; 3], def: PathType) -> PathType {
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if dx == 0 && dz == 0 {
                        continue;
                    }
                    match self.from_state([p[0] + dx, p[1] + dy, p[2] + dz]) {
                        PathType::Damaging => return PathType::DamagingInNeighbor,
                        PathType::Fire | PathType::Lava => return PathType::FireInNeighbor,
                        PathType::Water => return PathType::WaterBorder,
                        PathType::DamageCautious => return PathType::DamageCautious,
                        _ => {}
                    }
                }
            }
        }
        def
    }

    /// `getPathTypeWithinMobBB` как множество в порядке `PathType`.
    fn types_within(&self, p: [i32; 3]) -> Vec<PathType> {
        let mut set: Vec<PathType> = Vec::new();
        let mb = self.m.block();
        for i in 0..self.ew {
            for j in 0..self.eh {
                for k in 0..self.ed {
                    let q = [p[0] + i, p[1] + j, p[2] + k];
                    let mut t = self.type_static(q);
                    if t == PathType::DoorWoodClosed
                        && self.m.can_open_doors
                        && self.m.can_pass_doors
                    {
                        t = PathType::WalkableDoor;
                    }
                    if t == PathType::DoorOpen && !self.m.can_pass_doors {
                        t = PathType::Blocked;
                    }
                    if t == PathType::Rail
                        && self.type_static(mb) != PathType::Rail
                        && self.type_static([mb[0], mb[1] - 1, mb[2]]) != PathType::Rail
                    {
                        t = PathType::UnpassableRail;
                    }
                    if !set.contains(&t) {
                        set.push(t);
                    }
                }
            }
        }
        set.sort();
        set
    }

    /// `getPathTypeOfMob`.
    fn type_of_mob(&self, p: [i32; 3]) -> PathType {
        let set = self.types_within(p);
        if set.len() == 1 {
            return set[0];
        }
        if set.contains(&PathType::Fence) {
            return PathType::Fence;
        }
        if set.contains(&PathType::UnpassableRail) {
            return PathType::UnpassableRail;
        }
        let mut best = PathType::Blocked;
        let mut best_m = self.m.malus(best);
        for &t in &set {
            let m = self.m.malus(t);
            if m < 0.0 {
                return t;
            }
            if m >= best_m {
                best_m = m;
                best = t;
            }
        }
        let here = self.type_static(p);
        if self.ew > 1 {
            if self.m.malus(here) < best_m && self.m.malus(PathType::BigMobsCloseToDanger) < best_m
            {
                return PathType::BigMobsCloseToDanger;
            }
            return best;
        }
        if here == PathType::Open && best != PathType::Open && best_m == 0.0 {
            return PathType::Open;
        }
        best
    }

    /// `getCachedPathType`.
    fn cached(&mut self, p: [i32; 3]) -> PathType {
        if let Some(&t) = self.types.get(&p) {
            return t;
        }
        let t = self.type_of_mob(p);
        self.types.insert(p, t);
        t
    }

    // --- Геометрия --------------------------------------------------------

    /// `WalkNodeEvaluator.getFloorLevel`.
    fn floor_level(&self, p: [i32; 3]) -> f64 {
        if self.m.can_float && (self.w.water)(p) {
            return p[1] as f64 + 0.5;
        }
        floor_level(self.w, p)
    }

    /// `hasCollisions` (`noCollision` по блокам), с кэшем на расчёт.
    fn has_collisions(&mut self, lo: [f64; 3], hi: [f64; 3]) -> bool {
        let key = [
            lo[0].to_bits(),
            lo[1].to_bits(),
            lo[2].to_bits(),
            hi[0].to_bits(),
            hi[1].to_bits(),
            hi[2].to_bits(),
        ];
        if let Some(&c) = self.collisions.get(&key) {
            return c;
        }
        let c = !colliders(lo, hi, self.w).is_empty();
        self.collisions.insert(key, c);
        c
    }

    /// `canReachWithoutCollision`: рамка моба шагами к узлу.
    fn can_reach_without_collision(&mut self, i: u32) -> bool {
        let p = self.n(i).p;
        let (mut lo, mut hi) = self.m.bb();
        let size = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
        let mut v = [
            p[0] as f64 - self.m.pos[0] + size[0] / 2.0,
            p[1] as f64 - self.m.pos[1] + size[1] / 2.0,
            p[2] as f64 - self.m.pos[2] + size[2] / 2.0,
        ];
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        let avg = (size[0] + size[1] + size[2]) / 3.0;
        let steps = (len / avg).ceil() as i32;
        let k = f64::from(1.0f32 / steps as f32);
        v = [v[0] * k, v[1] * k, v[2] * k];
        for _ in 1..=steps {
            for a in 0..3 {
                lo[a] += v[a];
                hi[a] += v[a];
            }
            if self.has_collisions(lo, hi) {
                return false;
            }
        }
        true
    }

    // --- Соседи (`WalkNodeEvaluator`) -------------------------------------

    /// `getNodeAndUpdateCostToMax`.
    fn node_cost_max(&mut self, p: [i32; 3], t: PathType, malus: f32) -> u32 {
        let i = self.node(p);
        let n = self.nm(i);
        n.kind = t;
        n.malus = n.malus.max(malus);
        i
    }

    fn blocked_node(&mut self, p: [i32; 3]) -> u32 {
        let i = self.node(p);
        let n = self.nm(i);
        n.kind = PathType::Blocked;
        n.malus = -1.0;
        i
    }

    fn closed_node(&mut self, p: [i32; 3], t: PathType) -> u32 {
        let i = self.node(p);
        let n = self.nm(i);
        n.closed = true;
        n.kind = t;
        n.malus = t.malus();
        i
    }

    fn jump_height(&self) -> f64 {
        1.125f64.max(f64::from(self.m.step))
    }

    /// `findAcceptedNode`.
    fn find_accepted(
        &mut self,
        p: [i32; 3],
        jump: i32,
        floor: f64,
        dir: (i32, i32),
        cur: PathType,
    ) -> Option<u32> {
        let mut node = None;
        let fl = self.floor_level(p);
        if fl - floor > self.jump_height() {
            return None;
        }
        let t = self.cached(p);
        let malus = self.m.malus(t);
        if malus >= 0.0 {
            node = Some(self.node_cost_max(p, t, malus));
        }
        if partial_collision(cur)
            && let Some(i) = node
            && self.n(i).malus >= 0.0
            && !self.can_reach_without_collision(i)
        {
            node = None;
        }
        if t == PathType::Walkable {
            return node;
        }
        if node.is_none_or(|i| self.n(i).malus < 0.0)
            && jump > 0
            && (t != PathType::Fence || self.m.can_walk_over_fences)
            && t != PathType::UnpassableRail
            && t != PathType::Trapdoor
            && t != PathType::PowderSnow
        {
            node = self.try_jump_on(p, jump, floor, dir, cur);
        } else if t == PathType::Water && !self.m.can_float {
            node = self.first_non_water_below(p, node);
        } else if t == PathType::Open {
            node = Some(self.first_ground_below(p));
        } else if partial_collision(t) && node.is_none() {
            node = Some(self.closed_node(p, t));
        }
        node
    }

    /// `tryJumpOn`: клетка выше и свободна ли голова по пути.
    fn try_jump_on(
        &mut self,
        p: [i32; 3],
        jump: i32,
        floor: f64,
        dir: (i32, i32),
        cur: PathType,
    ) -> Option<u32> {
        let i = self.find_accepted([p[0], p[1] + 1, p[2]], jump - 1, floor, dir, cur)?;
        if self.m.width as f32 >= 1.0 {
            return Some(i);
        }
        let k = self.n(i).kind;
        if k != PathType::Open && k != PathType::Walkable {
            return Some(i);
        }
        let cx = (p[0] - dir.0) as f64 + 0.5;
        let cz = (p[2] - dir.1) as f64 + 0.5;
        let hw = f64::from(self.m.width as f32) / 2.0;
        let at = [cx.floor() as i32, p[1] + 1, cz.floor() as i32];
        let np = self.n(i).p;
        let lo = [cx - hw, self.floor_level(at) + 0.001, cz - hw];
        let hi = [
            cx + hw,
            f64::from(self.m.height as f32) + self.floor_level(np) - 0.002,
            cz + hw,
        ];
        if self.has_collisions(lo, hi) {
            None
        } else {
            Some(i)
        }
    }

    /// `tryFindFirstNonWaterBelow`.
    fn first_non_water_below(&mut self, p: [i32; 3], mut node: Option<u32>) -> Option<u32> {
        let mut y = p[1] - 1;
        while y > self.m.min_y {
            let q = [p[0], y, p[2]];
            let t = self.cached(q);
            if t != PathType::Water {
                return node;
            }
            let m = self.m.malus(t);
            node = Some(self.node_cost_max(q, t, m));
            y -= 1;
        }
        node
    }

    /// `tryFindFirstGroundNodeBelow`: падение не глубже `max_fall`.
    fn first_ground_below(&mut self, p: [i32; 3]) -> u32 {
        let mut i = p[1] - 1;
        while i >= self.m.min_y {
            let q = [p[0], i, p[2]];
            if p[1] - i > self.m.max_fall {
                return self.blocked_node(q);
            }
            let t = self.cached(q);
            let m = self.m.malus(t);
            if t != PathType::Open {
                if m >= 0.0 {
                    return self.node_cost_max(q, t, m);
                }
                return self.blocked_node(q);
            }
            i -= 1;
        }
        self.blocked_node(p)
    }

    fn neighbor_valid(&self, nb: Option<u32>, cur: u32) -> bool {
        nb.is_some_and(|i| {
            let n = self.n(i);
            !n.closed && (n.malus >= 0.0 || self.n(cur).malus < 0.0)
        })
    }

    /// `isDiagonalValid(node, a, b)`.
    fn diagonal_sides_valid(&self, cur: u32, a: Option<u32>, b: Option<u32>) -> bool {
        let (Some(a), Some(b)) = (a, b) else {
            return false;
        };
        let (c, a, b) = (self.n(cur), self.n(a), self.n(b));
        if b.p[1] > c.p[1] || a.p[1] > c.p[1] {
            return false;
        }
        if a.kind == PathType::WalkableDoor || b.kind == PathType::WalkableDoor {
            return false;
        }
        if self.m.width as f32 > 1.0 && (a.malus > 0.0 || b.malus > 0.0) {
            return false;
        }
        let fences =
            a.kind == PathType::Fence && b.kind == PathType::Fence && (self.m.width as f32) < 0.5;
        (b.p[1] < c.p[1] || b.malus >= 0.0 || fences)
            && (a.p[1] < c.p[1] || a.malus >= 0.0 || fences)
    }

    fn diagonal_valid(&self, d: Option<u32>) -> bool {
        d.is_some_and(|i| {
            let n = self.n(i);
            !n.closed && n.kind != PathType::WalkableDoor && n.malus >= 0.0
        })
    }

    /// `getNeighbors`.
    fn neighbors(&mut self, cur: u32) -> Vec<u32> {
        let mut out = Vec::with_capacity(8);
        let p = self.n(cur).p;
        let above = self.cached([p[0], p[1] + 1, p[2]]);
        let here = self.cached(p);
        let jump = if self.m.malus(above) >= 0.0 && here != PathType::StickyHoney {
            self.m.step.max(1.0).floor() as i32
        } else {
            0
        };
        let floor = self.floor_level(p);
        let mut side = [None; 4];
        for (k, d) in HORIZONTAL.iter().enumerate() {
            let n = self.find_accepted([p[0] + d.0, p[1], p[2] + d.1], jump, floor, *d, here);
            side[k] = n;
            if self.neighbor_valid(n, cur) {
                out.push(n.expect("проверен"));
            }
        }
        for (k, d) in HORIZONTAL.iter().enumerate() {
            let cw = (k + 1) % 4;
            let c = HORIZONTAL[cw];
            if self.diagonal_sides_valid(cur, side[k], side[cw]) {
                let n = self.find_accepted(
                    [p[0] + d.0 + c.0, p[1], p[2] + d.1 + c.1],
                    jump,
                    floor,
                    *d,
                    here,
                );
                if self.diagonal_valid(n) {
                    out.push(n.expect("проверен"));
                }
            }
        }
        out
    }

    // --- Старт ------------------------------------------------------------

    fn can_start_at(&mut self, p: [i32; 3]) -> bool {
        let t = self.cached(p);
        t != PathType::Open && self.m.malus(t) >= 0.0
    }

    fn start_node(&mut self, p: [i32; 3]) -> u32 {
        let i = self.node(p);
        let t = self.cached(p);
        let m = self.m.malus(t);
        let n = self.nm(i);
        n.kind = t;
        n.malus = m;
        i
    }

    /// `getStart`.
    fn start(&mut self) -> u32 {
        let m = *self.m;
        let b = m.block();
        let mut y = b[1];
        if m.can_float && m.in_water {
            while (self.w.water)([b[0], y, b[2]]) {
                y += 1;
            }
            y -= 1;
        } else if m.on_ground {
            y = (m.pos[1] + 0.5).floor() as i32;
        } else {
            let mut q = [b[0], (m.pos[1] + 1.0).floor() as i32, b[2]];
            while q[1] > m.min_y {
                y = q[1];
                q[1] -= 1;
                let c = cell(self.w, q);
                if !c.has(Cell::AIR) && !c.has(Cell::LAND) {
                    break;
                }
            }
        }
        let at = [b[0], y, b[2]];
        if !self.can_start_at(at) {
            let (lo, hi) = m.bb();
            for (x, z) in [
                (lo[0], lo[2]),
                (lo[0], hi[2]),
                (hi[0], lo[2]),
                (hi[0], hi[2]),
            ] {
                let c = [x.floor() as i32, y, z.floor() as i32];
                if self.can_start_at(c) {
                    return self.start_node(c);
                }
            }
        }
        self.start_node(at)
    }

    // --- A* (`PathFinder.findPath`) ---------------------------------------

    fn best_h(&self, i: u32, targets: &mut [Target]) -> f32 {
        let p = self.n(i).p;
        let mut best = f32::MAX;
        for t in targets.iter_mut() {
            let d = dist(p, t.p);
            if d < t.best {
                t.best = d;
                t.best_node = Some(i);
            }
            best = best.min(d);
        }
        best
    }

    fn reconstruct(&self, end: u32, target: [i32; 3], reached: bool) -> Path {
        let mut nodes = vec![(self.n(end).p, self.n(end).kind)];
        let mut cur = end;
        while let Some(prev) = self.n(cur).from {
            cur = prev;
            nodes.push((self.n(cur).p, self.n(cur).kind));
        }
        nodes.reverse();
        Path::new(nodes, target, reached)
    }

    fn run(
        &mut self,
        targets: &[[i32; 3]],
        max_range: f32,
        reach: i32,
        max_visited: i32,
        multiplier: f32,
    ) -> Option<Path> {
        let start = self.start();
        let mut ts: Vec<Target> = targets
            .iter()
            .map(|&p| Target {
                p,
                best: f32::MAX,
                best_node: None,
                reached: false,
            })
            .collect();
        let h = self.best_h(start, &mut ts);
        let s = self.nm(start);
        s.g = 0.0;
        s.h = h;
        s.f = h;
        self.insert(start);
        let start_p = self.n(start).p;
        let limit = (max_visited as f32 * multiplier) as i32;
        let mut i = 0;
        let mut any = false;
        while !self.heap.is_empty() {
            i += 1;
            if i >= limit {
                break;
            }
            let cur = self.pop();
            self.nm(cur).closed = true;
            let cp = self.n(cur).p;
            for t in ts.iter_mut() {
                if manhattan(cp, t.p) <= reach as f32 {
                    t.reached = true;
                    any = true;
                }
            }
            if any {
                break;
            }
            if dist(cp, start_p) >= max_range {
                continue;
            }
            for nb in self.neighbors(cur) {
                let d = dist(cp, self.n(nb).p);
                let walked = self.n(cur).walked + d;
                self.nm(nb).walked = walked;
                let g = self.n(cur).g + d + self.n(nb).malus;
                let open = self.n(nb).heap >= 0;
                if walked < max_range && (!open || g < self.n(nb).g) {
                    let h = self.best_h(nb, &mut ts) * 1.5;
                    let n = self.nm(nb);
                    n.from = Some(cur);
                    n.g = g;
                    n.h = h;
                    if open {
                        self.change_cost(nb, g + h);
                    } else {
                        self.nm(nb).f = g + h;
                        self.insert(nb);
                    }
                }
            }
        }
        let paths = ts
            .iter()
            .filter(|t| !any || t.reached)
            .filter_map(|t| t.best_node.map(|n| self.reconstruct(n, t.p, t.reached)));
        if any {
            paths.min_by_key(|p| p.nodes.len())
        } else {
            paths.min_by(|a, b| {
                a.dist_to_target
                    .total_cmp(&b.dist_to_target)
                    .then(a.nodes.len().cmp(&b.nodes.len()))
            })
        }
    }
}

/// Ограничивает ли блок рамку частично (`doesBlockHavePartialCollision`).
fn partial_collision(t: PathType) -> bool {
    matches!(
        t,
        PathType::Fence | PathType::DoorWoodClosed | PathType::DoorIronClosed
    )
}

/// `WalkNodeEvaluator.getFloorLevel(BlockGetter, BlockPos)`: верх формы блока снизу.
pub fn floor_level(w: &World, p: [i32; 3]) -> f64 {
    let below = [p[0], p[1] - 1, p[2]];
    let top = (w.shape)(below)
        .iter()
        .map(|b| b[4])
        .fold(None, |m: Option<f64>, y| Some(m.map_or(y, |m| m.max(y))));
    below[1] as f64 + top.unwrap_or(0.0)
}

/// `PathFinder.findPath` для наземного моба.
pub fn find_path(
    w: &World,
    m: &Walker,
    targets: &[[i32; 3]],
    max_range: f32,
    reach: i32,
    max_visited: i32,
    multiplier: f32,
) -> Option<Path> {
    Search::new(w, m).run(targets, max_range, reach, max_visited, multiplier)
}

/// Найденный путь (`Path`): узлы, следующий узел и цель.
/// `WalkNodeEvaluator.getPathTypeStatic` для моба (`GoalUtils.hasMalus`).
pub fn type_static(w: &World, m: &Walker, p: [i32; 3]) -> PathType {
    Search::new(w, m).type_static(p)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    pub nodes: Vec<([i32; 3], PathType)>,
    pub next: usize,
    pub target: [i32; 3],
    pub reached: bool,
    pub dist_to_target: f32,
}

impl Path {
    fn new(nodes: Vec<([i32; 3], PathType)>, target: [i32; 3], reached: bool) -> Self {
        let dist_to_target = nodes
            .last()
            .map_or(f32::MAX, |(p, _)| manhattan(*p, target));
        Self {
            nodes,
            next: 0,
            target,
            reached,
            dist_to_target,
        }
    }

    pub fn is_done(&self) -> bool {
        self.next >= self.nodes.len()
    }

    pub fn end(&self) -> Option<[i32; 3]> {
        self.nodes.last().map(|n| n.0)
    }

    /// `getEntityPosAtNode`: центр узла под ширину моба.
    pub fn entity_pos_at(&self, i: usize, width: f64) -> [f64; 3] {
        let p = self.nodes[i].0;
        let off = ((width as f32 + 1.0) as i32) as f64 * 0.5;
        [p[0] as f64 + off, p[1] as f64, p[2] as f64 + off]
    }

    /// `sameAs`: те же узлы.
    fn same_as(&self, other: &Path) -> bool {
        self.nodes.len() == other.nodes.len()
            && self.nodes.iter().zip(&other.nodes).all(|(a, b)| a.0 == b.0)
    }
}

/// `atBottomCenterOf`.
fn bottom_center(p: [i32; 3]) -> [f64; 3] {
    [p[0] as f64 + 0.5, p[1] as f64, p[2] as f64 + 0.5]
}

fn d2(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// Навигация по земле (`GroundPathNavigation`): путь, следование, застревание.
///
/// Игровое время — собственный счётчик тиков навигации: моб тикает каждый
/// тик сервера, пока его чанк загружен, а вне загрузки ванильный моб тоже стоит.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Navigation {
    pub path: Option<Path>,
    /// `speedModifier`.
    pub speed: f64,
    tick: i64,
    last_stuck_check: i64,
    last_stuck_pos: [f64; 3],
    timeout_node: [i32; 3],
    timeout_timer: i64,
    last_timeout_check: i64,
    timeout_limit: f64,
    pub stuck: bool,
    /// `targetPos` и `reachRange` последнего пути.
    pub target: Option<[i32; 3]>,
    reach: i32,
    last_recompute: i64,
    delayed_recompute: bool,
    /// Куда просили идти (для целей, пока их нет).
    pub requested: Option<[i32; 3]>,
    /// `GroundPathNavigation.avoidSun` (`RestrictSunGoal`).
    pub avoid_sun: bool,
    /// `WallClimberNavigation` паука и `pathToPosition`.
    pub climber: bool,
    pub to_position: Option<[i32; 3]>,
}

/// Множитель узлов поиска (`maxVisitedNodesMultiplier`).
const NODES_MULTIPLIER: f32 = 1.0;

impl Navigation {
    /// Наземная навигация; `climber` — `WallClimberNavigation` паука.
    pub fn new(climber: bool) -> Self {
        Self {
            climber,
            ..Self::default()
        }
    }

    pub fn is_done(&self) -> bool {
        self.path.as_ref().is_none_or(Path::is_done)
    }

    pub fn stop(&mut self) {
        self.path = None;
    }

    /// `canUpdatePath`: на земле или в жидкости.
    fn can_update(m: &Walker) -> bool {
        m.on_ground || m.in_water
    }

    /// `GroundPathNavigation.getTempMobPos`.
    fn temp_pos(m: &Walker, w: &World) -> [f64; 3] {
        let y = if m.in_water && m.can_float {
            let b = m.block();
            let mut y = b[1];
            let mut n = 0;
            loop {
                if !(w.water)([b[0], y, b[2]]) {
                    break y;
                }
                y += 1;
                n += 1;
                if n > 16 {
                    break b[1];
                }
            }
        } else {
            (m.pos[1] + 0.5).floor() as i32
        };
        [m.pos[0], y as f64, m.pos[2]]
    }

    /// `findSurfacePosition`: цель в воздухе — вниз до земли, в толще — вверх.
    fn surface(w: &World, m: &Walker, mut p: [i32; 3]) -> [i32; 3] {
        let max_y = m.min_y + 4064;
        if cell(w, p).has(Cell::AIR) {
            let mut q = [p[0], p[1] - 1, p[2]];
            while q[1] >= m.min_y && cell(w, q).has(Cell::AIR) {
                q[1] -= 1;
            }
            if q[1] >= m.min_y {
                return [q[0], q[1] + 1, q[2]];
            }
            q[1] = p[1] + 1;
            while q[1] <= max_y && cell(w, q).has(Cell::AIR) {
                q[1] += 1;
            }
            p = q;
        }
        if !cell(w, p).has(Cell::SOLID) {
            return p;
        }
        let mut q = [p[0], p[1] + 1, p[2]];
        while q[1] <= max_y && cell(w, q).has(Cell::SOLID) {
            q[1] += 1;
        }
        q
    }

    /// `PathNavigation.createPath(Set, regionOffset, offsetUpward, reach, maxPathLength)`.
    pub fn create_path(
        &mut self,
        w: &World,
        m: &Walker,
        target: [i32; 3],
        reach: i32,
        follow_range: f64,
    ) -> Option<Path> {
        if m.pos[1] < m.min_y as f64 || !Self::can_update(m) {
            return None;
        }
        if let Some(p) = &self.path
            && !p.is_done()
            && self.target == Some(target)
        {
            return Some(p.clone());
        }
        let max_len = (follow_range as f32).max(16.0);
        let max_visited = (follow_range * 16.0).floor() as i32;
        let path = find_path(
            w,
            m,
            &[target],
            max_len,
            reach,
            max_visited,
            NODES_MULTIPLIER,
        )?;
        self.target = Some(path.target);
        self.reach = reach;
        self.reset_stuck_timeout();
        Some(path)
    }

    /// `GroundPathNavigation.createPath(BlockPos, reach)` с поиском поверхности.
    pub fn path_to(
        &mut self,
        w: &World,
        m: &Walker,
        to: [f64; 3],
        reach: i32,
        follow_range: f64,
    ) -> Option<Path> {
        let p = [
            to[0].floor() as i32,
            to[1].floor() as i32,
            to[2].floor() as i32,
        ];
        if self.climber {
            self.to_position = Some(p);
        }
        let p = Self::surface(w, m, p);
        self.create_path(w, m, p, reach, follow_range)
    }

    /// `moveTo(Path, speed)`.
    pub fn move_to(&mut self, w: &World, m: &Walker, path: Option<Path>, speed: f64) -> bool {
        let Some(path) = path else {
            self.path = None;
            return false;
        };
        if !self.path.as_ref().is_some_and(|p| p.same_as(&path)) {
            self.path = Some(path);
        }
        if self.is_done() {
            return false;
        }
        self.trim_path(w, m);
        if self.path.as_ref().is_none_or(|p| p.nodes.is_empty()) {
            return false;
        }
        self.speed = speed;
        self.last_stuck_check = self.tick;
        self.last_stuck_pos = Self::temp_pos(m, w);
        true
    }

    /// `trimPath`: котлы — узел на блок выше.
    fn trim_path(&mut self, w: &World, m: &Walker) {
        let Some(path) = &mut self.path else {
            return;
        };
        for i in 0..path.nodes.len() {
            let (p, k) = path.nodes[i];
            if cell(w, p).has(Cell::CAULDRON) {
                path.nodes[i] = ([p[0], p[1] + 1, p[2]], k);
                if let Some(&(n, nk)) = path.nodes.get(i + 1)
                    && p[1] >= n[1]
                {
                    path.nodes[i + 1] = ([n[0], p[1] + 1, n[2]], nk);
                }
            }
        }
        // `GroundPathNavigation.trimPath`: с солнцем — обрезать у первого узла под небом.
        if self.avoid_sun {
            let sky = |p: [i32; 3]| w.info.is_some_and(|f| f(p).sky);
            let at = [
                m.pos[0].floor() as i32,
                (m.pos[1] + 0.5).floor() as i32,
                m.pos[2].floor() as i32,
            ];
            if sky(at) {
                return;
            }
            if let Some(i) = path.nodes.iter().position(|(p, _)| sky(*p)) {
                path.nodes.truncate(i);
            }
        }
    }

    /// `recomputePath`: не чаще раза в 20 тиков, иначе — отложенно.
    pub fn recompute(&mut self, w: &World, m: &Walker, follow_range: f64) {
        if self.tick - self.last_recompute > 20 && Self::can_update(m) {
            if let Some(t) = self.target {
                self.path = None;
                self.path = self.create_path(w, m, t, self.reach, follow_range);
                self.last_recompute = self.tick;
                self.delayed_recompute = false;
            }
        } else {
            self.delayed_recompute = true;
        }
    }

    /// `shouldRecomputePath`: блок изменился рядом с оставшимся путём.
    pub fn should_recompute(&self, at: [i32; 3], m: &Walker) -> bool {
        if self.delayed_recompute {
            return false;
        }
        let Some(path) = &self.path else {
            return false;
        };
        if path.is_done() || path.nodes.is_empty() {
            return false;
        }
        let end = path.end().expect("непустой путь");
        let mid = [
            (end[0] as f64 + m.pos[0]) / 2.0,
            (end[1] as f64 + m.pos[1]) / 2.0,
            (end[2] as f64 + m.pos[2]) / 2.0,
        ];
        let r = (path.nodes.len() - path.next) as f64;
        let c = [at[0] as f64 + 0.5, at[1] as f64 + 0.5, at[2] as f64 + 0.5];
        d2(c, mid) < r * r
    }

    /// `PathNavigation.tick`: куда идти (`MoveControl.setWantedPosition`).
    pub fn tick(&mut self, w: &World, m: &Walker, follow_range: f64) -> Option<([f64; 3], f64)> {
        // `WallClimberNavigation.tick`: путь кончился — прямо к блоку цели.
        if self.climber && self.is_done() {
            let p = self.to_position?;
            let w2 = m.width * m.width;
            let near = |q: [i32; 3]| d2(q.map(|v| f64::from(v) + 0.5), m.pos) < w2;
            if near(p)
                || (m.pos[1] > f64::from(p[1]) && near([p[0], m.pos[1].floor() as i32, p[2]]))
            {
                self.to_position = None;
                return None;
            }
            return Some((p.map(f64::from), self.speed));
        }
        self.tick += 1;
        if self.delayed_recompute {
            self.recompute(w, m, follow_range);
        }
        if self.is_done() {
            return None;
        }
        if Self::can_update(m) {
            self.follow(w, m);
        } else if let Some(path) = &mut self.path
            && !path.is_done()
        {
            let temp = Self::temp_pos(m, w);
            let next = path.entity_pos_at(path.next, m.width);
            if temp[1] > next[1]
                && !m.on_ground
                && temp[0].floor() == next[0].floor()
                && temp[2].floor() == next[2].floor()
            {
                path.next += 1;
            }
        }
        if self.is_done() {
            return None;
        }
        let path = self.path.as_ref().expect("путь не закончен");
        let next = path.entity_pos_at(path.next, m.width);
        Some(([next[0], ground_y(w, next), next[2]], self.speed))
    }

    /// `followThePath`.
    fn follow(&mut self, w: &World, m: &Walker) {
        let temp = Self::temp_pos(m, w);
        let width = m.width as f32;
        let max_d = if width > 0.75 {
            width / 2.0
        } else {
            0.75 - width / 2.0
        };
        let path = self.path.as_mut().expect("путь есть");
        let (next, kind) = path.nodes[path.next];
        let dx = (m.pos[0] - (next[0] as f64 + 0.5)).abs();
        let dy = (m.pos[1] - next[1] as f64).abs();
        let dz = (m.pos[2] - (next[2] as f64 + 0.5)).abs();
        let close = dx < max_d as f64 && dz < max_d as f64 && dy < 1.0;
        if close || (can_cut_corner(kind) && target_next_in_direction(path, temp)) {
            path.next += 1;
        }
        self.stuck_detection(temp, m);
    }

    /// `doStuckDetection`.
    fn stuck_detection(&mut self, temp: [f64; 3], m: &Walker) {
        if self.tick - self.last_stuck_check > 100 {
            let speed = if m.speed >= 1.0 {
                m.speed
            } else {
                m.speed * m.speed
            };
            let thr = speed * 100.0 * 0.25;
            if d2(temp, self.last_stuck_pos) < f64::from(thr * thr) {
                self.stuck = true;
                self.stop();
            } else {
                self.stuck = false;
            }
            self.last_stuck_check = self.tick;
            self.last_stuck_pos = temp;
        }
        if let Some(path) = &self.path
            && !path.is_done()
        {
            let next = path.nodes[path.next].0;
            let now = self.tick;
            if next == self.timeout_node {
                self.timeout_timer += now - self.last_timeout_check;
            } else {
                self.timeout_node = next;
                let d = d2(temp, bottom_center(next)).sqrt();
                self.timeout_limit = if m.speed > 0.0 {
                    d / f64::from(m.speed) * 20.0
                } else {
                    0.0
                };
            }
            if self.timeout_limit > 0.0 && self.timeout_timer as f64 > self.timeout_limit * 3.0 {
                self.reset_stuck_timeout();
                self.stop();
            }
            self.last_timeout_check = now;
        }
    }

    fn reset_stuck_timeout(&mut self) {
        self.timeout_node = [0; 3];
        self.timeout_timer = 0;
        self.timeout_limit = 0.0;
        self.stuck = false;
    }
}

/// `canCutCorner`: не срезать у огня, опасного и в двери.
fn can_cut_corner(t: PathType) -> bool {
    !matches!(
        t,
        PathType::FireInNeighbor | PathType::DamagingInNeighbor | PathType::WalkableDoor
    )
}

/// `shouldTargetNextNodeInDirection`.
fn target_next_in_direction(path: &Path, temp: [f64; 3]) -> bool {
    if path.next + 1 >= path.nodes.len() {
        return false;
    }
    let next = bottom_center(path.nodes[path.next].0);
    if d2(temp, next) >= 4.0 {
        return false;
    }
    // `canMoveDirectly` у наземной навигации всегда `false`.
    let after = bottom_center(path.nodes[path.next + 1].0);
    let a = [next[0] - temp[0], next[1] - temp[1], next[2] - temp[2]];
    let b = [after[0] - temp[0], after[1] - temp[1], after[2] - temp[2]];
    let la = a[0] * a[0] + a[1] * a[1] + a[2] * a[2];
    let lb = b[0] * b[0] + b[1] * b[1] + b[2] * b[2];
    if lb < la || la < 0.5 {
        let norm = |v: [f64; 3]| {
            let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            if l < 1.0e-5 {
                [0.0; 3]
            } else {
                [v[0] / l, v[1] / l, v[2] / l]
            }
        };
        let (a, b) = (norm(a), norm(b));
        return b[0] * a[0] + b[1] * a[1] + b[2] * a[2] < 0.0;
    }
    false
}

/// `Mth.atan2`: табличное приближение ванили, не `f64::atan2`. Расходится с
/// точным в последних знаках, и за сотню тиков это сдвигает моба на сотые блока.
pub fn atan2(mut y: f64, mut x: f64) -> f64 {
    static TABS: std::sync::OnceLock<(Vec<f64>, Vec<f64>)> = std::sync::OnceLock::new();
    let (asin_tab, cos_tab) = TABS.get_or_init(|| {
        (0..257)
            .map(|i| {
                let a = (f64::from(i) / 256.0).asin();
                (a, a.cos())
            })
            .unzip()
    });
    let len2 = x * x + y * y;
    if len2.is_nan() {
        return f64::NAN;
    }
    let neg_y = y < 0.0;
    if neg_y {
        y = -y;
    }
    let neg_x = x < 0.0;
    if neg_x {
        x = -x;
    }
    let swap = y > x;
    if swap {
        std::mem::swap(&mut x, &mut y);
    }
    // `fastInvSqrt`.
    let inv = {
        let half = 0.5 * len2;
        let v = f64::from_bits(6910469410427058090u64.wrapping_sub(len2.to_bits() >> 1));
        v * (1.5 - half * v * v)
    };
    x *= inv;
    y *= inv;
    let frac_bias = f64::from_bits(4805340802404319232);
    let biased = frac_bias + y;
    let i = biased.to_bits() as i32 as usize;
    let d5 = biased - frac_bias;
    let d6 = y * cos_tab[i] - x * d5;
    let mut r = asin_tab[i] + (6.0 + d6 * d6) * d6 * 0.16666666666666666;
    if swap {
        r = std::f64::consts::FRAC_PI_2 - r;
    }
    if neg_x {
        r = std::f64::consts::PI - r;
    }
    if neg_y {
        r = -r;
    }
    r
}

/// `PathNavigation.getGroundY`.
fn ground_y(w: &World, v: [f64; 3]) -> f64 {
    let p = [
        v[0].floor() as i32,
        v[1].floor() as i32,
        v[2].floor() as i32,
    ];
    if cell(w, [p[0], p[1] - 1, p[2]]).has(Cell::AIR) {
        v[1]
    } else {
        floor_level(w, p)
    }
}

/// Режим `MoveControl`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MoveOp {
    #[default]
    Wait,
    MoveTo,
    Jumping,
    Strafe,
}

/// `MoveControl`: куда хочет моб и с каким множителем скорости.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MoveControl {
    pub wanted: [f64; 3],
    pub speed: f64,
    pub op: MoveOp,
    /// `strafeForwards`, `strafeRight`.
    pub strafe_fwd: f32,
    pub strafe_right: f32,
}

impl MoveControl {
    /// `setWantedPosition`.
    pub fn set_wanted(&mut self, to: [f64; 3], speed: f64) {
        self.wanted = to;
        self.speed = speed;
        if self.op != MoveOp::Jumping {
            self.op = MoveOp::MoveTo;
        }
    }

    /// `strafe(forward, right)`: множитель скорости 0.25.
    pub fn strafe(&mut self, forward: f32, right: f32) {
        self.op = MoveOp::Strafe;
        self.strafe_fwd = forward;
        self.strafe_right = right;
        self.speed = 0.25;
    }
}

/// `MoveControl.rotlerp`.
pub fn rotlerp(from: f32, to: f32, max: f32) -> f32 {
    let d = wrap(to - from).clamp(-max, max);
    let mut r = from + d;
    if r < 0.0 {
        r += 360.0;
    } else if r > 360.0 {
        r -= 360.0;
    }
    r
}

/// `MoveControl.tick`: поворот к точке, скорость и прыжок на ступень.
pub fn move_control(m: &mut Mob, w: &World, in_water: bool) {
    let base = m.speed();
    match m.move_ctl.op {
        MoveOp::MoveTo => {
            m.move_ctl.op = MoveOp::Wait;
            let dx = m.move_ctl.wanted[0] - m.pos[0];
            let dz = m.move_ctl.wanted[2] - m.pos[2];
            let dy = m.move_ctl.wanted[1] - m.pos[1];
            if dx * dx + dy * dy + dz * dz < 2.500000277905201E-7 {
                m.zza = 0.0;
                return;
            }
            let yaw = (atan2(dz, dx) * 57.2957763671875) as f32 - 90.0;
            m.yaw = rotlerp(m.yaw, yaw, 90.0);
            m.set_speed((m.move_ctl.speed * base) as f32);
            let b = [
                m.pos[0].floor() as i32,
                m.pos[1].floor() as i32,
                m.pos[2].floor() as i32,
            ];
            let shape = (w.shape)(b);
            let top = shape.iter().map(|s| s[4]).fold(f64::MIN, f64::max);
            let c = cell(w, b);
            let width = m.size().0 as f32;
            if (dy > f64::from(m.species.info().step_height as f32)
                && dx * dx + dz * dz < f64::from(1.0f32.max(width)))
                || (!shape.is_empty()
                    && m.pos[1] < top + b[1] as f64
                    && !c.has(Cell::DOOR)
                    && !c.has(Cell::FENCE))
            {
                m.jump = true;
                m.move_ctl.op = MoveOp::Jumping;
            }
        }
        MoveOp::Jumping => {
            m.set_speed((m.move_ctl.speed * base) as f32);
            if m.on_ground || in_water {
                m.move_ctl.op = MoveOp::Wait;
            }
        }
        MoveOp::Strafe => {
            let f = m.move_ctl.speed as f32 * base as f32;
            let (mut f1, mut f2) = (m.move_ctl.strafe_fwd, m.move_ctl.strafe_right);
            let mut f3 = (f1 * f1 + f2 * f2).sqrt();
            if f3 < 1.0 {
                f3 = 1.0;
            }
            f3 = f / f3;
            f1 *= f3;
            f2 *= f3;
            let r = f64::from(m.yaw * 0.017453292f32);
            let (f4, f5) = (
                chunkwake_worldgen::carver::sin(r),
                chunkwake_worldgen::carver::cos(r),
            );
            let dx = f1 * f5 - f2 * f4;
            let dz = f2 * f5 + f1 * f4;
            // `isWalkable`: клетка, куда шагнём, проходима.
            let walk = super::walker(m, w, 0, in_water);
            let p = [
                (m.pos[0] + f64::from(dx)).floor() as i32,
                m.pos[1].floor() as i32,
                (m.pos[2] + f64::from(dz)).floor() as i32,
            ];
            if type_static(w, &walk, p) != PathType::Walkable {
                m.move_ctl.strafe_fwd = 1.0;
                m.move_ctl.strafe_right = 0.0;
            }
            m.set_speed(f);
            m.zza = f64::from(m.move_ctl.strafe_fwd);
            m.xxa = f64::from(m.move_ctl.strafe_right);
            m.move_ctl.op = MoveOp::Wait;
        }
        MoveOp::Wait => m.zza = 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::super::FULL;
    use super::*;

    fn walker(pos: [f64; 3]) -> Walker {
        Walker {
            pos,
            width: 0.6,
            height: 1.95,
            on_ground: true,
            in_water: false,
            step: 0.6,
            max_fall: 3,
            speed: 0.23,
            can_float: false,
            can_open_doors: false,
            can_pass_doors: true,
            can_walk_over_fences: false,
            malus: &[],
            min_y: -64,
        }
    }

    /// Мир по функции «твёрдый ли блок».
    fn solid(f: impl Fn([i32; 3]) -> bool + 'static) -> impl Fn([i32; 3]) -> &'static [[f64; 6]] {
        move |p| if f(p) { FULL } else { &[] }
    }

    fn search(
        shape: &dyn Fn([i32; 3]) -> &'static [[f64; 6]],
        cell: Option<&dyn Fn([i32; 3]) -> Cell>,
        from: [f64; 3],
        to: [i32; 3],
        visited: i32,
    ) -> Option<Path> {
        let w = World {
            shape,
            water: &|_| false,
            cell,
            info: None,
            min_y: -64,
        };
        find_path(&w, &walker(from), &[to], 35.0, 0, visited, 1.0)
    }

    fn cells(p: &Path) -> Vec<[i32; 3]> {
        p.nodes.iter().map(|n| n.0).collect()
    }

    #[test]
    fn straight_line_on_flat_floor() {
        let shape = solid(|p| p[1] < 64);
        let p = search(&shape, None, [0.5, 64.0, 0.5], [5, 64, 0], 560).unwrap();
        assert!(p.reached);
        assert_eq!(
            cells(&p),
            (0..=5).map(|x| [x, 64, 0]).collect::<Vec<_>>(),
            "{p:?}"
        );
    }

    #[test]
    fn goes_around_a_wall() {
        // Стена в два блока x = 3, z от −3 до 3: обход по краю.
        let shape = solid(|p| p[1] < 64 || (p[0] == 3 && (-3..=3).contains(&p[2]) && p[1] < 66));
        let p = search(&shape, None, [0.5, 64.0, 0.5], [6, 64, 0], 560).unwrap();
        assert!(p.reached, "{p:?}");
        for c in cells(&p) {
            assert!(
                !(c[0] == 3 && (-3..=3).contains(&c[2])),
                "through the wall: {p:?}"
            );
            assert_eq!(c[1], 64);
        }
        // Обход — по самому короткому краю: через z = 4 или −4.
        assert!(cells(&p).iter().any(|c| c[2].abs() == 4), "{p:?}");
    }

    #[test]
    fn climbs_one_block_but_not_two() {
        // Ступень в один блок: путь идёт по ней.
        let shape = solid(|p| p[1] < 64 || (p[0] >= 3 && p[1] < 65));
        let p = search(&shape, None, [0.5, 64.0, 0.5], [6, 65, 0], 560).unwrap();
        assert!(p.reached);
        assert!(cells(&p).contains(&[3, 65, 0]), "{p:?}");
        // Две ступени сразу — нет: только частичный путь к краю.
        let shape = solid(|p| p[1] < 64 || (p[0] >= 3 && p[1] < 66));
        let p = search(&shape, None, [0.5, 64.0, 0.5], [6, 66, 0], 560).unwrap();
        assert!(!p.reached, "{p:?}");
        assert!(cells(&p).iter().all(|c| c[1] == 64), "{p:?}");
    }

    #[test]
    fn steps_onto_slab_and_down() {
        const SLAB: &[[f64; 6]] = &[[0.0, 0.0, 0.0, 1.0, 0.5, 1.0]];
        let shape = |p: [i32; 3]| {
            if p[1] < 64 {
                FULL
            } else if p[1] == 64 && p[0] == 3 {
                SLAB
            } else {
                &[]
            }
        };
        let p = search(&shape, None, [0.5, 64.0, 0.5], [6, 64, 0], 560).unwrap();
        assert!(p.reached);
        // Полублок — клетка выше (64 → 65 над плитой), без обхода.
        assert!(cells(&p).iter().all(|c| c[2] == 0), "{p:?}");
        assert!(cells(&p).contains(&[3, 65, 0]), "{p:?}");
    }

    #[test]
    fn falls_at_most_three_blocks() {
        // Обрыв на x ≥ 2: пол на 61 (3 вниз) проходим, на 60 (4 вниз) — нет.
        for (floor, ok) in [(61, true), (60, false)] {
            let shape = solid(move |p| if p[0] >= 2 { p[1] < floor } else { p[1] < 64 });
            let p = search(&shape, None, [0.5, 64.0, 0.5], [5, floor, 0], 560).unwrap();
            assert_eq!(p.reached, ok, "floor {floor}: {p:?}");
        }
    }

    #[test]
    fn lava_is_impassable_and_fire_avoided() {
        let shape = solid(|p| p[1] < 64);
        // Лавовое озеро на x = 2..3, z −1..1 под уровнем пола: в обход.
        let lava = |p: [i32; 3]| (2..=3).contains(&p[0]) && (-1..=1).contains(&p[2]) && p[1] == 63;
        let cellf = move |p: [i32; 3]| {
            if lava(p) {
                Cell {
                    kind: PathType::Lava,
                    flags: 0,
                }
            } else if p[1] < 64 {
                Cell {
                    kind: PathType::Blocked,
                    flags: Cell::SOLID,
                }
            } else {
                Cell::AIR_CELL
            }
        };
        let shape2 = move |p: [i32; 3]| if lava(p) { &[][..] } else { shape(p) };
        let p = search(&shape2, Some(&cellf), [0.5, 64.0, 0.5], [6, 64, 0], 560).unwrap();
        assert!(p.reached, "{p:?}");
        for c in cells(&p) {
            assert!(
                !lava([c[0], c[1] - 1, c[2]]) && !lava(c),
                "over lava: {p:?}"
            );
        }
    }

    #[test]
    fn maze() {
        // Коридор-змейка: стены по чётным x, проход то у z = 0, то у z = 4.
        let wall = |p: [i32; 3]| {
            if p[1] < 64 {
                return true;
            }
            if p[1] > 65 {
                return false;
            }
            if !(0..=4).contains(&p[2]) {
                return p[0] >= -1 && p[0] <= 9;
            }
            if p[0] == -1 || p[0] == 9 {
                return true;
            }
            match p[0] {
                2 => p[2] != 4,
                4 => p[2] != 0,
                6 => p[2] != 4,
                _ => false,
            }
        };
        let shape = solid(wall);
        let p = search(&shape, None, [0.5, 64.0, 0.5], [8, 64, 0], 560).unwrap();
        assert!(p.reached, "{p:?}");
        // Прошёл через все три прохода.
        let cs = cells(&p);
        assert!(cs.contains(&[2, 64, 4]) && cs.contains(&[4, 64, 0]) && cs.contains(&[6, 64, 4]));
        for w in cs.windows(2) {
            let d = (w[0][0] - w[1][0]).abs() + (w[0][2] - w[1][2]).abs();
            assert!(d <= 2, "jump in path {w:?}");
        }
    }

    #[test]
    fn node_limit_gives_partial_path() {
        // Стена с обходом далеко: при малом лимите узлов — частичный путь,
        // ближе всего к цели по прямой.
        let shape = solid(|p| p[1] < 64 || (p[0] == 3 && (-20..=20).contains(&p[2]) && p[1] < 66));
        let p = search(&shape, None, [0.5, 64.0, 0.5], [6, 64, 0], 40).unwrap();
        assert!(!p.reached, "{p:?}");
        assert_eq!(p.end(), Some([2, 64, 0]), "{p:?}");
    }

    #[test]
    fn rotlerp_matches_vanilla() {
        assert_eq!(rotlerp(0.0, 45.0, 90.0), 45.0);
        assert_eq!(rotlerp(0.0, 170.0, 90.0), 90.0);
        assert_eq!(rotlerp(10.0, -100.0, 90.0), 280.0);
        assert_eq!(rotlerp(350.0, 30.0, 90.0), 30.0);
    }

    #[test]
    fn atan2_close_to_exact() {
        for (y, x) in [
            (1.0, 1.0),
            (-0.03, 0.8),
            (2.0, -0.5),
            (-1.0, -3.0),
            (0.0, 1.0),
        ] {
            let d: f64 = atan2(y, x) - f64::atan2(y, x);
            assert!(d.abs() < 1e-5, "{y} {x}: {d}");
        }
        assert_eq!(atan2(0.0, 1.0), 0.0);
    }

    #[test]
    fn max_fall_with_target() {
        assert_eq!(max_fall_distance(false, 20.0, 20.0, 2), 3);
        // 20 − 6.6 = 13 − (3 − 2)·4 = 9 → 12.
        assert_eq!(max_fall_distance(true, 20.0, 20.0, 2), 12);
        assert_eq!(max_fall_distance(true, 5.0, 20.0, 1), 3);
    }
}
