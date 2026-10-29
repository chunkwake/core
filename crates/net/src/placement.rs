//! Состояние ставимого блока (`Block.getStateForPlacement` 26.3): ориентация
//! по взгляду игрока и грани, половины ступеней и плит, настенные варианты,
//! двухблочные двери, растения и кровати.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Грани как в протоколе: низ, верх, север, юг, запад, восток.
const DIRS: [&str; 6] = ["down", "up", "north", "south", "west", "east"];
const DOWN: usize = 0;
const UP: usize = 1;

/// Смещение блока по направлению.
pub fn step(dir: usize) -> [i32; 3] {
    match dir {
        0 => [0, -1, 0],
        1 => [0, 1, 0],
        2 => [0, 0, -1],
        3 => [0, 0, 1],
        4 => [-1, 0, 0],
        _ => [1, 0, 0],
    }
}

fn opposite(dir: usize) -> usize {
    dir ^ 1
}

/// По часовой стрелке (вид сверху).
fn counter_clockwise(dir: usize) -> usize {
    opposite(clockwise(dir))
}

fn clockwise(dir: usize) -> usize {
    match dir {
        2 => 5,
        5 => 3,
        3 => 4,
        4 => 2,
        d => d,
    }
}

/// Откуда и как ставят блок.
#[derive(Clone, Copy, Debug)]
pub struct Ctx {
    /// Грань, по которой кликнули (0–5).
    pub face: usize,
    pub yaw: f32,
    pub pitch: f32,
    /// Высота точки клика внутри блока, 0–1.
    pub cursor_y: f32,
    /// Точка клика по X и Z относительно ставимого блока.
    pub cursor_x: f32,
    pub cursor_z: f32,
    /// Соседи по сторонам север, юг, запад, восток: на уровне блока и выше.
    pub near: [[u32; 2]; 4],
}

impl Ctx {
    /// `getHorizontalDirection`: куда смотрит игрок.
    fn horizontal(&self) -> usize {
        [3, 4, 2, 5][((self.yaw / 90.0 + 0.5).floor() as i32 & 3) as usize]
    }

    /// `getNearestLookingDirection` с учётом наклона.
    fn looking(&self) -> usize {
        if self.pitch > 45.0 {
            DOWN
        } else if self.pitch < -45.0 {
            UP
        } else {
            self.horizontal()
        }
    }

    /// Верхняя половина: клик снизу или в верхнюю часть боковой грани.
    fn top_half(&self) -> bool {
        self.face == DOWN || (self.face != UP && self.cursor_y > 0.5)
    }
}

type Props = Vec<(String, String)>;

/// Свойства одного блока в смешанной системе счисления: код состояния —
/// сумма `индекс значения × шаг`, смена свойства — арифметика без строк.
struct Table {
    keys: Vec<String>,
    values: Vec<Vec<String>>,
    strides: Vec<u32>,
    /// Код → состояние (`u32::MAX` — такого сочетания нет).
    states: Vec<u32>,
}

pub struct Placement {
    states: Arc<Vec<Option<(String, Props)>>>,
    /// По состоянию: таблица блока и код в ней.
    codes: Vec<(u32, u32)>,
    tables: Vec<Table>,
    /// Минимальный id состояния с этим именем (настенные варианты).
    by_name: HashMap<String, u32>,
    /// Прочные грани по состоянию (биты по `DIRS`).
    sturdy: Vec<u8>,
    /// Тег `minecraft:walls`.
    walls: HashSet<String>,
}

/// Чем блок соединяется с соседями (`CrossCollisionBlock`, `WallBlock`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Joint {
    /// Деревянный забор или нет (`isSameFence`).
    Fence(bool),
    Pane,
    Wall,
}

/// `Block.isExceptionForConnection`.
fn exception(name: &str) -> bool {
    name.ends_with("_leaves")
        || name.ends_with("shulker_box")
        || matches!(
            name,
            "minecraft:barrier"
                | "minecraft:carved_pumpkin"
                | "minecraft:jack_o_lantern"
                | "minecraft:melon"
                | "minecraft:pumpkin"
        )
}

