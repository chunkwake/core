//! Вагонетки (`Minecart`, `AbstractMinecart`, `MinecartItem`) в мире.
//!
//! В отличие от лодки вагонеткой правит сервер: физика рельсов — в
//! [`crate::minecart`], здесь — сущность, тик, рассылка, посадка, детекторный
//! рельс, удар и запись. Ездит только обычная вагонетка (`minecraft:minecart`).

use super::*;
use crate::minecart::{Cart, Kind, Rail, Shape, Track};

const ITEM: &str = "minecraft:minecart";
/// `VehicleEntity.hurtServer`, как у лодки.
const DAMAGE_SCALE: f32 = 10.0;
const BREAK_AT: f32 = 40.0;
const HURT_TIME: i32 = 10;

/// Биты `player_input` (`Input`).
const IN_FORWARD: u8 = 0x01;
const IN_BACK: u8 = 0x02;
const IN_LEFT: u8 = 0x04;
const IN_RIGHT: u8 = 0x08;

pub(crate) struct MinecartEntry {
    pub cart: Cart,
    uuid: Uuid,
    rider: Option<(Uuid, i32)>,
    /// Последние клавиши водителя.
    input: u8,
    damage: f32,
    hit_at: i64,
    hurt_dir: i32,
    sent: ([f64; 3], f32),
    custom_name: Option<String>,
}

impl MinecartEntry {
    fn new(cart: Cart) -> Self {
        Self {
            sent: (cart.pos, cart.yaw),
            cart,
            uuid: Uuid::from_u128(rand::random()),
            rider: None,
            input: 0,
            damage: 0.0,
            hit_at: 0,
            hurt_dir: 1,
            custom_name: None,
        }
    }

    fn damage_at(&self, now: i64) -> f32 {
        (self.damage - (now - self.hit_at).max(0) as f32).max(0.0)
    }

    /// Коробка вагонетки: `[min, max]`.
    fn bounds(&self) -> ([f64; 3], [f64; 3]) {
        let h = crate::minecart::WIDTH / 2.0;
        let p = self.cart.pos;
        ([p[0] - h, p[1], p[2] - h], [p[0] + h, p[1] + crate::minecart::HEIGHT, p[2] + h])
    }
}

/// Мир для физики вагонетки.
struct PlanTrack<'a>(&'a PlayPlan);

impl Track for PlanTrack<'_> {
    fn rail(&self, p: [i32; 3]) -> Option<Rail> {
        let s = self.0.block(p)?;
        let name = self.0.state_name(s);
        if !name.ends_with("rail") {
            return None;
        }
        let pl = &self.0.survival.placement;
        let shape = Shape::parse(pl.prop(s, "shape")?)?;
        let on = pl.prop(s, "powered") == Some("true");
        let kind = match name {
            "minecraft:powered_rail" => Kind::Powered(on),
            "minecraft:activator_rail" => Kind::Activator(on),
            "minecraft:detector_rail" => Kind::Detector,
            _ => Kind::Plain,
        };
        Some(Rail { shape, kind })
    }

    fn conductor(&self, p: [i32; 3]) -> bool {
        let at = |q: [i32; 3]| self.0.block(q).unwrap_or(0);
        let w = crate::redstone::World {
            pl: &self.0.survival.placement,
            at: &at,
            output: &|_| 0,
            analog: &|_| None,
        };
        w.conductor(at(p))
    }

    fn solid(&self, p: [i32; 3]) -> bool {
        self.0.block(p).is_some_and(|s| !self.0.state_name(s).ends_with("rail") && self.0.blocks_boat(s))
    }

    fn water(&self, p: [i32; 3]) -> bool {
        self.0.block(p).is_some_and(|s| {
            crate::fluids::state_fluid(&self.0.survival.placement, s)
                .is_some_and(|f| f.kind == crate::fluids::Kind::Water)
        })
    }
}

