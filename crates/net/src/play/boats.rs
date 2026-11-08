//! Лодки и плоты (`AbstractBoat`, `VehicleEntity`, `BoatItem`).
//!
//! Как у ванили, лодкой с водителем правит его клиент: он считает плавучесть
//! и вёсла и шлёт `move_vehicle`, а сервер принимает позицию, рассылает её
//! остальным и тянет за ней игрока (подгрузку чанков). Пустая лодка стоит на
//! месте: дрейф по течению и падение без водителя не перенесены.

use super::*;

/// `EntityType.BOAT`: ширина и высота коробки.
const WIDTH: f64 = 1.375;
const HEIGHT: f64 = 0.5625;
/// `VehicleEntity.hurtServer`: урон за удар множится на 10, больше 40 — сломана.
const DAMAGE_SCALE: f32 = 10.0;
const BREAK_AT: f32 = 40.0;
/// `setHurtTime(10)`.
const HURT_TIME: i32 = 10;

pub(crate) struct Boat {
    /// Имя типа сущности, оно же предмет: `minecraft:oak_boat`, `minecraft:bamboo_chest_raft`.
    pub kind: String,
    pub uuid: Uuid,
    pub pos: [f64; 3],
    pub yaw: f32,
    /// Накопленный урон (`DATA_ID_DAMAGE`) на тик `hit_at`; дальше −1 за тик.
    damage: f32,
    hit_at: i64,
    hurt_dir: i32,
    paddles: (bool, bool),
    /// Пассажиры: UUID и ID сущности игрока. Первый — водитель.
    riders: Vec<(Uuid, i32)>,
    custom_name: Option<String>,
}

impl Boat {
    fn new(kind: String, pos: [f64; 3], yaw: f32) -> Self {
        Self {
            kind,
            uuid: Uuid::from_u128(rand::random()),
            pos,
            yaw,
            damage: 0.0,
            hit_at: 0,
            hurt_dir: 1,
            paddles: (false, false),
            riders: Vec::new(),
            custom_name: None,
        }
    }

    /// `getMaxPassengers`: в лодке с сундуком место одно.
    fn seats(&self) -> usize {
        if self.kind.contains("_chest_") { 1 } else { 2 }
    }

    /// Урон сейчас: ваниль снимает по единице каждый тик (`AbstractBoat.tick`).
    fn damage_at(&self, now: i64) -> f32 {
        (self.damage - (now - self.hit_at).max(0) as f32).max(0.0)
    }
}

/// Предмет лодки или плота (`BoatItem`): имя типа сущности совпадает с предметом.
pub(super) fn is_boat_item(name: &str) -> bool {
    name.ends_with("_boat") || name.ends_with("_raft")
}

/// Точка, куда смотрит луч `ClipContext.Fluid.ANY`: грань блока; сверху —
/// высота `top` внутри клетки (верх формы или `FluidState.getHeight`:
/// источник 8/9, течение (8−уровень)/9).
fn hit_point(eye: [f64; 3], dir: [f64; 3], block: [i32; 3], face: usize, top: Option<f64>) -> [f64; 3] {
    let axis = match face {
        0 | 1 => 1,
        2 | 3 => 2,
        _ => 0,
    };
    let plane = match (axis, face, top) {
        (1, 1, Some(h)) => block[1] as f64 + h,
        (_, 0 | 2 | 4, _) => block[axis] as f64,
        _ => block[axis] as f64 + 1.0,
    };
    let t = if dir[axis].abs() < 1e-9 {
        0.0
    } else {
        ((plane - eye[axis]) / dir[axis]).max(0.0)
    };
    let mut p = [eye[0] + dir[0] * t, eye[1] + dir[1] * t, eye[2] + dir[2] * t];
    p[axis] = plane;
    p
}

/// `Entity.getCollisionHorizontalEscapeVector`: шаг в сторону взгляда за
/// край коробки транспорта.
fn escape_vector(vehicle_width: f64, passenger_width: f64, yaw: f32) -> [f64; 2] {
    let d = (vehicle_width + passenger_width + 1.0e-5) / 2.0;
    let r = (yaw as f64).to_radians();
    let (f, g) = (-r.sin(), r.cos());
    let h = f.abs().max(g.abs());
    [f * d / h, g * d / h]
}

impl PlayPlan {
    fn boat_type(&self, kind: &str) -> Option<i32> {
        self.survival.entity_types.get(kind).copied()
    }