/// Блоки с `facing` по взгляду игрока, а не к нему.
fn faces_away(name: &str) -> bool {
    name.ends_with("_stairs")
        || name.ends_with("_door")
        || name.ends_with("_fence_gate")
        || name.ends_with("_bed")
        || name == "minecraft:observer"
        || name == "minecraft:campfire"
        || name == "minecraft:soul_campfire"
}

/// Шестисторонний `facing` по грани клика (стержни, кристаллы).
fn faces_clicked(name: &str) -> bool {
    name.ends_with("_rod")
        || name.ends_with("amethyst_cluster")
        || name.ends_with("amethyst_bud")
        || name.ends_with("shulker_box")
}

impl Placement {
    pub fn default_state(&self, name: &str) -> Option<u32> {
        self.by_name.get(name).copied()
    }

    pub fn new(
        states: Vec<Option<(String, Props)>>,
        sturdy: Vec<u8>,
        walls: HashSet<String>,
    ) -> Self {
        Self::with_states(Arc::new(states), sturdy, walls)
    }

    /// То же, но список состояний уже есть у вызывающего: копии не нужны.
    pub fn with_states(
        states: Arc<Vec<Option<(String, Props)>>>,
        sturdy: Vec<u8>,
        walls: HashSet<String>,
    ) -> Self {
        let mut by_name = HashMap::new();
        // Ключи и значения каждого блока в порядке первого появления.
        let mut table_of: HashMap<&str, usize> = HashMap::new();
        let mut tables: Vec<Table> = Vec::new();
        for (id, s) in states.iter().enumerate() {
            if let Some((n, p)) = s {
                by_name
                    .entry(n.clone())
                    .and_modify(|m: &mut u32| *m = (*m).min(id as u32))
                    .or_insert(id as u32);
                let t = *table_of.entry(n.as_str()).or_insert_with(|| {
                    tables.push(Table {
                        keys: Vec::new(),
                        values: Vec::new(),
                        strides: Vec::new(),
                        states: Vec::new(),
                    });
                    tables.len() - 1
                });
                let t = &mut tables[t];
                for (k, v) in p {
                    let i = match t.keys.iter().position(|x| x == k) {
                        Some(i) => i,
                        None => {
                            t.keys.push(k.clone());
                            t.values.push(Vec::new());
                            t.keys.len() - 1
                        }
                    };
                    if !t.values[i].contains(v) {
                        t.values[i].push(v.clone());
                    }
                }
            }
        }
        for t in &mut tables {
            let mut stride = 1u32;
            for v in &t.values {
                t.strides.push(stride);
                stride *= v.len() as u32;
            }
            t.states = vec![u32::MAX; stride as usize];
        }
        let mut codes = vec![(u32::MAX, 0); states.len()];
        for (id, s) in states.iter().enumerate() {
            if let Some((n, p)) = s {
                let ti = table_of[n.as_str()];
                let t = &mut tables[ti];
                let mut code = 0;
                for (k, v) in p {
                    let i = t.keys.iter().position(|x| x == k).unwrap_or(0);
                    let j = t.values[i].iter().position(|x| x == v).unwrap_or(0);
                    code += j as u32 * t.strides[i];
                }
                t.states[code as usize] = id as u32;
                codes[id] = (ti as u32, code);
            }
        }
        drop(table_of);
        Self {
            states,
            codes,
            tables,
            by_name,
            sturdy,
            walls,
        }
    }

    fn joint(&self, state: u32) -> Option<Joint> {
        let name = self.name(state)?;
        if self.walls.contains(name) {
            Some(Joint::Wall)
        } else if name.ends_with("_fence") {
            Some(Joint::Fence(name != "minecraft:nether_brick_fence"))
        } else if name.ends_with("glass_pane") || name.ends_with("_bars") {
            Some(Joint::Pane)
        } else {
            None
        }
    }

