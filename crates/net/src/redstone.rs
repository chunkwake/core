//! Редстоун 26.3: сила сигнала (слабый и сильный, проводники), провод с формой
//! и затуханием, факелы, повторители, лампы, плиты, рычаги и кнопки.

pub const NEIGHBORS: [[i32; 3]; 6] = [
    [0, -1, 0],
    [0, 1, 0],
    [0, 0, -1],
    [0, 0, 1],
    [-1, 0, 0],
    [1, 0, 0],
];

pub fn add(p: [i32; 3], d: [i32; 3]) -> [i32; 3] {
    [p[0] + d[0], p[1] + d[1], p[2] + d[2]]
}

pub fn is_lever(name: &str) -> bool {
    name == "minecraft:lever"
}

pub fn is_button(name: &str) -> bool {
    name.ends_with("_button")
}

/// Сколько тиков кнопка нажата (`BlockSetType.ticksToStayPressed`): каменные 20, прочие 30.
pub fn button_ticks(name: &str) -> u64 {
    if name.contains("stone") { 20 } else { 30 }
}

/// Блок, к которому прикреплён рычаг или кнопка (`FaceAttachedHorizontalDirectionalBlock`):
/// пол — снизу, потолок — сверху, стена — позади `facing`.
pub fn attached(pos: [i32; 3], face: &str, facing: &str) -> [i32; 3] {
    let d = match (face, facing) {
        ("floor", _) => [0, -1, 0],
        ("ceiling", _) => [0, 1, 0],
        (_, "north") => [0, 0, 1],
        (_, "south") => [0, 0, -1],
        (_, "west") => [1, 0, 0],
        _ => [-1, 0, 0],
    };
    add(pos, d)
}

/// Принимает ли блок сигнал как дверь (открывается при питании).
pub fn openable(name: &str) -> bool {
    name.ends_with("_door") || name.ends_with("_trapdoor") || name.ends_with("_fence_gate")
}

/// Звук рычага и кнопки (`LeverBlock.playSound`, `ButtonBlock.getSound`):
/// (звук, громкость, высота).
pub fn click_sound(name: &str, on: bool) -> (String, f32, f32) {
    if is_lever(name) {
        return (
            "minecraft:block.lever.click".into(),
            0.3,
            if on { 0.6 } else { 0.5 },
        );
    }
    let path = name.strip_prefix("minecraft:").unwrap_or(name);
    let set = if path.contains("stone") {
        "stone"
    } else if path.starts_with("cherry") {
        "cherry_wood"
    } else if path.starts_with("bamboo") {
        "bamboo_wood"
    } else if path.starts_with("crimson") || path.starts_with("warped") {
        "nether_wood"
    } else {
        "wooden"
    };
    let action = if on { "click_on" } else { "click_off" };
    (format!("minecraft:block.{set}_button.{action}"), 1.0, 1.0)
}

/// Направления как в `placement::step`: низ, верх, север, юг, запад, восток.
pub const DIRS: [&str; 6] = ["down", "up", "north", "south", "west", "east"];
const DOWN: usize = 0;
const UP: usize = 1;

pub fn opposite(d: usize) -> usize {
    d ^ 1
}

fn dir_index(name: &str) -> Option<usize> {
    DIRS.iter().position(|d| *d == name)
}

pub fn step(p: [i32; 3], d: usize) -> [i32; 3] {
    add(p, crate::placement::step(d))
}

pub fn is_wire(name: &str) -> bool {
    name == "minecraft:redstone_wire"
}

pub fn is_torch(name: &str) -> bool {
    name == "minecraft:redstone_torch" || name == "minecraft:redstone_wall_torch"
}

pub fn is_repeater(name: &str) -> bool {
    name == "minecraft:repeater"
}

pub fn is_lamp(name: &str) -> bool {
    name == "minecraft:redstone_lamp"
}

pub fn is_plate(name: &str) -> bool {
    name.ends_with("_pressure_plate")
}

/// Утяжелённая плита: золотая (лёгкая) или железная (тяжёлая).
pub fn weighted(name: &str) -> Option<bool> {
    match name {
        "minecraft:light_weighted_pressure_plate" => Some(false),
        "minecraft:heavy_weighted_pressure_plate" => Some(true),
        _ => None,
    }
}

/// Сила плиты от числа игроков на ней (`getSignalStrength`).
pub fn plate_power(name: &str, count: usize) -> i32 {
    match weighted(name) {
        Some(false) => count.min(15) as i32,
        Some(true) => (count.min(150) as i32 + 9) / 10,
        None => {
            if count > 0 {
                15
            } else {
                0
            }
        }
    }
}

/// Каменные плиты (`PressurePlateBlock.Sensitivity.MOBS`): только живые.
pub fn living_only(name: &str) -> bool {
    weighted(name).is_none() && name.contains("stone")
}

/// Тики до проверки нажатой плиты (`getPressedTime`): 20, утяжелённые — 10.
pub fn plate_ticks(name: &str) -> u64 {
    if weighted(name).is_some() { 10 } else { 20 }
}

/// Набор звуков блока (`BlockSetType`) по имени.
fn sound_set(name: &str) -> &'static str {
    let path = name.strip_prefix("minecraft:").unwrap_or(name);
    if weighted(name).is_some() {
        "metal"
    } else if path.contains("stone") {
        "stone"
    } else if path.starts_with("cherry") {
        "cherry_wood"
    } else if path.starts_with("bamboo") {
        "bamboo_wood"
    } else if path.starts_with("crimson") || path.starts_with("warped") {
        "nether_wood"
    } else {
        "wooden"
    }
}

/// Звук плиты (`BlockSetType.pressurePlateClickOn/Off`).
pub fn plate_sound(name: &str, on: bool) -> String {
    format!(
        "minecraft:block.{}_pressure_plate.click_{}",
        sound_set(name),
        if on { "on" } else { "off" }
    )
}

/// Источник сигнала (`isSignalSource`): к нему тянется провод.
fn is_source(name: &str) -> bool {
    is_lever(name)
        || is_button(name)
        || is_torch(name)
        || is_plate(name)
        || matches!(
            name,
            "minecraft:redstone_block"
                | "minecraft:repeater"
                | "minecraft:comparator"
                | "minecraft:observer"
                | "minecraft:daylight_detector"
                | "minecraft:target"
                | "minecraft:trapped_chest"
                | "minecraft:lectern"
                | "minecraft:tripwire_hook"
                | "minecraft:detector_rail"
                | "minecraft:lightning_rod"
                | "minecraft:sculk_sensor"
                | "minecraft:calibrated_sculk_sensor"
                | "minecraft:redstone_wire"
        )
}

/// Полные блоки, не проводящие сигнал (`isRedstoneConductor(never)`).
fn insulator(name: &str) -> bool {
    name.contains("glass")
        || name.ends_with("_leaves")
        || matches!(
            name,
            "minecraft:redstone_block"
                | "minecraft:piston"
                | "minecraft:sticky_piston"
                | "minecraft:observer"
                | "minecraft:glowstone"
                | "minecraft:soul_sand"
                | "minecraft:beacon"
        )
}

/// Направление от опоры к рычагу или кнопке (`getConnectedDirection`).
fn connected(face: &str, facing: &str) -> usize {
    match face {
        "floor" => UP,
        "ceiling" => DOWN,
        _ => dir_index(facing).unwrap_or(2),
    }
}

/// Мир для расчёта сигнала: таблица состояний и блок по позиции.
pub struct World<'a> {
    pub pl: &'a crate::placement::Placement,
    pub at: &'a dyn Fn([i32; 3]) -> u32,
    /// Сохранённый выход компаратора или число зрителей сундука-ловушки.
    pub output: &'a dyn Fn([i32; 3]) -> i32,
    /// Сигнал содержимого для компаратора (`getAnalogOutputSignal`): сундуки, печи.
    pub analog: &'a dyn Fn([i32; 3]) -> Option<i32>,
}