    fn boat_packets(&self, id: i32, b: &Boat, now: i64) -> Vec<Vec<u8>> {
        let Some(ty) = self.boat_type(&b.kind) else {
            return Vec::new();
        };
        let mut out = vec![
            play::add_entity_moving(self.ids.add_entity, id, b.uuid, ty, b.pos, [0.0; 3], b.yaw, 0.0, 0),
            inv::boat_data(self.ids.set_entity_data, id, 0, b.hurt_dir, b.damage_at(now), b.paddles),
        ];
        if !b.riders.is_empty() {
            let ids: Vec<i32> = b.riders.iter().map(|r| r.1).collect();
            out.push(play::set_passengers(self.ids.set_passengers, id, &ids));
        }
        out
    }

    /// Все лодки для зашедшего игрока (после игроков: пассажиры уже известны).
    pub(super) fn boat_frames(&self) -> Vec<u8> {
        let now = self.game_time.load(Ordering::Relaxed);
        let mut out = Vec::new();
        for (&id, b) in self.sim().boats.lock().unwrap().iter() {
            for p in self.boat_packets(id, b, now) {
                out.extend(self.hub.frame(&p));
            }
        }
        out
    }

    pub(super) fn is_boat(&self, id: i32) -> bool {
        self.sim().boats.lock().unwrap().contains_key(&id)
    }

    fn add_boat(&self, b: Boat) -> i32 {
        let id = self.next_entity_id.fetch_add(1, Ordering::Relaxed);
        let now = self.game_time.load(Ordering::Relaxed);
        let packets = self.boat_packets(id, &b, now);
        self.sim().boats.lock().unwrap().insert(id, b);
        self.hub.send_many(packets);
        id
    }

    /// `BoatItem.use`: луч с жидкостями на 5 блоков, лодка в точке попадания
    /// с поворотом игрока. Без места (`noCollision`) или мимо — `false`.
    pub(super) fn place_boat(&self, uuid: Uuid, item: &str) -> bool {
        if self.boat_type(item).is_none() {
            return false;
        }
        let Some((feet, yaw, pitch)) = self.player_at(uuid) else {
            return false;
        };
        let eye = eye_of(feet);
        let dir = crate::arrows::aim(yaw as f64, pitch as f64, 0.0);
        // Формы контура у нас нет: луч ловит жидкость или коробку коллизии,
        // поэтому трава и цветы пропускаются, а лодка ложится на землю под ними.
        let fluid = |s: u32| crate::fluids::state_fluid(&self.survival.placement, s).is_some();
        let Some((block, face)) = crate::fluids::trace(eye, dir, 5.0, |p| {
            self.block(p).is_some_and(|s| fluid(s) || self.blocks_boat(s))
        }) else {
            return false;
        };
        let state = self.block(block).unwrap_or(0);
        let top = if fluid(state) {
            match self.survival.placement.prop(state, "level").and_then(|l| l.parse::<u8>().ok()) {
                Some(l) if l >= 8 => 1.0,
                Some(l) if l > 0 => (8 - l) as f64 / 9.0,
                _ => 8.0 / 9.0,
            }
        } else {
            self.survival.physics.as_deref().map_or(1.0, |p| {
                p.boxes_of(state).iter().map(|b| b[4]).fold(0.0, f64::max)
            })
        };
        let at = hit_point(eye, dir, block, face, Some(top));
        // `level.noCollision(boat, box.deflate(0.1))`: над точкой не должно быть твёрдого.
        let above = [at[0].floor() as i32, (at[1] + 0.1).floor() as i32, at[2].floor() as i32];
        if self.block(above).is_some_and(|s| self.blocks_boat(s)) {
            return false;
        }
        self.add_boat(Boat::new(item.to_owned(), at, yaw));
        true
    }

