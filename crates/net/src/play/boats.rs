//! Лодки и плоты (`AbstractBoat`, `VehicleEntity`, `BoatItem`).
//!
//! Пустая лодка живёт на сервере: `AbstractBoat.tick`/`floatBoat` —
//! гравитация, плавучесть, трение о воду/землю, статусы
//! `IN_WATER`/`UNDER_WATER`/`UNDER_FLOWING_WATER`/`IN_AIR`/`ON_LAND`,
//! движение `move(MoverType.SELF, ...)` со столкновениями и рассылка
//! сдвига. Лодкой с водителем правит его клиент (`move_vehicle`), как у
//! ванили: сервер принимает позицию, рассылает её остальным и тянет за
//! ней игрока (подгрузку чанков).

use super::*;

/// `EntityType.BOAT`: ширина и высота коробки.
pub(super) const WIDTH: f64 = 1.375;
pub(super) const HEIGHT: f64 = 0.5625;
/// `VehicleEntity.hurtServer`: урон за удар множится на 10, больше 40 — сломана.
const DAMAGE_SCALE: f32 = 10.0;
const BREAK_AT: f32 = 40.0;
/// `setHurtTime(10)`.
const HURT_TIME: i32 = 10;

pub(crate) struct Boat {
    /// Имя типа сущности, оно же предмет: `minecraft:oak_boat`, `minecraft:bamboo_chest_raft`.
    pub kind: String,
    /// 27 слотов сундука (только `*_chest_boat*`/`*_chest_raft`).
    pub slots: Vec<Slot>,
    pub uuid: Uuid,
    pub pos: [f64; 3],
    pub yaw: f32,
    /// Накопленный урон (`DATA_ID_DAMAGE`) на тик `hit_at`; дальше −1 за тик.
    damage: f32,
    hit_at: i64,
    hurt_dir: i32,
    paddles: (bool, bool),
    /// Скорость (`getDeltaMovement`).
    pub(super) vel: [f64; 3],
    /// `onGround` после последнего `move`.
    on_ground: bool,
    /// Статус плавучести (`getStatus`) и прошлого тика.
    status: Status,
    old_status: Status,
    /// Верх воды под лодкой и трение земли (`checkInWater`, `getGroundFriction`).
    water_level: f64,
    land_friction: f32,
    /// Вертикальный сдвиг прошлого тика (`checkFallDamage`).
    last_yd: f64,
    /// Тики под водой (`outOfControlTicks`): на 60 пассажиры сходят.
    out_of_control: f32,
    /// Последняя разосланная позиция и поворот (`ServerEntity.sendChanges`).
    sent: ([f64; 3], f32),
    /// Пассажиры: UUID и ID сущности игрока. Первый — водитель.
    riders: Vec<(Uuid, i32)>,
    /// Поводок (`Leashable`).
    pub leash: Option<crate::leash::Leash>,
    custom_name: Option<String>,
}

/// `AbstractBoat.Status`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Status {
    InWater,
    UnderWater,
    UnderFlowingWater,
    InAir,
    OnLand,
}

