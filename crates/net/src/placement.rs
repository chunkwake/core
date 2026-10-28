//! Состояние ставимого блока (`Block.getStateForPlacement` 26.3): ориентация
//! по взгляду игрока и грани, половины ступеней и плит, настенные варианты,
//! двухблочные двери, растения и кровати.

use std::collections::HashMap;

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

pub struct Placement {
    states: Vec<Option<(String, Props)>>,
    by_key: HashMap<(String, Props), u32>,
}

/// Блоки с `facing` по взгляду игрока, а не к нему.
fn faces_away(name: &str) -> bool {
    name.ends_with("_stairs")
        || name.ends_with("_door")
        || name.ends_with("_fence_gate")
        || name.ends_with("_bed")
        || name == "minecraft:observer"
        || name == "minecraft:repeater"
        || name == "minecraft:comparator"
}

/// Шестисторонний `facing` по грани клика (стержни, кристаллы).
fn faces_clicked(name: &str) -> bool {
    name.ends_with("_rod") || name.ends_with("amethyst_cluster") || name.ends_with("amethyst_bud") || name.ends_with("shulker_box")
}

impl Placement {
    pub fn new(states: Vec<Option<(String, Props)>>) -> Self {
        let mut by_key = HashMap::new();
        for (id, s) in states.iter().enumerate() {
            if let Some((n, p)) = s {
                let mut p = p.clone();
                p.sort();
                by_key.insert((n.clone(), p), id as u32);
            }
        }
        Self { states, by_key }
    }

    pub fn name(&self, state: u32) -> Option<&str> {
        self.states.get(state as usize)?.as_ref().map(|s| s.0.as_str())
    }

    pub fn prop(&self, state: u32, key: &str) -> Option<&str> {
        let (_, p) = self.states.get(state as usize)?.as_ref()?;
        p.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    fn with(&self, state: u32, key: &str, value: &str) -> Option<u32> {
        let (n, p) = self.states.get(state as usize)?.as_ref()?;
        p.iter().find(|(k, _)| k == key)?;
        let mut p: Props = p
            .iter()
            .map(|(k, v)| (k.clone(), if k == key { value.to_owned() } else { v.clone() }))
            .collect();
        p.sort();
        self.by_key.get(&(n.clone(), p)).copied()
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
        self.by_key
            .iter()
            .filter(|((n, _), _)| *n == wall)
            .map(|(_, &id)| id)
            .min()
    }

    /// Состояния для установки: `(смещение от цели, состояние)`, первое — сам блок.
    pub fn place(&self, default: u32, c: &Ctx) -> Vec<([i32; 3], u32)> {
        let Some(name) = self.name(default).map(str::to_owned) else {
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
            let dir = if faces_clicked(&name) {
                c.face
            } else if name.ends_with("_trapdoor") {
                if c.face > UP { c.face } else { opposite(player) }
            } else if name == "minecraft:hopper" {
                if c.face > UP { opposite(c.face) } else { DOWN }
            } else if name.ends_with("anvil") {
                clockwise(player)
            } else if six {
                if faces_away(&name) { c.looking() } else { opposite(c.looking()) }
            } else if faces_away(&name) {
                player
            } else {
                opposite(player)
            };
            self.set(&mut s, "facing", DIRS[dir]);
        }
        if self.prop(s, "hanging").is_some() {
            self.set(&mut s, "hanging", if c.face == DOWN { "true" } else { "false" });
        }
        let top = c.top_half();
        if self.prop(s, "type").is_some() && name.ends_with("_slab") {
            self.set(&mut s, "type", if top { "top" } else { "bottom" });
        }
        match self.prop(s, "half") {
            Some("top" | "bottom") => self.set(&mut s, "half", if top { "top" } else { "bottom" }),
            Some("upper" | "lower") => {
                self.set(&mut s, "half", "lower");
                let upper = self.with(s, "half", "upper").unwrap_or(s);
                return vec![([0; 3], s), ([0, 1, 0], upper)];
            }
            _ => {}
        }
        if self.prop(s, "part") == Some("foot")
            && let Some(f) = self.prop(s, "facing").and_then(|f| DIRS.iter().position(|d| *d == f))
            && let Some(head) = self.with(s, "part", "head")
        {
            return vec![([0; 3], s), (step(f), head)];
        }
        vec![([0; 3], s)]
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
        let f = self.prop(state, "facing").and_then(|f| DIRS.iter().position(|d| *d == f))?;
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
        p.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
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
            add(&format!("furnace_{f}"), "minecraft:furnace", &[("facing", f), ("lit", "false")]);
            add(&format!("wall_torch_{f}"), "minecraft:wall_torch", &[("facing", f)]);
            for h in ["top", "bottom"] {
                add(&format!("stairs_{f}_{h}"), "minecraft:oak_stairs", &[("facing", f), ("half", h)]);
            }
            for h in ["upper", "lower"] {
                add(&format!("door_{f}_{h}"), "minecraft:oak_door", &[("facing", f), ("half", h)]);
            }
        }
        add("torch", "minecraft:torch", &[]);
        for t in ["top", "bottom", "double"] {
            add(&format!("slab_{t}"), "minecraft:oak_slab", &[("type", t)]);
        }
        (Placement::new(states), ids)
    }

    #[test]
    fn orientation() {
        let (p, id) = table();
        // Смотрит на север (yaw 180), кликает по верхней грани.
        let c = Ctx { face: UP, yaw: 180.0, pitch: 0.0, cursor_y: 1.0 };
        assert_eq!(p.place(id["furnace_north"], &c), vec![([0; 3], id["furnace_south"])]);
        assert_eq!(p.place(id["stairs_east_top"], &c), vec![([0; 3], id["stairs_north_bottom"])]);
        assert_eq!(p.place(id["log_y"], &Ctx { face: 4, ..c }), vec![([0; 3], id["log_x"])]);
        // Боковая грань, верх блока — перевёрнутые ступени.
        assert_eq!(p.place(id["stairs_east_top"], &Ctx { face: 3, cursor_y: 0.8, ..c })[0].1, id["stairs_north_top"]);
        assert_eq!(p.place(id["torch"], &Ctx { face: 5, ..c }), vec![([0; 3], id["wall_torch_east"])]);
        assert_eq!(p.place(id["torch"], &c), vec![([0; 3], id["torch"])]);
        assert_eq!(
            p.place(id["door_west_upper"], &c),
            vec![([0; 3], id["door_north_lower"]), ([0, 1, 0], id["door_north_upper"])]
        );
        assert_eq!(p.partner(id["door_north_upper"]), Some([0, -1, 0]));
        assert_eq!(p.place(id["slab_bottom"], &Ctx { face: DOWN, ..c })[0].1, id["slab_top"]);
        assert_eq!(p.merge_slab(id["slab_bottom"], id["slab_bottom"], &c, true), Some(id["slab_double"]));
        assert_eq!(p.merge_slab(id["slab_bottom"], id["slab_bottom"], &Ctx { face: 3, ..c }, true), None);
    }
}