    /// `BoatDispenseItemBehavior.execute`: лодка на `0.5625 + ширина/2` перед
    /// раздатчиком, по высоте `центр + шагY·1.125`; вода спереди — ещё +1,
    /// воздух с водой под ним — +0, иначе обычный выброс (`None`). Поворот —
    /// сторона раздатчика (`Direction.toYRot`).
    ///
    /// Лодку без седока у нас не ведёт физика, поэтому она сразу ложится на
    /// поверхность воды, куда ванильная доплывает за пару тиков.
    pub(super) fn dispense_boat(&self, item: &str, dispenser: [i32; 3], facing: usize) -> Option<bool> {
        self.boat_type(item)?;
        let d = crate::placement::step(facing);
        let front = [dispenser[0] + d[0], dispenser[1] + d[1], dispenser[2] + d[2]];
        let water = |p: [i32; 3]| {
            self.block(p)
                .and_then(|s| crate::fluids::state_fluid(&self.survival.placement, s))
                .filter(|f| f.kind == crate::fluids::Kind::Water)
        };
        let below = [front[0], front[1] - 1, front[2]];
        let lift = if water(front).is_some() {
            1.0
        } else if self.block(front).is_some_and(|s| self.state_name(s).ends_with(":air")) && water(below).is_some() {
            0.0
        } else {
            return None;
        };
        // Ширина `EntityType` лодок и плотов — 1.375.
        let reach = 0.5625 + 1.375 / 2.0;
        let mut at = [
            dispenser[0] as f64 + 0.5 + reach * d[0] as f64,
            dispenser[1] as f64 + 0.5 + (d[1] as f32 * 1.125) as f64 + lift,
            dispenser[2] as f64 + 0.5 + reach * d[2] as f64,
        ];
        // Поверхность столба воды под точкой (`FluidState.getHeight`).
        let mut top = if lift > 0.0 { front } else { below };
        top[0] = at[0].floor() as i32;
        top[2] = at[2].floor() as i32;
        if water(top).is_some() {
            while water([top[0], top[1] + 1, top[2]]).is_some() {
                top[1] += 1;
            }
            if let Some(f) = water(top) {
                at[1] = top[1] as f64 + if f.source { 8.0 / 9.0 } else { f.amount as f64 / 9.0 };
            }
        }
        let yaw = match facing {
            2 => 180.0,
            4 => 90.0,
            5 => -90.0,
            _ => 0.0,
        };
        self.add_boat(Boat::new(item.to_owned(), at, yaw));
        Some(true)
    }

    /// Твёрдый блок для коробки лодки: не воздух, не жидкость, не трава и не цветы.
    pub(super) fn blocks_boat(&self, state: u32) -> bool {
        let name = self.state_name(state);
        if name.ends_with(":air") || crate::fluids::state_fluid(&self.survival.placement, state).is_some() {
            return false;
        }
        match self.survival.physics.as_deref() {
            Some(p) => !p.boxes_of(state).is_empty(),
            None => true,
        }
    }

    /// `AbstractBoat.interact` → `startRiding`: есть место и игрок ещё не сидит.
    pub(super) fn mount_boat(&self, uuid: Uuid, entity_id: i32, boat: i32) -> bool {
        let (riders, boat_uuid) = {
            let mut boats = self.sim().boats.lock().unwrap();
            let Some(b) = boats.get_mut(&boat) else {
                return false;
            };
            if b.riders.len() >= b.seats() || b.riders.iter().any(|r| r.0 == uuid) {
                return false;
            }
            b.riders.push((uuid, entity_id));
            (b.riders.iter().map(|r| r.1).collect::<Vec<_>>(), b.uuid)
        };
        self.link_rider(uuid, boat_uuid);
        self.send_passengers(boat, &riders);
        true
    }

    /// Снять игрока с лодки (`stopRiding`). Возвращает место высадки
    /// (`getDismountLocationForPassenger`), если он в ней был.
    pub(super) fn leave_boat(&self, uuid: Uuid, yaw: f32) -> Option<[f64; 3]> {
        let (boat, pos, riders) = {
            let mut boats = self.sim().boats.lock().unwrap();
            let (&id, b) = boats.iter_mut().find(|(_, b)| b.riders.iter().any(|r| r.0 == uuid))?;
            b.riders.retain(|r| r.0 != uuid);
            if b.riders.is_empty() {
                b.paddles = (false, false);
            }
            (id, b.pos, b.riders.iter().map(|r| r.1).collect::<Vec<_>>())
        };
        self.unlink_rider(uuid);
        self.send_passengers(boat, &riders);
        Some(self.dismount_spot(pos, yaw))
    }