impl PlayPlan {
    fn minecart_type(&self) -> Option<i32> {
        self.survival.entity_types.get(ITEM).copied()
    }

    fn minecart_packets(&self, id: i32, e: &MinecartEntry, now: i64) -> Vec<Vec<u8>> {
        let Some(ty) = self.minecart_type() else {
            return Vec::new();
        };
        let c = &e.cart;
        let mut out = vec![
            play::add_entity_moving(self.ids.add_entity, id, e.uuid, ty, c.pos, c.vel, c.yaw, 0.0, 0),
            inv::boat_data_hurt(self.ids.set_entity_data, id, 0, e.hurt_dir, e.damage_at(now)),
        ];
        if let Some((_, rider)) = e.rider {
            out.push(play::set_passengers(self.ids.set_passengers, id, &[rider]));
        }
        out
    }

    /// Все вагонетки для зашедшего игрока.
    pub(super) fn minecart_frames(&self) -> Vec<u8> {
        let now = self.game_time.load(Ordering::Relaxed);
        let mut out = Vec::new();
        for (&id, e) in self.sim().minecarts.lock().unwrap().iter() {
            for p in self.minecart_packets(id, e, now) {
                out.extend(self.hub.frame(&p));
            }
        }
        out
    }

    pub(super) fn is_minecart(&self, id: i32) -> bool {
        self.sim().minecarts.lock().unwrap().contains_key(&id)
    }

    fn add_minecart(&self, e: MinecartEntry) -> i32 {
        let id = self.next_entity_id.fetch_add(1, Ordering::Relaxed);
        let packets = self.minecart_packets(id, &e, self.game_time.load(Ordering::Relaxed));
        self.sim().minecarts.lock().unwrap().insert(id, e);
        self.mark_work();
        self.hub.send_many(packets);
        id
    }

    /// `MinecartItem.useOn`: только на рельс; центр клетки на высоте рельса,
    /// на склоне на полблока выше.
    pub(super) fn place_minecart(&self, item: &str, rail: [i32; 3]) -> bool {
        if item != ITEM || self.minecart_type().is_none() {
            return false;
        }
        let Some(r) = PlanTrack(self).rail(rail) else {
            return false;
        };
        let lift = if r.shape.is_slope() { 0.5 } else { 0.0 };
        let pos = [rail[0] as f64 + 0.5, rail[1] as f64 + 0.0625 + lift, rail[2] as f64 + 0.5];
        // `getEntities(minecart.getBoundingBox())`: на месте уже есть вагонетка — нельзя.
        let busy = self.sim().minecarts.lock().unwrap().values().any(|e| {
            let (lo, hi) = e.bounds();
            (0..3).all(|i| pos[i] + 0.49 > lo[i] && pos[i] - 0.49 < hi[i])
        });
        if busy {
            return false;
        }
        self.add_minecart(MinecartEntry::new(Cart::new(pos, 0.0)));
        true
    }

    /// `MinecartDispenseItemBehavior.execute`: вагонетка на 1.125 перед
    /// центром раздатчика, по высоте `floor(центр) + шагY`; рельс спереди —
    /// +0.1 (на подъёме +0.6), воздух с рельсом под ним — −0.9 (−0.4, если
    /// нижний — подъём и раздатчик смотрит не вниз), иначе обычный выброс.
    pub(super) fn dispense_minecart(&self, item: &str, dispenser: [i32; 3], facing: usize) -> Option<bool> {
        if item != ITEM {
            return None;
        }
        self.minecart_type()?;
        let d = crate::placement::step(facing);
        let front = [dispenser[0] + d[0], dispenser[1] + d[1], dispenser[2] + d[2]];
        let track = PlanTrack(self);
        let lift = if let Some(r) = track.rail(front) {
            if r.shape.is_slope() { 0.6 } else { 0.1 }
        } else if self.block(front).is_some_and(|s| self.state_name(s).ends_with(":air"))
            && let Some(r) = track.rail([front[0], front[1] - 1, front[2]])
        {
            if facing == 0 || !r.shape.is_slope() { -0.9 } else { -0.4 }
        } else {
            return None;
        };
        let pos = [
            dispenser[0] as f64 + 0.5 + d[0] as f64 * 1.125,
            dispenser[1] as f64 + d[1] as f64 + lift,
            dispenser[2] as f64 + 0.5 + d[2] as f64 * 1.125,
        ];
        self.add_minecart(MinecartEntry::new(Cart::new(pos, 0.0)));
        Some(true)
    }