impl World<'_> {
    fn name(&self, s: u32) -> &str {
        self.pl.name(s).unwrap_or("")
    }

    fn prop(&self, s: u32, k: &str) -> Option<&str> {
        self.pl.prop(s, k)
    }

    fn on(&self, s: u32, k: &str) -> bool {
        self.prop(s, k) == Some("true")
    }

    fn int(&self, s: u32, k: &str) -> i32 {
        self.prop(s, k).and_then(|v| v.parse().ok()).unwrap_or(0)
    }

    fn facing(&self, s: u32) -> usize {
        self.prop(s, "facing").and_then(dir_index).unwrap_or(2)
    }

    /// Проводник (`isRedstoneConductor`): полный блок, кроме стекла и подобных.
    pub fn conductor(&self, s: u32) -> bool {
        (0..6).all(|f| self.pl.sturdy(s, f)) && !insulator(self.name(s))
    }

    /// Слабый сигнал блока в `n` для соседа, от которого к нему направление `dir`
    /// (`getSignal`). `wires: false` — провод молчит (`shouldSignal`).
    pub fn weak(&self, n: [i32; 3], dir: usize, wires: bool) -> i32 {
        let s = (self.at)(n);
        let name = self.name(s);
        if is_lever(name) || is_button(name) {
            return if self.on(s, "powered") { 15 } else { 0 };
        }
        match name {
            "minecraft:redstone_block" => 15,
            "minecraft:redstone_torch" => i32::from(self.on(s, "lit") && dir != UP) * 15,
            "minecraft:redstone_wall_torch" => {
                i32::from(self.on(s, "lit") && self.facing(s) != dir) * 15
            }
            "minecraft:repeater" | "minecraft:observer" => {
                i32::from(self.on(s, "powered") && self.facing(s) == dir) * 15
            }
            "minecraft:daylight_detector" | "minecraft:target" => self.int(s, "power"),
            "minecraft:trapped_chest" => (self.output)(n),
            "minecraft:comparator" => {
                if self.facing(s) == dir {
                    (self.output)(n)
                } else {
                    0
                }
            }
            "minecraft:redstone_wire" => {
                if !wires || dir == DOWN {
                    return 0;
                }
                let power = self.int(s, "power");
                if dir == UP {
                    return power;
                }
                let toward = DIRS[opposite(dir)];
                if self.prop(s, toward).is_some_and(|v| v != "none") {
                    power
                } else {
                    0
                }
            }
            _ if is_plate(name) => {
                if weighted(name).is_some() {
                    self.int(s, "power")
                } else if self.on(s, "powered") {
                    15
                } else {
                    0
                }
            }
            // Крюк растяжки — источник (`isSignalSource`), слабый выход 15 во
            // все стороны: `ownSignal` не смотрит на направление.
            _ if is_tripwire_hook(name) => i32::from(self.on(s, "powered")) * 15,
            // Скалк-сенсор: слабый выход — свой `power`, у калиброванного в
            // сторону `facing` его нет (`CalibratedSculkSensorBlock.getSignal`).
            _ if is_sculk_sensor(name) => {
                if name == "minecraft:calibrated_sculk_sensor" && self.facing(s) == dir {
                    0
                } else {
                    self.int(s, "power")
                }
            }
            _ => 0,
        }
    }

    /// Сильный сигнал (`getDirectSignal`): питает проводник насквозь.
    pub fn strong(&self, n: [i32; 3], dir: usize, wires: bool) -> i32 {
        let s = (self.at)(n);
        let name = self.name(s);
        if is_lever(name) || is_button(name) {
            let face = self.prop(s, "face").unwrap_or("wall");
            let facing = self.prop(s, "facing").unwrap_or("north");
            return i32::from(self.on(s, "powered") && connected(face, facing) == dir) * 15;
        }
        if is_torch(name) {
            return if dir == DOWN {
                self.weak(n, dir, wires)
            } else {
                0
            };
        }
        if is_plate(name) {
            return if dir == UP {
                self.weak(n, dir, wires)
            } else {
                0
            };
        }
        // Сундук-ловушка сильно питает только блок под собой.
        if name == "minecraft:trapped_chest" {
            return if dir == UP {
                self.weak(n, dir, wires)
            } else {
                0
            };
        }
        // `getDirectSignal` крюка: 15 только в сторону `facing`, то есть в
        // блок, на котором он висит, — и оттуда питает проводник насквозь.
        if is_tripwire_hook(name) {
            return i32::from(self.on(s, "powered") && self.facing(s) == dir) * 15;
        }
        // `SculkSensorBlock.getDirectSignal`: сильно питает только верх.
        if is_sculk_sensor(name) {
            return if sculk_direct(dir) {
                self.weak(n, dir, wires)
            } else {
                0
            };
        }
        match name {
            "minecraft:repeater"
            | "minecraft:comparator"
            | "minecraft:observer"
            | "minecraft:redstone_wire"
            | "minecraft:target" => self.weak(n, dir, wires),
            _ => 0,
        }
    }

    /// Сигнал из `n` в сторону `dir` (`Level.getSignal`): проводник отдаёт
    /// сильнейший сильный сигнал, что в него входит.
    pub fn signal(&self, n: [i32; 3], dir: usize, wires: bool) -> i32 {
        let w = self.weak(n, dir, wires);
        if w >= 15 || !self.conductor((self.at)(n)) {
            return w;
        }
        (0..6)
            .map(|d| self.strong(step(n, d), d, wires))
            .fold(w, i32::max)
    }

    /// Сильнейший сигнал соседей (`getBestNeighborSignal`).
    pub fn best_neighbor(&self, p: [i32; 3], wires: bool) -> i32 {
        let mut best = 0;
        for d in 0..6 {
            best = best.max(self.signal(step(p, d), d, wires));
            if best >= 15 {
                break;
            }
        }
        best
    }

    /// `hasNeighborSignal`.
    pub fn powered(&self, p: [i32; 3]) -> bool {
        self.best_neighbor(p, true) > 0
    }

    /// Гасит ли сигнал опоры факел в `p` (`RedstoneTorchBlock.hasNeighborSignal`).
    pub fn torch_off(&self, p: [i32; 3]) -> bool {
        let s = (self.at)(p);
        let d = if self.name(s) == "minecraft:redstone_wall_torch" {
            opposite(self.facing(s))
        } else {
            DOWN
        };
        self.signal(step(p, d), d, true) > 0
    }

    /// Вход повторителя сзади (`DiodeBlock.getInputSignal`).
    pub fn repeater_input(&self, p: [i32; 3]) -> bool {
        let f = self.facing((self.at)(p));
        self.signal(step(p, f), f, true) > 0
    }

    /// Выход повторителя: блок перед ним.
    pub fn repeater_front(&self, p: [i32; 3]) -> [i32; 3] {
        step(p, opposite(self.facing((self.at)(p))))
    }

    /// Заблокирован ли повторитель включённым повторителем сбоку (`isLocked`).
    pub fn repeater_locked(&self, p: [i32; 3]) -> bool {
        let f = self.facing((self.at)(p));
        (2..6).filter(|&d| d != f && d != opposite(f)).any(|d| {
            let n = step(p, d);
            matches!(
                self.name((self.at)(n)),
                "minecraft:repeater" | "minecraft:comparator"
            ) && self.weak(n, d, true) > 0
        })
    }

    /// Новый выход компаратора в `p` (`ComparatorBlock.calculateOutputSignal`):
    /// сзади сигнал или заполненность контейнера (в т. ч. через проводник),
    /// сбоку — вычитаемое; сравнение гасит, если сбоку больше.
    pub fn comparator_target(&self, p: [i32; 3]) -> i32 {
        let s = (self.at)(p);
        let f = self.facing(s);
        let back = step(p, f);
        let bs = (self.at)(back);
        let mut input = self.signal(back, f, true);
        if is_wire(self.name(bs)) {
            input = input.max(self.int(bs, "power"));
        }
        if let Some(a) = (self.analog)(back) {
            input = a;
        } else if input < 15
            && self.conductor(bs)
            && let Some(a) = (self.analog)(step(back, f))
        {
            input = a;
        }
        if input == 0 {
            return 0;
        }
        let side = (2..6)
            .filter(|&d| d != f && d != opposite(f))
            .map(|d| {
                let n = step(p, d);
                let ns = (self.at)(n);
                match self.name(ns) {
                    "minecraft:redstone_block" => 15,
                    "minecraft:redstone_wire" => self.int(ns, "power"),
                    // Сбоку — слабый сигнал (`getSignal`): факел, повторитель, проводник.
                    _ => self.signal(n, d, true),
                }
            })
            .max()
            .unwrap_or(0);
        if self.prop(s, "mode") == Some("subtract") {
            (input - side).max(0)
        } else if side > input {
            0
        } else {
            input
        }
    }

    /// Выход наблюдателя: блок позади него.
    pub fn observer_back(&self, p: [i32; 3]) -> [i32; 3] {
        step(p, opposite(self.facing((self.at)(p))))
    }

    /// Держится ли компонент на опоре; `None` — не компонент редстоуна.
    pub fn supported(&self, p: [i32; 3]) -> Option<bool> {
        let s = (self.at)(p);
        let name = self.name(s);
        let below = (self.at)(step(p, DOWN));
        let top = || self.pl.sturdy(below, UP) || matches!(self.name(below), "minecraft:hopper");
        let center = || {
            let b = self.name(below);
            top()
                || b.ends_with("_fence")
                || b.ends_with("_wall")
                || b.ends_with("_pane")
                || b == "minecraft:iron_bars"
        };
        Some(
            if is_wire(name) || is_repeater(name) || name == "minecraft:comparator" {
                top()
            } else if name == "minecraft:redstone_torch" || is_plate(name) {
                center()
            } else if name == "minecraft:redstone_wall_torch" {
                let f = self.facing(s);
                self.pl.sturdy((self.at)(step(p, opposite(f))), f)
            } else if is_lever(name) || is_button(name) {
                let d = connected(
                    self.prop(s, "face").unwrap_or("wall"),
                    self.prop(s, "facing").unwrap_or("north"),
                );
                self.pl.sturdy((self.at)(step(p, opposite(d))), d)
            } else {
                return None;
            },
        )
    }

    /// `shouldConnectTo(state, dir)`; `dir: None` — сверху или снизу (только провод).
    fn connects_to(&self, s: u32, dir: Option<usize>) -> bool {
        let name = self.name(s);
        if is_wire(name) {
            return true;
        }
        let Some(d) = dir else { return false };
        match name {
            "minecraft:repeater" => {
                let f = self.facing(s);
                f == d || f == opposite(d)
            }
            "minecraft:observer" => self.facing(s) == d,
            _ => is_source(name),
        }
    }

    /// Сторона провода в `dir`: 0 нет, 1 вбок, 2 вверх (`getConnectingSide`).
    fn side(&self, p: [i32; 3], dir: usize, can_up: bool) -> u8 {
        let rel = step(p, dir);
        let rs = (self.at)(rel);
        if can_up {
            let placeable = self.name(rs).ends_with("_trapdoor")
                || self.pl.sturdy(rs, UP)
                || self.name(rs) == "minecraft:hopper";
            if placeable && self.connects_to((self.at)(step(rel, UP)), None) {
                return if self.pl.sturdy(rs, opposite(dir)) {
                    2
                } else {
                    1
                };
            }
        }
        let connect = self.connects_to(rs, Some(dir))
            || (!self.conductor(rs) && self.connects_to((self.at)(step(rel, DOWN)), None));
        u8::from(connect)
    }

    /// Форма провода в `p` (`getConnectionState`): север, юг, запад, восток.
    pub fn wire_shape(&self, p: [i32; 3]) -> [&'static str; 4] {
        const NAMES: [&str; 3] = ["none", "side", "up"];
        let s = (self.at)(p);
        let dot = (2..6).all(|d| self.prop(s, DIRS[d]) == Some("none"));
        let can_up = !self.conductor((self.at)(step(p, UP)));
        let mut sides = [0u8; 4];
        for (i, side) in sides.iter_mut().enumerate() {
            *side = self.side(p, i + 2, can_up);
        }
        let [n, so, w, e] = sides.map(|v| v > 0);
        if !(dot && !n && !so && !w && !e) {
            let (no_ns, no_ew) = (!n && !so, !e && !w);
            if !w && no_ns {
                sides[2] = 1;
            }
            if !e && no_ns {
                sides[3] = 1;
            }
            if !n && no_ew {
                sides[0] = 1;
            }
            if !so && no_ew {
                sides[1] = 1;
            }
        }
        sides.map(|v| NAMES[v as usize])
    }

    /// Соседние провода, куда течёт сигнал: только по сторонам, которые не `none`.
    fn wire_links(&self, p: [i32; 3]) -> Vec<[i32; 3]> {
        let mut out = Vec::new();
        let here = (self.at)(p);
        let above = self.conductor((self.at)(step(p, UP)));
        for (d, side) in DIRS.iter().enumerate().skip(2) {
            if self.prop(here, side).is_none_or(|v| v == "none") {
                continue;
            }
            let rel = step(p, d);
            let rs = (self.at)(rel);
            if is_wire(self.name(rs)) {
                out.push(rel);
            } else if self.conductor(rs) {
                let up = step(rel, UP);
                if !above && is_wire(self.name((self.at)(up))) {
                    out.push(up);
                }
            } else {
                let down = step(rel, DOWN);
                if is_wire(self.name((self.at)(down))) {
                    out.push(down);
                }
            }
        }
        out
    }

    /// Новая сила проводов сети, задетой `seeds` (`calculateTargetStrength` до
    /// установившегося состояния): провод берёт сигнал блоков, соседний провод — минус один.
    pub fn solve(&self, seeds: &[[i32; 3]]) -> Vec<([i32; 3], i32)> {
        const LIMIT: usize = 20_000;
        let mut index: std::collections::HashMap<[i32; 3], usize> =
            std::collections::HashMap::new();
        let mut nodes: Vec<[i32; 3]> = Vec::new();
        for &p in seeds {
            if is_wire(self.name((self.at)(p))) && !index.contains_key(&p) {
                index.insert(p, nodes.len());
                nodes.push(p);
            }
        }
        let mut links: Vec<Vec<usize>> = Vec::new();
        let mut i = 0;
        while i < nodes.len() {
            let mut l = Vec::new();
            for q in self.wire_links(nodes[i]) {
                let k = *index.entry(q).or_insert_with(|| {
                    nodes.push(q);
                    nodes.len() - 1
                });
                l.push(k);
            }
            links.push(l);
            i += 1;
            if nodes.len() > LIMIT {
                break;
            }
        }
        let n = links.len();
        let mut power: Vec<i32> = (0..n)
            .map(|k| self.best_neighbor(nodes[k], false))
            .collect();
        // Корзины по силе: от 15 вниз, каждый провод — максимум из источника и соседей − 1.
        let mut buckets: Vec<Vec<usize>> = vec![Vec::new(); 16];
        for (k, &v) in power.iter().enumerate() {
            buckets[v.clamp(0, 15) as usize].push(k);
        }
        for level in (1..16).rev() {
            while let Some(k) = buckets[level].pop() {
                if power[k] != level as i32 {
                    continue;
                }
                for &m in &links[k] {
                    if m < n && power[m] < level as i32 - 1 {
                        power[m] = level as i32 - 1;
                        buckets[level - 1].push(m);
                    }
                }
            }
        }
        (0..n).map(|k| (nodes[k], power[k])).collect()
    }
}

