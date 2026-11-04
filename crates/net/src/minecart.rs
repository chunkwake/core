//! Физика вагонетки по рельсам (`OldMinecartBehavior` — поведение по умолчанию,
//! новое `NewMinecartBehavior` включается только экспериментом).
//!
//! Модуль чистый: мир виден через [`Track`], поэтому каждый шаг проверяется
//! тестом без сервера.

/// `RailShape` в порядке `RailShape.values()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    NorthSouth,
    EastWest,
    AscendingEast,
    AscendingWest,
    AscendingNorth,
    AscendingSouth,
    SouthEast,
    SouthWest,
    NorthWest,
    NorthEast,
}

impl Shape {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "north_south" => Self::NorthSouth,
            "east_west" => Self::EastWest,
            "ascending_east" => Self::AscendingEast,
            "ascending_west" => Self::AscendingWest,
            "ascending_north" => Self::AscendingNorth,
            "ascending_south" => Self::AscendingSouth,
            "south_east" => Self::SouthEast,
            "south_west" => Self::SouthWest,
            "north_west" => Self::NorthWest,
            "north_east" => Self::NorthEast,
            _ => return None,
        })
    }

    pub fn is_slope(self) -> bool {
        matches!(
            self,
            Self::AscendingEast | Self::AscendingWest | Self::AscendingNorth | Self::AscendingSouth
        )
    }

    /// `AbstractMinecart.EXITS`: два выхода рельса относительно его клетки.
    pub fn exits(self) -> ([i32; 3], [i32; 3]) {
        match self {
            Self::NorthSouth => ([0, 0, -1], [0, 0, 1]),
            Self::EastWest => ([-1, 0, 0], [1, 0, 0]),
            Self::AscendingEast => ([-1, -1, 0], [1, 0, 0]),
            Self::AscendingWest => ([-1, 0, 0], [1, -1, 0]),
            Self::AscendingNorth => ([0, 0, -1], [0, -1, 1]),
            Self::AscendingSouth => ([0, -1, -1], [0, 0, 1]),
            Self::SouthEast => ([0, 0, 1], [1, 0, 0]),
            Self::SouthWest => ([0, 0, 1], [-1, 0, 0]),
            Self::NorthWest => ([0, 0, -1], [-1, 0, 0]),
            Self::NorthEast => ([0, 0, -1], [1, 0, 0]),
        }
    }
}

/// Тип рельса: питаемый разгоняет или тормозит, активирующий и детектор
/// отмечаются для игровой части.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Plain,
    Powered(bool),
    Detector,
    Activator(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rail {
    pub shape: Shape,
    pub kind: Kind,
}

/// Что вагонетка видит в мире.
pub trait Track {
    fn rail(&self, p: [i32; 3]) -> Option<Rail>;
    /// `isRedstoneConductor`: полный проводящий блок (упор для старта с питаемого рельса).
    fn conductor(&self, p: [i32; 3]) -> bool;
    /// Клетка с коллизией: стена на пути и пол под вагонеткой вне рельсов.
    fn solid(&self, p: [i32; 3]) -> bool;
    fn water(&self, p: [i32; 3]) -> bool;
}

/// `EntityType.MINECART`: 0.98 × 0.7.
pub const WIDTH: f64 = 0.98;
pub const HEIGHT: f64 = 0.7;
/// `Entity.getDefaultGravity` вагонетки.
const GRAVITY: f64 = 0.04;
const SLOPE: f64 = 0.0078125;

#[derive(Clone, Debug, PartialEq)]
pub struct Cart {
    pub pos: [f64; 3],
    pub vel: [f64; 3],
    pub yaw: f32,
    pub flipped: bool,
    pub on_ground: bool,
    /// `isVehicle`: с пассажиром трение меньше и скорость по рельсам ×0.75.
    pub ridden: bool,
    /// Чем тормозит вид вагонетки (`applyNaturalSlowdown`).
    pub drag: Drag,
    /// Тяга печной вагонетки (`MinecartFurnace.push`), по горизонтали.
    pub push: [f64; 2],
    /// За этот тик упёрлась в стену.
    collided: bool,
}

/// Своё трение вида вагонетки.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Drag {
    /// `AbstractMinecart`: 0.997 с пассажиром, 0.96 пустая.
    Plain,
    /// `AbstractMinecartContainer`: `0.98 + (15 − сигнал)·0.001`, без
    /// общего трения; сигнал — заполненность для компаратора.
    Container(u8),
    /// `MinecartFurnace`: тяга `push`, скорость ×0.5 (в воде ×0.75).
    Furnace,
}