    /// `Minecart.interact`: без шифта и в пустую — сесть.
    pub(super) fn mount_minecart(&self, uuid: Uuid, entity_id: i32, cart: i32) -> bool {
        let cart_uuid = {
            let mut carts = self.sim().minecarts.lock().unwrap();
            let Some(e) = carts.get_mut(&cart) else {
                return false;
            };
            if e.rider.is_some() {
                return false;
            }
            e.rider = Some((uuid, entity_id));
            e.input = 0;
            e.uuid
        };
        self.link_rider(uuid, cart_uuid);
        self.send_passengers(cart, &[entity_id]);
        true
    }

    /// Клавиши водителя из `player_input`.
    pub(super) fn minecart_input(&self, uuid: Uuid, input: u8) {
        if let Some(e) = self.sim().minecarts.lock().unwrap().values_mut().find(|e| e.rider.is_some_and(|r| r.0 == uuid)) {
            e.input = input;
        }
    }

    /// Снять с вагонетки; место высадки (`AbstractMinecart.getDismountLocationForPassenger`).
    pub(super) fn leave_minecart(&self, uuid: Uuid) -> Option<[f64; 3]> {
        let (id, cart) = {
            let mut carts = self.sim().minecarts.lock().unwrap();
            let (&id, e) = carts.iter_mut().find(|(_, e)| e.rider.is_some_and(|r| r.0 == uuid))?;
            e.rider = None;
            e.input = 0;
            (id, e.cart.clone())
        };
        self.unlink_rider(uuid);
        self.send_passengers(id, &[]);
        Some(self.minecart_dismount(&cart))
    }

    /// `DismountHelper.offsetsForDirection` по направлению движения: сначала
    /// по бокам, затем наискосок, назад и вперёд; высоты 0, +1, −1. Нет места —
    /// на вагонетку сверху.
    fn minecart_dismount(&self, c: &Cart) -> [f64; 3] {
        let base = [c.pos[0].floor() as i32, c.pos[1].floor() as i32, c.pos[2].floor() as i32];
        // Направление: по скорости, а у стоящей — по рельсу.
        let dir: [i32; 2] = if c.vel[0].abs() > 1e-4 || c.vel[2].abs() > 1e-4 {
            if c.vel[0].abs() > c.vel[2].abs() {
                [c.vel[0].signum() as i32, 0]
            } else {
                [0, c.vel[2].signum() as i32]
            }
        } else {
            match PlanTrack(self).rail(c.rail_pos(&PlanTrack(self))).map(|r| r.shape) {
                Some(Shape::EastWest | Shape::AscendingEast | Shape::AscendingWest) => [1, 0],
                _ => [0, 1],
            }
        };
        let cw = [-dir[1], dir[0]];
        let ccw = [-cw[0], -cw[1]];
        let back = [-dir[0], -dir[1]];
        let offsets = [
            cw,
            ccw,
            [back[0] + cw[0], back[1] + cw[1]],
            [back[0] + ccw[0], back[1] + ccw[1]],
            [dir[0] + cw[0], dir[1] + cw[1]],
            [dir[0] + ccw[0], dir[1] + ccw[1]],
            back,
            dir,
        ];
        let solid = |p: [i32; 3]| self.block(p).is_some_and(|s| self.blocks_boat(s));
        for dy in [0, 1, -1] {
            for o in offsets {
                let m = [base[0] + o[0], base[1] + dy, base[2] + o[1]];
                let below = [m[0], m[1] - 1, m[2]];
                let head = [m[0], m[1] + 1, m[2]];
                if !solid(m) && solid(below) && !solid(head) {
                    return [m[0] as f64 + 0.5, m[1] as f64, m[2] as f64 + 0.5];
                }
            }
        }
        [c.pos[0], c.pos[1] + crate::minecart::HEIGHT, c.pos[2]]
    }