    /// Соединяемый ли блок (забор, стена, стекло, решётка).
    pub fn connects(&self, state: u32) -> bool {
        self.joint(state).is_some()
    }

    /// Прочная ли грань `face` (`isFaceSturdy`, грани по `DIRS`).
    pub fn sturdy(&self, state: u32, face: usize) -> bool {
        self.sturdy
            .get(state as usize)
            .is_some_and(|m| m >> face & 1 == 1)
    }

    /// Калитка поперёк направления `dir` (`FenceGateBlock.connectsToDirection`).
    fn gate_across(&self, state: u32, dir: usize) -> bool {
        self.name(state).is_some_and(|n| n.ends_with("_fence_gate"))
            && self
                .prop(state, "facing")
                .and_then(|f| DIRS.iter().position(|d| *d == f))
                .is_some_and(|f| (f < 4) == (clockwise(dir) < 4))
    }

    /// Цепляется ли блок вида `j` к соседу `other` в сторону `dir`.
    fn attaches(&self, j: Joint, other: u32, dir: usize) -> bool {
        let solid = self.sturdy(other, opposite(dir)) && !self.name(other).is_some_and(exception);
        let oj = self.joint(other);
        match j {
            Joint::Fence(wood) => {
                solid || oj == Some(Joint::Fence(wood)) || self.gate_across(other, dir)
            }
            Joint::Pane => solid || matches!(oj, Some(Joint::Pane | Joint::Wall)),
            Joint::Wall => {
                solid
                    || matches!(oj, Some(Joint::Pane | Joint::Wall))
                    || self.gate_across(other, dir)
            }
        }
    }

    /// Состояние соединяемого блока по соседям: `sides` — север, юг, запад, восток,
    /// `above` — блок сверху (для стен).
    pub fn connect(&self, state: u32, sides: [u32; 4], above: u32) -> u32 {
        let Some(j) = self.joint(state) else {
            return state;
        };
        let mut s = state;
        let on: [bool; 4] = std::array::from_fn(|i| self.attaches(j, sides[i], i + 2));
        for (i, &c) in on.iter().enumerate() {
            let key = DIRS[i + 2];
            if j == Joint::Wall {
                // Высокая сторона, если сверху блок с прочной нижней гранью.
                let tall = self.sturdy(above, DOWN) || self.joint(above) == Some(Joint::Wall);
                self.set(
                    &mut s,
                    key,
                    if !c {
                        "none"
                    } else if tall {
                        "tall"
                    } else {
                        "low"
                    },
                );
            } else {
                self.set(&mut s, key, if c { "true" } else { "false" });
            }
        }
        if j == Joint::Wall {
            // Столб — если стена не прямая (`WallBlock.shouldRaisePost`).
            let [n, so, w, e] = [on[0], on[1], on[2], on[3]];
            let straight = (n && so && !w && !e) || (w && e && !n && !so);
            let post = !straight || self.prop(above, "up") == Some("true");
            self.set(&mut s, "up", if post { "true" } else { "false" });
        }
        s
    }

    pub fn name(&self, state: u32) -> Option<&str> {
        self.states
            .get(state as usize)?
            .as_ref()
            .map(|s| s.0.as_str())
    }

    /// Все свойства состояния (по имени).
    pub fn props(&self, state: u32) -> &[(String, String)] {
        self.states
            .get(state as usize)
            .and_then(|s| s.as_ref())
            .map_or(&[], |s| s.1.as_slice())
    }