    /// `AbstractBoat.getDismountLocationForPassenger`: шаг за борт по взгляду;
    /// над водой или без опоры — на лодку сверху (`Entity` по умолчанию).
    fn dismount_spot(&self, boat: [f64; 3], yaw: f32) -> [f64; 3] {
        let fallback = [boat[0], boat[1] + HEIGHT, boat[2]];
        let v = escape_vector(WIDTH * std::f64::consts::SQRT_2, 0.6, yaw);
        let (x, z) = (boat[0] + v[0], boat[2] + v[1]);
        let below = [x.floor() as i32, (boat[1] + HEIGHT).floor() as i32 - 1, z.floor() as i32];
        let state = |p: [i32; 3]| self.block(p).unwrap_or(0);
        if crate::fluids::state_fluid(&self.survival.placement, state(below)).is_some() {
            return fallback;
        }
        let free = |p: [i32; 3]| !self.blocks_boat(state(p));
        for floor in [below, [below[0], below[1] - 1, below[2]], [below[0], below[1] + 1, below[2]]] {
            let up = [floor[0], floor[1] + 1, floor[2]];
            let head = [floor[0], floor[1] + 2, floor[2]];
            if self.blocks_boat(state(floor)) && free(up) && free(head) {
                return [x, up[1] as f64, z];
            }
        }
        fallback
    }

    /// `ServerGamePacketListenerImpl.handleMoveVehicle`: водитель двигает свою лодку.
    /// Возвращает позицию лодки, если `uuid` — её водитель.
    pub(super) fn drive_boat(&self, uuid: Uuid, pos: [f64; 3], yaw: f32) -> Option<[f64; 3]> {
        if !pos.iter().all(|v| v.is_finite()) {
            return None;
        }
        let now = self.game_time.load(Ordering::Relaxed);
        let border = self.border_at(now);
        let (id, pos, corrected) = {
            let mut boats = self.sim().boats.lock().unwrap();
            let (&id, b) = boats.iter_mut().find(|(_, b)| b.riders.first().is_some_and(|r| r.0 == uuid))?;
            let clipped = border.clip_movement(now, b.pos, pos, WIDTH);
            let corrected = clipped != pos;
            let pos = clipped;
            b.pos = pos;
            b.yaw = yaw;
            (id, pos, corrected)
        };
        self.hub.send(
            if corrected { None } else { Some(uuid) },
            play::teleport_entity(self.ids.teleport_entity, id, pos, yaw, 0.0, false),
        );
        Some(pos)
    }

    /// `handlePaddleBoat`: вёсла водителя видят остальные.
    pub(super) fn paddle_boat(&self, uuid: Uuid, left: bool, right: bool) {
        let now = self.game_time.load(Ordering::Relaxed);
        let packet = {
            let mut boats = self.sim().boats.lock().unwrap();
            let Some((&id, b)) = boats.iter_mut().find(|(_, b)| b.riders.first().is_some_and(|r| r.0 == uuid)) else {
                return;
            };
            if b.paddles == (left, right) {
                return;
            }
            b.paddles = (left, right);
            inv::boat_data(self.ids.set_entity_data, id, 0, b.hurt_dir, b.damage_at(now), b.paddles)
        };
        self.hub.send(Some(uuid), packet);
    }

    /// Удар по лодке (`VehicleEntity.hurtServer`). `None` — это не лодка.
    /// В творческом лодка исчезает с первого удара без дропа, иначе ломается
    /// при уроне больше 40 и падает предметом (`entity_drops`).
    pub(super) fn attack_boat(&self, uuid: Uuid, target: i32, held: Slot, creative: bool, crit: bool) -> Option<()> {
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
        let mut boats = self.sim().boats.lock().unwrap();
        let b = boats.get_mut(&target)?;
        let d: f64 = (0..3).map(|i| (b.pos[i] - pos[i]).powi(2)).sum();
        if d > 36.0 {
            return Some(());
        }
        b.hurt_dir = -b.hurt_dir;
        b.damage = b.damage_at(now) + amount * DAMAGE_SCALE;
        b.hit_at = now;
        let broken = !creative && b.damage > BREAK_AT;
        if !broken && !creative {
            let packet = inv::boat_data(self.ids.set_entity_data, target, HURT_TIME, b.hurt_dir, b.damage, b.paddles);
            drop(boats);
            self.hub.send(None, packet);
            return Some(());
        }
        let b = boats.remove(&target)?;
        drop(boats);
        self.remove_boat(target, &b);
        if broken && self.vars.flag("entity_drops")
            && let Some(item) = self.item_id(&b.kind)
        {
            let vel = items::pop_velocity(rand::random(), rand::random());
            self.drop_item(b.pos, vel, Stack::new(item, 1), items::BLOCK_DELAY);
        }
        Some(())
    }

    /// Лодка исчезла: пассажиры сходят, клиенты её забывают.
    fn remove_boat(&self, id: i32, b: &Boat) {
        for r in &b.riders {
            self.unlink_rider(r.0);
        }
        self.hub.send(None, play::remove_entities(self.ids.remove_entities, &[id]));
    }