    /// Удар (`VehicleEntity.hurtServer`); `None` — не вагонетка.
    pub(super) fn attack_minecart(&self, uuid: Uuid, target: i32, held: Slot, creative: bool, crit: bool) -> Option<()> {
        let pos = self.player_at(uuid)?.0;
        let now = self.game_time.load(Ordering::Relaxed);
        let item = held.and_then(|s| self.survival.items.get(s.item as usize));
        let mut amount = 1.0 + item.map_or(0.0, |i| i.attack);
        let sharp = held.map_or(0, |s| chunkwake_protocol::components::stack_level(s.extra, "minecraft:sharpness"));
        if sharp > 0 {
            amount += 0.5 * sharp as f32 + 0.5;
        }
        if crit {
            amount *= 1.5;
        }
        let mut carts = self.sim().minecarts.lock().unwrap();
        let e = carts.get_mut(&target)?;
        let d: f64 = (0..3).map(|i| (e.cart.pos[i] - pos[i]).powi(2)).sum();
        if d > 36.0 {
            return Some(());
        }
        e.hurt_dir = -e.hurt_dir;
        e.damage = e.damage_at(now) + amount * DAMAGE_SCALE;
        e.hit_at = now;
        let broken = !creative && e.damage > BREAK_AT;
        if !broken && !creative {
            let packet = inv::boat_data_hurt(self.ids.set_entity_data, target, HURT_TIME, e.hurt_dir, e.damage);
            drop(carts);
            self.hub.send(None, packet);
            return Some(());
        }
        let e = carts.remove(&target)?;
        drop(carts);
        if let Some((rider, _)) = e.rider {
            self.unlink_rider(rider);
        }
        self.hub.send(None, play::remove_entities(self.ids.remove_entities, &[target]));
        if broken && self.vars.flag("entity_drops")
            && let Some(item) = self.item_id(ITEM)
        {
            let vel = items::pop_velocity(rand::random(), rand::random());
            self.drop_item(e.cart.pos, vel, Stack::new(item, 1), items::BLOCK_DELAY);
        }
        Some(())
    }