/// Что случилось за тик: рельс под вагонеткой (для детектора и активатора).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Step {
    pub rail: Option<([i32; 3], Rail)>,
    /// Упёрлась в стену (`horizontalCollision`).
    pub collided: bool,
}

fn floor(v: f64) -> i32 {
    v.floor() as i32
}

impl Cart {
    pub fn new(pos: [f64; 3], yaw: f32) -> Self {
        Self {
            pos,
            vel: [0.0; 3],
            yaw,
            flipped: false,
            on_ground: false,
            ridden: false,
            drag: Drag::Plain,
            push: [0.0; 2],
            collided: false,
        }
    }

    fn in_water(&self, t: &impl Track) -> bool {
        t.water([floor(self.pos[0]), floor(self.pos[1]), floor(self.pos[2])])
    }

    /// `OldMinecartBehavior.getMaxSpeed`: 8 блоков/с, в воде 4.
    fn max_speed(&self, t: &impl Track) -> f64 {
        let water = self.in_water(t);
        let base = (if water { 4.0 } else { 8.0 }) / 20.0;
        match self.drag {
            // `MinecartFurnace.getMaxSpeed`.
            Drag::Furnace => base * if water { 0.75 } else { 0.5 },
            _ => base,
        }
    }

    /// `applyNaturalSlowdown` вида вагонетки; `vel[1]` обнуляется.
    fn natural_slowdown(&mut self, water: bool) {
        let [vx, _, vz] = self.vel;
        let (mut vx, mut vz) = match self.drag {
            Drag::Container(signal) => {
                let mut f = 0.98f32 + (15 - signal.min(15) as i32) as f32 * 0.001;
                if water {
                    f *= 0.95;
                }
                self.vel = [vx * f as f64, 0.0, vz * f as f64];
                return;
            }
            Drag::Furnace if self.push[0] * self.push[0] + self.push[1] * self.push[1] > 1.0e-7 => {
                self.push = self.push_along([vx, vz]);
                let (mut x, mut z) = (vx * 0.8 + self.push[0], vz * 0.8 + self.push[1]);
                if water {
                    x *= 0.1;
                    z *= 0.1;
                }
                (x, z)
            }
            Drag::Furnace => (vx * 0.98, vz * 0.98),
            Drag::Plain => (vx, vz),
        };
        let f = if self.ridden { 0.997 } else { 0.96 };
        vx *= f;
        vz *= f;
        if water {
            vx *= 0.95;
            vz *= 0.95;
        }
        self.vel = [vx, 0.0, vz];
    }

    /// `MinecartFurnace.calculateNewPushAlong`: тяга поворачивает вслед за
    /// скоростью, сохраняя длину.
    fn push_along(&self, v: [f64; 2]) -> [f64; 2] {
        let p = self.push;
        let (p2, v2) = (p[0] * p[0] + p[1] * p[1], v[0] * v[0] + v[1] * v[1]);
        if p2 <= 1.0e-4 || v2 <= 0.001 {
            return p;
        }
        // `push.projectedOn(v).normalize().scale(push.length())`
        let k = (p[0] * v[0] + p[1] * v[1]) / v2;
        let q = [v[0] * k, v[1] * k];
        let ql = (q[0] * q[0] + q[1] * q[1]).sqrt();
        if ql < 1.0e-5 {
            return [0.0; 2];
        }
        let l = p2.sqrt();
        [q[0] / ql * l, q[1] / ql * l]
    }

    /// `AbstractMinecart.getCurrentBlockPosOrRailBelow`.
    pub fn rail_pos(&self, t: &impl Track) -> [i32; 3] {
        let p = [floor(self.pos[0]), floor(self.pos[1]), floor(self.pos[2])];
        let below = [p[0], p[1] - 1, p[2]];
        if t.rail(below).is_some() { below } else { p }
    }

