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

fn opposite(d: usize) -> usize {
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
    /// Сохранённый выход компаратора (`OutputSignal`).
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
}