    /// Тик вагонеток в симулируемых чанках: физика, толчки игроков,
    /// детекторные рельсы, рассылка движения, позиция водителя.
    pub(super) fn tick_minecarts(&self) {
        if self.sim().minecarts.lock().unwrap().is_empty() {
            return;
        }
        let simulated = self.simulated_set();
        let awake = |p: [f64; 3]| {
            simulated.is_empty() || simulated.contains(&((p[0].floor() as i32) >> 4, (p[2].floor() as i32) >> 4))
        };
        // Игроки пешком: позиция, поворот (для водителя) и uuid.
        let players: Vec<(Uuid, [f64; 3], f32)> = self
            .hub
            .players
            .lock()
            .unwrap()
            .values()
            .map(|p| (p.entry.uuid, p.pos, p.yaw))
            .collect();
        let track = PlanTrack(self);
        let mut packets = Vec::new();
        let mut riders = Vec::new();
        let mut pressed = Vec::new();
        let mut ejected = Vec::new();
        {
            let mut carts = self.sim().minecarts.lock().unwrap();
            for (&id, e) in carts.iter_mut() {
                if !awake(e.cart.pos) {
                    continue;
                }
                // `AbstractMinecart.push`: игрок, чья коробка задевает вагонетку, толкает её.
                for &(uuid, p, _) in &players {
                    if e.rider.is_some_and(|r| r.0 == uuid) {
                        continue;
                    }
                    let (lo, hi) = e.bounds();
                    let overlap = p[0] + 0.3 > lo[0] && p[0] - 0.3 < hi[0]
                        && p[2] + 0.3 > lo[2] && p[2] - 0.3 < hi[2]
                        && p[1] + 1.8 > lo[1] && p[1] < hi[1];
                    if !overlap {
                        continue;
                    }
                    let (mut dx, mut dz) = (p[0] - e.cart.pos[0], p[2] - e.cart.pos[2]);
                    let d2 = dx * dx + dz * dz;
                    if d2 >= 1.0e-4 {
                        let d = d2.sqrt();
                        dx /= d;
                        dz /= d;
                        let k = (1.0 / d).min(1.0) * 0.05;
                        e.cart.vel[0] -= dx * k;
                        e.cart.vel[2] -= dz * k;
                    }
                }
                e.cart.ridden = e.rider.is_some();
                let intent = e.rider.and_then(|(r, _)| players.iter().find(|p| p.0 == r)).map_or([0.0; 2], |p| {
                    crate::minecart::move_intent(
                        e.input & IN_FORWARD != 0,
                        e.input & IN_BACK != 0,
                        e.input & IN_LEFT != 0,
                        e.input & IN_RIGHT != 0,
                        p.2,
                    )
                });
                let step = e.cart.tick(&track, intent);
                match step.rail {
                    Some((pos, Rail { kind: Kind::Detector, .. })) => pressed.push(pos),
                    // `Minecart.activateMinecart`: включённый активатор высаживает.
                    Some((_, Rail { kind: Kind::Activator(true), .. })) => {
                        if let Some((rider, _)) = e.rider.take() {
                            ejected.push((id, rider));
                        }
                    }
                    _ => {}
                }
                // Под мир — исчезнуть (`checkBelowWorld`).
                if e.cart.pos[1] < self.min_y() as f64 - 64.0 {
                    ejected.push((id, Uuid::nil()));
                    continue;
                }
                // `ServerEntity.sendChanges`: сдвиг шлётся от квадрата длины
                // 7.6293945e-6, иначе затухающая на 0.96 вагонетка слала бы
                // крошечные шаги бесконечно.
                let d: f64 = (0..3).map(|i| (e.cart.pos[i] - e.sent.0[i]).powi(2)).sum();
                let moved = d >= 7.629_394_5e-6 || (e.cart.yaw - e.sent.1).abs() > 0.5;
                if moved {
                    e.sent = (e.cart.pos, e.cart.yaw);
                    packets.push(play::teleport_entity(
                        self.ids.teleport_entity,
                        id,
                        e.cart.pos,
                        e.cart.yaw,
                        0.0,
                        e.cart.on_ground,
                    ));
                    if let Some((rider, _)) = e.rider {
                        riders.push((rider, e.cart.pos));
                    }
                }
            }
            for &(id, rider) in &ejected {
                if rider.is_nil() {
                    carts.remove(&id);
                }
            }
        }
        for (id, rider) in ejected {
            if rider.is_nil() {
                packets.push(play::remove_entities(self.ids.remove_entities, &[id]));
            } else {
                self.unlink_rider(rider);
                packets.push(play::set_passengers(self.ids.set_passengers, id, &[]));
            }
        }
        if !packets.is_empty() {
            self.hub.send_many(packets);
        }
        // Водитель едет с вагонеткой: сессия по этой позиции грузит чанки.
        for (uuid, pos) in riders {
            self.moved(uuid, Move { pos: Some(pos), rot: None, on_ground: true });
        }
        for pos in pressed {
            self.press_detector_rail(pos);
        }
    }