/// Сигнал заполненности контейнера (`getRedstoneSignalFromContainer`):
/// `items` — (число, максимум стака) по занятым слотам.
pub fn fullness(items: impl IntoIterator<Item = (i32, i32)>, size: usize) -> i32 {
    if size == 0 {
        return 0;
    }
    let p: f32 = items
        .into_iter()
        .map(|(c, m)| c as f32 / m.max(1) as f32)
        .sum::<f32>()
        / size as f32;
    (p * 14.0).floor() as i32 + i32::from(p > 0.0)
}

/// Аналоговый выход блоков без инвентаря (`getAnalogOutputSignal`).
pub(crate) fn block_analog(pl: &crate::placement::Placement, state: u32) -> Option<i32> {
    let name = pl.name(state)?;
    match name {
        "minecraft:cake" => {
            let bites = pl.prop(state, "bites")?.parse::<i32>().ok()?;
            Some(14 - 2 * bites)
        }
        "minecraft:cauldron" => Some(0),
        "minecraft:water_cauldron" | "minecraft:powder_snow_cauldron" => {
            pl.prop(state, "level")?.parse().ok()
        }
        "minecraft:lava_cauldron" => Some(3),
        "minecraft:composter" => pl.prop(state, "level")?.parse().ok(),
        "minecraft:beehive" | "minecraft:bee_nest" => pl.prop(state, "honey_level")?.parse().ok(),
        "minecraft:respawn_anchor" => {
            let charges = pl.prop(state, "charges")?.parse::<i32>().ok()?;
            Some(charges * 15 / 4)
        }
        _ => None,
    }
}

pub(crate) fn trapped_chest_power(viewers: usize) -> i32 {
    viewers.min(15) as i32
}

pub fn is_comparator(name: &str) -> bool {
    name == "minecraft:comparator"
}

pub fn is_observer(name: &str) -> bool {
    name == "minecraft:observer"
}

/// Смотрит ли наблюдатель с `facing` на изменённый блок.
/// `d` — направление от изменённого блока к наблюдателю.
pub fn watching(d: usize, facing: &str) -> bool {
    dir_index(facing) == Some(opposite(d))
}

/// Позиции, которые задевает изменение в `p` (`updateNeighborsAt` и соседи соседей).
///
/// Ровно 25 позиций в порядке «ближние первыми». Возвращается массив, а не
/// `Vec`: функция зовётся на каждую правку блока из `scheduled_tick` и
/// `update_plate`, и аллокация там была дороже самих вычислений.
pub fn around(p: [i32; 3]) -> [[i32; 3]; 25] {
    // Все сдвиги с манхэттенским расстоянием ≤ 2, по возрастанию расстояния.
    let mut out = [[0i32; 3]; 25];
    let mut n = 0;
    for dist in 0..=2 {
        for dx in -2i32..=2 {
            for dy in -2i32..=2 {
                for dz in -2i32..=2 {
                    if dx.abs() + dy.abs() + dz.abs() == dist {
                        out[n] = [p[0] + dx, p[1] + dy, p[2] + dz];
                        n += 1;
                    }
                }
            }
        }
    }
    debug_assert_eq!(n, 25, "манхэттенский шар радиуса 2");
    out
}

/// Звук перегоревшего факела (`RedstoneTorchBlock`): громкость 0.5.
pub const BURNOUT: &str = "minecraft:block.redstone_torch.burnout";
/// Перегорание: 8 переключений за 60 тиков, остывает 160.
pub const BURNOUT_COUNT: usize = 8;
pub const BURNOUT_WINDOW: u64 = 60;
pub const BURNOUT_COOLDOWN: u64 = 160;

/// Задержка повторителя в тиках: `delay` × 2.
pub fn repeater_delay(pl: &crate::placement::Placement, state: u32) -> u64 {
    pl.prop(state, "delay")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(1)
        * 2
}

/// Позиция на `n` шагов в сторону `dir`.
pub fn place(p: [i32; 3], dir: usize, n: usize) -> [i32; 3] {
    let d = crate::placement::step(dir);
    [
        p[0] + d[0] * n as i32,
        p[1] + d[1] * n as i32,
        p[2] + d[2] * n as i32,
    ]
}

/// Название противоположного направления (`Direction.getOpposite`).
pub fn opposite_name(dir: usize) -> &'static str {
    DIRS[opposite(dir)]
}

/// Растяжка — та самая нить между двумя крюками.
pub fn is_tripwire(name: &str) -> bool {
    name == "minecraft:tripwire"
}

pub fn is_tripwire_hook(name: &str) -> bool {
    name == "minecraft:tripwire_hook"
}

/// `TripWireHookBlock.WIRE_DIST_MAX`: дальше 42 блоков крюк второй не ищет.
pub const WIRE_DIST_MAX: i32 = 42;
/// `RECHECK_PERIOD`: пока по нити кто-то идёт, проверка раз в 10 тиков.
pub const RECHECK_PERIOD: u64 = 10;

/// Габариты нити (`TripWireBlock.getShape`): натянутая тоньше и выше,
/// провисшая — от пола до половины блока. Границы — в блоках от угла клетки.
///
/// `Block.column(16.0, 1.0, 2.5)` — это `box(0, 1, 0, 16, 2.5, 16)`, то есть
/// в блоках `y` от `1/16` до `2.5/16`; провисшая — `0…8/16`.
pub fn tripwire_shape(attached: bool) -> ([f64; 2], [f64; 2]) {
    if attached {
        ([0.0, 1.0 / 16.0], [1.0, 2.5 / 16.0])
    } else {
        ([0.0, 0.0], [1.0, 0.5])
    }
}

/// Задевает ли сущность нить в клетке `wire` (`level.getEntities(null, shape.bounds().move(pos))`).
///
/// Сущность задаётся рамкой: `min` и `max` в мировых координатах. Рамка нити
/// берётся по её состоянию, поэтому натянутая ловится только в тонкой полосе
/// у пола — как в ванили.
pub fn touches_wire(attached: bool, wire: [i32; 3], min: [f64; 3], max: [f64; 3]) -> bool {
    let (lo, hi) = tripwire_shape(attached);
    for axis in 0..3 {
        let (from, to) = if axis == 1 {
            (lo[1], hi[1])
        } else {
            (lo[0], hi[0])
        };
        let a = wire[axis] as f64 + from;
        let b = wire[axis] as f64 + to;
        // `AABB.intersects`: касание границей не считается.
        if max[axis] <= a || min[axis] >= b {
            return false;
        }
    }
    true
}

/// `TripWireBlock.updateSource`: в какую сторону искать второй крюк.
///
/// Ванила перебирает только `SOUTH` и `WEST`, поэтому цепь ищется в одну
/// сторону от каждой нити — второй крюк найдётся сам, когда дойдёт очередь.
pub const WIRE_SCAN: [usize; 2] = [3, 4];

/// Какой стороной крюк смотрит в нить (`TripWireHookBlock.FACING`): крюк
/// соединяется с нитью позади себя, то есть `facing` указывает на опору.
pub fn hook_reaches(facing: &str) -> usize {
    // `pos.relative(direction, i)` в `calculateState`: `direction` — это
    // `facing`, и нить лежит именно в ту сторону.
    DIRS.iter().position(|d| *d == facing).unwrap_or(2)
}

/// `shouldConnectTo`: нить соединяется с соседней нитью и с крюком, который
/// смотрит в её сторону (`facing == direction.getOpposite()`).
pub fn tripwire_connects(neighbor: &str, neighbor_facing: Option<&str>, dir: usize) -> bool {
    if is_tripwire_hook(neighbor) {
        return neighbor_facing.is_some_and(|f| DIRS[opposite(dir)] == f);
    }
    is_tripwire(neighbor)
}

// ---------------------------------------------------------------------------
// Скалк: частоты вибраций (`VibrationSystem.VIBRATION_FREQUENCY_FOR_EVENT`).
// ---------------------------------------------------------------------------