    pub fn prop(&self, state: u32, key: &str) -> Option<&str> {
        let (_, p) = self.states.get(state as usize)?.as_ref()?;
        p.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    fn with(&self, state: u32, key: &str, value: &str) -> Option<u32> {
        let &(ti, code) = self.codes.get(state as usize)?;
        let t = self.tables.get(ti as usize)?;
        let i = t.keys.iter().position(|k| k == key)?;
        let j = t.values[i].iter().position(|v| v == value)? as u32;
        let (n, stride) = (t.values[i].len() as u32, t.strides[i]);
        let cur = code / stride % n;
        let next = code - cur * stride + j * stride;
        Some(t.states[next as usize]).filter(|&s| s != u32::MAX)
    }

    /// Состояние с другим значением свойства, если оно у блока есть.
    pub fn with_prop(&self, state: u32, key: &str, value: &str) -> Option<u32> {
        self.with(state, key, value)
    }

    /// Ставит свойство, если у блока оно есть и значение допустимо.
    fn set(&self, state: &mut u32, key: &str, value: &str) {
        if let Some(s) = self.with(*state, key, value) {
            *state = s;
        }
    }

    /// Настенный вариант: `torch` → `wall_torch`, `oak_sign` → `oak_wall_sign`.
    fn wall_variant(&self, state: u32) -> Option<u32> {
        let name = self.name(state)?;
        let (ns, path) = name.split_once(':')?;
        let wall = match path.rsplit_once('_') {
            Some((head, tail)) => format!("{ns}:{head}_wall_{tail}"),
            None => format!("{ns}:wall_{path}"),
        };
        self.by_name.get(&wall).copied()
    }

    /// Состояния для установки: `(смещение от цели, состояние)`, первое — сам блок.
    pub fn place(&self, default: u32, c: &Ctx) -> Vec<([i32; 3], u32)> {
        let Some(name) = self.name(default) else {
            return vec![([0; 3], default)];
        };
        let mut s = default;
        // Настенные факелы, таблички, флаги, головы.
        if c.face > UP
            && self.prop(s, "rotation").is_some() | name.ends_with("torch")
            && let Some(w) = self.wall_variant(s)
        {
            s = w;
            self.set(&mut s, "facing", DIRS[c.face]);
            return vec![([0; 3], s)];
        }
        if self.prop(s, "rotation").is_some() {
            let r = (((c.yaw + 180.0) * 16.0 / 360.0) + 0.5).floor() as i32 & 15;
            self.set(&mut s, "rotation", &r.to_string());
        }
        if self.prop(s, "axis").is_some() {
            self.set(&mut s, "axis", ["y", "y", "z", "z", "x", "x"][c.face]);
        }
        let player = c.horizontal();
        if self.prop(s, "face").is_some() {
            // Кнопки, рычаги, точило.
            let (attach, facing) = match c.face {
                UP => ("floor", player),
                DOWN => ("ceiling", player),
                f => ("wall", f),
            };
            self.set(&mut s, "face", attach);
            self.set(&mut s, "facing", DIRS[facing]);
        } else if self.prop(s, "facing").is_some() {
            let six = self.with(s, "facing", "up").is_some();
            let dir = if faces_clicked(name) {
                c.face
            } else if name.ends_with("_trapdoor") {
                if c.face > UP {
                    c.face
                } else {
                    opposite(player)
                }
            } else if name == "minecraft:hopper" {
                if c.face > UP { opposite(c.face) } else { DOWN }
            } else if name.ends_with("anvil") {
                clockwise(player)
            } else if six {
                if faces_away(name) {
                    c.looking()
                } else {
                    opposite(c.looking())
                }
            } else if faces_away(name) {
                player
            } else {
                opposite(player)
            };
            self.set(&mut s, "facing", DIRS[dir]);
        }
        // Костёр ставится горящим (`CampfireBlock.getStateForPlacement`).
        if name == "minecraft:campfire" || name == "minecraft:soul_campfire" {
            self.set(&mut s, "lit", "true");
        }
        if self.prop(s, "hanging").is_some() {
            self.set(
                &mut s,
                "hanging",
                if c.face == DOWN { "true" } else { "false" },
            );
        }
        let top = c.top_half();
        if self.prop(s, "type").is_some() && name.ends_with("_slab") {
            self.set(&mut s, "type", if top { "top" } else { "bottom" });
        }
        match self.prop(s, "half") {
            Some("top" | "bottom") => self.set(&mut s, "half", if top { "top" } else { "bottom" }),
            Some("upper" | "lower") => {
                self.set(&mut s, "half", "lower");
                if self.prop(s, "hinge").is_some() {
                    self.set(&mut s, "hinge", self.hinge(name, player, c));
                }
                let upper = self.with(s, "half", "upper").unwrap_or(s);
                return vec![([0; 3], s), ([0, 1, 0], upper)];
            }
            _ => {}
        }
        if self.prop(s, "part") == Some("foot")
            && let Some(f) = self
                .prop(s, "facing")
                .and_then(|f| DIRS.iter().position(|d| *d == f))
            && let Some(head) = self.with(s, "part", "head")
        {
            return vec![([0; 3], s), (step(f), head)];
        }
        vec![([0; 3], s)]
    }

    /// `DoorBlock.getHinge`: петли к полным блокам и от соседней двери, иначе по точке клика.
    fn hinge(&self, name: &str, facing: usize, c: &Ctx) -> &'static str {
        let near = |d: usize| c.near[d - 2];
        let full = |st: u32| self.sturdy.get(st as usize) == Some(&63);
        let door = |st: u32| self.name(st) == Some(name) && self.prop(st, "half") == Some("lower");
        let (l, r) = (near(counter_clockwise(facing)), near(clockwise(facing)));
        let i = -(full(l[0]) as i32) - full(l[1]) as i32 + full(r[0]) as i32 + full(r[1]) as i32;
        let (ld, rd) = (door(l[0]), door(r[0]));
        if (ld && !rd) || i > 0 {
            return "right";
        }
        if (rd && !ld) || i < 0 {
            return "left";
        }
        let o = step(facing);
        let (j, k) = (o[0], o[2]);
        let (d0, d1) = (c.cursor_x, c.cursor_z);
        if (j < 0 && d1 < 0.5) || (j > 0 && d1 > 0.5) || (k < 0 && d0 > 0.5) || (k > 0 && d0 < 0.5)
        {
            "right"
        } else {
            "left"
        }
    }