    /// `DetectorRailBlock.entityInside` → `checkPressed`: включить и
    /// перепроверять раз в 20 тиков.
    fn press_detector_rail(&self, pos: [i32; 3]) {
        let pl = &self.survival.placement;
        let Some(s) = self.block(pos) else { return };
        if !crate::redstone::is_detector_rail(self.state_name(s)) || pl.prop(s, "powered") == Some("true") {
            return;
        }
        if !self.detector_rail_pressed(pos) {
            return;
        }
        if let Some(on) = pl.with_prop(s, "powered", "true") {
            self.set_block(pos, on);
            let mut around = crate::redstone::around(pos).to_vec();
            around.extend_from_slice(&crate::redstone::around([pos[0], pos[1] - 1, pos[2]]));
            self.redstone_update(&around);
            self.schedule(pos, crate::redstone::DETECTOR_RAIL_CHECK_PERIOD);
        }
    }

    /// Есть ли вагонетка в рамке детектора (`getSearchBB`).
    pub(super) fn minecart_in_box(&self, min: [f64; 3], max: [f64; 3]) -> bool {
        self.sim().minecarts.lock().unwrap().values().any(|e| {
            let (lo, hi) = e.bounds();
            (0..3).all(|i| lo[i] < max[i] && hi[i] > min[i])
        })
    }

    /// Вагонетки из `entities/*.mca`.
    pub(super) fn load_minecart(&self, c: &Compound) -> bool {
        if !matches!(c.get("id"), Some(Tag::String(id)) if id == ITEM) {
            return false;
        }
        let list = |k: &str| match c.get(k) {
            Some(Tag::List(l)) => l
                .iter()
                .filter_map(|t| match t {
                    Tag::Double(v) => Some(*v),
                    Tag::Float(v) => Some(*v as f64),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        };
        let pos = list("Pos");
        if pos.len() != 3 || !pos.iter().all(|v| v.is_finite()) {
            return false;
        }
        let yaw = list("Rotation").first().copied().unwrap_or(0.0) as f32;
        let mut cart = Cart::new([pos[0], pos[1], pos[2]], yaw);
        if let [x, y, z] = list("Motion")[..]
            && [x, y, z].iter().all(|v| v.is_finite())
        {
            cart.vel = [x, y, z];
        }
        let mut e = MinecartEntry::new(cart);
        if let Some(Tag::IntArray(u)) = c.get("UUID")
            && u.len() == 4
        {
            e.uuid = Uuid::from_u128(u.iter().fold(0u128, |acc, &w| acc << 32 | w as u32 as u128));
        }
        if let Some(Tag::String(n)) = c.get("CustomName") {
            e.custom_name = Some(n.clone());
        }
        self.add_minecart(e);
        true
    }

    /// Вагонетки для записи, по чанкам.
    pub(super) fn minecarts_nbt(&self) -> Vec<((i32, i32), Compound)> {
        self.sim()
            .minecarts
            .lock()
            .unwrap()
            .values()
            .map(|e| {
                let c0 = &e.cart;
                let mut c = Compound::new();
                c.insert("id".into(), Tag::String(ITEM.into()));
                c.insert("Pos".into(), Tag::List(c0.pos.iter().map(|v| Tag::Double(*v)).collect()));
                c.insert("Motion".into(), Tag::List(c0.vel.iter().map(|v| Tag::Double(*v)).collect()));
                c.insert("Rotation".into(), Tag::List(vec![Tag::Float(c0.yaw), Tag::Float(0.0)]));
                let u = e.uuid.as_u128();
                c.insert(
                    "UUID".into(),
                    Tag::IntArray((0..4).rev().map(|i| (u >> (32 * i)) as u32 as i32).collect()),
                );
                if let Some(n) = &e.custom_name {
                    c.insert("CustomName".into(), Tag::String(n.clone()));
                }
                ((c0.pos[0].floor() as i32 >> 4, c0.pos[2].floor() as i32 >> 4), c)
            })
            .collect()
    }
}