    /// Один тик (`OldMinecartBehavior.tick`, серверная ветка). `intent` —
    /// `ServerPlayer.getLastClientMoveIntent` водителя (вектор в мире).
    pub fn tick(&mut self, t: &impl Track, intent: [f64; 2]) -> Step {
        let old = self.pos;
        self.collided = false;
        self.vel[1] -= GRAVITY;
        let pos = self.rail_pos(t);
        let mut step = Step::default();
        if let Some(rail) = t.rail(pos) {
            self.move_along_track(t, pos, rail, intent);
            step.rail = Some((pos, rail));
        } else {
            self.come_off_track(t);
        }
        // Поворот по пройденному пути; разворот больше 170° — «переворот».
        let (dx, dz) = (old[0] - self.pos[0], old[2] - self.pos[2]);
        let mut yaw = self.yaw;
        if dx * dx + dz * dz > 0.001 {
            yaw = (dz.atan2(dx).to_degrees()) as f32;
            if self.flipped {
                yaw += 180.0;
            }
        }
        let mut d = (yaw - self.yaw).rem_euclid(360.0);
        if d >= 180.0 {
            d -= 360.0;
        }
        if !(-170.0..170.0).contains(&d) {
            yaw += 180.0;
            self.flipped = !self.flipped;
        }
        self.yaw = yaw.rem_euclid(360.0);
        step.collided = self.collided;
        step
    }

    /// `AbstractMinecart.comeOffTrack`.
    fn come_off_track(&mut self, t: &impl Track) {
        let max = self.max_speed(t);
        self.vel[0] = self.vel[0].clamp(-max, max);
        self.vel[2] = self.vel[2].clamp(-max, max);
        if self.on_ground {
            self.vel = self.vel.map(|v| v * 0.5);
        }
        let v = self.vel;
        self.travel(t, v);
        if !self.on_ground {
            self.vel = self.vel.map(|v| v * 0.95);
        }
    }

    /// Упрощённый `Entity.move`: по осям, стены и пол — целые клетки.
    fn travel(&mut self, t: &impl Track, v: [f64; 3]) {
        let half = WIDTH / 2.0;
        let blocked = |p: [f64; 3]| {
            let y = floor(p[1] + 0.01);
            [(-half, -half), (-half, half), (half, -half), (half, half)]
                .iter()
                .any(|&(ox, oz)| t.solid([floor(p[0] + ox * 0.99), y, floor(p[2] + oz * 0.99)]))
        };
        for axis in [0, 2] {
            let mut next = self.pos;
            next[axis] += v[axis];
            if blocked(next) {
                self.vel[axis] = 0.0;
                self.collided = true;
            } else {
                self.pos = next;
            }
        }
        let y = self.pos[1] + v[1];
        if v[1] < 0.0 {
            let floor_y = floor(y);
            let cell = [floor(self.pos[0]), floor_y, floor(self.pos[2])];
            if t.solid(cell) && (floor_y + 1) as f64 <= self.pos[1] + 1e-9 {
                self.pos[1] = (floor_y + 1) as f64;
                self.vel[1] = 0.0;
                self.on_ground = true;
                return;
            }
        }
        self.pos[1] = y;
        self.on_ground = false;
    }

    /// `OldMinecartBehavior.getPos`: точка на рельсе под `pos` (с высотой склона).
    pub fn track_point(t: &impl Track, pos: [f64; 3]) -> Option<[f64; 3]> {
        let (i, mut j, k) = (floor(pos[0]), floor(pos[1]), floor(pos[2]));
        if t.rail([i, j - 1, k]).is_some() {
            j -= 1;
        }
        let rail = t.rail([i, j, k])?;
        let (a, b) = rail.shape.exits();
        let x0 = i as f64 + 0.5 + a[0] as f64 * 0.5;
        let y0 = j as f64 + 0.0625 + a[1] as f64 * 0.5;
        let z0 = k as f64 + 0.5 + a[2] as f64 * 0.5;
        let x1 = i as f64 + 0.5 + b[0] as f64 * 0.5;
        let y1 = j as f64 + 0.0625 + b[1] as f64 * 0.5;
        let z1 = k as f64 + 0.5 + b[2] as f64 * 0.5;
        let (ex, ey, ez) = (x1 - x0, (y1 - y0) * 2.0, z1 - z0);
        let s = if ex == 0.0 {
            pos[2] - k as f64
        } else if ez == 0.0 {
            pos[0] - i as f64
        } else {
            ((pos[0] - x0) * ex + (pos[2] - z0) * ez) * 2.0
        };
        let mut y = y0 + ey * s;
        if ey < 0.0 {
            y += 1.0;
        } else if ey > 0.0 {
            y += 0.5;
        }
        Some([x0 + ex * s, y, z0 + ez * s])
    }