    fn is_rail(&self, state: u32) -> bool {
        self.name(state).is_some_and(|n| n.ends_with("rail")) && self.prop(state, "shape").is_some()
    }

    /// Стороны, куда ведёт форма рельса, и подъём (+1) в ту сторону.
    fn rail_links(&self, state: u32) -> Vec<(usize, i32)> {
        let (n, so, w, e) = (2, 3, 4, 5);
        match self.prop(state, "shape").unwrap_or("") {
            "north_south" => vec![(n, 0), (so, 0)],
            "east_west" => vec![(w, 0), (e, 0)],
            "ascending_north" => vec![(n, 1), (so, 0)],
            "ascending_south" => vec![(n, 0), (so, 1)],
            "ascending_east" => vec![(w, 0), (e, 1)],
            "ascending_west" => vec![(w, 1), (e, 0)],
            "south_east" => vec![(so, 0), (e, 0)],
            "south_west" => vec![(so, 0), (w, 0)],
            "north_west" => vec![(n, 0), (w, 0)],
            "north_east" => vec![(n, 0), (e, 0)],
            _ => Vec::new(),
        }
    }

    /// Рельс рядом в сторону `d`: на уровне, выше или ниже (`RailState.getRail`).
    fn rail_near(&self, pos: [i32; 3], d: usize, at: &dyn Fn([i32; 3]) -> u32) -> Option<[i32; 3]> {
        let o = step(d);
        [0, 1, -1]
            .into_iter()
            .map(|dy| [pos[0] + o[0], pos[1] + dy, pos[2] + o[2]])
            .find(|&p| self.is_rail(at(p)))
    }

    /// Ведёт ли рельс в `q` к блоку `p` (по горизонтали).
    fn rail_points_to(&self, q: [i32; 3], p: [i32; 3], at: &dyn Fn([i32; 3]) -> u32) -> bool {
        self.rail_links(at(q)).iter().any(|&(d, _)| {
            let o = step(d);
            q[0] + o[0] == p[0] && q[2] + o[2] == p[2]
        })
    }