/// Частота события по его имени (`minecraft:block_place` → 13).
///
/// Таблица — из `VibrationSystem.VIBRATION_FREQUENCY_FOR_EVENT`; ноль значит
/// «событие скалк не слышит». События резонанса (`minecraft:resonate_1`…`_15`)
/// дают ту же частоту, что и в имени.
pub fn vibration_frequency(event: &str) -> i32 {
    let path = event.strip_prefix("minecraft:").unwrap_or(event);
    if let Some(n) = path.strip_prefix("resonate_").and_then(|n| n.parse().ok())
        && (1..=15).contains(&n)
    {
        return n;
    }
    match path {
        // 1 — шаг и плавание.
        "step" | "swim" | "flap" => 1,
        // 2 — падение и удар о землю.
        "projectile_land" | "hit_ground" | "splash" | "bounce" => 2,
        // 3 — выстрел, взаимодействие с предметом, инструмент.
        "item_interact_finish" | "projectile_shoot" | "instrument_play" => 3,
        // 4 — действие сущности, элитры.
        "entity_action" | "elytra_glide" | "unequip" => 4,
        // 5 — посадка и снятие снаряжения.
        "entity_dismount" | "equip" => 5,
        // 6 — взаимодействие, стрижка, верховая езда.
        "entity_interact" | "shear" | "entity_mount" => 6,
        // 7 — урон.
        "entity_damage" => 7,
        // 8 — еда и питьё.
        "drink" | "eat" => 8,
        // 9 — закрытие.
        "container_close" | "block_close" | "block_deactivate" | "block_detach" => 9,
        // 10 — открытие и активация.
        "container_open" | "block_open" | "block_activate" | "block_attach" | "prime_fuse"
        | "note_block_play" => 10,
        // 11 — смена блока.
        "block_change" => 11,
        // 12 — разрушение.
        "block_destroy" | "fluid_pickup" => 12,
        // 13 — установка.
        "block_place" | "fluid_place" => 13,
        // 14 — появление сущности и молния.
        "entity_place" | "lightning_strike" | "teleport" => 14,
        // 15 — смерть и взрыв.
        "entity_die" | "explode" => 15,
        _ => 0,
    }
}

/// `VibrationSystem.getRedstoneStrengthForDistance`.
///
/// `max(1, 15 - floor(15 / радиус * расстояние))`: вплотную 15, на границе 1.
pub fn vibration_power(distance: f32, radius: i32) -> i32 {
    let scale = 15.0 / radius as f64;
    (15 - (scale * distance as f64).floor() as i32).max(1)
}

/// Радиус слуха скалк-сенсора и крикуна (`VibrationUser.LISTENER_RANGE`).
pub const SCULK_LISTENER_RANGE: i32 = 8;
/// `SculkSensorBlock.ACTIVE_TICKS`: сколько сенсор держит сигнал.
pub const SCULK_ACTIVE_TICKS: u64 = 30;
/// `SculkSensorBlock.COOLDOWN_TICKS`: пауза перед новым срабатыванием.
pub const SCULK_COOLDOWN_TICKS: u64 = 10;

/// Фаза сенсора (`SculkSensorPhase`) по строке свойства.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SculkPhase {
    Inactive,
    Active,
    Cooldown,
}

impl SculkPhase {
    pub fn parse(value: &str) -> Self {
        match value {
            "active" => Self::Active,
            "cooldown" => Self::Cooldown,
            _ => Self::Inactive,
        }
    }

    /// `canActivate`: сработать можно только из `inactive`.
    pub fn can_activate(self) -> bool {
        self == Self::Inactive
    }
}

/// Что делает сенсор, когда истёк его тик.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SculkTick {
    /// Ничего: сенсор уже неактивен.
    Nothing,
    /// Был активен — снять сигнал, перейти в `cooldown`.
    Deactivate,
    /// Был в `cooldown` — вернуться в `inactive`, звук `sculk_clicking_stop`.
    Reset,
}

/// `SculkSensorBlock.tick`: активный гаснет, отдохнувший становится неактивным.
pub fn sculk_tick(phase: SculkPhase) -> SculkTick {
    match phase {
        SculkPhase::Active => SculkTick::Deactivate,
        SculkPhase::Cooldown => SculkTick::Reset,
        SculkPhase::Inactive => SculkTick::Nothing,
    }
}

/// `getDirectSignal` сенсора: сильно питает только верх.
pub fn sculk_direct(dir: usize) -> bool {
    dir == 1
}

/// `CalibratedSculkSensorBlock.getSignal`: сенсор молчит в сторону `facing`.
pub fn calibrated_signal_blocked(facing: usize, dir: usize) -> bool {
    facing == dir
}

/// Фильтр калиброванного сенсора (`canReceiveVibration`): сигнал сзади задаёт
/// частоту, и слышны только вибрации этой частоты. Ноль — фильтра нет.
pub fn calibrated_accepts(back_signal: i32, frequency: i32) -> bool {
    back_signal == 0 || frequency == back_signal
}

/// `tryResonateVibration`: высота звука резонанса по частоте.
///
/// Тона `[0, 0, 2, 4, 6, 7, 9, 10, 12, 14, 15, 18, 19, 21, 22, 24]` — это
/// `NoteBlock.getPitchFromNote`, индекс — частота.
pub fn resonance_pitch(frequency: i32) -> f32 {
    const TONES: [i32; 16] = [0, 0, 2, 4, 6, 7, 9, 10, 12, 14, 15, 18, 19, 21, 22, 24];
    let note = TONES[frequency.clamp(0, 15) as usize];
    note_pitch(note)
}

/// Событие резонанса по частоте: `minecraft:resonate_<N>`.
pub fn resonance_event(frequency: i32) -> String {
    format!("minecraft:resonate_{}", frequency.clamp(1, 15))
}

/// Блоки, которые откликаются на резонанс (`VIBRATION_RESONATORS`).
pub fn is_resonator(name: &str) -> bool {
    matches!(
        name,
        "minecraft:amethyst_block"
            | "minecraft:budding_amethyst"
            | "minecraft:amethyst_cluster"
            | "minecraft:large_amethyst_bud"
            | "minecraft:medium_amethyst_bud"
            | "minecraft:small_amethyst_bud"
    )
}

/// Скалк-сенсор или его калиброванный вариант.
pub fn is_sculk_sensor(name: &str) -> bool {
    name == "minecraft:sculk_sensor" || name == "minecraft:calibrated_sculk_sensor"
}

pub fn is_sculk_shrieker(name: &str) -> bool {
    name == "minecraft:sculk_shrieker"
}

pub fn is_sculk_catalyst(name: &str) -> bool {
    name == "minecraft:sculk_catalyst"
}

/// Нотный блок — тот самый, что звучит по сигналу.
pub fn is_note_block(name: &str) -> bool {
    name == "minecraft:note_block"
}

/// `NoteBlock.NOTE_VOLUME`: громкость звука ноты.
pub const NOTE_VOLUME: f32 = 3.0;

/// `NoteBlock.getPitchFromNote`: `2^((note - 12) / 12)`.
pub fn note_pitch(note: i32) -> f32 {
    2f32.powf((note - 12) as f32 / 12.0)
}

/// Смещение частицы ноты над блоком (`triggerEvent`): `(0.5, 1.2, 0.5)`,
/// скорость `(note / 24, 0, 0)`.
pub fn note_particle(note: i32, pos: [i32; 3]) -> ([f64; 3], [f32; 3]) {
    (
        [
            pos[0] as f64 + 0.5,
            pos[1] as f64 + 1.2,
            pos[2] as f64 + 0.5,
        ],
        [note as f32 / 24.0, 0.0, 0.0],
    )
}

/// Позиция звука ноты (`triggerEvent`): центр блока, а не верх — `(0.5, 0.5, 0.5)`.
pub fn note_sound_pos(pos: [i32; 3]) -> [f64; 3] {
    [
        pos[0] as f64 + 0.5,
        pos[1] as f64 + 0.5,
        pos[2] as f64 + 0.5,
    ]
}

/// `NoteBlock.setInstrument`: инструмент от блока над, иначе от блока под.
///
/// Инструмент сверху берётся, только если он «работает над нотным блоком»
/// (`worksAboveNoteBlock`, то есть голова моба или игрока). Инструмент снизу,
/// который сам работает над блоком, тоже не считается — вместо него `harp`.
/// Именно это делает голову под нотным блоком обычным звуком.
pub fn instrument_for(
    above: Option<chunkwake_data::world::NoteInstrument>,
    below: Option<chunkwake_data::world::NoteInstrument>,
) -> chunkwake_data::world::NoteInstrument {
    use chunkwake_data::world::NoteInstrument;
    if let Some(kind) = above
        && kind.works_above()
    {
        return kind;
    }
    match below {
        Some(kind) if !kind.works_above() => kind,
        _ => NoteInstrument::Harp,
    }
}

/// `neighborChanged` нотного блока: звук только на фронте сигнала.
pub fn note_signal(signal: bool, powered: bool) -> NoteAction {
    if signal == powered {
        NoteAction::Nothing
    } else if signal {
        NoteAction::Play
    } else {
        NoteAction::UpdateOnly
    }
}

/// Что делать нотному блоку при смене сигнала.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteAction {
    /// Сигнал не изменился.
    Nothing,
    /// Включился: сыграть ноту и записать `powered = true`.
    Play,
    /// Выключился: только записать `powered = false`.
    UpdateOnly,
}

/// `playNote`: звук идёт, если инструмент выше нотного блока или над ним воздух.
pub fn note_sounds(works_above: bool, above_is_air: bool) -> bool {
    works_above || above_is_air
}

/// Итог прохода по цепи от крюка (`TripWireHookBlock.calculateState`).
pub struct Chain {
    pub attached: bool,
    pub powered: bool,
    /// Номер клетки второго крюка; 0 — не найден.
    pub receiver: i32,
    /// Встретилась клетка `wire_source`: крюку нужен повторный тик.
    pub reschedule: bool,
    /// Состояния нитей по номеру клетки: нужны, чтобы проставить `attached`.
    wires: Vec<Option<u32>>,
}

impl Chain {
    pub fn wire(&self, i: i32) -> Option<u32> {
        usize::try_from(i)
            .ok()
            .and_then(|i| self.wires.get(i).copied().flatten())
    }
}