    /// Лодки чанка `entities/*.mca` (`id` — тип лодки), остальное — мобам.
    pub(super) fn load_boat(&self, c: &Compound) -> bool {
        let Some(Tag::String(kind)) = c.get("id") else {
            return false;
        };
        if !self.survival.entity_types.contains_key(kind) || !is_boat_item(kind) {
            return false;
        }
        let doubles = |k: &str| match c.get(k) {
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
        let pos = doubles("Pos");
        if pos.len() != 3 || !pos.iter().all(|v| v.is_finite()) {
            return false;
        }
        let yaw = doubles("Rotation").first().copied().unwrap_or(0.0) as f32;
        let mut b = Boat::new(kind.clone(), [pos[0], pos[1], pos[2]], yaw);
        if let Some(Tag::IntArray(u)) = c.get("UUID")
            && u.len() == 4
        {
            b.uuid = Uuid::from_u128(u.iter().fold(0u128, |acc, &w| acc << 32 | w as u32 as u128));
        }
        if let Some(Tag::String(n)) = c.get("CustomName") {
            b.custom_name = Some(n.clone());
        }
        self.add_boat(b);
        true
    }

    /// Лодки для записи `entities/*.mca`, по чанкам.
    pub(super) fn boats_nbt(&self) -> Vec<((i32, i32), Compound)> {
        self.sim()
            .boats
            .lock()
            .unwrap()
            .values()
            .map(|b| {
                let mut c = Compound::new();
                c.insert("id".into(), Tag::String(b.kind.clone()));
                c.insert("Pos".into(), Tag::List(b.pos.iter().map(|v| Tag::Double(*v)).collect()));
                c.insert("Motion".into(), Tag::List(vec![Tag::Double(0.0); 3]));
                c.insert("Rotation".into(), Tag::List(vec![Tag::Float(b.yaw), Tag::Float(0.0)]));
                let u = b.uuid.as_u128();
                c.insert(
                    "UUID".into(),
                    Tag::IntArray((0..4).rev().map(|i| (u >> (32 * i)) as u32 as i32).collect()),
                );
                if let Some(n) = &b.custom_name {
                    c.insert("CustomName".into(), Tag::String(n.clone()));
                }
                let chunk = ((b.pos[0].floor() as i32) >> 4, (b.pos[2].floor() as i32) >> 4);
                (chunk, c)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_vector_steps_past_the_boat() {
        // Взгляд на юг (+z): шаг (0.6 + 1.375·√2)/2 по z.
        let v = escape_vector(WIDTH * std::f64::consts::SQRT_2, 0.6, 0.0);
        assert!(v[0].abs() < 1e-9);
        assert!((v[1] - 1.2722).abs() < 1e-3, "{v:?}");
        // По диагонали шаг растягивается до угла квадрата.
        let d = escape_vector(1.0, 0.0, 45.0);
        assert!((d[0] + 0.5).abs() < 1e-4 && (d[1] - 0.5).abs() < 1e-4, "{d:?}");
    }

    #[test]
    fn hit_point_lands_on_the_water_surface() {
        // Сверху вниз по диагонали в источник на y=62: точка на 62 + 8/9.
        let eye = [0.5, 65.0, 0.5];
        let dir = [0.0, -0.6, 0.8];
        let p = hit_point(eye, dir, [0, 62, 2], 1, Some(8.0 / 9.0));
        assert!((p[1] - (62.0 + 8.0 / 9.0)).abs() < 1e-9);
        let t = (p[1] - eye[1]) / dir[1];
        assert!((p[2] - (eye[2] + dir[2] * t)).abs() < 1e-9);
        // В бок блока: x на грани.
        let q = hit_point([0.5, 64.5, 0.5], [1.0, 0.0, 0.0], [3, 64, 0], 4, None);
        assert_eq!(q, [3.0, 64.5, 0.5]);
    }

    #[test]
    fn damage_wears_off_by_one_per_tick() {
        let mut b = Boat::new("minecraft:oak_boat".into(), [0.0; 3], 0.0);
        b.damage = 30.0;
        b.hit_at = 100;
        assert_eq!(b.damage_at(110), 20.0);
        assert_eq!(b.damage_at(200), 0.0);
        assert_eq!(b.seats(), 2);
        b.kind = "minecraft:oak_chest_boat".into();
        assert_eq!(b.seats(), 1);
    }
}