    /// Живые связи рельса: стороны, где рельс действительно есть.
    fn rail_live(&self, q: [i32; 3], at: &dyn Fn([i32; 3]) -> u32) -> Vec<usize> {
        self.rail_links(at(q))
            .into_iter()
            .filter(|&(d, _)| {
                self.rail_near(q, d, at)
                    .is_some_and(|n| self.rail_points_to(n, q, at))
            })
            .map(|(d, _)| d)
            .collect()
    }

    /// Форма по сторонам (как `RailState.place`): прямая, поворот, подъём к рельсу выше.
    fn rail_shape(
        &self,
        state: u32,
        pos: [i32; 3],
        sides: &[usize],
        at: &dyn Fn([i32; 3]) -> u32,
    ) -> u32 {
        let has = |d| sides.contains(&d);
        let (n, so, w, e) = (has(2), has(3), has(4), has(5));
        let curve = |name: &str| self.with(state, "shape", name);
        let mut shape = if (n || so) && !w && !e {
            "north_south"
        } else if (w || e) && !n && !so {
            "east_west"
        } else if so && e && !n && !w && curve("south_east").is_some() {
            "south_east"
        } else if so && w && !n && !e && curve("south_west").is_some() {
            "south_west"
        } else if n && w && !so && !e && curve("north_west").is_some() {
            "north_west"
        } else if n && e && !so && !w && curve("north_east").is_some() {
            "north_east"
        } else if n || so {
            "north_south"
        } else if w || e {
            "east_west"
        } else {
            return state;
        };
        let above = |d: usize| {
            let o = step(d);
            self.is_rail(at([pos[0] + o[0], pos[1] + 1, pos[2] + o[2]]))
        };
        if shape == "north_south" {
            if above(2) {
                shape = "ascending_north";
            } else if above(3) {
                shape = "ascending_south";
            }
        } else if shape == "east_west" {
            if above(5) {
                shape = "ascending_east";
            } else if above(4) {
                shape = "ascending_west";
            }
        }
        curve(shape).unwrap_or(state)
    }

    /// Новый рельс в `pos`: его форма и перестроенные к нему соседи.
    pub fn rails(&self, pos: [i32; 3], at: &dyn Fn([i32; 3]) -> u32) -> Vec<([i32; 3], u32)> {
        let state = at(pos);
        if !self.is_rail(state) {
            return Vec::new();
        }
        // Соседи со свободной связью или уже ведущие сюда.
        let free: Vec<(usize, [i32; 3])> = (2..6)
            .filter_map(|d| self.rail_near(pos, d, at).map(|q| (d, q)))
            .filter(|&(_, q)| self.rail_live(q, at).len() < 2 || self.rail_points_to(q, pos, at))
            .collect();
        let dirs: Vec<usize> = free.iter().map(|f| f.0).collect();
        let me = self.rail_shape(state, pos, &dirs, at);
        let mut out = vec![(pos, me)];
        let view = |p: [i32; 3]| if p == pos { me } else { at(p) };
        for (d, q) in free {
            if !self.rail_links(me).iter().any(|l| l.0 == d) || self.rail_points_to(q, pos, &view) {
                continue;
            }
            let mut sides = self.rail_live(q, &view);
            sides.push(opposite(d));
            let qs = self.rail_shape(at(q), q, &sides, &view);
            if qs != at(q) {
                out.push((q, qs));
            }
        }
        out
    }

    /// Плита в плиту того же вида: двойная (`SlabBlock.canBeReplaced`).
    pub fn merge_slab(&self, existing: u32, placing: u32, c: &Ctx, clicked: bool) -> Option<u32> {
        let name = self.name(existing)?;
        if !name.ends_with("_slab") || self.name(placing)? != name {
            return None;
        }
        let ok = match self.prop(existing, "type")? {
            "bottom" => !clicked || c.face == UP,
            "top" => !clicked || c.face == DOWN,
            _ => false,
        };
        ok.then(|| self.with(existing, "type", "double")).flatten()
    }