impl Boat {
    fn new(kind: String, pos: [f64; 3], yaw: f32) -> Self {
        let size = if kind.contains("_chest_") { 27 } else { 0 };
        Self {
            kind,
            slots: vec![None; size],
            uuid: Uuid::from_u128(rand::random()),
            pos,
            yaw,
            damage: 0.0,
            hit_at: 0,
            hurt_dir: 1,
            paddles: (false, false),
            vel: [0.0; 3],
            on_ground: false,
            status: Status::InAir,
            old_status: Status::InAir,
            water_level: f64::NEG_INFINITY,
            land_friction: 0.6,
            last_yd: 0.0,
            out_of_control: 0.0,
            sent: (pos, yaw),
            riders: Vec::new(),
            leash: None,
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
fn hit_point(
    eye: [f64; 3],
    dir: [f64; 3],
    block: [i32; 3],
    face: usize,
    top: Option<f64>,
) -> [f64; 3] {
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
    let mut p = [
        eye[0] + dir[0] * t,
        eye[1] + dir[1] * t,
        eye[2] + dir[2] * t,
    ];
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

/// `AbstractChestBoat.interact`: с сундуком ПКМ открывает контейнер.
pub(super) enum BoatUse {
    Ride,
    /// Размер контейнера и заголовок окна.
    Open(usize, serde_json::Value),
}

impl PlayPlan {
    pub(super) fn boat_type(&self, kind: &str) -> Option<i32> {
        self.survival.entity_types.get(kind).copied()
    }

    /// `AbstractChestBoat.interact`: лодка с сундуком открывает окно.
    pub(super) fn boat_use(&self, id: i32) -> BoatUse {
        let boats = self.sim().boats.lock().unwrap();
        let Some(b) = boats.get(&id).filter(|b| !b.slots.is_empty()) else {
            return BoatUse::Ride;
        };
        let name = b.kind.strip_prefix("minecraft:").unwrap_or(&b.kind);
        let title = b
            .custom_name
            .as_deref()
            .map(|n| serde_json::from_str(n).unwrap_or_else(|_| serde_json::json!({ "text": n })))
            .unwrap_or_else(
                || serde_json::json!({ "translate": format!("entity.minecraft.{name}") }),
            );
        BoatUse::Open(b.slots.len(), title)
    }

    /// `AbstractBoat.canAddPassenger`: мест больше нет.
    pub(super) fn boat_full(&self, id: i32) -> bool {
        self.sim()
            .boats
            .lock()
            .unwrap()
            .get(&id)
            .is_some_and(|b| b.riders.len() >= b.seats())
    }

    /// Слоты сундука лодки (только у `*_chest_*`).
    pub(super) fn boat_slots(&self, id: i32) -> Option<Vec<Slot>> {
        self.sim()
            .boats
            .lock()
            .unwrap()
            .get(&id)
            .filter(|b| !b.slots.is_empty())
            .map(|b| b.slots.clone())
    }

    pub(super) fn set_boat_slots(&self, id: i32, slots: &[Slot]) {
        let (changed, pos) = {
            let mut boats = self.sim().boats.lock().unwrap();
            match boats.get_mut(&id) {
                Some(b) if b.slots.len() == slots.len() => {
                    b.slots = slots.to_vec();
                    (true, b.pos)
                }
                _ => (false, [0.0; 3]),
            }
        };
        if changed {
            self.mark_entity_changed(pos);
            self.mark_work();
        }
    }

    /// `HopperBlockEntity.getContainerAt`: сундук лодки в клетке `p`.
    pub(super) fn boat_cont_at(&self, p: [i32; 3]) -> Option<Cont> {
        let lo = p.map(f64::from);
        let hi = lo.map(|v| v + 1.0);
        let boats = self.sim().boats.lock().unwrap();
        let (&id, b) = boats.iter().find(|(_, b)| {
            if b.slots.is_empty() {
                return false;
            }
            let h = WIDTH / 2.0;
            let a = [b.pos[0] - h, b.pos[1], b.pos[2] - h];
            let z = [b.pos[0] + h, b.pos[1] + HEIGHT, b.pos[2] + h];
            (0..3).all(|i| a[i] < hi[i] && z[i] > lo[i])
        })?;
        Some(Cont {
            parts: vec![boat_pos(id)],
            slots: b.slots.clone(),
            furnace: false,
            name: b.kind.clone(),
        })
    }

    fn boat_packets(&self, id: i32, b: &Boat, now: i64) -> Vec<Vec<u8>> {
        let Some(ty) = self.boat_type(&b.kind) else {
            return Vec::new();
        };
        let mut out = vec![
            play::add_entity_moving(
                self.ids.add_entity,
                id,
                b.uuid,
                ty,
                b.pos,
                [0.0; 3],
                b.yaw,
                0.0,
                0,
            ),
            inv::boat_data(
                self.ids.set_entity_data,
                id,
                0,
                b.hurt_dir,
                b.damage_at(now),
                b.paddles,
            ),
        ];
        if !b.riders.is_empty() {
            let ids: Vec<i32> = b.riders.iter().map(|r| r.1).collect();
            out.push(play::set_passengers(self.ids.set_passengers, id, &ids));
        }
        out
    }

    fn boat_tracked(&self, id: i32, b: &Boat) -> Tracked {
        let ty = self.boat_type(&b.kind).unwrap_or_default();
        // `getEffectiveRange`: не меньше дальности пассажиров — игрок 32 чанка.
        let range = if b.riders.is_empty() {
            self.tracking_range(ty)
        } else {
            self.tracking_range(ty).max(PLAYER_TRACKING_RANGE)
        };
        Tracked {
            id,
            pos: b.pos,
            range,
        }
    }

    pub(super) fn track_boats(&self, out: &mut Vec<Tracked>) {
        for (&id, b) in self.sim().boats.lock().unwrap().iter() {
            out.push(self.boat_tracked(id, b));
        }
    }

    /// Пакеты спавна лодки для нового зрителя (пассажиры — `set_passengers`).
    pub(super) fn boat_spawn(&self, id: i32) -> Option<Vec<Vec<u8>>> {
        let now = self.game_time.load(Ordering::Relaxed);
        let boats = self.sim().boats.lock().unwrap();
        Some(self.boat_packets(id, boats.get(&id)?, now))
    }

    pub(super) fn is_boat(&self, id: i32) -> bool {
        self.sim().boats.lock().unwrap().contains_key(&id)
    }

    /// `ServerEntity.sendPairingData` для пассажира: клиент, впервые
    /// увидевший моба, получает и список пассажиров его лодки.
    pub(super) fn mob_vehicle_passengers(&self, id: i32) -> Option<Vec<u8>> {
        // Порядок замков как в `tick_boats`: лодки, затем мобы.
        let boats = self.sim().boats.lock().unwrap();
        let vehicle = self.sim().mobs.lock().unwrap().get(&id)?.vehicle?;
        let b = boats.get(&vehicle)?;
        let ids: Vec<i32> = b.riders.iter().map(|r| r.1).collect();
        Some(play::set_passengers(self.ids.set_passengers, vehicle, &ids))
    }

    /// `/summon minecraft:oak_boat`: лодка без предмета в руке.
    pub(super) fn summon_boat(&self, kind: &str, pos: [f64; 3]) -> Option<i32> {
        // В реестре есть все сущности: лодкой считаем только `*_boat`/`*_raft`.
        if !(kind.ends_with("_boat") || kind.ends_with("_raft")) {
            return None;
        }
        self.boat_type(kind)?;
        let id = self.add_boat(Boat::new(kind.to_owned(), pos, 0.0));
        self.mark_entity_changed(pos);
        Some(id)
    }

    fn add_boat(&self, b: Boat) -> i32 {
        let id = self.next_entity_id.fetch_add(1, Ordering::Relaxed);
        let now = self.game_time.load(Ordering::Relaxed);
        let packets = self.boat_packets(id, &b, now);
        let tracked = self.boat_tracked(id, &b);
        self.sim().boats.lock().unwrap().insert(id, b);
        self.hub.spawn_tracked(tracked, packets);
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
            self.block(p)
                .is_some_and(|s| fluid(s) || self.blocks_boat(s))
        }) else {
            return false;
        };
        let state = self.block(block).unwrap_or(0);
        let top = if fluid(state) {
            match self
                .survival
                .placement
                .prop(state, "level")
                .and_then(|l| l.parse::<u8>().ok())
            {
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
        let above = [
            at[0].floor() as i32,
            (at[1] + 0.1).floor() as i32,
            at[2].floor() as i32,
        ];
        if self.block(above).is_some_and(|s| self.blocks_boat(s)) {
            return false;
        }
        self.add_boat(Boat::new(item.to_owned(), at, yaw));
        self.mark_entity_changed(at);
        true
    }

    /// `BoatDispenseItemBehavior.execute`: лодка на `0.5625 + ширина/2` перед
    /// раздатчиком, по высоте `центр + шагY·1.125`; вода спереди — ещё +1,
    /// воздух с водой под ним — +0, иначе обычный выброс (`None`). Поворот —
    /// сторона раздатчика (`Direction.toYRot`).
    ///
    /// Лодку без седока у нас не ведёт физика, поэтому она сразу ложится на
    /// поверхность воды, куда ванильная доплывает за пару тиков.
    pub(super) fn dispense_boat(
        &self,
        item: &str,
        dispenser: [i32; 3],
        facing: usize,
    ) -> Option<bool> {
        self.boat_type(item)?;
        let d = crate::placement::step(facing);
        let front = [
            dispenser[0] + d[0],
            dispenser[1] + d[1],
            dispenser[2] + d[2],
        ];
        let water = |p: [i32; 3]| {
            self.block(p)
                .and_then(|s| crate::fluids::state_fluid(&self.survival.placement, s))
                .filter(|f| f.kind == crate::fluids::Kind::Water)
        };
        let below = [front[0], front[1] - 1, front[2]];
        let lift = if water(front).is_some() {
            1.0
        } else if self
            .block(front)
            .is_some_and(|s| self.state_name(s).ends_with(":air"))
            && water(below).is_some()
        {
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
                at[1] = top[1] as f64
                    + if f.source {
                        8.0 / 9.0
                    } else {
                        f.amount as f64 / 9.0
                    };
            }
        }
        let yaw = match facing {
            2 => 180.0,
            4 => 90.0,
            5 => -90.0,
            _ => 0.0,
        };
        self.add_boat(Boat::new(item.to_owned(), at, yaw));
        self.mark_entity_changed(at);
        Some(true)
    }

    /// Твёрдый блок для коробки лодки: не воздух, не жидкость, не трава и не цветы.
    pub(super) fn blocks_boat(&self, state: u32) -> bool {
        let name = self.state_name(state);
        if name.ends_with(":air")
            || crate::fluids::state_fluid(&self.survival.placement, state).is_some()
        {
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
            let (&id, b) = boats
                .iter_mut()
                .find(|(_, b)| b.riders.iter().any(|r| r.0 == uuid))?;
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
        let below = [
            x.floor() as i32,
            (boat[1] + HEIGHT).floor() as i32 - 1,
            z.floor() as i32,
        ];
        let state = |p: [i32; 3]| self.block(p).unwrap_or(0);
        if crate::fluids::state_fluid(&self.survival.placement, state(below)).is_some() {
            return fallback;
        }
        let free = |p: [i32; 3]| !self.blocks_boat(state(p));
        for floor in [
            below,
            [below[0], below[1] - 1, below[2]],
            [below[0], below[1] + 1, below[2]],
        ] {
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
        let (id, pos, corrected, moved) = {
            let mut boats = self.sim().boats.lock().unwrap();
            let (&id, b) = boats
                .iter_mut()
                .find(|(_, b)| b.riders.first().is_some_and(|r| r.0 == uuid))?;
            let clipped = border.clip_movement(now, b.pos, pos, WIDTH);
            let corrected = clipped != pos;
            let old = b.pos;
            let pos = clipped;
            let dx = pos[0] - old[0];
            let dy = pos[1] - old[1];
            let dz = pos[2] - old[2];
            let moved = (dx * dx + dy * dy + dz * dz).sqrt();
            b.pos = pos;
            b.yaw = yaw;
            (id, pos, corrected, moved)
        };
        let cm = (moved * 100.0).round() as i32;
        if cm > 0 {
            self.award_stat(uuid, "minecraft:custom", "minecraft:boat_one_cm", cm);
        }
        self.hub.send_about_except(
            if corrected { None } else { Some(uuid) },
            id,
            play::teleport_entity(self.ids.teleport_entity, id, pos, yaw, 0.0, false),
        );
        Some(pos)
    }

    /// `handlePaddleBoat`: вёсла водителя видят остальные.
    pub(super) fn paddle_boat(&self, uuid: Uuid, left: bool, right: bool) {
        let now = self.game_time.load(Ordering::Relaxed);
        let (id, packet) = {
            let mut boats = self.sim().boats.lock().unwrap();
            let Some((&id, b)) = boats
                .iter_mut()
                .find(|(_, b)| b.riders.first().is_some_and(|r| r.0 == uuid))
            else {
                return;
            };
            if b.paddles == (left, right) {
                return;
            }
            b.paddles = (left, right);
            (
                id,
                inv::boat_data(
                    self.ids.set_entity_data,
                    id,
                    0,
                    b.hurt_dir,
                    b.damage_at(now),
                    b.paddles,
                ),
            )
        };
        self.hub.send_about_except(Some(uuid), id, packet);
    }

    /// Удар по лодке (`VehicleEntity.hurtServer`). `None` — это не лодка.
    /// В творческом лодка исчезает с первого удара без дропа, иначе ломается
    /// при уроне больше 40 и падает предметом (`entity_drops`).
    pub(super) fn attack_boat(
        &self,
        uuid: Uuid,
        target: i32,
        held: Slot,
        creative: bool,
        hit: super::mobs::MeleeHit,
    ) -> Option<()> {
        let pos = self.player_at(uuid)?.0;
        let now = self.game_time.load(Ordering::Relaxed);
        let item = held.and_then(|s| self.survival.items.get(s.item as usize));
        let sharp = held.map_or(0, |s| {
            chunkwake_protocol::components::stack_level(s.extra, "minecraft:sharpness")
        });
        let bonus = if sharp > 0 {
            0.5 * sharp as f32 + 0.5
        } else {
            0.0
        };
        let amount = Self::melee_damage(
            1.0 + item.map_or(0.0, |i| i.attack),
            bonus,
            hit.scale,
            hit.crit,
        );
        let mut boats = self.sim().boats.lock().unwrap();
        let b = boats.get_mut(&target)?;
        let d: f64 = (0..3).map(|i| (b.pos[i] - pos[i]).powi(2)).sum();
        if d > 36.0 {
            return None;
        }
        b.hurt_dir = -b.hurt_dir;
        b.damage = b.damage_at(now) + amount * DAMAGE_SCALE;
        b.hit_at = now;
        let broken = !creative && b.damage > BREAK_AT;
        if !broken && !creative {
            let packet = inv::boat_data(
                self.ids.set_entity_data,
                target,
                HURT_TIME,
                b.hurt_dir,
                b.damage,
                b.paddles,
            );
            drop(boats);
            self.hub.send_about(target, packet);
            return Some(());
        }
        let b = boats.remove(&target)?;
        drop(boats);
        self.remove_boat(target, &b);
        self.mark_entity_changed(b.pos);
        if broken
            && self.vars.flag("entity_drops")
            && let Some(item) = self.item_id(&b.kind)
        {
            let vel = items::pop_velocity(rand::random(), rand::random());
            self.drop_item(b.pos, vel, Stack::new(item, 1), items::BLOCK_DELAY);
        }
        for stack in b.slots.iter().flatten() {
            let vel = items::pop_velocity(rand::random(), rand::random());
            self.drop_item(b.pos, vel, *stack, items::BLOCK_DELAY);
        }
        Some(())
    }

    /// Убрать пассажира из лодки (например, `/kill` моба) и вернуть
    /// актуальный список ID для `set_passengers`.
    pub(super) fn drop_boat_rider(&self, boat: i32, entity_id: i32) -> Option<Vec<i32>> {
        let mut boats = self.sim().boats.lock().unwrap();
        let b = boats.get_mut(&boat)?;
        b.riders.retain(|r| r.1 != entity_id);
        Some(b.riders.iter().map(|r| r.1).collect())
    }

    /// Лодка исчезла: пассажиры сходят, клиенты её забывают.
    pub(super) fn remove_boat(&self, id: i32, b: &Boat) {
        let mut spots = Vec::new();
        {
            let mut mobs = self.sim().mobs.lock().unwrap();
            for r in &b.riders {
                if let Some(e) = mobs.get_mut(&r.1) {
                    e.vehicle = None;
                    let yaw = e.mob.yaw;
                    let old = e.mob.pos;
                    let spot = self.dismount_spot(b.pos, yaw);
                    e.ride_to(spot, yaw);
                    spots.push(old);
                    spots.push(spot);
                    self.hub.send_about(
                        r.1,
                        play::teleport_entity(self.ids.teleport_entity, r.1, spot, yaw, 0.0, true),
                    );
                }
                self.unlink_rider(r.0);
            }
        }
        for pos in spots {
            self.mark_entity_changed(pos);
        }
        self.hub.remove_tracked(&[id]);
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
        let motion = doubles("Motion");
        if motion.len() == 3 && motion.iter().all(|v| v.is_finite()) {
            b.vel = [motion[0], motion[1], motion[2]];
        }
        if let Some(Tag::IntArray(u)) = c.get("UUID")
            && u.len() == 4
        {
            b.uuid = Uuid::from_u128(u.iter().fold(0u128, |acc, &w| acc << 32 | w as u32 as u128));
        }
        if let Some(Tag::String(n)) = c.get("CustomName") {
            b.custom_name = Some(n.clone());
        }
        if let Some(Tag::Compound(l)) = c.get("leash")
            && let Some(leash) = crate::leash::from_nbt(l)
        {
            b.leash = Some(leash);
        }
        if !b.slots.is_empty()
            && let Some(Tag::List(list)) = c.get("Items")
        {
            for t in list {
                let Some(item) = t.as_compound() else {
                    continue;
                };
                let slot = item.get("Slot").and_then(Tag::as_i64).unwrap_or(-1);
                if let Some(dst) = b.slots.get_mut(slot.max(0) as usize).filter(|_| slot >= 0) {
                    *dst = crate::stack_nbt::from_nbt(item, |n| self.item_id(n));
                }
            }
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
                c.insert(
                    "Pos".into(),
                    Tag::List(b.pos.iter().map(|v| Tag::Double(*v)).collect()),
                );
                c.insert(
                    "Motion".into(),
                    Tag::List(b.vel.iter().map(|v| Tag::Double(*v)).collect()),
                );
                c.insert(
                    "Rotation".into(),
                    Tag::List(vec![Tag::Float(b.yaw), Tag::Float(0.0)]),
                );
                let u = b.uuid.as_u128();
                c.insert(
                    "UUID".into(),
                    Tag::IntArray(
                        (0..4)
                            .rev()
                            .map(|i| (u >> (32 * i)) as u32 as i32)
                            .collect(),
                    ),
                );
                if let Some(n) = &b.custom_name {
                    c.insert("CustomName".into(), Tag::String(n.clone()));
                }
                if let Some(leash) = &b.leash {
                    c.insert("leash".into(), crate::leash::to_nbt(leash));
                }
                if !b.slots.is_empty() {
                    c.insert("Items".into(), Tag::List(self.items_nbt(&b.slots)));
                }
                let chunk = (
                    (b.pos[0].floor() as i32) >> 4,
                    (b.pos[2].floor() as i32) >> 4,
                );
                (chunk, c)
            })
            .collect()
    }

    /// Тик лодок в симулируемых чанках (`AbstractBoat.tick`): статус воды,
    /// `floatBoat`, движение `move(MoverType.SELF, ...)` со столкновениями и
    /// рассылка сдвига. Лодку с водителем двигает клиент; мобы-пассажиры
    /// едут с лодкой (`positionRider`), в конце тика лодка подбирает мобов и
    /// толкает задевающие сущности (`push`).
    pub(super) fn tick_boats(&self) {
        if self.sim().boats.lock().unwrap().is_empty() {
            return;
        }
        let simulated = self.simulated_set();
        let awake = |p: [f64; 3]| {
            simulated.is_empty()
                || simulated.contains(&((p[0].floor() as i32) >> 4, (p[2].floor() as i32) >> 4))
        };
        let players: FastMap<Uuid, f32> = self
            .hub
            .players
            .lock()
            .unwrap()
            .values()
            .map(|p| (p.entry.uuid, p.yaw))
            .collect();
        let now = self.game_time.load(Ordering::Relaxed);
        let mut packets = Vec::new();
        let mut gone = Vec::new();
        let mut marks = Vec::new();
        let mut player_ejects = Vec::new();
        self.with_world(|w| {
            let mut boats = self.sim().boats.lock().unwrap();
            let mut mobs = self.sim().mobs.lock().unwrap();
            let ids: Vec<i32> = boats.keys().copied().collect();
            for id in ids {
                let Some(b) = boats.get_mut(&id) else {
                    continue;
                };
                if !awake(b.pos) {
                    continue;
                }
                // В лодке с водителем физику считает его клиент
                // (`handleMoveVehicle`), сервер только принимает позицию.
                let player_controlled =
                    b.riders.first().is_some_and(|r| players.contains_key(&r.0));
                b.old_status = b.status;
                b.status = self.boat_status(b, w);
                b.out_of_control =
                    if matches!(b.status, Status::UnderWater | Status::UnderFlowingWater) {
                        b.out_of_control + 1.0
                    } else {
                        0.0
                    };
                // `AbstractBoat.tick`: три секунды под водой — пассажиры
                // сходят (`ejectPassengers`), в том числе игрок.
                if b.out_of_control >= 60.0 && !b.riders.is_empty() {
                    let riders: Vec<(Uuid, i32)> = std::mem::take(&mut b.riders);
                    for (uuid, rider) in riders {
                        let yaw = players.get(&uuid).copied().unwrap_or(b.yaw);
                        let spot = self.dismount_spot(b.pos, yaw);
                        if players.contains_key(&uuid) {
                            player_ejects.push((uuid, spot));
                        } else if let Some(e) = mobs.get_mut(&rider) {
                            e.vehicle = None;
                            e.ride_to(spot, yaw);
                            marks.push(spot);
                            packets.push((
                                rider,
                                play::teleport_entity(
                                    self.ids.teleport_entity,
                                    rider,
                                    spot,
                                    yaw,
                                    0.0,
                                    true,
                                ),
                            ));
                        }
                        self.unlink_rider(uuid);
                    }
                    packets.push((id, play::set_passengers(self.ids.set_passengers, id, &[])));
                }
                if !player_controlled {
                    // `Entity.baseTick` → `updateFluidInteraction`: течение
                    // добавляется к скорости до `floatBoat`.
                    self.apply_fluid_current(b, w);
                    self.float_boat(b, w);
                    let moved = self.move_boat(b, w, now);
                    // `checkFallDamage`: `lastYd` — фактический сдвиг по Y.
                    b.last_yd = moved[1];
                    // `checkBelowWorld`: под миром лодка исчезает.
                    if b.pos[1] < self.min_y() as f64 - 64.0 {
                        gone.push(id);
                        continue;
                    }
                    // `ServerEntity.sendChanges`: сдвиг от квадрата длины
                    // 7.6293945e-6, иначе затухающая лодка слала бы крошечные шаги.
                    let d: f64 = (0..3).map(|i| (b.pos[i] - b.sent.0[i]).powi(2)).sum();
                    if d >= 7.629_394_5e-6 || (b.yaw - b.sent.1).abs() > 0.5 {
                        b.sent = (b.pos, b.yaw);
                        packets.push((
                            id,
                            play::teleport_entity(
                                self.ids.teleport_entity,
                                id,
                                b.pos,
                                b.yaw,
                                0.0,
                                b.on_ground,
                            ),
                        ));
                    }
                }
                self.boat_riders(b, &players, &mut mobs, &mut packets, &mut marks);
                // `AbstractBoat.tick` в конце: посадка мобов и толчки.
                if let Some(b) = boats.get_mut(&id) {
                    self.boat_contacts(b, id, &players, &mut mobs, &mut packets, &mut marks);
                }
            }
            // Лодки толкают друг друга (`AbstractBoat.push` → `Entity.push`).
            let boxes: Vec<(i32, [f64; 3], [f64; 3])> = boats
                .iter()
                .filter(|(_, b)| awake(b.pos))
                .map(|(&id, b)| {
                    let (lo, hi) = b.bounds();
                    (
                        id,
                        [lo[0] - 0.2, lo[1] + 0.01, lo[2] - 0.2],
                        [hi[0] + 0.2, hi[1] - 0.01, hi[2] + 0.2],
                    )
                })
                .collect();
            for i in 0..boxes.len() {
                for j in i + 1..boxes.len() {
                    let (ia, la, ha) = boxes[i];
                    let (ib, lb, hb) = boxes[j];
                    if (0..3).any(|k| la[k] >= hb[k] || lb[k] >= ha[k]) {
                        continue;
                    }
                    if let [Some(a), Some(b)] = boats.get_disjoint_mut([&ia, &ib]) {
                        push_boats(a, b);
                    }
                }
            }
            for &id in &gone {
                if let Some(b) = boats.remove(&id) {
                    for r in &b.riders {
                        if let Some(e) = mobs.get_mut(&r.1) {
                            e.vehicle = None;
                            let yaw = e.mob.yaw;
                            let spot = self.dismount_spot(b.pos, yaw);
                            e.ride_to(spot, yaw);
                            marks.push(spot);
                            packets.push((
                                r.1,
                                play::teleport_entity(
                                    self.ids.teleport_entity,
                                    r.1,
                                    spot,
                                    yaw,
                                    0.0,
                                    true,
                                ),
                            ));
                        }
                        self.unlink_rider(r.0);
                    }
                }
            }
        });
        if !packets.is_empty() {
            self.hub.send_many_about(packets);
        }
        self.hub.remove_tracked(&gone);
        for pos in marks {
            self.mark_entity_changed(pos);
        }
        // Игрока-пассажира высаживаем после снятия замков: клиент сам
        // спешивается по `set_passengers`, а сервер ставит его в точку высадки.
        for (uuid, spot) in player_ejects {
            self.moved(
                uuid,
                Move {
                    pos: Some(spot),
                    rot: None,
                    on_ground: true,
                },
            );
        }
    }

    /// `AbstractBoat.positionRider`: мобы-пассажиры едут с лодкой (игроков
    /// двигает их клиент).
    fn boat_riders(
        &self,
        b: &mut Boat,
        players: &FastMap<Uuid, f32>,
        mobs: &mut FastMap<i32, mobs::MobEntry>,
        packets: &mut Vec<(i32, Vec<u8>)>,
        marks: &mut Vec<[f64; 3]>,
    ) {
        let mut i = 0;
        while i < b.riders.len() {
            let (uuid, rider) = b.riders[i];
            if players.contains_key(&uuid) {
                i += 1;
                continue;
            }
            let Some(e) = mobs.get_mut(&rider) else {
                b.riders.remove(i);
                self.unlink_rider(uuid);
                continue;
            };
            let animal = e.mob.species.is_animal();
            let at = self.passenger_seat(b, i, animal);
            let yaw = self.passenger_yaw(b, i, animal);
            let old = e.mob.pos;
            if e.ride_to(at, yaw) {
                packets.push((
                    rider,
                    play::teleport_entity(self.ids.teleport_entity, rider, at, yaw, 0.0, true),
                ));
                if (old[0].floor() as i32) >> 4 != (at[0].floor() as i32) >> 4
                    || (old[2].floor() as i32) >> 4 != (at[2].floor() as i32) >> 4
                {
                    marks.push(old);
                    marks.push(at);
                }
            }
            i += 1;
        }
    }

    /// Точка пассажира в лодке (`getPassengerAttachmentPoint`).
    fn passenger_seat(&self, b: &Boat, index: usize, animal: bool) -> [f64; 3] {
        let mut offset = if b.kind.contains("_chest_") {
            0.15
        } else {
            0.0
        };
        if b.riders.len() > 1 {
            offset = if index == 0 { 0.2 } else { -0.6 };
            if animal {
                offset += 0.2;
            }
        }
        let angle = -(b.yaw as f64).to_radians();
        let (sin, cos) = angle.sin_cos();
        [
            b.pos[0] + offset * sin,
            b.pos[1] + HEIGHT / 3.0,
            b.pos[2] + offset * cos,
        ]
    }

    /// `calculatePassengerBodyYRot`: животные в полной лодке сидят боком.
    fn passenger_yaw(&self, b: &Boat, index: usize, animal: bool) -> f32 {
        if animal && b.riders.len() == b.seats() {
            let id = b.riders[index].1;
            b.yaw + if id % 2 == 0 { 90.0 } else { 270.0 }
        } else {
            b.yaw
        }
    }

    /// `AbstractBoat.tick` в конце: задевающих мобов сажает
    /// (`startRiding`), остальных толкает (`push`).
    fn boat_contacts(
        &self,
        b: &mut Boat,
        id: i32,
        players: &FastMap<Uuid, f32>,
        mobs: &mut FastMap<i32, mobs::MobEntry>,
        packets: &mut Vec<(i32, Vec<u8>)>,
        marks: &mut Vec<[f64; 3]>,
    ) {
        let (lo, hi) = b.bounds();
        let slo = [lo[0] - 0.2, lo[1] + 0.01, lo[2] - 0.2];
        let shi = [hi[0] + 0.2, hi[1] - 0.01, hi[2] + 0.2];
        let player_controlled = b.riders.first().is_some_and(|r| players.contains_key(&r.0));
        let can_board = !player_controlled
            && !matches!(b.status, Status::UnderWater | Status::UnderFlowingWater)
            && b.out_of_control < 60.0
            && b.riders.len() < b.seats();
        for (&rider, e) in mobs.iter_mut() {
            if e.vehicle.is_some() || !e.mob.alive() {
                continue;
            }
            let (w, h) = e.mob.size();
            let mlo = [e.mob.pos[0] - w / 2.0, e.mob.pos[1], e.mob.pos[2] - w / 2.0];
            let mhi = [
                e.mob.pos[0] + w / 2.0,
                e.mob.pos[1] + h,
                e.mob.pos[2] + w / 2.0,
            ];
            if (0..3).any(|k| slo[k] >= mhi[k] || mlo[k] >= shi[k]) {
                continue;
            }
            if can_board && b.riders.len() < b.seats() && w < WIDTH && boardable(&e.mob) {
                e.vehicle = Some(id);
                b.riders.push((Uuid::from_u128(e.mob.uuid), rider));
                self.link_rider(Uuid::from_u128(e.mob.uuid), b.uuid);
                let index = b.riders.len() - 1;
                let animal = e.mob.species.is_animal();
                let at = self.passenger_seat(b, index, animal);
                let yaw = self.passenger_yaw(b, index, animal);
                e.ride_to(at, yaw);
                packets.push((
                    rider,
                    play::teleport_entity(self.ids.teleport_entity, rider, at, yaw, 0.0, true),
                ));
                let ids: Vec<i32> = b.riders.iter().map(|r| r.1).collect();
                packets.push((id, play::set_passengers(self.ids.set_passengers, id, &ids)));
                marks.push(e.mob.pos);
                continue;
            }
            self.push_mob(b, &mut e.mob);
        }
    }

    /// `Entity.push(Entity)` для моба: взаимный импульс 0.05.
    fn push_mob(&self, b: &mut Boat, m: &mut crate::mobs::Mob) {
        let (dx, dz) = (m.pos[0] - b.pos[0], m.pos[2] - b.pos[2]);
        let dd = dx.abs().max(dz.abs());
        if dd < 0.01 {
            return;
        }
        let d = dd.sqrt();
        let power = (1.0 / d).min(1.0) * 0.05;
        let (ix, iz) = (dx / d * power, dz / d * power);
        if b.riders.is_empty() {
            b.vel[0] -= ix;
            b.vel[2] -= iz;
        }
        m.vel[0] += ix;
        m.vel[2] += iz;
    }

    /// `AbstractBoat.getStatus`: сверху вниз вода, трение земли, воздух.
    fn boat_status(&self, b: &mut Boat, w: &WorldView<'_>) -> Status {
        if let Some(status) = self.boat_underwater(b, w) {
            let (_, hi) = b.bounds();
            b.water_level = hi[1];
            return status;
        }
        if self.boat_check_in_water(b, w) {
            return Status::InWater;
        }
        let friction = self.boat_ground_friction(b, w);
        if friction > 0.0 {
            b.land_friction = friction;
            Status::OnLand
        } else {
            Status::InAir
        }
    }

    /// Высота воды в клетке (`FluidState.getHeight`): источник 8/9, течение
    /// `(8 − уровень)/9`, падающая — 8/9.
    fn water_height(&self, w: &WorldView<'_>, p: [i32; 3]) -> f32 {
        let Some(state) = w.block(p) else {
            return 0.0;
        };
        match crate::fluids::state_fluid(&self.survival.placement, state) {
            Some(f) if f.kind == crate::fluids::Kind::Water => f.amount as f32 / 9.0,
            _ => 0.0,
        }
    }

    /// `EntityFluidInteraction` + `CurrentAccumulator`: сумма течений воды
    /// под коробкой, нормированная, с силой 0.014 (и минимумом 0.0045, если
    /// лодка стоит).
    fn apply_fluid_current(&self, b: &mut Boat, w: &WorldView<'_>) {
        let (lo, hi) = b.bounds();
        let x0 = lo[0].floor() as i32;
        let y0 = lo[1].floor() as i32;
        let z0 = lo[2].floor() as i32;
        let x1 = hi[0].ceil() as i32 - 1;
        let y1 = hi[1].ceil() as i32 - 1;
        let z1 = hi[2].ceil() as i32 - 1;
        let mut height = 0.0f64;
        let mut acc = [0.0; 3];
        let mut any = false;
        for x in x0..=x1 {
            for y in y0..=y1 {
                for z in z0..=z1 {
                    let Some(state) = w.block([x, y, z]) else {
                        continue;
                    };
                    let Some(f) = crate::fluids::state_fluid(&self.survival.placement, state)
                    else {
                        continue;
                    };
                    if f.kind != crate::fluids::Kind::Water {
                        continue;
                    }
                    let top = y as f64 + f.amount as f64 / 9.0;
                    if top < lo[1] {
                        continue;
                    }
                    any = true;
                    height = height.max(top - lo[1]);
                    let mut flow = self.fluid_flow(w, [x, y, z]);
                    if height < 0.4 {
                        for v in &mut flow {
                            *v *= height;
                        }
                    }
                    for (a, v) in acc.iter_mut().zip(flow) {
                        *a += v;
                    }
                }
            }
        }
        let len = (acc[0] * acc[0] + acc[2] * acc[2]).sqrt();
        if !any || len * len < 1.0e-5 {
            return;
        }
        let mut impulse = [acc[0] / len * 0.014, 0.0, acc[2] / len * 0.014];
        if b.vel[0].abs() < 0.003 && b.vel[2].abs() < 0.003 {
            let l = (impulse[0] * impulse[0] + impulse[2] * impulse[2]).sqrt();
            if l < 0.004_5 {
                impulse = [impulse[0] / l * 0.0045, 0.0, impulse[2] / l * 0.0045];
            }
        }
        b.vel[0] += impulse[0];
        b.vel[1] += impulse[1];
        b.vel[2] += impulse[2];
    }

    /// `FlowingFluid.getFlow`: разница высот воды с соседями (в том числе
    /// с водой под пустой клеткой).
    fn fluid_flow(&self, w: &WorldView<'_>, p: [i32; 3]) -> [f64; 3] {
        let own = self.water_height(w, p);
        if own <= 0.0 {
            return [0.0; 3];
        }
        let mut flow = [0.0f64; 3];
        for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            let n = [p[0] + dx, p[1], p[2] + dz];
            let nh = self.water_height(w, n);
            let distance = if nh > 0.0 {
                own - nh
            } else {
                let below = self.water_height(w, [n[0], n[1] - 1, n[2]]);
                if below > 0.0 {
                    own - (below - 0.888_888_9)
                } else {
                    0.0
                }
            };
            flow[0] += dx as f64 * distance as f64;
            flow[2] += dz as f64 * distance as f64;
        }
        let len = (flow[0] * flow[0] + flow[2] * flow[2]).sqrt();
        if len > 0.0 {
            flow[0] /= len;
            flow[2] /= len;
        }
        flow
    }

    /// `AbstractBoat.getWaterLevelAbove`: верх воды над коробкой (или
    /// `maxY + 1`, если воды нет).
    fn boat_water_level_above(&self, b: &Boat, w: &WorldView<'_>) -> f32 {
        let (lo, hi) = b.bounds();
        let min_x = lo[0].floor() as i32;
        let max_x = hi[0].ceil() as i32;
        let min_y = hi[1].floor() as i32;
        let max_y = (hi[1] - b.last_yd).ceil() as i32;
        let min_z = lo[2].floor() as i32;
        let max_z = hi[2].ceil() as i32;
        for y in min_y..max_y {
            let mut block_height = 0.0f32;
            'x: for x in min_x..max_x {
                for z in min_z..max_z {
                    block_height = block_height.max(self.water_height(w, [x, y, z]));
                    if block_height >= 1.0 {
                        break 'x;
                    }
                }
            }
            if block_height < 1.0 {
                return y as f32 + block_height;
            }
        }
        (max_y + 1) as f32
    }

    /// `AbstractBoat.getGroundFriction`: среднее трение блоков под дном,
    /// задевающих коробку.
    fn boat_ground_friction(&self, b: &Boat, w: &WorldView<'_>) -> f32 {
        let (lo, hi) = b.bounds();
        let box_lo = [lo[0], lo[1] - 0.001, lo[2]];
        let box_hi = [hi[0], lo[1], hi[2]];
        let x0 = box_lo[0].floor() as i32 - 1;
        let x1 = box_hi[0].ceil() as i32 + 1;
        let y0 = box_lo[1].floor() as i32 - 1;
        let y1 = box_hi[1].ceil() as i32 + 1;
        let z0 = box_lo[2].floor() as i32 - 1;
        let z1 = box_hi[2].ceil() as i32 + 1;
        let mut friction = 0.0f32;
        let mut count = 0i32;
        for x in x0..x1 {
            for z in z0..z1 {
                let edges = i32::from(x == x0 || x == x1 - 1) + i32::from(z == z0 || z == z1 - 1);
                if edges == 2 {
                    continue;
                }
                for y in y0..y1 {
                    if edges > 0 && (y == y0 || y == y1 - 1) {
                        continue;
                    }
                    let Some(state) = w.block([x, y, z]) else {
                        continue;
                    };
                    let name = self.state_name(state);
                    // `LilyPadBlock`: лист не даёт трения.
                    if name.ends_with(":lily_pad") {
                        continue;
                    }
                    let touches = self.collision_boxes(state).iter().any(|s| {
                        let a = [
                            s[0] + x as f64,
                            s[1] + y as f64,
                            s[2] + z as f64,
                            s[3] + x as f64,
                            s[4] + y as f64,
                            s[5] + z as f64,
                        ];
                        a[3] > box_lo[0]
                            && a[0] < box_hi[0]
                            && a[4] > box_lo[1]
                            && a[1] < box_hi[1]
                            && a[5] > box_lo[2]
                            && a[2] < box_hi[2]
                    });
                    if touches {
                        friction += block_friction(name);
                        count += 1;
                    }
                }
            }
        }
        if count == 0 {
            0.0
        } else {
            friction / count as f32
        }
    }

    /// `AbstractBoat.checkInWater`: коробка задевает воду — запомнить
    /// уровень и вернуть `true`.
    fn boat_check_in_water(&self, b: &mut Boat, w: &WorldView<'_>) -> bool {
        let (lo, hi) = b.bounds();
        let min_x = lo[0].floor() as i32;
        let max_x = hi[0].ceil() as i32;
        let min_y = lo[1].floor() as i32;
        let max_y = (lo[1] + 0.001).ceil() as i32;
        let min_z = lo[2].floor() as i32;
        let max_z = hi[2].ceil() as i32;
        let mut in_water = false;
        b.water_level = f64::NEG_INFINITY;
        for x in min_x..max_x {
            for y in min_y..max_y {
                for z in min_z..max_z {
                    let h = self.water_height(w, [x, y, z]);
                    if h <= 0.0 {
                        continue;
                    }
                    let height = y as f64 + h as f64;
                    b.water_level = b.water_level.max(height);
                    in_water |= lo[1] < height;
                }
            }
        }
        in_water
    }

    /// `AbstractBoat.isUnderwater`: коробка целиком под водой.
    fn boat_underwater(&self, b: &Boat, w: &WorldView<'_>) -> Option<Status> {
        let (lo, hi) = b.bounds();
        let max_y = hi[1] + 0.001;
        let x0 = lo[0].floor() as i32;
        let x1 = hi[0].ceil() as i32;
        let y0 = hi[1].floor() as i32;
        let y1 = max_y.ceil() as i32;
        let z0 = lo[2].floor() as i32;
        let z1 = hi[2].ceil() as i32;
        let mut under_water = false;
        for x in x0..x1 {
            for y in y0..y1 {
                for z in z0..z1 {
                    let Some(state) = w.block([x, y, z]) else {
                        continue;
                    };
                    let Some(f) = crate::fluids::state_fluid(&self.survival.placement, state)
                    else {
                        continue;
                    };
                    if f.kind != crate::fluids::Kind::Water {
                        continue;
                    }
                    let height = y as f64 + f.amount as f64 / 9.0;
                    if max_y >= height {
                        continue;
                    }
                    if f.source {
                        under_water = true;
                    } else {
                        return Some(Status::UnderFlowingWater);
                    }
                }
            }
        }
        under_water.then_some(Status::UnderWater)
    }

    /// `AbstractBoat.floatBoat`: гравитация, плавучесть, трение.
    fn float_boat(&self, b: &mut Boat, w: &WorldView<'_>) {
        let mut vspeed = -GRAVITY;
        let mut buoyancy = 0.0f32;
        let friction;
        if b.old_status == Status::InAir && b.status != Status::InAir && b.status != Status::OnLand
        {
            // Вход в воду: лодку ставит на поверхность, вертикаль гасится.
            b.water_level = b.pos[1] + HEIGHT;
            let target = self.boat_water_level_above(b, w) as f64 - HEIGHT + 0.101;
            let (lo, hi) = b.bounds();
            let dy = target - b.pos[1];
            let (l, h) = ([lo[0], lo[1] + dy, lo[2]], [hi[0], hi[1] + dy, hi[2]]);
            if self.boat_boxes(l, h, w).is_empty() {
                b.pos[1] = target;
                b.vel[1] = 0.0;
                b.last_yd = 0.0;
            }
            b.status = Status::InWater;
            return;
        }
        match b.status {
            Status::InWater => {
                buoyancy = ((b.water_level - b.pos[1]) / HEIGHT) as f32;
                friction = 0.9;
            }
            Status::UnderFlowingWater => {
                vspeed = -7.0e-4;
                friction = 0.9;
            }
            Status::UnderWater => {
                buoyancy = 0.01;
                friction = 0.45;
            }
            Status::InAir => friction = 0.9,
            // `getControllingPassenger() instanceof Player` — у пустой лодки
            // водителя нет, поэтому трение не делится пополам.
            Status::OnLand => friction = b.land_friction,
        }
        b.vel[0] *= friction as f64;
        b.vel[1] += vspeed;
        b.vel[2] *= friction as f64;
        if buoyancy > 0.0 {
            b.vel[1] = (b.vel[1] + buoyancy as f64 * (GRAVITY / 0.65)) * 0.75;
        }
    }

    /// `Entity.move(MoverType.SELF, delta)` для коробки лодки: подъём по
    /// осям Y, затем большей из X/Z, и мировой бордер.
    fn move_boat(&self, b: &mut Boat, w: &WorldView<'_>, now: i64) -> [f64; 3] {
        let mv = b.vel;
        let (lo, hi) = b.bounds();
        let l = [
            lo[0] + mv[0].min(0.0),
            lo[1] + mv[1].min(0.0),
            lo[2] + mv[2].min(0.0),
        ];
        let h = [
            hi[0] + mv[0].max(0.0),
            hi[1] + mv[1].max(0.0),
            hi[2] + mv[2].max(0.0),
        ];
        let got = collide_all(lo, hi, mv, &self.boat_boxes(l, h, w));
        let old = b.pos;
        b.pos = [old[0] + got[0], old[1] + got[1], old[2] + got[2]];
        let clipped = self.border_at(now).clip_movement(now, old, b.pos, WIDTH);
        for axis in [0, 2] {
            if clipped[axis] != b.pos[axis] {
                b.pos[axis] = clipped[axis];
            }
        }
        // `verticalCollisionBelow` == упёрлись при движении вниз.
        b.on_ground = got[1] != mv[1] && mv[1] < 0.0;
        got
    }

    /// Коробки блоков, задевающих область `lo..hi`.
    fn boat_boxes(&self, lo: [f64; 3], hi: [f64; 3], w: &WorldView<'_>) -> Vec<[f64; 6]> {
        let r = |v: f64| (v - COLLIDE_EPS).floor() as i32;
        let mut out = Vec::new();
        for x in r(lo[0])..=r(hi[0] + 2.0 * COLLIDE_EPS) {
            for y in r(lo[1]) - 1..=r(hi[1] + 2.0 * COLLIDE_EPS) {
                for z in r(lo[2])..=r(hi[2] + 2.0 * COLLIDE_EPS) {
                    let Some(state) = w.block([x, y, z]) else {
                        continue;
                    };
                    for s in self.collision_boxes(state) {
                        let a = [
                            s[0] + x as f64,
                            s[1] + y as f64,
                            s[2] + z as f64,
                            s[3] + x as f64,
                            s[4] + y as f64,
                            s[5] + z as f64,
                        ];
                        if a[3] > lo[0]
                            && a[0] < hi[0]
                            && a[4] > lo[1]
                            && a[1] < hi[1]
                            && a[5] > lo[2]
                            && a[2] < hi[2]
                        {
                            out.push(a);
                        }
                    }
                }
            }
        }
        out
    }
}

/// `getDefaultGravity` лодки.
const GRAVITY: f64 = 0.04;
/// Допуск `Shapes.collide`.
const COLLIDE_EPS: f64 = 1e-7;

impl Boat {
    fn bounds(&self) -> ([f64; 3], [f64; 3]) {
        let half = WIDTH / 2.0;
        (
            [self.pos[0] - half, self.pos[1], self.pos[2] - half],
            [self.pos[0] + half, self.pos[1] + HEIGHT, self.pos[2] + half],
        )
    }
}

/// `EntityTypeTags.CANNOT_BE_PUSHED_ONTO_BOATS`.
fn boardable(m: &crate::mobs::Mob) -> bool {
    !matches!(
        m.species.short(),
        "elder_guardian"
            | "cod"
            | "pufferfish"
            | "salmon"
            | "tropical_fish"
            | "dolphin"
            | "squid"
            | "glow_squid"
            | "tadpole"
            | "creaking"
            | "nautilus"
            | "zombie_nautilus"
            | "sulfur_cube"
    )
}

/// `AbstractBoat.push` → `Entity.push` между двумя лодками.
fn push_boats(a: &mut Boat, b: &mut Boat) {
    // Чужую лодку толкаем, только если её низ ниже нашего верха.
    let (_, ahi) = a.bounds();
    if b.pos[1] >= ahi[1] {
        return;
    }
    let (dx, dz) = (b.pos[0] - a.pos[0], b.pos[2] - a.pos[2]);
    let dd = dx.abs().max(dz.abs());
    if dd < 0.01 {
        return;
    }
    let d = dd.sqrt();
    let power = (1.0 / d).min(1.0) * 0.05;
    let (ix, iz) = (dx / d * power, dz / d * power);
    if a.riders.is_empty() {
        a.vel[0] -= ix;
        a.vel[2] -= iz;
    }
    if b.riders.is_empty() {
        b.vel[0] += ix;
        b.vel[2] += iz;
    }
}

/// `Block.getFriction`: лёд и слизь скользят, остальное — 0.6.
fn block_friction(name: &str) -> f32 {
    if name.ends_with(":blue_ice") {
        0.989
    } else if name.ends_with(":ice")
        || name.ends_with(":packed_ice")
        || name.ends_with(":frosted_ice")
    {
        0.98
    } else if name.ends_with(":slime_block") {
        0.8
    } else {
        0.6
    }
}

/// `Shapes.collide`: сколько коробка `lo..hi` пройдёт по оси `axis` на `d`.
fn collide(lo: [f64; 3], hi: [f64; 3], axis: usize, mut d: f64, boxes: &[[f64; 6]]) -> f64 {
    if d == 0.0 {
        return 0.0;
    }
    let (o1, o2) = ((axis + 1) % 3, (axis + 2) % 3);
    for b in boxes {
        let overlaps = |o: usize| b[o + 3] - COLLIDE_EPS > lo[o] && b[o] + COLLIDE_EPS < hi[o];
        if !overlaps(o1) || !overlaps(o2) {
            continue;
        }
        if d > 0.0 && b[axis] >= hi[axis] - COLLIDE_EPS {
            d = d.min(b[axis] - hi[axis]);
        } else if d < 0.0 && b[axis + 3] <= lo[axis] + COLLIDE_EPS {
            d = d.max(b[axis + 3] - lo[axis]);
        }
    }
    d
}

/// `Entity.collideWithShapes`: оси по `Direction.axisStepOrder` — Y, затем
/// большая из X и Z.
fn collide_all(lo: [f64; 3], hi: [f64; 3], mv: [f64; 3], boxes: &[[f64; 6]]) -> [f64; 3] {
    let order = if mv[0].abs() < mv[2].abs() {
        [1, 2, 0]
    } else {
        [1, 0, 2]
    };
    let mut done = [0.0; 3];
    for axis in order {
        let l = [lo[0] + done[0], lo[1] + done[1], lo[2] + done[2]];
        let h = [hi[0] + done[0], hi[1] + done[1], hi[2] + done[2]];
        done[axis] = collide(l, h, axis, mv[axis], boxes);
    }
    done
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boats_push_each_other_apart() {
        let mut a = Boat::new("minecraft:oak_boat".into(), [0.0, 0.0, 0.0], 0.0);
        let mut b = Boat::new("minecraft:spruce_boat".into(), [0.9, 0.0, 0.0], 0.0);
        push_boats(&mut a, &mut b);
        assert!(a.vel[0] < 0.0, "{:?}", a.vel);
        assert!(b.vel[0] > 0.0, "{:?}", b.vel);
        assert_eq!(a.vel[1], 0.0);
    }

    #[test]
    fn collide_all_stops_at_wall_and_floor() {
        // Стена x∈[1,2] и пол y∈[-1,0] под коробкой.
        let boxes = [
            [1.0, 0.0, 0.0, 2.0, 1.0, 1.0],
            [0.0, -1.0, 0.0, 2.0, 0.0, 1.0],
        ];
        let got = collide_all([0.0, 0.0, 0.0], [0.5, 0.5, 0.5], [2.0, -1.0, 0.0], &boxes);
        assert!((got[0] - 0.5).abs() < 1e-9, "{got:?}");
        assert!(got[1].abs() < 1e-9, "{got:?}");
        assert!(got[2].abs() < 1e-9, "{got:?}");
    }

    #[test]
    fn escape_vector_steps_past_the_boat() {
        // Взгляд на юг (+z): шаг (0.6 + 1.375·√2)/2 по z.
        let v = escape_vector(WIDTH * std::f64::consts::SQRT_2, 0.6, 0.0);
        assert!(v[0].abs() < 1e-9);
        assert!((v[1] - 1.2722).abs() < 1e-3, "{v:?}");
        // По диагонали шаг растягивается до угла квадрата.
        let d = escape_vector(1.0, 0.0, 45.0);
        assert!(
            (d[0] + 0.5).abs() < 1e-4 && (d[1] - 0.5).abs() < 1e-4,
            "{d:?}"
        );
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