/// Проход от крюка в сторону `direction` (это его `facing`).
///
/// `at` отдаёт состояние блока через `i` шагов; `None` — чанк не загружен,
/// что в ванили тоже обрывает цепь.
///
/// Формулы — из `TripWireHookBlock.calculateState`:
/// `attached` падает на постороннем блоке и на разоружённой нити,
/// `powered` — «есть включённая и неразоружённая нить», а оба требуют,
/// чтобы второй крюк стоял дальше первого блока (`receiverPos > 1`).
pub fn scan_chain(
    pl: &crate::placement::Placement,
    direction: usize,
    being_destroyed: bool,
    wire_source: i32,
    wire_state: Option<u32>,
    at: impl Fn(usize) -> Option<u32>,
) -> Chain {
    let mut attached = !being_destroyed;
    let mut powered = false;
    let mut receiver = 0;
    let mut reschedule = false;
    let mut wires = vec![None; WIRE_DIST_MAX as usize];
    let opposite = opposite_name(direction);
    for (i, slot) in wires.iter_mut().enumerate().skip(1) {
        let Some(mut wire) = at(i) else {
            attached = false;
            break;
        };
        let name = pl.name(wire).unwrap_or("");
        if is_tripwire_hook(name) {
            // Второй крюк считается, только если смотрит навстречу.
            if pl.prop(wire, "facing") != Some(opposite) {
                break;
            }
            receiver = i;
            break;
        }
        if is_tripwire(name) || i == wire_source as usize {
            if i == wire_source as usize
                && let Some(given) = wire_state
            {
                wire = given;
            }
            let armed = pl.prop(wire, "disarmed") != Some("true");
            let on = pl.prop(wire, "powered") == Some("true");
            powered |= armed && on;
            *slot = Some(wire);
            if i == wire_source as usize {
                reschedule = true;
                attached &= armed;
            }
            continue;
        }
        // Посторонний блок снимает натяжение, но поиск крюка продолжается.
        attached = false;
    }
    attached &= receiver > 1;
    powered &= attached;
    Chain {
        attached,
        powered,
        receiver: receiver as i32,
        reschedule,
        wires,
    }
}

/// Звук крюка (`TripWireHookBlock.emitState`): (звук, громкость, высота).
pub fn hook_sound(
    powered: bool,
    was_powered: bool,
    attached: bool,
    was_attached: bool,
    random: f32,
) -> Option<(&'static str, f32, f32)> {
    if powered && !was_powered {
        Some(("minecraft:block.tripwire.click_on", 0.4, 0.6))
    } else if !powered && was_powered {
        Some(("minecraft:block.tripwire.click_off", 0.4, 0.5))
    } else if attached && !was_attached {
        Some(("minecraft:block.tripwire.attach", 0.4, 0.7))
    } else if !attached && was_attached {
        // Высота дрожит: `1.2 / (random * 0.2 + 0.9)`.
        Some((
            "minecraft:block.tripwire.detach",
            0.4,
            1.2 / (random * 0.2 + 0.9),
        ))
    } else {
        None
    }
}