    /// Вторая половина двухблочного блока (дверь, высокая трава, кровать).
    pub fn partner(&self, state: u32) -> Option<[i32; 3]> {
        match self.prop(state, "half") {
            Some("lower") => return Some([0, 1, 0]),
            Some("upper") => return Some([0, -1, 0]),
            _ => {}
        }
        let f = self
            .prop(state, "facing")
            .and_then(|f| DIRS.iter().position(|d| *d == f))?;
        match self.prop(state, "part")? {
            "foot" => Some(step(f)),
            "head" => Some(step(opposite(f))),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn props(p: &[(&str, &str)]) -> Props {
        p.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn table() -> (Placement, HashMap<String, u32>) {
        let mut states = Vec::new();
        let mut ids = HashMap::new();
        let mut add = |key: &str, name: &str, p: &[(&str, &str)]| {
            ids.insert(key.to_owned(), states.len() as u32);
            states.push(Some((name.to_owned(), props(p))));
        };
        for a in ["x", "y", "z"] {
            add(&format!("log_{a}"), "minecraft:oak_log", &[("axis", a)]);
        }
        for f in ["north", "south", "west", "east"] {
            add(
                &format!("furnace_{f}"),
                "minecraft:furnace",
                &[("facing", f), ("lit", "false")],
            );
            add(
                &format!("wall_torch_{f}"),
                "minecraft:wall_torch",
                &[("facing", f)],
            );
            for h in ["top", "bottom"] {
                add(
                    &format!("stairs_{f}_{h}"),
                    "minecraft:oak_stairs",
                    &[("facing", f), ("half", h)],
                );
            }
            for h in ["upper", "lower"] {
                add(
                    &format!("door_{f}_{h}"),
                    "minecraft:oak_door",
                    &[("facing", f), ("half", h)],
                );
            }
        }
        add("torch", "minecraft:torch", &[]);
        for (n, s, w, e) in [
            ("true", "false", "false", "false"),
            ("true", "false", "false", "true"),
            ("false", "false", "false", "false"),
        ] {
            add(
                &format!("fence_{n}{s}{w}{e}"),
                "minecraft:oak_fence",
                &[("north", n), ("south", s), ("west", w), ("east", e)],
            );
        }
        add("air", "minecraft:air", &[]);
        for t in ["top", "bottom", "double"] {
            add(&format!("slab_{t}"), "minecraft:oak_slab", &[("type", t)]);
        }
        let walls = HashSet::from(["minecraft:cobblestone_wall".to_owned()]);
        let mut sturdy = vec![0u8; states.len()];
        sturdy[ids["log_y"] as usize] = 63;
        (Placement::new(states, sturdy, walls), ids)
    }

    /// Арифметика таблицы совпадает с перебором состояний (и с неполными наборами, как у забора).
    #[test]
    fn with_prop_matches_search() {
        let (pl, _) = table();
        for (s, entry) in pl.states.iter().enumerate() {
            let Some((name, p)) = entry else { continue };
            for (k, _) in p {
                for v in [
                    "true", "false", "north", "east", "top", "double", "x", "upper", "nope",
                ] {
                    let mut want: Props = p
                        .iter()
                        .map(|(a, b)| (a.clone(), if a == k { v.to_owned() } else { b.clone() }))
                        .collect();
                    want.sort();
                    let expect = pl.states.iter().position(|e| {
                        e.as_ref().is_some_and(|(n, q)| {
                            let mut q = q.clone();
                            q.sort();
                            n == name && q == want
                        })
                    });
                    assert_eq!(
                        pl.with_prop(s as u32, k, v),
                        expect.map(|i| i as u32),
                        "{name} {k}={v}"
                    );
                }
            }
        }
    }

    #[test]
    fn orientation() {
        let (p, id) = table();
        // Смотрит на север (yaw 180), кликает по верхней грани.
        let c = Ctx {
            face: UP,
            yaw: 180.0,
            pitch: 0.0,
            cursor_y: 1.0,
            cursor_x: 0.5,
            cursor_z: 0.5,
            near: [[u32::MAX; 2]; 4],
        };
        assert_eq!(
            p.place(id["furnace_north"], &c),
            vec![([0; 3], id["furnace_south"])]
        );
        assert_eq!(
            p.place(id["stairs_east_top"], &c),
            vec![([0; 3], id["stairs_north_bottom"])]
        );
        assert_eq!(
            p.place(id["log_y"], &Ctx { face: 4, ..c }),
            vec![([0; 3], id["log_x"])]
        );
        // Боковая грань, верх блока — перевёрнутые ступени.
        assert_eq!(
            p.place(
                id["stairs_east_top"],
                &Ctx {
                    face: 3,
                    cursor_y: 0.8,
                    ..c
                }
            )[0]
            .1,
            id["stairs_north_top"]
        );
        assert_eq!(
            p.place(id["torch"], &Ctx { face: 5, ..c }),
            vec![([0; 3], id["wall_torch_east"])]
        );
        assert_eq!(p.place(id["torch"], &c), vec![([0; 3], id["torch"])]);
        assert_eq!(
            p.place(id["door_west_upper"], &c),
            vec![
                ([0; 3], id["door_north_lower"]),
                ([0, 1, 0], id["door_north_upper"])
            ]
        );
        assert_eq!(p.partner(id["door_north_upper"]), Some([0, -1, 0]));
        assert_eq!(
            p.place(id["slab_bottom"], &Ctx { face: DOWN, ..c })[0].1,
            id["slab_top"]
        );
        assert_eq!(
            p.merge_slab(id["slab_bottom"], id["slab_bottom"], &c, true),
            Some(id["slab_double"])
        );
        assert_eq!(
            p.merge_slab(
                id["slab_bottom"],
                id["slab_bottom"],
                &Ctx { face: 3, ..c },
                true
            ),
            None
        );
    }

    #[test]
    fn rails_curve_and_join() {
        let mut states = Vec::new();
        let shapes = [
            "north_south",
            "east_west",
            "ascending_east",
            "ascending_west",
            "ascending_north",
            "ascending_south",
            "south_east",
            "south_west",
            "north_west",
            "north_east",
        ];
        for sh in shapes {
            states.push(Some(("minecraft:rail".to_owned(), props(&[("shape", sh)]))));
        }
        states.push(Some(("minecraft:air".to_owned(), Vec::new())));
        let p = Placement::new(states, vec![0; 11], HashSet::new());
        let id = |sh: &str| shapes.iter().position(|x| *x == sh).unwrap() as u32;
        let air = 10;
        // Рельс на (0,0,1) смотрит север–юг; новый на (0,0,0) и сосед на востоке (1,0,0).
        let world = |q: [i32; 3]| match q {
            [0, 0, 0] => id("north_south"),
            [0, 0, 1] => id("north_south"),
            [1, 0, 0] => id("east_west"),
            _ => air,
        };
        let out = p.rails([0, 0, 0], &world);
        assert_eq!(out[0], ([0, 0, 0], id("south_east")));
        // Рельс выше на севере — подъём.
        let world = |q: [i32; 3]| match q {
            [0, 0, 0] => id("north_south"),
            [0, 1, -1] => id("north_south"),
            _ => air,
        };
        assert_eq!(
            p.rails([0, 0, 0], &world)[0],
            ([0, 0, 0], id("ascending_north"))
        );
    }

    #[test]
    fn fences_connect() {
        let (p, id) = table();
        let f = id["fence_falsefalsefalsefalse"];
        let air = id["air"];
        // Забор на севере и бревно на востоке.
        assert_eq!(
            p.connect(f, [f, air, air, id["log_y"]], air),
            id["fence_truefalsefalsetrue"]
        );
        // Факел не цепляется.
        assert_eq!(
            p.connect(f, [f, air, air, id["torch"]], air),
            id["fence_truefalsefalsefalse"]
        );
    }
}