    /// `OldMinecartBehavior.moveAlongTrack`.
    fn move_along_track(&mut self, t: &impl Track, pos: [i32; 3], rail: Rail, intent: [f64; 2]) {
        let before = Self::track_point(t, self.pos);
        self.pos[1] = pos[1] as f64;
        let (powered, mut braking) = match rail.kind {
            Kind::Powered(on) => (on, !on),
            _ => (false, false),
        };
        let slope = if self.in_water(t) { SLOPE * 0.2 } else { SLOPE };
        match rail.shape {
            Shape::AscendingEast => {
                self.vel[0] -= slope;
                self.pos[1] += 1.0;
            }
            Shape::AscendingWest => {
                self.vel[0] += slope;
                self.pos[1] += 1.0;
            }
            Shape::AscendingNorth => {
                self.vel[2] += slope;
                self.pos[1] += 1.0;
            }
            Shape::AscendingSouth => {
                self.vel[2] -= slope;
                self.pos[1] += 1.0;
            }
            _ => {}
        }
        let (a, b) = rail.shape.exits();
        let (mut ex, mut ez) = ((b[0] - a[0]) as f64, (b[2] - a[2]) as f64);
        let len = (ex * ex + ez * ez).sqrt();
        if self.vel[0] * ex + self.vel[2] * ez < 0.0 {
            ex = -ex;
            ez = -ez;
        }
        let speed = self.vel[0].hypot(self.vel[2]).min(2.0);
        self.vel[0] = speed * ex / len;
        self.vel[2] = speed * ez / len;
        // Водитель толкает стоящую вагонетку (`getLastClientMoveIntent` × 0.001).
        if intent[0] * intent[0] + intent[1] * intent[1] > 0.0
            && self.vel[0] * self.vel[0] + self.vel[2] * self.vel[2] < 0.01
        {
            self.vel[0] += intent[0] * 0.001;
            self.vel[2] += intent[1] * 0.001;
            braking = false;
        }
        if braking {
            if self.vel[0].hypot(self.vel[2]) < 0.03 {
                self.vel = [0.0; 3];
            } else {
                self.vel[0] *= 0.5;
                self.vel[2] *= 0.5;
            }
        }
        let (px, pz) = (pos[0] as f64, pos[2] as f64);
        let x0 = px + 0.5 + a[0] as f64 * 0.5;
        let z0 = pz + 0.5 + a[2] as f64 * 0.5;
        let x1 = px + 0.5 + b[0] as f64 * 0.5;
        let z1 = pz + 0.5 + b[2] as f64 * 0.5;
        let (ex, ez) = (x1 - x0, z1 - z0);
        let s = if ex == 0.0 {
            self.pos[2] - pz
        } else if ez == 0.0 {
            self.pos[0] - px
        } else {
            ((self.pos[0] - x0) * ex + (self.pos[2] - z0) * ez) * 2.0
        };
        self.pos[0] = x0 + ex * s;
        self.pos[2] = z0 + ez * s;
        let m = if self.ridden { 0.75 } else { 1.0 };
        let max = self.max_speed(t);
        let v = [
            (m * self.vel[0]).clamp(-max, max),
            0.0,
            (m * self.vel[2]).clamp(-max, max),
        ];
        self.travel(t, v);
        let cell = |p: [f64; 3]| (floor(p[0]) - pos[0], floor(p[2]) - pos[2]);
        if a[1] != 0 && cell(self.pos) == (a[0], a[2]) {
            self.pos[1] += a[1] as f64;
        } else if b[1] != 0 && cell(self.pos) == (b[0], b[2]) {
            self.pos[1] += b[1] as f64;
        }
        // `applyNaturalSlowdown`: с пассажиром 0.997, пустая 0.96; в воде ещё ×0.95.
        let water = self.in_water(t);
        self.natural_slowdown(water);
        // Сохранение энергии на склоне: скорость от перепада высоты.
        if let (Some(after), Some(before)) = (Self::track_point(t, self.pos), before) {
            let dy = (before[1] - after[1]) * 0.05;
            let h = self.vel[0].hypot(self.vel[2]);
            if h > 0.0 {
                self.vel[0] *= (h + dy) / h;
                self.vel[2] *= (h + dy) / h;
            }
            self.pos[1] = after[1];
        }
        // Переехали в соседнюю клетку: скорость вдоль шага.
        let (nx, nz) = (floor(self.pos[0]), floor(self.pos[2]));
        if nx != pos[0] || nz != pos[2] {
            let h = self.vel[0].hypot(self.vel[2]);
            self.vel[0] = h * (nx - pos[0]) as f64;
            self.vel[2] = h * (nz - pos[2]) as f64;
        }
        if powered {
            let h = self.vel[0].hypot(self.vel[2]);
            if h > 0.01 {
                self.vel[0] += self.vel[0] / h * 0.06;
                self.vel[2] += self.vel[2] / h * 0.06;
            } else {
                let at = |dx: i32, dz: i32| t.conductor([pos[0] + dx, pos[1], pos[2] + dz]);
                match rail.shape {
                    Shape::EastWest => {
                        if at(-1, 0) {
                            self.vel[0] = 0.02;
                        } else if at(1, 0) {
                            self.vel[0] = -0.02;
                        }
                    }
                    Shape::NorthSouth => {
                        if at(0, -1) {
                            self.vel[2] = 0.02;
                        } else if at(0, 1) {
                            self.vel[2] = -0.02;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

/// `ServerPlayer.getLastClientMoveIntent`: клавиши относительно взгляда
/// (`Entity.getInputVector` со скоростью 1).
pub fn move_intent(forward: bool, back: bool, left: bool, right: bool, yaw: f32) -> [f64; 2] {
    let strafe = match (left, right) {
        (true, false) => 1.0,
        (false, true) => -1.0,
        _ => 0.0,
    };
    let fwd = match (forward, back) {
        (true, false) => 1.0,
        (false, true) => -1.0,
        _ => 0.0,
    };
    let (mut x, mut z) = (strafe as f64, fwd as f64);
    let len2 = x * x + z * z;
    if len2 < 1.0e-7 {
        return [0.0, 0.0];
    }
    if len2 > 1.0 {
        let l = len2.sqrt();
        x /= l;
        z /= l;
    }
    let r = (yaw as f64).to_radians();
    let (s, c) = (r.sin(), r.cos());
    [x * c - z * s, z * c + x * s]
}

/// `PoweredRailBlock.findPoweredRailSignal`: сигнал по цепочке того же рельса
/// не дальше 8 звеньев. `at` — рельс того же типа (`shape`, `powered`), `signal`
/// — `hasNeighborSignal`.
pub fn rail_signal(
    pos: [i32; 3],
    shape: Shape,
    forward: bool,
    depth: u32,
    at: &impl Fn([i32; 3]) -> Option<(Shape, bool)>,
    signal: &impl Fn([i32; 3]) -> bool,
) -> bool {
    if depth >= 8 {
        return false;
    }
    let [mut x, mut y, mut z] = pos;
    let mut below = true;
    let mut line = shape;
    match shape {
        Shape::NorthSouth => z += if forward { 1 } else { -1 },
        Shape::EastWest => x += if forward { -1 } else { 1 },
        Shape::AscendingEast => {
            if forward {
                x -= 1;
            } else {
                x += 1;
                y += 1;
                below = false;
            }
            line = Shape::EastWest;
        }
        Shape::AscendingWest => {
            if forward {
                x -= 1;
                y += 1;
                below = false;
            } else {
                x += 1;
            }
            line = Shape::EastWest;
        }
        Shape::AscendingNorth => {
            if forward {
                z += 1;
            } else {
                z -= 1;
                y += 1;
                below = false;
            }
            line = Shape::NorthSouth;
        }
        Shape::AscendingSouth => {
            if forward {
                z += 1;
                y += 1;
                below = false;
            } else {
                z -= 1;
            }
            line = Shape::NorthSouth;
        }
        _ => {}
    }
    let same = |p: [i32; 3]| -> bool {
        let Some((s, powered)) = at(p) else { return false };
        let ns = matches!(s, Shape::NorthSouth | Shape::AscendingNorth | Shape::AscendingSouth);
        let ew = matches!(s, Shape::EastWest | Shape::AscendingEast | Shape::AscendingWest);
        if (line == Shape::EastWest && ns) || (line == Shape::NorthSouth && ew) || !powered {
            return false;
        }
        signal(p) || rail_signal(p, s, forward, depth + 1, at, signal)
    };
    same([x, y, z]) || (below && same([x, y - 1, z]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct World {
        rails: HashMap<[i32; 3], Rail>,
        solid: Vec<[i32; 3]>,
    }

    impl Track for World {
        fn rail(&self, p: [i32; 3]) -> Option<Rail> {
            self.rails.get(&p).copied()
        }
        fn conductor(&self, p: [i32; 3]) -> bool {
            self.solid.contains(&p)
        }
        fn solid(&self, p: [i32; 3]) -> bool {
            self.solid.contains(&p) || p[1] < 0
        }
        fn water(&self, _: [i32; 3]) -> bool {
            false
        }
    }

    fn line(n: i32, kind: Kind) -> World {
        let mut w = World::default();
        for x in 0..n {
            w.rails.insert([x, 0, 0], Rail { shape: Shape::EastWest, kind });
        }
        w
    }

    #[test]
    fn cart_sits_on_the_rail_height() {
        let w = line(3, Kind::Plain);
        let p = Cart::track_point(&w, [1.5, 0.3, 0.5]).unwrap();
        assert_eq!(p, [1.5, 0.0625, 0.5]);
    }

    #[test]
    fn pushed_cart_slows_down_and_stays_on_track() {
        let w = line(40, Kind::Plain);
        let mut c = Cart::new([2.5, 0.0625, 0.5], 0.0);
        c.vel = [0.3, 0.0, 0.0];
        let mut last = c.vel[0];
        for _ in 0..20 {
            c.tick(&w, [0.0; 2]);
            assert!((c.pos[2] - 0.5).abs() < 1e-9, "{c:?}");
            assert!((c.pos[1] - 0.0625).abs() < 1e-9, "{c:?}");
            assert!(c.vel[0] <= last + 1e-12 && c.vel[0] > 0.0, "{c:?}");
            last = c.vel[0];
        }
        // Пустая теряет 4 % за тик.
        assert!((last - 0.3 * 0.96f64.powi(20)).abs() < 0.01, "{last}");
    }

    #[test]
    fn container_drag_depends_on_fill() {
        // Пустой сундук: 0.98 + 15·0.001 = 0.995 за тик; полный — 0.98.
        for (signal, f) in [(0u8, 0.995f32), (15, 0.98)] {
            let mut c = Cart::new([0.5, 0.0, 0.5], 0.0);
            c.drag = Drag::Container(signal);
            c.vel = [0.1, 0.0, 0.0];
            c.natural_slowdown(false);
            assert!((c.vel[0] - 0.1 * f as f64).abs() < 1e-9, "{signal}: {c:?}");
        }
    }

    #[test]
    fn furnace_push_drives_cart_at_half_speed() {
        let w = line(60, Kind::Plain);
        let mut c = Cart::new([2.5, 0.0625, 0.5], 0.0);
        c.drag = Drag::Furnace;
        c.push = [2.0, 0.0];
        let x0 = c.pos[0];
        for _ in 0..40 {
            c.tick(&w, [0.0; 2]);
        }
        // Предел печной — половина обычного: 0.2 блока за тик.
        let moved = c.pos[0] - x0;
        assert!(moved > 7.0 && moved <= 8.0 + 1e-9, "{moved}");
        // Тяга повёрнута вдоль скорости и сохранила длину.
        assert!((c.push[0] - 2.0).abs() < 1e-9 && c.push[1].abs() < 1e-9, "{c:?}");
    }

    #[test]
    fn unpowered_rail_brakes_and_powered_rail_boosts() {
        let mut w = line(10, Kind::Plain);
        w.rails.insert([5, 0, 0], Rail { shape: Shape::EastWest, kind: Kind::Powered(false) });
        let mut c = Cart::new([5.5, 0.0625, 0.5], 0.0);
        c.vel = [0.2, 0.0, 0.0];
        c.tick(&w, [0.0; 2]);
        assert!(c.vel[0] < 0.1, "тормозит: {c:?}");
        w.rails.insert([5, 0, 0], Rail { shape: Shape::EastWest, kind: Kind::Powered(true) });
        let mut c = Cart::new([5.5, 0.0625, 0.5], 0.0);
        c.vel = [0.2, 0.0, 0.0];
        c.tick(&w, [0.0; 2]);
        assert!(c.vel[0] > 0.2, "разгоняет: {c:?}");
    }

    #[test]
    fn powered_rail_starts_cart_away_from_a_block() {
        let mut w = line(10, Kind::Plain);
        w.rails.insert([0, 0, 0], Rail { shape: Shape::EastWest, kind: Kind::Powered(true) });
        w.solid.push([-1, 0, 0]);
        let mut c = Cart::new([0.5, 0.0625, 0.5], 0.0);
        c.tick(&w, [0.0; 2]);
        assert!(c.vel[0] > 0.0, "{c:?}");
        for _ in 0..10 {
            c.tick(&w, [0.0; 2]);
        }
        assert!(c.pos[0] > 0.6, "{c:?}");
    }

    #[test]
    fn cart_climbs_a_slope_with_its_speed() {
        let mut w = line(3, Kind::Plain);
        w.rails.insert([3, 0, 0], Rail { shape: Shape::AscendingEast, kind: Kind::Plain });
        for x in 4..8 {
            w.rails.insert([x, 1, 0], Rail { shape: Shape::EastWest, kind: Kind::Plain });
        }
        let mut c = Cart::new([1.5, 0.0625, 0.5], 0.0);
        c.vel = [0.4, 0.0, 0.0];
        for _ in 0..15 {
            c.tick(&w, [0.0; 2]);
        }
        assert!(c.pos[0] > 4.0 && (c.pos[1] - 1.0625).abs() < 1e-6, "{c:?}");
    }

    #[test]
    fn cart_turns_on_a_curve() {
        let mut w = line(3, Kind::Plain);
        w.rails.insert([3, 0, 0], Rail { shape: Shape::SouthWest, kind: Kind::Plain });
        for z in 1..6 {
            w.rails.insert([3, 0, z], Rail { shape: Shape::NorthSouth, kind: Kind::Plain });
        }
        let mut c = Cart::new([1.5, 0.0625, 0.5], 0.0);
        c.vel = [0.3, 0.0, 0.0];
        for _ in 0..20 {
            c.tick(&w, [0.0; 2]);
        }
        assert!(c.pos[2] > 2.0 && (c.pos[0] - 3.5).abs() < 1e-6, "{c:?}");
    }

    #[test]
    fn rider_intent_nudges_a_stopped_cart() {
        let w = line(5, Kind::Plain);
        let mut c = Cart::new([2.5, 0.0625, 0.5], 0.0);
        c.ridden = true;
        // Взгляд на запад (yaw 90): вперёд — −x.
        let i = move_intent(true, false, false, false, 90.0);
        assert!((i[0] + 1.0).abs() < 1e-9 && i[1].abs() < 1e-9, "{i:?}");
        c.tick(&w, i);
        assert!(c.vel[0] < 0.0, "{c:?}");
    }

    #[test]
    fn cart_off_rails_falls_to_the_ground() {
        let w = World::default();
        let mut c = Cart::new([0.5, 3.0, 0.5], 0.0);
        for _ in 0..40 {
            c.tick(&w, [0.0; 2]);
        }
        assert!(c.on_ground && c.pos[1] == 0.0, "{c:?}");
    }

    #[test]
    fn powered_rail_signal_runs_eight_rails() {
        // Источник у x=0; рельсы 0..12 на восток.
        let at = |p: [i32; 3]| (p[1] == 0 && p[2] == 0 && (0..12).contains(&p[0])).then_some((Shape::EastWest, true));
        let signal = |p: [i32; 3]| p == [0, 0, 0];
        let lit = |x: i32| {
            signal([x, 0, 0])
                || rail_signal([x, 0, 0], Shape::EastWest, true, 0, &at, &signal)
                || rail_signal([x, 0, 0], Shape::EastWest, false, 0, &at, &signal)
        };
        assert!(lit(8));
        assert!(!lit(9));
    }
}