/// Разоружённая ножницами нить (`TripWireBlock.playerWillDestroy`).
pub fn tripwire_disarm_flag(pl: &crate::placement::Placement, state: u32) -> Option<u32> {
    pl.with_prop(state, "disarmed", "true")
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::placement::Placement;
    use std::collections::HashMap;

    /// Таблица: воздух, камень, провод со всеми формами, рычаг на полу, факел.
    fn table() -> (Placement, HashMap<String, u32>) {
        let mut states = Vec::new();
        let mut ids = HashMap::new();
        let mut add = |key: String, name: &str, props: Vec<(String, String)>| {
            ids.insert(key, states.len() as u32);
            states.push(Some((name.to_owned(), props)));
        };
        add("air".into(), "minecraft:air", vec![]);
        add("stone".into(), "minecraft:stone", vec![]);
        let sides = ["none", "side", "up"];
        for p in 0..16 {
            for n in sides {
                for s in sides {
                    for w in sides {
                        for e in sides {
                            let props = [
                                ("power", p.to_string()),
                                ("north", n.into()),
                                ("south", s.into()),
                                ("west", w.into()),
                                ("east", e.into()),
                            ];
                            add(
                                format!("wire_{p}_{n}_{s}_{w}_{e}"),
                                "minecraft:redstone_wire",
                                props
                                    .iter()
                                    .map(|(k, v)| (k.to_string(), v.clone()))
                                    .collect(),
                            );
                        }
                    }
                }
            }
        }
        for on in ["true", "false"] {
            add(
                format!("lever_{on}"),
                "minecraft:lever",
                vec![
                    ("face".into(), "floor".into()),
                    ("facing".into(), "north".into()),
                    ("powered".into(), on.into()),
                ],
            );
            add(
                format!("torch_{on}"),
                "minecraft:redstone_torch",
                vec![("lit".into(), on.into())],
            );
        }
        // Растяжка: крюк (снятый и включённый) и нить с `disarmed`.
        add(
            "hook".into(),
            "minecraft:tripwire_hook",
            vec![
                ("facing".into(), "north".into()),
                ("powered".into(), "false".into()),
                ("attached".into(), "false".into()),
            ],
        );
        for powered in ["true", "false"] {
            add(
                format!("hook_on_{powered}"),
                "minecraft:tripwire_hook",
                vec![
                    ("facing".into(), "north".into()),
                    ("powered".into(), powered.into()),
                    ("attached".into(), "true".into()),
                ],
            );
            // Крюк на дальнем конце цепи по оси X смотрит навстречу (запад).
            add(
                format!("hook_west_{powered}"),
                "minecraft:tripwire_hook",
                vec![
                    ("facing".into(), "west".into()),
                    ("powered".into(), powered.into()),
                    ("attached".into(), "true".into()),
                ],
            );
            for disarmed in ["true", "false"] {
                add(
                    format!("wire_{powered}_{disarmed}"),
                    "minecraft:tripwire",
                    vec![
                        ("powered".into(), powered.into()),
                        ("disarmed".into(), disarmed.into()),
                        ("attached".into(), "true".into()),
                        ("north".into(), "true".into()),
                        ("south".into(), "true".into()),
                        ("west".into(), "false".into()),
                        ("east".into(), "false".into()),
                    ],
                );
            }
        }
        let mut sturdy = vec![0u8; states.len()];
        sturdy[ids["stone"] as usize] = 63;
        (Placement::new(states, sturdy, Default::default()), ids)
    }

    #[test]
    fn wire_line_and_shapes() {
        let (pl, id) = table();
        let mut blocks: HashMap<[i32; 3], u32> = HashMap::new();
        for x in -1..20 {
            blocks.insert([x, 0, 0], id["stone"]);
        }
        let cross = id["wire_0_side_side_side_side"];
        for x in 1..18 {
            blocks.insert([x, 1, 0], cross);
        }
        blocks.insert([0, 1, 0], id["lever_true"]);
        let at = |p: [i32; 3]| blocks.get(&p).copied().unwrap_or(0);
        let w = World {
            pl: &pl,
            at: &at,
            output: &|_| 0,
            analog: &|_| None,
        };
        assert_eq!(w.wire_shape([5, 1, 0]), ["none", "none", "side", "side"]);
        let mut got: Vec<_> = w.solve(&[[5, 1, 0]]);
        got.sort();
        assert_eq!(got.len(), 17);
        assert_eq!(got[0], ([1, 1, 0], 15));
        assert_eq!(got[14], ([15, 1, 0], 1));
        assert_eq!(got[15], ([16, 1, 0], 0));
        // Рычаг сильно питает камень под собой, факел на соседнем камне — нет.
        assert!(w.conductor(id["stone"]));
        assert_eq!(w.signal([0, 0, 0], 0, true), 15);
        // Одинокий провод — крест.
        let lone = HashMap::from([
            ([0, 0, 0], id["stone"]),
            ([0, 1, 0], id["wire_0_none_none_none_none"]),
        ]);
        let at2 = |p: [i32; 3]| lone.get(&p).copied().unwrap_or(0);
        let w2 = World {
            pl: &pl,
            at: &at2,
            output: &|_| 0,
            analog: &|_| None,
        };
        assert_eq!(
            w2.wire_shape([0, 1, 0]),
            ["none"; 4],
            "точка остаётся точкой"
        );
        assert_eq!(w2.supported([0, 1, 0]), Some(true));
        assert_eq!(w2.supported([0, 0, 0]), None);
    }

    #[test]
    fn torch_turns_off_by_powered_block() {
        let (pl, id) = table();
        let blocks = HashMap::from([
            ([0, 0, 0], id["stone"]),
            ([0, 1, 0], id["torch_true"]),
            ([0, -1, 0], id["stone"]),
            ([1, 0, 0], id["air"]),
        ]);
        let at = |p: [i32; 3]| blocks.get(&p).copied().unwrap_or(0);
        let w = World {
            pl: &pl,
            at: &at,
            output: &|_| 0,
            analog: &|_| None,
        };
        assert!(!w.torch_off([0, 1, 0]));
        // Факел сильно питает блок над собой.
        assert_eq!(w.strong([0, 1, 0], 0, true), 15);
        assert_eq!(w.weak([0, 1, 0], 1, true), 0);
        let mut b2 = blocks.clone();
        b2.insert([1, 0, 0], id["lever_true"]);
        let at2 = |p: [i32; 3]| b2.get(&p).copied().unwrap_or(0);
        let w2 = World {
            pl: &pl,
            at: &at2,
            output: &|_| 0,
            analog: &|_| None,
        };
        // Рычаг на полу рядом с камнем питает его слабо: факел горит.
        assert!(!w2.torch_off([0, 1, 0]));
        assert_eq!(fullness([(64, 64)], 27), 1);
        assert_eq!(fullness((0..27).map(|_| (64, 64)), 27), 15);
        assert_eq!(fullness([], 27), 0);
        assert_eq!(
            plate_power("minecraft:heavy_weighted_pressure_plate", 11),
            2
        );
        assert_eq!(
            plate_sound("minecraft:oak_pressure_plate", true),
            "minecraft:block.wooden_pressure_plate.click_on"
        );
    }

    #[test]
    fn attachment_and_sounds() {
        assert_eq!(attached([0, 0, 0], "wall", "north"), [0, 0, 1]);
        assert_eq!(attached([0, 0, 0], "floor", "east"), [0, -1, 0]);
        assert_eq!(
            click_sound("minecraft:polished_blackstone_button", true).0,
            "minecraft:block.stone_button.click_on"
        );
        assert_eq!(
            click_sound("minecraft:oak_button", false).0,
            "minecraft:block.wooden_button.click_off"
        );
        assert_eq!(button_ticks("minecraft:stone_button"), 20);
        assert!(openable("minecraft:iron_door") && !openable("minecraft:lever"));
    }

    #[test]
    fn dot_wire_does_not_power_a_neighbour() {
        let (pl, id) = table();
        let blocks = HashMap::from([
            ([1, 1, -1], id["lever_true"]),
            ([1, 1, 0], id["wire_0_none_none_none_none"]),
            ([2, 1, 0], id["wire_0_none_none_none_none"]),
        ]);
        let at = |p: [i32; 3]| blocks.get(&p).copied().unwrap_or(0);
        let w = World {
            pl: &pl,
            at: &at,
            output: &|_| 0,
            analog: &|_| None,
        };
        let got = w.solve(&[[1, 1, 0], [2, 1, 0]]);
        let power = |p| got.iter().find(|g| g.0 == p).map(|g| g.1).unwrap_or(-1);
        assert_eq!(power([1, 1, 0]), 15);
        assert_eq!(power([2, 1, 0]), 0);
    }

    #[test]
    fn comparator_side_reads_a_torch() {
        let states = vec![
            Some(("minecraft:air".into(), Vec::new())),
            Some(("minecraft:redstone_block".into(), Vec::new())),
            Some((
                "minecraft:redstone_torch".into(),
                vec![("lit".into(), "true".into())],
            )),
            Some((
                "minecraft:comparator".into(),
                vec![
                    ("facing".into(), "north".into()),
                    ("mode".into(), "subtract".into()),
                    ("powered".into(), "true".into()),
                ],
            )),
        ];
        let pl = Placement::new(states, vec![0; 4], Default::default());
        let blocks = HashMap::from([([0, 0, -1], 1u32), ([1, 0, 0], 2), ([0, 0, 0], 3)]);
        let at = |p: [i32; 3]| blocks.get(&p).copied().unwrap_or(0);
        let w = World {
            pl: &pl,
            at: &at,
            output: &|_| 0,
            analog: &|_| None,
        };
        assert_eq!(w.comparator_target([0, 0, 0]), 0);
    }

    #[test]
    fn trapped_chest_signal_tracks_viewers() {
        let states = vec![
            Some(("minecraft:air".into(), vec![])),
            Some(("minecraft:trapped_chest".into(), vec![])),
            Some(("minecraft:stone".into(), vec![])),
        ];
        let pl = Placement::new(states, vec![0, 0, 63], Default::default());
        // Обе половины открытого двойного сундука получают одинаковый счётчик.
        let blocks = HashMap::from([([0, 0, 0], 1u32), ([1, 0, 0], 1), ([0, -1, 0], 2)]);
        let at = |p: [i32; 3]| blocks.get(&p).copied().unwrap_or(0);
        for viewers in [0, 1, 2, 15, 16] {
            let power = trapped_chest_power(viewers);
            let w = World {
                pl: &pl,
                at: &at,
                output: &|_| power,
                analog: &|_| None,
            };
            for pos in [[0, 0, 0], [1, 0, 0]] {
                for dir in 0..6 {
                    assert_eq!(w.weak(pos, dir, true), power);
                    assert_eq!(w.strong(pos, dir, true), if dir == UP { power } else { 0 });
                }
            }
            assert_eq!(w.signal([0, -1, 0], DOWN, true), power);
        }
    }

    #[test]
    fn cake_and_cauldrons_feed_comparator_through_block() {
        let mut states = vec![
            Some(("minecraft:air".into(), vec![])),
            Some(("minecraft:stone".into(), vec![])),
            Some((
                "minecraft:comparator".into(),
                vec![
                    ("facing".into(), "north".into()),
                    ("mode".into(), "compare".into()),
                ],
            )),
        ];
        let mut cases = Vec::new();
        for bites in 0..=6 {
            let id = states.len() as u32;
            states.push(Some((
                "minecraft:cake".into(),
                vec![("bites".into(), bites.to_string())],
            )));
            cases.push((id, 14 - 2 * bites));
        }
        let empty = states.len() as u32;
        states.push(Some(("minecraft:cauldron".into(), vec![])));
        cases.push((empty, 0));
        for name in ["minecraft:water_cauldron", "minecraft:powder_snow_cauldron"] {
            for level in 1..=3 {
                let id = states.len() as u32;
                states.push(Some((
                    name.into(),
                    vec![("level".into(), level.to_string())],
                )));
                cases.push((id, level));
            }
        }
        let lava = states.len() as u32;
        states.push(Some(("minecraft:lava_cauldron".into(), vec![])));
        cases.push((lava, 3));
        let mut sturdy = vec![0; states.len()];
        sturdy[1] = 63;
        let pl = Placement::new(states, sturdy, Default::default());
        for (id, expected) in cases {
            assert_eq!(block_analog(&pl, id), Some(expected));
            let blocks = HashMap::from([([0, 0, 0], 2u32), ([0, 0, -1], 1), ([0, 0, -2], id)]);
            let at = |p: [i32; 3]| blocks.get(&p).copied().unwrap_or(0);
            let w = World {
                pl: &pl,
                at: &at,
                output: &|_| 0,
                analog: &|p| block_analog(&pl, at(p)),
            };
            assert_eq!(w.comparator_target([0, 0, 0]), expected);
        }
    }

    #[test]
    fn composter_and_hives_feed_comparator_directly_and_through_block() {
        let mut states = vec![
            Some(("minecraft:air".into(), vec![])),
            Some(("minecraft:stone".into(), vec![])),
            Some((
                "minecraft:comparator".into(),
                vec![
                    ("facing".into(), "north".into()),
                    ("mode".into(), "compare".into()),
                ],
            )),
        ];
        let mut cases = Vec::new();
        for (name, property, max_level) in [
            ("minecraft:composter", "level", 8),
            ("minecraft:beehive", "honey_level", 5),
            ("minecraft:bee_nest", "honey_level", 5),
        ] {
            for level in 0..=max_level {
                let id = states.len() as u32;
                states.push(Some((
                    name.into(),
                    vec![(property.into(), level.to_string())],
                )));
                cases.push((id, level));
            }
        }
        let mut sturdy = vec![0; states.len()];
        sturdy[1] = 63;
        let pl = Placement::new(states, sturdy, Default::default());
        for (id, expected) in cases {
            assert_eq!(block_analog(&pl, id), Some(expected));
            for through_block in [false, true] {
                let mut blocks = HashMap::from([([0, 0, 0], 2u32)]);
                if through_block {
                    blocks.insert([0, 0, -1], 1);
                    blocks.insert([0, 0, -2], id);
                } else {
                    blocks.insert([0, 0, -1], id);
                }
                let at = |p: [i32; 3]| blocks.get(&p).copied().unwrap_or(0);
                let w = World {
                    pl: &pl,
                    at: &at,
                    output: &|_| 0,
                    analog: &|p| block_analog(&pl, at(p)),
                };
                assert_eq!(w.comparator_target([0, 0, 0]), expected);
            }
        }
    }

    #[test]
    fn respawn_anchor_charge_feeds_comparator_directly_and_through_block() {
        let mut states = vec![
            Some(("minecraft:air".into(), vec![])),
            Some(("minecraft:stone".into(), vec![])),
            Some((
                "minecraft:comparator".into(),
                vec![
                    ("facing".into(), "north".into()),
                    ("mode".into(), "compare".into()),
                ],
            )),
        ];
        let mut cases = Vec::new();
        for (charges, expected) in [(0, 0), (1, 3), (2, 7), (3, 11), (4, 15)] {
            let id = states.len() as u32;
            states.push(Some((
                "minecraft:respawn_anchor".into(),
                vec![("charges".into(), charges.to_string())],
            )));
            cases.push((id, expected));
        }
        let mut sturdy = vec![0; states.len()];
        sturdy[1] = 63;
        let pl = Placement::new(states, sturdy, Default::default());
        for (id, expected) in cases {
            assert_eq!(block_analog(&pl, id), Some(expected));
            for through_block in [false, true] {
                let mut blocks = HashMap::from([([0, 0, 0], 2u32)]);
                if through_block {
                    blocks.insert([0, 0, -1], 1);
                    blocks.insert([0, 0, -2], id);
                } else {
                    blocks.insert([0, 0, -1], id);
                }
                let at = |p: [i32; 3]| blocks.get(&p).copied().unwrap_or(0);
                let w = World {
                    pl: &pl,
                    at: &at,
                    output: &|_| 0,
                    analog: &|p| block_analog(&pl, at(p)),
                };
                assert_eq!(w.comparator_target([0, 0, 0]), expected);
            }
        }
    }

    /// Крюк растяжки — источник: сигнал 15 слабым выходом и сильным только в
    /// сторону `facing` (`ownSignal` и `getDirectSignal`).
    #[test]
    fn tripwire_hook_signal_is_directional() {
        let (pl, id) = table();
        let blocks = HashMap::from([([0, 0, 0], id["hook_on_true"]), ([1, 0, 0], id["hook"])]);
        let at = |p: [i32; 3]| blocks.get(&p).copied().unwrap_or(0);
        let w = World {
            pl: &pl,
            at: &at,
            output: &|_| 0,
            analog: &|_| None,
        };
        // Слабый сигнал — во все шесть сторон, пока крюк включён.
        for dir in 0..6 {
            assert_eq!(
                w.weak([0, 0, 0], dir, true),
                15,
                "слабый, направление {dir}"
            );
        }
        // Сильный — только в `facing` (север = 2), то есть в опору.
        assert_eq!(w.strong([0, 0, 0], 2, true), 15);
        for dir in [0, 1, 3, 4, 5] {
            assert_eq!(
                w.strong([0, 0, 0], dir, true),
                0,
                "сильный, направление {dir}"
            );
        }
        // Выключенный крюк молчит.
        for dir in 0..6 {
            assert_eq!(w.weak([1, 0, 0], dir, true), 0);
            assert_eq!(w.strong([1, 0, 0], dir, true), 0);
        }
    }

    /// Габариты нити и попадание в них: натянутая ловится узкой полосой у
    /// пола, провисшая — до половины блока.
    #[test]
    fn tripwire_shape_matches_vanilla() {
        let (attached_lo, attached_hi) = tripwire_shape(true);
        assert_eq!(attached_lo[1], 1.0 / 16.0);
        assert_eq!(attached_hi[1], 2.5 / 16.0);
        let (loose_lo, loose_hi) = tripwire_shape(false);
        assert_eq!(loose_lo[1], 0.0);
        assert_eq!(loose_hi[1], 0.5);
        // По горизонтали нить занимает всю клетку.
        for (lo, hi) in [(attached_lo, attached_hi), (loose_lo, loose_hi)] {
            assert_eq!(lo[0], 0.0);
            assert_eq!(hi[0], 1.0);
        }

        let wire = [0, 0, 0];
        // Стоящий на полу игрок (ноги на y=0) задевает обе формы.
        let feet = ([0.2, 0.0, 0.2], [0.8, 1.8, 0.8]);
        assert!(touches_wire(false, wire, feet.0, feet.1));
        assert!(touches_wire(true, wire, feet.0, feet.1));
        // Висящий над нитью в метре — нет.
        let above = ([0.2, 1.0, 0.2], [0.8, 2.8, 0.8]);
        assert!(!touches_wire(false, wire, above.0, above.1));
        // Сущность, стоящая внутри пологой нити (`0…8/16`), но выше полосы
        // натянутой (`1/16…2.5/16`): провисшая ловит, натянутая — нет.
        let gap = ([0.2, 0.4, 0.2], [0.8, 0.9, 0.8]);
        assert!(touches_wire(false, wire, gap.0, gap.1), "провисшая ловит");
        assert!(
            !touches_wire(true, wire, gap.0, gap.1),
            "натянутая не ловит"
        );
        // А сущность в полосе натянутой — ловится обеими.
        let band = ([0.2, 0.07, 0.2], [0.8, 1.87, 0.8]);
        assert!(touches_wire(true, wire, band.0, band.1));
        // Соседняя клетка не ловится.
        assert!(!touches_wire(false, [1, 0, 0], feet.0, feet.1));
    }

    /// Нить соединяется с соседней нитью и с крюком, смотрящим навстречу.
    #[test]
    fn tripwire_connects_like_vanilla() {
        // Нить к северу от нас: dir = 2 (север), крюк должен смотреть на юг.
        assert!(tripwire_connects("minecraft:tripwire", None, 2));
        assert!(tripwire_connects(
            "minecraft:tripwire_hook",
            Some("south"),
            2
        ));
        // Крюк, смотрящий в ту же сторону, не соединяется.
        assert!(!tripwire_connects(
            "minecraft:tripwire_hook",
            Some("north"),
            2
        ));
        // Посторонний блок — нет.
        assert!(!tripwire_connects("minecraft:stone", None, 2));
    }

    /// Цепь крюк–нить–нить–крюк: `attached` и `powered` как в ванили.
    #[test]
    fn tripwire_chain_matches_vanilla() {
        let (pl, id) = table();
        // Цепь по оси X: 0 — крюк, 1 и 2 — нить, 3 — второй крюк.
        let chain = |wire_powered: &'static str, disarmed: &'static str| {
            let key = format!("wire_{wire_powered}_{disarmed}");
            let blocks = [
                id["hook"],
                id[key.as_str()],
                id[key.as_str()],
                id["hook_west_false"],
            ];
            scan_chain(&pl, 5, false, -1, None, |i| blocks.get(i).copied())
        };
        // Нить включена — крюк натянут и под сигналом.
        let on = chain("true", "false");
        assert!(on.attached, "цепь из двух нитей натянута");
        assert!(on.powered, "включённая нить питает крюк");
        assert_eq!(on.receiver, 3, "второй крюк через две клетки");
        // Нить выключена — натянута, но сигнала нет.
        let off = chain("false", "false");
        assert!(off.attached);
        assert!(!off.powered);
        // Разоружённая нить: сигнал не поднимается (`powered |= armed && on`),
        // но `attached` снимается только при вызове из этой нити
        // (`if (i != wireSource) continue; attached &= wireArmed`).
        let disarmed = chain("true", "true");
        assert!(!disarmed.powered, "разоружённая нить сигнала не даёт");
        // Обход от крюка (wire_source = −1) `attached` не трогает.
        assert!(disarmed.attached);
        // А вызов из разоружённой нити — снимает.
        let blocks = [
            id["hook"],
            id["wire_true_true"],
            id["wire_true_false"],
            id["hook_west_false"],
        ];
        let from_wire = scan_chain(&pl, 5, false, 1, None, |i| blocks.get(i).copied());
        assert!(
            !from_wire.attached,
            "нить, из которой пришёл вызов, разоружена — натяжение снимается"
        );
        assert!(from_wire.reschedule, "крюку нужен повторный тик");

        // Крюк вплотную (`receiverPos == 1`) цепью не считается:
        // `attached &= receiverPos > 1`.
        let blocks = [id["hook"], id["hook_west_false"]];
        let short = scan_chain(&pl, 5, false, -1, None, |i| blocks.get(i).copied());
        assert_eq!(short.receiver, 1);
        assert!(!short.attached, "крюк без нити не натягивает");
        assert!(!short.powered);
        // А через одну нить (receiver == 2) — уже считается.
        let blocks = [id["hook"], id["wire_true_false"], id["hook_west_false"]];
        let one_wire = scan_chain(&pl, 5, false, -1, None, |i| blocks.get(i).copied());
        assert_eq!(one_wire.receiver, 2);
        assert!(one_wire.attached);

        // Посторонний блок снимает натяжение, но поиск второго крюка ванила
        // не прекращает: `receiverPos` всё равно находится, а `attached`
        // остаётся ложным.
        let blocks = [
            id["hook"],
            id["wire_true_false"],
            id["stone"],
            id["hook_west_false"],
        ];
        let broken = scan_chain(&pl, 5, false, -1, None, |i| blocks.get(i).copied());
        assert!(!broken.attached, "посторонний блок снимает натяжение");
        assert!(!broken.powered);
        assert_eq!(broken.receiver, 3, "второй крюк всё равно найден");
    }

    /// Звуки крюка: включение, выключение, натяжение и обрыв.
    #[test]
    fn tripwire_hook_sounds() {
        assert_eq!(
            hook_sound(true, false, true, true, 0.5),
            Some(("minecraft:block.tripwire.click_on", 0.4, 0.6))
        );
        assert_eq!(
            hook_sound(false, true, true, true, 0.5),
            Some(("minecraft:block.tripwire.click_off", 0.4, 0.5))
        );
        assert_eq!(
            hook_sound(false, false, true, false, 0.5),
            Some(("minecraft:block.tripwire.attach", 0.4, 0.7))
        );
        let (sound, volume, pitch) = hook_sound(false, false, false, true, 0.5).expect("обрыв");
        assert_eq!(sound, "minecraft:block.tripwire.detach");
        assert_eq!(volume, 0.4);
        // Высота дрожит: `1.2 / (random * 0.2 + 0.9)`.
        assert!((pitch - 1.2 / (0.5 * 0.2 + 0.9)).abs() < 1e-6);
        // Ничего не изменилось — молчание.
        assert_eq!(hook_sound(true, true, true, true, 0.5), None);
    }

    /// `NoteBlock.getPitchFromNote`: `2^((note - 12) / 12)`, 25 значений 0…24.
    #[test]
    fn note_pitch_matches_vanilla() {
        // Ноты сняты с самой формулы: 12 — «нет транспозиции», выше — октава.
        assert!((note_pitch(12) - 1.0).abs() < 1e-6, "нота 12 — 1.0");
        assert!((note_pitch(0) - 0.5).abs() < 1e-6, "нота 0 — октавой ниже");
        assert!(
            (note_pitch(24) - 2.0).abs() < 1e-6,
            "нота 24 — октавой выше"
        );
        assert!((note_pitch(6) - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
        // Все 25 нот растут монотонно.
        for note in 1..25 {
            assert!(note_pitch(note) > note_pitch(note - 1), "нота {note}");
        }
    }

    /// `NoteBlock.setInstrument`: инструмент от блока над, иначе от блока под.
    ///
    /// Голова над нотным блоком важнее любого блока под, а голова под —
    /// обычный `harp` (у неё `worksAboveNoteBlock`, поэтому снизу не считается).
    #[test]
    fn note_instrument_matches_vanilla() {
        use chunkwake_data::world::NoteInstrument as N;

        // Ничего необычного вокруг — арфа.
        assert_eq!(instrument_for(Some(N::Harp), Some(N::Harp)), N::Harp);
        assert_eq!(instrument_for(Some(N::Harp), None), N::Harp);
        // Камень под — барабан.
        assert_eq!(
            instrument_for(Some(N::Harp), Some(N::BaseDrum)),
            N::BaseDrum
        );
        // Дерево под — бас.
        assert_eq!(instrument_for(Some(N::Harp), Some(N::Bass)), N::Bass);
        // Стекло под — арфа, хотя у стекла нет своего инструмента.
        assert_eq!(instrument_for(None, Some(N::Harp)), N::Harp);
        // Голова над блоком важнее камня под.
        assert_eq!(
            instrument_for(Some(N::Dragon), Some(N::BaseDrum)),
            N::Dragon
        );
        // Голова под — не считается: инструмент снизу работает «над блоком».
        assert_eq!(instrument_for(Some(N::Harp), Some(N::Zombie)), N::Harp);
        // Пустоты нет ни сверху, ни снизу.
        assert_eq!(instrument_for(None, None), N::Harp);
    }

    /// `NeighborChanged`: звук только на фронте сигнала.
    #[test]
    fn note_signal_only_on_rising_edge() {
        assert_eq!(note_signal(true, false), NoteAction::Play);
        assert_eq!(note_signal(false, true), NoteAction::UpdateOnly);
        assert_eq!(note_signal(true, true), NoteAction::Nothing);
        assert_eq!(note_signal(false, false), NoteAction::Nothing);
    }

    /// `playNote`: инструмент «над блоком» звучит всегда, настраиваемый —
    /// только под открытым небом (над ним воздух).
    #[test]
    fn note_plays_in_vanilla_conditions() {
        // Арфа под блоком не звучит.
        assert!(!note_sounds(false, false));
        // Арфа под открытым небом звучит.
        assert!(note_sounds(false, true));
        // Голова звучит и под блоком: у неё `worksAboveNoteBlock`.
        assert!(note_sounds(true, false));
        assert!(note_sounds(true, true));
    }

    /// Частица и звук ноты: позиция, дельта и громкость из `triggerEvent`.
    #[test]
    fn note_visuals_match_vanilla() {
        let (at, delta) = note_particle(7, [10, 64, -3]);
        assert_eq!(at, [10.5, 65.2, -2.5]);
        assert!((delta[0] - 7.0 / 24.0).abs() < 1e-6);
        assert_eq!(delta[1], 0.0);
        assert_eq!(delta[2], 0.0);
        // Звук — из центра блока, не сверху.
        assert_eq!(note_sound_pos([10, 64, -3]), [10.5, 64.5, -2.5]);
        assert_eq!(NOTE_VOLUME, 3.0);
    }

    /// Отчёт `instruments.csv` читается и даёт инструменты по ванили.
    ///
    /// Значения сверены с `Blocks.java`: `STONE` — барабан, `OAK_PLANKS` —
    /// бас, `GLASS` — хэт, `SAND` — снейр, `NOTE_BLOCK` — бас (у самого
    /// нотного блока `instrument(BASS)`), `SKELETON_SKULL` — скелет.
    /// Без кэша тест пропускается: файла нет.
    #[test]
    fn instruments_report_matches_vanilla() {
        use chunkwake_data::world::{NoteInstrument as N, load_instruments};
        let Some(root) = std::env::var_os("CHUNKWAKE_DATA") else {
            return;
        };
        let ok = load_instruments(std::path::Path::new(&root)).expect("instruments.csv");
        for (block, want) in [
            ("minecraft:stone", N::BaseDrum),
            ("minecraft:oak_planks", N::Bass),
            ("minecraft:glass", N::Hat),
            ("minecraft:sand", N::Snare),
            ("minecraft:note_block", N::Bass),
            ("minecraft:skeleton_skull", N::Skeleton),
            ("minecraft:dragon_head", N::Dragon),
        ] {
            assert_eq!(ok.of(block), want, "{block}");
        }
        // У воздуха инструмента нет — арфа по умолчанию.
        assert_eq!(ok.of("minecraft:air"), N::Harp);
    }

    /// Имена инструментов: те же строки, что в свойстве блока и в отчёте.
    #[test]
    fn note_instrument_names_round_trip() {
        use chunkwake_data::world::NoteInstrument as N;
        let all = [
            N::Harp,
            N::BaseDrum,
            N::Snare,
            N::Hat,
            N::Bass,
            N::Flute,
            N::Bell,
            N::Guitar,
            N::Chime,
            N::Xylophone,
            N::IronXylophone,
            N::CowBell,
            N::Didgeridoo,
            N::Bit,
            N::Banjo,
            N::Pling,
            N::Trumpet,
            N::TrumpetExposed,
            N::TrumpetOxidized,
            N::TrumpetWeathered,
            N::Zombie,
            N::Skeleton,
            N::Creeper,
            N::Dragon,
            N::WitherSkeleton,
            N::Piglin,
            N::CustomHead,
        ];
        for kind in all {
            assert_eq!(N::parse(kind.name()), Some(kind), "{}", kind.name());
        }
        // У настраиваемых звук зависит от `note`, у голов — нет.
        assert!(N::Harp.tunable());
        assert!(!N::Zombie.tunable());
        assert!(N::Zombie.works_above());
        assert!(!N::Harp.works_above());
        // Голова игрока берёт звук из блок-сущности.
        assert!(N::CustomHead.custom_sound());
        assert!(!N::Harp.custom_sound());
        // Имя звука головы дракона — с `ender_`.
        assert_eq!(
            N::Dragon.sound(),
            "minecraft:block.note_block.imitate.ender_dragon"
        );
    }

    /// Частоты вибраций (`VibrationSystem.VIBRATION_FREQUENCY_FOR_EVENT`).
    #[test]
    fn vibration_frequencies_match_vanilla() {
        // Значения сняты с таблицы ванили: 1 — шаги, 15 — смерть и взрыв.
        for (event, want) in [
            ("minecraft:step", 1),
            ("minecraft:swim", 1),
            ("minecraft:flap", 1),
            ("minecraft:projectile_land", 2),
            ("minecraft:hit_ground", 2),
            ("minecraft:splash", 2),
            ("minecraft:bounce", 2),
            ("minecraft:projectile_shoot", 3),
            ("minecraft:item_interact_finish", 3),
            ("minecraft:instrument_play", 3),
            ("minecraft:entity_action", 4),
            ("minecraft:elytra_glide", 4),
            ("minecraft:unequip", 4),
            ("minecraft:entity_dismount", 5),
            ("minecraft:equip", 5),
            ("minecraft:entity_interact", 6),
            ("minecraft:shear", 6),
            ("minecraft:entity_mount", 6),
            ("minecraft:entity_damage", 7),
            ("minecraft:drink", 8),
            ("minecraft:eat", 8),
            ("minecraft:container_close", 9),
            ("minecraft:block_close", 9),
            ("minecraft:block_deactivate", 9),
            ("minecraft:container_open", 10),
            ("minecraft:block_open", 10),
            ("minecraft:block_activate", 10),
            ("minecraft:note_block_play", 10),
            ("minecraft:block_change", 11),
            ("minecraft:block_destroy", 12),
            ("minecraft:fluid_pickup", 12),
            ("minecraft:block_place", 13),
            ("minecraft:fluid_place", 13),
            ("minecraft:entity_place", 14),
            ("minecraft:lightning_strike", 14),
            ("minecraft:teleport", 14),
            ("minecraft:entity_die", 15),
            ("minecraft:explode", 15),
        ] {
            assert_eq!(vibration_frequency(event), want, "{event}");
        }
        // Резонанс: частота из имени события.
        assert_eq!(vibration_frequency("minecraft:resonate_1"), 1);
        assert_eq!(vibration_frequency("minecraft:resonate_15"), 15);
        // Неизвестное событие и резонанс вне диапазона скалк не слышит.
        assert_eq!(vibration_frequency("minecraft:unknown"), 0);
        assert_eq!(vibration_frequency("minecraft:resonate_16"), 0);
        assert_eq!(vibration_frequency("minecraft:resonate_0"), 0);
    }

    /// `getRedstoneStrengthForDistance`: вплотную 15, на границе радиуса 1.
    #[test]
    fn sculk_sensor_power_matches_vanilla() {
        // Радиус 8: 15/8 = 1.875 на блок расстояния.
        assert_eq!(vibration_power(0.0, 8), 15);
        // 1.875 * 1 = 1.875 → floor 1 → 14.
        assert_eq!(vibration_power(1.0, 8), 14);
        assert_eq!(vibration_power(2.0, 8), 12);
        assert_eq!(vibration_power(4.0, 8), 8);
        assert_eq!(vibration_power(7.0, 8), 2);
        // На границе радиуса `15 - 15 = 0`, но `max(1, …)` даёт единицу.
        assert_eq!(vibration_power(8.0, 8), 1);
        // «Максимум с единицей» уже внутри формулы — зажим не нужен.
        assert_eq!(vibration_power(8.0, 8).max(1), 1);
        // И на большом расстоянии формула не уходит в минус.
        assert_eq!(vibration_power(100.0, 8), 1);
    }

    /// Фазы сенсора: активный гаснет, отдохнувший возвращается в покой.
    #[test]
    fn sculk_phase_transitions_match_vanilla() {
        assert_eq!(SculkPhase::parse("active"), SculkPhase::Active);
        assert_eq!(SculkPhase::parse("cooldown"), SculkPhase::Cooldown);
        assert_eq!(SculkPhase::parse("inactive"), SculkPhase::Inactive);
        assert_eq!(SculkPhase::parse("что-то"), SculkPhase::Inactive);
        // Сработать можно только из покоя.
        assert!(SculkPhase::Inactive.can_activate());
        assert!(!SculkPhase::Active.can_activate());
        assert!(!SculkPhase::Cooldown.can_activate());
        // Тик: активный → погасить, отдохнувший → покой, покой → ничего.
        assert_eq!(sculk_tick(SculkPhase::Active), SculkTick::Deactivate);
        assert_eq!(sculk_tick(SculkPhase::Cooldown), SculkTick::Reset);
        assert_eq!(sculk_tick(SculkPhase::Inactive), SculkTick::Nothing);
        assert_eq!(SCULK_ACTIVE_TICKS, 30);
        assert_eq!(SCULK_COOLDOWN_TICKS, 10);
    }

    /// Калиброванный сенсор: молчит в сторону `facing` и фильтрует частоты.
    #[test]
    fn calibrated_sculk_sensor_rules() {
        // FACING = north (2): в эту сторону сигнала нет, в остальные — есть.
        assert!(calibrated_signal_blocked(2, 2));
        for dir in [0, 1, 3, 4, 5] {
            assert!(!calibrated_signal_blocked(2, dir), "{dir}");
        }
        // Обычный сенсор сильно питает только верх.
        assert!(sculk_direct(1));
        for dir in [0, 2, 3, 4, 5] {
            assert!(!sculk_direct(dir), "{dir}");
        }
        // Фильтр: без сигнала сзади слышно всё, с сигналом — только его частоту.
        assert!(calibrated_accepts(0, 1));
        assert!(calibrated_accepts(0, 15));
        assert!(calibrated_accepts(7, 7));
        assert!(!calibrated_accepts(7, 8));
    }

    /// Резонанс: высота и событие по частоте, отклик аметиста.
    #[test]
    fn resonance_matches_vanilla() {
        // Индекс 0 — тон 0, индекс 1 — тоже 0, дальше по таблице.
        assert!((resonance_pitch(0) - note_pitch(0)).abs() < 1e-6);
        assert!((resonance_pitch(1) - note_pitch(0)).abs() < 1e-6);
        assert!((resonance_pitch(2) - note_pitch(2)).abs() < 1e-6);
        assert!((resonance_pitch(15) - note_pitch(24)).abs() < 1e-6);
        // За пределами диапазона — зажим.
        assert!((resonance_pitch(99) - note_pitch(24)).abs() < 1e-6);
        assert_eq!(resonance_event(1), "minecraft:resonate_1");
        assert_eq!(resonance_event(15), "minecraft:resonate_15");
        assert_eq!(resonance_event(99), "minecraft:resonate_15");
        // Резонируют блоки и ростки аметиста.
        assert!(is_resonator("minecraft:amethyst_block"));
        assert!(is_resonator("minecraft:budding_amethyst"));
        assert!(is_resonator("minecraft:amethyst_cluster"));
        assert!(!is_resonator("minecraft:stone"));
    }
}
