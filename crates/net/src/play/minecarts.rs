//! Вагонетки (`AbstractMinecart` и виды) в мире.
//!
//! В отличие от лодки вагонеткой правит сервер: физика рельсов — в
//! [`crate::minecart`], здесь — сущность, тик, рассылка, посадка, детекторный
//! рельс, удар и запись. Виды: обычная (`Minecart`, ездит игрок), с сундуком
//! и воронкой (`AbstractMinecartContainer`, инвентарь — условная позиция
//! [`cart_pos`]), печная (`MinecartFurnace`) и с ТНТ (`MinecartTNT`).

use super::*;
use crate::minecart::{Cart, Drag, Kind, Rail, Shape, Track};

/// `VehicleEntity.hurtServer`, как у лодки.
const DAMAGE_SCALE: f32 = 10.0;
const BREAK_AT: f32 = 40.0;
const HURT_TIME: i32 = 10;

/// Биты `player_input` (`Input`).
const IN_FORWARD: u8 = 0x01;
const IN_BACK: u8 = 0x02;
const IN_LEFT: u8 = 0x04;
const IN_RIGHT: u8 = 0x08;

/// `MinecartFurnace`: топливо от одного угля и предел.
const FUEL_PER_ITEM: i32 = 3600;
const FUEL_MAX: i32 = 32000;
/// `MinecartCommandBlock.ACTIVATION_DELAY`.
const COMMAND_ACTIVATION_DELAY: i64 = 4;
/// `MinecartTNT.primeFuse`.
const TNT_FUSE: i32 = 80;
/// `MinecartTNT.handleEntityEvent`: клиент поджигает фитиль.
const EVENT_PRIME: u8 = 70;
/// `MinecartSpawner$1.broadcastEvent`: `BaseSpawner` шлёт событие 1.
const SPAWNER_EVENT: u8 = 1;
/// `MinecartTNT.explode`: сила `4 + 1·rnd·1.5·min(√v², 5)`.
const TNT_POWER: f32 = 4.0;
const TNT_SPEED_FACTOR: f32 = 1.0;

/// `SpawnData`: 26.3 хранит сущность под `entity`, старый формат — прямо в
/// `SpawnData`/`data`; оба дают NBT с `id`.
fn spawner_entity(data: &Compound) -> Option<(crate::mobs::Species, &Compound)> {
    let entity = data
        .get("entity")
        .and_then(Tag::as_compound)
        .unwrap_or(data);
    let id = entity.get("id").and_then(Tag::as_str)?;
    Some((crate::mobs::Species::from_name(id)?, entity))
}

fn command_ready(last_activated: i64, now: i64) -> bool {
    now.wrapping_sub(last_activated) >= COMMAND_ACTIVATION_DELAY
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Variant {
    Plain,
    Chest,
    Hopper,
    Furnace,
    Tnt,
    CommandBlock,
    Spawner,
}

impl Variant {
    const ALL: [Variant; 7] = [
        Variant::Plain,
        Variant::Chest,
        Variant::Hopper,
        Variant::Furnace,
        Variant::Tnt,
        Variant::CommandBlock,
        Variant::Spawner,
    ];

    /// Тип сущности; предмет называется так же.
    fn name(self) -> &'static str {
        match self {
            Variant::Plain => "minecraft:minecart",
            Variant::Chest => "minecraft:chest_minecart",
            Variant::Hopper => "minecraft:hopper_minecart",
            Variant::Furnace => "minecraft:furnace_minecart",
            Variant::Tnt => "minecraft:tnt_minecart",
            Variant::CommandBlock => "minecraft:command_block_minecart",
            Variant::Spawner => "minecraft:spawner_minecart",
        }
    }

    /// `getDropItem`: командный блок и спаунер выпадают обычной вагонеткой.
    fn drop_name(self) -> &'static str {
        match self {
            Variant::CommandBlock | Variant::Spawner => "minecraft:minecart",
            other => other.name(),
        }
    }

    fn of(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.name() == name)
    }

    /// Слоты инвентаря (`getContainerSize`).
    fn size(self) -> usize {
        match self {
            Variant::Chest => 27,
            Variant::Hopper => 5,
            _ => 0,
        }
    }
}

/// Что делает правый клик по вагонетке.
pub(super) enum CartUse {
    /// `Minecart.interact`: сесть.
    Ride,
    /// `ContainerEntity.interactWithContainerVehicle`: окно на столько слотов, заголовок.
    Open(usize, serde_json::Value),
    /// `MinecartFurnace.interact`: подбросить топливо.
    Fuel,
    /// `MinecartCommandBlock.interact`: редактор для gamemaster.
    CommandBlock,
    Pass,
}

/// `BaseSpawner`: поля, которые пишутся в NBT и тикают вместе с вагонеткой.
#[derive(Clone, Debug)]
struct SpawnerState {
    delay: i32,
    min_delay: i32,
    max_delay: i32,
    spawn_count: i32,
    max_nearby: i32,
    required_player_range: i32,
    spawn_range: i32,
    spawn_data: Option<Compound>,
    /// `SpawnPotentials`: вес и `data`.
    potentials: Vec<(i32, Compound)>,
}

impl Default for SpawnerState {
    fn default() -> Self {
        Self {
            delay: 20,
            min_delay: 200,
            max_delay: 800,
            spawn_count: 4,
            max_nearby: 6,
            required_player_range: 16,
            spawn_range: 4,
            spawn_data: None,
            potentials: Vec::new(),
        }
    }
}

impl SpawnerState {
    fn load(c: &Compound) -> Self {
        let mut s = Self::default();
        let int = |k: &str| match c.get(k) {
            Some(Tag::Short(v)) => Some(i32::from(*v)),
            Some(Tag::Int(v)) => Some(*v),
            Some(Tag::Byte(v)) => Some(i32::from(*v)),
            _ => None,
        };
        s.delay = int("Delay").unwrap_or(s.delay);
        s.min_delay = int("MinSpawnDelay").unwrap_or(s.min_delay);
        s.max_delay = int("MaxSpawnDelay").unwrap_or(s.max_delay);
        s.spawn_count = int("SpawnCount").unwrap_or(s.spawn_count);
        s.max_nearby = int("MaxNearbyEntities").unwrap_or(s.max_nearby);
        s.required_player_range = int("RequiredPlayerRange").unwrap_or(s.required_player_range);
        s.spawn_range = int("SpawnRange").unwrap_or(s.spawn_range);
        if let Some(Tag::Compound(data)) = c.get("SpawnData") {
            s.spawn_data = Some(data.clone());
        }
        for t in c
            .get("SpawnPotentials")
            .and_then(Tag::as_list)
            .unwrap_or(&[])
        {
            let Some(entry) = t.as_compound() else {
                continue;
            };
            let weight = entry
                .get("weight")
                .and_then(Tag::as_i64)
                .unwrap_or(1)
                .max(1) as i32;
            let data = entry
                .get("data")
                .and_then(Tag::as_compound)
                .cloned()
                .or_else(|| entry.get("entity").and_then(Tag::as_compound).cloned());
            if let Some(data) = data {
                s.potentials.push((weight, data));
            }
        }
        s
    }

    fn save(&self, c: &mut Compound) {
        c.insert("Delay".into(), Tag::Short(self.delay as i16));
        c.insert("MinSpawnDelay".into(), Tag::Short(self.min_delay as i16));
        c.insert("MaxSpawnDelay".into(), Tag::Short(self.max_delay as i16));
        c.insert("SpawnCount".into(), Tag::Short(self.spawn_count as i16));
        c.insert(
            "MaxNearbyEntities".into(),
            Tag::Short(self.max_nearby as i16),
        );
        c.insert(
            "RequiredPlayerRange".into(),
            Tag::Short(self.required_player_range as i16),
        );
        c.insert("SpawnRange".into(), Tag::Short(self.spawn_range as i16));
        if let Some(data) = &self.spawn_data {
            c.insert("SpawnData".into(), Tag::Compound(data.clone()));
        }
        if !self.potentials.is_empty() {
            let list = self
                .potentials
                .iter()
                .map(|(weight, data)| {
                    Tag::Compound(Compound::from([
                        ("weight".into(), Tag::Int(*weight)),
                        ("data".into(), Tag::Compound(data.clone())),
                    ]))
                })
                .collect();
            c.insert("SpawnPotentials".into(), Tag::List(list));
        }
    }

    /// `getOrCreateNextSpawnData`: взвешенный `SpawnPotentials` или `SpawnData`.
    fn next_data(&self, r: f64) -> Option<Compound> {
        let total: i32 = self.potentials.iter().map(|(w, _)| *w).sum();
        if total > 0 {
            let mut pick = ((r * f64::from(total)).floor() as i32).min(total - 1);
            for (weight, data) in &self.potentials {
                if pick < *weight {
                    return Some(data.clone());
                }
                pick -= *weight;
            }
        }
        self.spawn_data.clone()
    }
}

pub(crate) struct MinecartEntry {
    pub cart: Cart,
    variant: Variant,
    uuid: Uuid,
    rider: Option<(Uuid, i32)>,
    /// Последние клавиши водителя.
    input: u8,
    damage: f32,
    hit_at: i64,
    hurt_dir: i32,
    sent: ([f64; 3], f32),
    custom_name: Option<String>,
    /// Инвентарь сундука или воронки.
    slots: Vec<Slot>,
    /// `MinecartHopper.enabled`: включённый активатор выключает.
    enabled: bool,
    /// `MinecartFurnace.fuel` и последний разосланный `hasFuel`.
    fuel: i32,
    lit: bool,
    /// `MinecartTNT.fuse`: −1 — не подожжена.
    fuse: i32,
    /// Пройденное падением (`fallDistance`) — для взрыва ТНТ при ударе о землю.
    fall: f64,
    /// `MinecartCommandBlock`: команда, вывод и задержка активации.
    command: String,
    track_output: bool,
    last_output: Option<String>,
    success_count: i32,
    last_activated: i64,
    /// `MinecartSpawner` → `BaseSpawner`.
    spawner: SpawnerState,
}

impl MinecartEntry {
    pub(super) fn variant_name(&self) -> &'static str {
        self.variant.name()
    }

    /// UUID сущности: для наблюдателя (`teleport_to_entity`).
    pub(super) fn uuid(&self) -> Uuid {
        self.uuid
    }

    fn new(cart: Cart, variant: Variant) -> Self {
        let mut cart = cart;
        cart.drag = match variant {
            Variant::Chest | Variant::Hopper => Drag::Container(0),
            Variant::Furnace => Drag::Furnace,
            _ => Drag::Plain,
        };
        Self {
            sent: (cart.pos, cart.yaw),
            cart,
            variant,
            uuid: Uuid::from_u128(rand::random()),
            rider: None,
            input: 0,
            damage: 0.0,
            hit_at: 0,
            hurt_dir: 1,
            custom_name: None,
            slots: vec![None; variant.size()],
            enabled: true,
            fuel: 0,
            lit: false,
            fuse: -1,
            fall: 0.0,
            command: String::new(),
            track_output: true,
            last_output: None,
            success_count: 0,
            last_activated: i64::MIN / 2,
            spawner: SpawnerState::default(),
        }
    }

    fn damage_at(&self, now: i64) -> f32 {
        (self.damage - (now - self.hit_at).max(0) as f32).max(0.0)
    }

    /// Коробка вагонетки: `[min, max]`.
    fn bounds(&self) -> ([f64; 3], [f64; 3]) {
        let h = crate::minecart::WIDTH / 2.0;
        let p = self.cart.pos;
        (
            [p[0] - h, p[1], p[2] - h],
            [p[0] + h, p[1] + crate::minecart::HEIGHT, p[2] + h],
        )
    }

    fn speed_sq(&self) -> f64 {
        self.cart.vel[0] * self.cart.vel[0] + self.cart.vel[2] * self.cart.vel[2]
    }
}

/// Пересекает ли предмет на земле (коробка 0.25×0.25) коробку `[lo, hi]`.
fn item_in(lo: [f64; 3], hi: [f64; 3], p: [f64; 3]) -> bool {
    p[0] + 0.125 > lo[0]
        && p[0] - 0.125 < hi[0]
        && p[2] + 0.125 > lo[2]
        && p[2] - 0.125 < hi[2]
        && p[1] + 0.25 > lo[1]
        && p[1] < hi[1]
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
            frame: &|_, _| None,
        };
        w.conductor(at(p))
    }

    fn solid(&self, p: [i32; 3]) -> bool {
        self.0
            .block(p)
            .is_some_and(|s| !self.0.state_name(s).ends_with("rail") && self.0.blocks_boat(s))
    }

    fn water(&self, p: [i32; 3]) -> bool {
        self.0.block(p).is_some_and(|s| {
            crate::fluids::state_fluid(&self.0.survival.placement, s)
                .is_some_and(|f| f.kind == crate::fluids::Kind::Water)
        })
    }
}

impl PlayPlan {
    /// Предмет и коробка вагонетки для средней кнопки.
    pub(super) fn pick_minecart(&self, id: i32) -> Option<(i32, [[f64; 3]; 2])> {
        let carts = self.sim().minecarts.lock().unwrap();
        let cart = carts.get(&id)?;
        let item = self.item_id(cart.variant.name())?;
        let (min, max) = cart.bounds();
        Some((item, [min, max]))
    }

    fn minecart_packets(&self, id: i32, e: &MinecartEntry, now: i64) -> Vec<Vec<u8>> {
        let Some(&ty) = self.survival.entity_types.get(e.variant.name()) else {
            return Vec::new();
        };
        let c = &e.cart;
        let mut out = vec![
            play::add_entity_moving(
                self.ids.add_entity,
                id,
                e.uuid,
                ty,
                c.pos,
                c.vel,
                c.yaw,
                0.0,
                0,
            ),
            inv::boat_data_hurt(
                self.ids.set_entity_data,
                id,
                0,
                e.hurt_dir,
                e.damage_at(now),
            ),
        ];
        if e.lit {
            out.push(inv::furnace_cart_fuel(self.ids.set_entity_data, id, true));
        }
        if e.fuse >= 0 {
            out.push(inv::entity_event(self.ids.entity_event, id, EVENT_PRIME));
        }
        if e.variant == Variant::CommandBlock {
            out.extend(self.minecart_command_meta(id, e));
        }
        if let Some((_, rider)) = e.rider {
            out.push(play::set_passengers(self.ids.set_passengers, id, &[rider]));
        }
        out
    }

    fn minecart_tracked(&self, id: i32, e: &MinecartEntry) -> Tracked {
        let ty = self
            .survival
            .entity_types
            .get(e.variant.name())
            .copied()
            .unwrap_or_default();
        // `getEffectiveRange`: с игроком внутри — его 32 чанка.
        let range = if e.rider.is_some() {
            self.tracking_range(ty).max(PLAYER_TRACKING_RANGE)
        } else {
            self.tracking_range(ty)
        };
        Tracked {
            id,
            pos: e.cart.pos,
            range,
        }
    }

    pub(super) fn track_minecarts(&self, out: &mut Vec<Tracked>) {
        for (&id, e) in self.sim().minecarts.lock().unwrap().iter() {
            out.push(self.minecart_tracked(id, e));
        }
    }

    /// Пакеты спавна вагонетки для нового зрителя.
    pub(super) fn minecart_spawn(&self, id: i32) -> Option<Vec<Vec<u8>>> {
        let now = self.game_time.load(Ordering::Relaxed);
        let carts = self.sim().minecarts.lock().unwrap();
        Some(self.minecart_packets(id, carts.get(&id)?, now))
    }

    /// `MinecartCommandBlock.DATA_ID_COMMAND_NAME` (10) и
    /// `DATA_ID_LAST_OUTPUT` (11): клиент открывает редактор по этим данным.
    pub(super) fn minecart_command_meta(&self, id: i32, e: &MinecartEntry) -> Vec<Vec<u8>> {
        let output = e
            .last_output
            .as_ref()
            .map(|t| serde_json::json!({ "text": t }))
            .unwrap_or_else(|| serde_json::json!({ "text": "" }));
        vec![play::entity_meta(
            self.ids.set_entity_data,
            id,
            &[
                (13, play::Meta::String(e.command.clone())),
                (14, play::Meta::Component(output)),
            ],
        )]
    }

    pub(super) fn minecart_command_packets(&self, id: i32) -> Option<Vec<Vec<u8>>> {
        let carts = self.sim().minecarts.lock().unwrap();
        let e = carts.get(&id)?;
        Some(self.minecart_command_meta(id, e))
    }

    /// `ServerboundSetCommandMinecartPacket`: сохранить команду в NBT и
    /// вернуть метаданные 10/11 для клиента.
    pub(super) fn set_command_minecart(
        &self,
        id: i32,
        command: String,
        track_output: bool,
    ) -> Option<Vec<Vec<u8>>> {
        let mut carts = self.sim().minecarts.lock().unwrap();
        let e = carts.get_mut(&id)?;
        if e.variant != Variant::CommandBlock {
            return None;
        }
        e.command = command;
        e.track_output = track_output;
        if !track_output {
            e.last_output = None;
        }
        Some(self.minecart_command_meta(id, e))
    }

    pub(super) fn is_minecart(&self, id: i32) -> bool {
        self.sim().minecarts.lock().unwrap().contains_key(&id)
    }

    /// `/summon minecraft:minecart`: вагонетка без рельса (`SummonCommand`).
    pub(super) fn summon_minecart(
        &self,
        name: &str,
        pos: [f64; 3],
        nbt: Option<&Compound>,
    ) -> Option<i32> {
        let variant = Variant::of(name)?;
        // `SummonCommand`: позиция из команды, затем NBT (её `Pos` сильнее).
        let e = match nbt {
            Some(c) => {
                let pos = match c.get("Pos").and_then(Tag::as_list) {
                    Some(list) => {
                        let v: Vec<f64> = list
                            .iter()
                            .filter_map(|t| match t {
                                Tag::Double(v) => Some(*v),
                                Tag::Float(v) => Some(*v as f64),
                                _ => None,
                            })
                            .collect();
                        if v.len() == 3 && v.iter().all(|x| x.is_finite()) {
                            [v[0], v[1], v[2]]
                        } else {
                            pos
                        }
                    }
                    None => pos,
                };
                self.minecart_from_nbt(variant, pos, c)
            }
            None => MinecartEntry::new(Cart::new(pos, 0.0), variant),
        };
        Some(self.add_minecart(e))
    }

    fn add_minecart(&self, e: MinecartEntry) -> i32 {
        let id = self.next_entity_id.fetch_add(1, Ordering::Relaxed);
        let packets = self.minecart_packets(id, &e, self.game_time.load(Ordering::Relaxed));
        let tracked = self.minecart_tracked(id, &e);
        self.sim().minecarts.lock().unwrap().insert(id, e);
        self.mark_work();
        self.hub.spawn_tracked(tracked, packets);
        id
    }

    /// Вид по предмету, если сервер знает его тип сущности.
    fn cart_variant(&self, item: &str) -> Option<Variant> {
        Variant::of(item).filter(|v| self.survival.entity_types.contains_key(v.name()))
    }

    /// `MinecartItem.useOn`: только на рельс; центр клетки на высоте рельса,
    /// на склоне на полблока выше.
    pub(super) fn place_minecart(&self, item: &str, rail: [i32; 3]) -> bool {
        let Some(variant) = self.cart_variant(item) else {
            return false;
        };
        let Some(r) = PlanTrack(self).rail(rail) else {
            return false;
        };
        let lift = if r.shape.is_slope() { 0.5 } else { 0.0 };
        let pos = [
            rail[0] as f64 + 0.5,
            rail[1] as f64 + 0.0625 + lift,
            rail[2] as f64 + 0.5,
        ];
        // `getEntities(minecart.getBoundingBox())`: на месте уже есть вагонетка — нельзя.
        let busy = self.sim().minecarts.lock().unwrap().values().any(|e| {
            let (lo, hi) = e.bounds();
            (0..3).all(|i| pos[i] + 0.49 > lo[i] && pos[i] - 0.49 < hi[i])
        });
        if busy {
            return false;
        }
        self.add_minecart(MinecartEntry::new(Cart::new(pos, 0.0), variant));
        true
    }

    /// `MinecartDispenseItemBehavior.execute`: вагонетка на 1.125 перед
    /// центром раздатчика, по высоте `floor(центр) + шагY`; рельс спереди —
    /// +0.1 (на подъёме +0.6), воздух с рельсом под ним — −0.9 (−0.4, если
    /// нижний — подъём и раздатчик смотрит не вниз), иначе обычный выброс.
    pub(super) fn dispense_minecart(
        &self,
        item: &str,
        dispenser: [i32; 3],
        facing: usize,
    ) -> Option<bool> {
        let variant = self.cart_variant(item)?;
        let d = crate::placement::step(facing);
        let front = [
            dispenser[0] + d[0],
            dispenser[1] + d[1],
            dispenser[2] + d[2],
        ];
        let track = PlanTrack(self);
        let lift = if let Some(r) = track.rail(front) {
            if r.shape.is_slope() { 0.6 } else { 0.1 }
        } else if self
            .block(front)
            .is_some_and(|s| self.state_name(s).ends_with(":air"))
            && let Some(r) = track.rail([front[0], front[1] - 1, front[2]])
        {
            if facing == 0 || !r.shape.is_slope() {
                -0.9
            } else {
                -0.4
            }
        } else {
            return None;
        };
        let pos = [
            dispenser[0] as f64 + 0.5 + d[0] as f64 * 1.125,
            dispenser[1] as f64 + d[1] as f64 + lift,
            dispenser[2] as f64 + 0.5 + d[2] as f64 * 1.125,
        ];
        self.add_minecart(MinecartEntry::new(Cart::new(pos, 0.0), variant));
        Some(true)
    }

    /// Что сделает правый клик по вагонетке `cart`.
    pub(super) fn minecart_use(&self, cart: i32) -> CartUse {
        let carts = self.sim().minecarts.lock().unwrap();
        let Some(e) = carts.get(&cart) else {
            return CartUse::Pass;
        };
        match e.variant {
            Variant::Plain => CartUse::Ride,
            Variant::Chest | Variant::Hopper => {
                // `Entity.getDisplayName`: своё имя или имя типа.
                let title = e
                    .custom_name
                    .as_deref()
                    .map(|n| serde_json::from_str(n).unwrap_or_else(|_| serde_json::json!({ "text": n })))
                    .unwrap_or_else(|| serde_json::json!({ "translate": format!("entity.minecraft.{}", &e.variant.name()[10..]) }));
                CartUse::Open(e.slots.len(), title)
            }
            Variant::Furnace => CartUse::Fuel,
            Variant::CommandBlock => CartUse::CommandBlock,
            Variant::Tnt | Variant::Spawner => CartUse::Pass,
        }
    }

    /// `MinecartFurnace.addFuel`: уголь даёт 3600 тиков, если влезает в
    /// 32000; тяга — от игрока к вагонетке. `true` — предмет потрачен.
    pub(super) fn fuel_minecart(&self, cart: i32, player: [f64; 3], item: &str) -> bool {
        let fuel = self
            .survival
            .item_tags
            .get("minecraft:furnace_minecart_fuel")
            .is_some_and(|t| t.contains(item));
        if !fuel {
            return false;
        }
        let mut carts = self.sim().minecarts.lock().unwrap();
        let Some(e) = carts.get_mut(&cart) else {
            return false;
        };
        if e.fuel + FUEL_PER_ITEM > FUEL_MAX {
            return false;
        }
        e.fuel += FUEL_PER_ITEM;
        e.cart.push = [e.cart.pos[0] - player[0], e.cart.pos[2] - player[2]];
        drop(carts);
        self.mark_work();
        true
    }

    /// Инвентарь вагонетки с сундуком или воронкой.
    pub(super) fn minecart_slots(&self, cart: i32) -> Option<Vec<Slot>> {
        let carts = self.sim().minecarts.lock().unwrap();
        carts
            .get(&cart)
            .filter(|e| e.variant.size() > 0)
            .map(|e| e.slots.clone())
    }

    /// Записать инвентарь; заполненность меняет трение (`applyNaturalSlowdown`).
    pub(super) fn set_minecart_slots(&self, cart: i32, slots: &[Slot]) {
        let signal = crate::redstone::fullness(
            slots
                .iter()
                .flatten()
                .map(|s| (s.count, self.max_stack(s.item))),
            slots.len(),
        );
        if let Some(e) = self.sim().minecarts.lock().unwrap().get_mut(&cart)
            && e.variant.size() == slots.len()
        {
            e.slots = slots.to_vec();
            e.cart.drag = Drag::Container(signal as u8);
        }
    }

    /// `HopperBlockEntity.getEntityContainer`: вагонетка с инвентарём,
    /// задевающая клетку `p`.
    pub(super) fn minecart_cont_at(&self, p: [i32; 3]) -> Option<Cont> {
        let lo = p.map(f64::from);
        let hi = lo.map(|v| v + 1.0);
        let carts = self.sim().minecarts.lock().unwrap();
        let (&id, e) = carts.iter().find(|(_, e)| {
            let (a, b) = e.bounds();
            e.variant.size() > 0 && (0..3).all(|i| a[i] < hi[i] && b[i] > lo[i])
        })?;
        Some(Cont {
            parts: vec![cart_pos(id)],
            slots: e.slots.clone(),
            furnace: false,
            name: e.variant.name().into(),
        })
    }

    /// `DetectorRailBlock.getAnalogOutputSignal`: заполненность вагонетки
    /// с инвентарём в рамке, иначе 0.
    pub(super) fn minecart_signal_in_box(&self, min: [f64; 3], max: [f64; 3]) -> i32 {
        let slots = {
            let carts = self.sim().minecarts.lock().unwrap();
            let Some(e) = carts.values().find(|e| {
                let (lo, hi) = e.bounds();
                e.variant.size() > 0 && (0..3).all(|i| lo[i] < max[i] && hi[i] > min[i])
            }) else {
                return 0;
            };
            e.slots.clone()
        };
        crate::redstone::fullness(
            slots
                .iter()
                .flatten()
                .map(|s| (s.count, self.max_stack(s.item))),
            slots.len(),
        )
    }

    /// `Minecart.interact`: без шифта и в пустую — сесть.
    pub(super) fn mount_minecart(&self, uuid: Uuid, entity_id: i32, cart: i32) -> bool {
        let cart_uuid = {
            let mut carts = self.sim().minecarts.lock().unwrap();
            let Some(e) = carts.get_mut(&cart) else {
                return false;
            };
            if e.rider.is_some() || e.variant != Variant::Plain {
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
        if let Some(e) = self
            .sim()
            .minecarts
            .lock()
            .unwrap()
            .values_mut()
            .find(|e| e.rider.is_some_and(|r| r.0 == uuid))
        {
            e.input = input;
        }
    }

    /// Снять с вагонетки; место высадки (`AbstractMinecart.getDismountLocationForPassenger`).
    pub(super) fn leave_minecart(&self, uuid: Uuid) -> Option<[f64; 3]> {
        let (id, cart) = {
            let mut carts = self.sim().minecarts.lock().unwrap();
            let (&id, e) = carts
                .iter_mut()
                .find(|(_, e)| e.rider.is_some_and(|r| r.0 == uuid))?;
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
        let base = [
            c.pos[0].floor() as i32,
            c.pos[1].floor() as i32,
            c.pos[2].floor() as i32,
        ];
        // Направление: по скорости, а у стоящей — по рельсу.
        let dir: [i32; 2] = if c.vel[0].abs() > 1e-4 || c.vel[2].abs() > 1e-4 {
            if c.vel[0].abs() > c.vel[2].abs() {
                [c.vel[0].signum() as i32, 0]
            } else {
                [0, c.vel[2].signum() as i32]
            }
        } else {
            match PlanTrack(self)
                .rail(c.rail_pos(&PlanTrack(self)))
                .map(|r| r.shape)
            {
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
    pub(super) fn attack_minecart(
        &self,
        uuid: Uuid,
        target: i32,
        held: Slot,
        creative: bool,
        hit: super::mobs::MeleeHit,
    ) -> Option<()> {
        let pos = self.player_at(uuid)?.0;
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
        {
            let carts = self.sim().minecarts.lock().unwrap();
            let e = carts.get(&target)?;
            let d: f64 = (0..3).map(|i| (e.cart.pos[i] - pos[i]).powi(2)).sum();
            if d > 36.0 {
                return None;
            }
        }
        self.hurt_minecart(target, amount, creative, false);
        Some(())
    }

    /// `VehicleEntity.hurtServer`: урон ×10 копится; больше 40 — `destroy`.
    /// Творческий игрок убирает без добычи. `explosion` поджигает ТНТ.
    fn hurt_minecart(&self, target: i32, amount: f32, creative: bool, explosion: bool) {
        let now = self.game_time.load(Ordering::Relaxed);
        let mut carts = self.sim().minecarts.lock().unwrap();
        let Some(e) = carts.get_mut(&target) else {
            return;
        };
        e.hurt_dir = -e.hurt_dir;
        e.damage = e.damage_at(now) + amount * DAMAGE_SCALE;
        e.hit_at = now;
        let broken = !creative && e.damage > BREAK_AT;
        if !broken && !creative {
            let packet = inv::boat_data_hurt(
                self.ids.set_entity_data,
                target,
                HURT_TIME,
                e.hurt_dir,
                e.damage,
            );
            drop(carts);
            self.hub.send_about(target, packet);
            return;
        }
        // `MinecartTNT.destroy`: огонь, взрыв или скорость — поджечь
        // (фитиль 0–38) и оставить, иначе выпасть предметом.
        if broken && e.variant == Variant::Tnt && (explosion || e.speed_sq() >= 0.01) {
            if e.fuse < 0 && self.vars.flag("tnt_explodes") {
                let pos = e.cart.pos;
                e.fuse = rand::random::<u32>() as i32 % 20 + rand::random::<u32>() as i32 % 20;
                drop(carts);
                self.announce_prime(target, pos);
            }
            return;
        }
        let Some(e) = carts.remove(&target) else {
            return;
        };
        drop(carts);
        self.remove_minecart(target, e, broken);
    }

    /// Убрать вагонетку из мира; `drops` — предмет вагонетки и её инвентарь
    /// (`VehicleEntity.destroy`, `ContainerEntity.chestVehicleDestroyed`).
    fn remove_minecart(&self, id: i32, e: MinecartEntry, drops: bool) {
        if let Some((rider, _)) = e.rider {
            self.unlink_rider(rider);
        }
        self.hub.remove_tracked(&[id]);
        if !drops {
            return;
        }
        let pop = |stack: Stack| {
            let vel = items::pop_velocity(rand::random(), rand::random());
            self.drop_item(e.cart.pos, vel, stack, items::BLOCK_DELAY);
        };
        if self.vars.flag("entity_drops")
            && let Some(item) = self.item_id(e.variant.drop_name())
        {
            pop(Stack::new(item, 1));
        }
        for stack in e.slots.iter().flatten() {
            pop(*stack);
        }
    }

    /// `MinecartTNT.primeFuse`: событие клиенту и звук (фитиль ставит вызывающий).
    fn announce_prime(&self, id: i32, pos: [f64; 3]) {
        self.hub.send_about(
            id,
            inv::entity_event(self.ids.entity_event, id, EVENT_PRIME),
        );
        self.play_sound(
            None,
            "minecraft:entity.tnt.primed",
            crate::sounds::BLOCKS,
            pos,
            1.0,
            1.0,
        );
    }

    /// Взрыв задел вагонетки (`ServerExplosion.hurtEntities`): урон и отброс.
    pub(super) fn explode_minecarts(&self, center: [f64; 3], power: f32) {
        let reach = power as f64 * 2.0;
        let near: Vec<(i32, [f64; 3])> = self
            .sim()
            .minecarts
            .lock()
            .unwrap()
            .iter()
            .map(|(&id, e)| (id, e.cart.pos))
            .filter(|(_, p)| {
                (0..3).map(|i| (p[i] - center[i]).powi(2)).sum::<f64>() < reach * reach
            })
            .collect();
        for (id, pos) in near {
            let dist = (0..3)
                .map(|i| (pos[i] - center[i]).powi(2))
                .sum::<f64>()
                .sqrt();
            let exposure = self.explosion_exposure(center, pos);
            let damage = crate::tnt::damage(dist, exposure, power);
            if damage <= 0.0 {
                continue;
            }
            // Отброс: от центра, сила `(1 − d/r)·видимость`.
            if dist > 1.0e-4 {
                let k = (1.0 - dist / reach) * exposure / dist;
                if let Some(e) = self.sim().minecarts.lock().unwrap().get_mut(&id) {
                    for i in 0..3 {
                        e.cart.vel[i] += (pos[i] - center[i]) * k;
                    }
                }
            }
            self.hurt_minecart(id, damage, false, true);
        }
    }

    /// Тик вагонеток в симулируемых чанках: физика, толчки игроков,
    /// детекторные и активаторные рельсы, топливо печи, фитиль ТНТ,
    /// воронка, рассылка движения, позиция водителя.
    fn spawner_nearby(&self, pos: [f64; 3], range: i32, species: crate::mobs::Species) -> usize {
        let r = f64::from(range.max(0));
        self.sim()
            .mobs
            .lock()
            .unwrap()
            .values()
            .filter(|e| {
                e.mob.species == species && (0..3).all(|i| (e.mob.pos[i] - pos[i]).abs() <= r)
            })
            .count()
    }

    /// `MinecartSpawner.tick` → `BaseSpawner.serverTick`: задержка, игрок в
    /// `RequiredPlayerRange`, до `SpawnCount` попыток в окне `SpawnRange`,
    /// лимит `MaxNearbyEntities`, затем новая случайная задержка.
    fn tick_spawner(&self, id: i32, e: &mut MinecartEntry, players: &[(Uuid, [f64; 3], f32)]) {
        let pos = e.cart.pos;
        let s = &mut e.spawner;
        if s.spawn_count <= 0 || s.max_nearby <= 0 {
            return;
        }
        if s.delay > 0 {
            s.delay -= 1;
            return;
        }
        let range = f64::from(s.required_player_range.max(0));
        if !players
            .iter()
            .any(|(_, p, _)| crate::mobs::dist2(*p, pos) <= range * range)
        {
            return;
        }
        let Some(data) = s.next_data(rand::random()) else {
            return;
        };
        let Some((species, entity)) = spawner_entity(&data) else {
            return;
        };
        let spawn_range = f64::from(s.spawn_range.max(0));
        let mut spawned = false;
        for _ in 0..s.spawn_count.max(0) {
            let x = pos[0] + (rand::random::<f64>() - rand::random::<f64>()) * spawn_range + 0.5;
            let y = pos[1] + f64::from((rand::random::<u32>() % 3) as i32 - 1);
            let z = pos[2] + (rand::random::<f64>() - rand::random::<f64>()) * spawn_range + 0.5;
            let cell = [x.floor() as i32, y.floor() as i32, z.floor() as i32];
            if !self.spawner_fits(species, cell) {
                continue;
            }
            if self.spawner_nearby(pos, s.spawn_range, species) >= s.max_nearby as usize {
                break;
            }
            let yaw = rand::random::<f32>() * 360.0;
            if self
                .spawn_mob_spawner(species, [x, y, z], yaw, entity)
                .is_some()
            {
                spawned = true;
            }
        }
        if spawned {
            // `MinecartSpawner$1.broadcastEvent` → `broadcastEntityEvent`: у
            // вагонетки событие 1, не блочное 2004.
            self.hub.send_about(
                id,
                inv::entity_event(self.ids.entity_event, id, SPAWNER_EVENT),
            );
        }
        let span = s.max_delay.saturating_sub(s.min_delay).max(0) + 1;
        let delay = s.min_delay + (rand::random::<f64>() * f64::from(span)) as i32;
        s.delay = delay.max(1);
    }

    pub(super) fn tick_minecarts(&self) {
        if self.sim().minecarts.lock().unwrap().is_empty() {
            return;
        }
        let simulated = self.simulated_set();
        let awake = |p: [f64; 3]| {
            simulated.is_empty()
                || simulated.contains(&((p[0].floor() as i32) >> 4, (p[2].floor() as i32) >> 4))
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
        let mut hoppers = Vec::new();
        let mut primed = Vec::new();
        let mut commands: Vec<(i32, String, [f64; 3])> = Vec::new();
        let now = self.game_time.load(Ordering::Relaxed);
        let border = self.border_at(now);
        // Взорвавшиеся ТНТ: позиция, квадрат скорости, был ли фитиль.
        let mut blasts = Vec::new();
        {
            let mut carts = self.sim().minecarts.lock().unwrap();
            let ids: Vec<i32> = carts.keys().copied().collect();
            for id in ids {
                let Some(e) = carts.get_mut(&id) else {
                    continue;
                };
                if !awake(e.cart.pos) {
                    continue;
                }
                // `AbstractMinecart.push`: игрок, чья коробка задевает вагонетку, толкает её.
                for &(uuid, p, _) in &players {
                    if e.rider.is_some_and(|r| r.0 == uuid) {
                        continue;
                    }
                    let (lo, hi) = e.bounds();
                    let overlap = p[0] + 0.3 > lo[0]
                        && p[0] - 0.3 < hi[0]
                        && p[2] + 0.3 > lo[2]
                        && p[2] - 0.3 < hi[2]
                        && p[1] + 1.8 > lo[1]
                        && p[1] < hi[1];
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
                let intent = e
                    .rider
                    .and_then(|(r, _)| players.iter().find(|p| p.0 == r))
                    .map_or([0.0; 2], |p| {
                        crate::minecart::move_intent(
                            e.input & IN_FORWARD != 0,
                            e.input & IN_BACK != 0,
                            e.input & IN_LEFT != 0,
                            e.input & IN_RIGHT != 0,
                            p.2,
                        )
                    });
                let old_pos = e.cart.pos;
                let y0 = old_pos[1];
                let step = e.cart.tick(&track, intent);
                if let Some((rider, _)) = e.rider {
                    let dx = e.cart.pos[0] - old_pos[0];
                    let dy = e.cart.pos[1] - old_pos[1];
                    let dz = e.cart.pos[2] - old_pos[2];
                    let cm = ((dx * dx + dy * dy + dz * dz).sqrt() * 100.0).round() as i32;
                    if cm > 0 {
                        self.award_stat(rider, "minecraft:custom", "minecraft:minecart_one_cm", cm);
                    }
                }
                let clipped =
                    border.clip_movement(now, old_pos, e.cart.pos, crate::minecart::WIDTH);
                for axis in [0, 2] {
                    if clipped[axis] != e.cart.pos[axis] {
                        e.cart.pos[axis] = clipped[axis];
                        e.cart.vel[axis] = 0.0;
                    }
                }
                match step.rail {
                    Some((
                        pos,
                        Rail {
                            kind: Kind::Detector,
                            ..
                        },
                    )) => pressed.push(pos),
                    // `activateMinecart` каждый тик на активаторе.
                    Some((
                        _,
                        Rail {
                            kind: Kind::Activator(on),
                            ..
                        },
                    )) => match e.variant {
                        // `Minecart`: включённый высаживает.
                        Variant::Plain if on => {
                            if let Some((rider, _)) = e.rider.take() {
                                ejected.push((id, rider));
                            }
                        }
                        // `MinecartHopper`: включённый выключает воронку.
                        Variant::Hopper => e.enabled = !on,
                        // `MinecartTNT.primeFuse`: без `tnt_explodes` не поджигается.
                        Variant::Tnt if on && e.fuse < 0 && self.vars.flag("tnt_explodes") => {
                            e.fuse = TNT_FUSE;
                            primed.push((id, e.cart.pos));
                        }
                        // `MinecartCommandBlock.activateMinecart`: повторная
                        // активация не чаще, чем раз в 4 тика.
                        Variant::CommandBlock
                            if on
                                && command_ready(e.last_activated, now)
                                && self.vars.flag("command_blocks_work") =>
                        {
                            e.last_activated = now;
                            commands.push((id, e.command.clone(), e.cart.pos));
                        }
                        _ => {}
                    },
                    _ => {}
                }
                // `OldMinecartBehavior.pushAndPickupEntities`: в конце своего
                // тика вагонетка толкает другие, задевающие её коробку с
                // запасом 0.2 по горизонтали.
                let (lo, hi) = e.bounds();
                let (lo, hi) = (
                    [lo[0] - 0.2, lo[1], lo[2] - 0.2],
                    [hi[0] + 0.2, hi[1], hi[2] + 0.2],
                );
                let near: Vec<i32> = carts
                    .iter()
                    .filter(|(o, b)| {
                        let (a, z) = b.bounds();
                        **o != id && (0..3).all(|i| a[i] < hi[i] && z[i] > lo[i])
                    })
                    .map(|(o, _)| *o)
                    .collect();
                for o in near {
                    if let [Some(a), Some(b)] = carts.get_disjoint_mut([&id, &o]) {
                        crate::minecart::push_pair(&mut a.cart, &mut b.cart);
                    }
                }
                let Some(e) = carts.get_mut(&id) else {
                    continue;
                };
                // Падение: на рельсе сбрасывается, о землю — `causeFallDamage`.
                let mut landed = 0.0;
                if step.rail.is_some() {
                    e.fall = 0.0;
                } else if e.cart.pos[1] < y0 {
                    e.fall += y0 - e.cart.pos[1];
                } else if e.cart.on_ground {
                    landed = std::mem::take(&mut e.fall);
                }
                match e.variant {
                    // `MinecartFurnace.tick`: топливо тратится, без него тяга пропадает.
                    Variant::Furnace => {
                        if e.fuel > 0 {
                            e.fuel -= 1;
                        }
                        if e.fuel <= 0 {
                            e.cart.push = [0.0; 2];
                        }
                        if e.lit != (e.fuel > 0) {
                            e.lit = e.fuel > 0;
                            packets.push((
                                id,
                                inv::furnace_cart_fuel(self.ids.set_entity_data, id, e.lit),
                            ));
                        }
                    }
                    // `MinecartTNT.tick`: фитиль, удар о стену на скорости,
                    // падение с трёх блоков.
                    Variant::Tnt => {
                        let speed = e.speed_sq();
                        if e.fuse > 0 {
                            e.fuse -= 1;
                        } else if e.fuse == 0 {
                            blasts.push((id, e.cart.pos, speed, true));
                            continue;
                        }
                        if step.collided && speed >= 0.01 {
                            blasts.push((id, e.cart.pos, speed, e.fuse >= 0));
                            continue;
                        }
                        if landed >= 3.0 {
                            let f = landed / 10.0;
                            blasts.push((id, e.cart.pos, f * f, e.fuse >= 0));
                            continue;
                        }
                    }
                    Variant::Hopper if e.enabled => hoppers.push((id, e.cart.pos)),
                    Variant::Spawner => self.tick_spawner(id, e, &players),
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
                    packets.push((
                        id,
                        play::teleport_entity(
                            self.ids.teleport_entity,
                            id,
                            e.cart.pos,
                            e.cart.yaw,
                            0.0,
                            e.cart.on_ground,
                        ),
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
        for (id, line, pos) in commands {
            let line = line.trim().trim_start_matches('/');
            if line.is_empty() {
                continue;
            }
            let source = crate::play::commands::dispatch::Source {
                uuid: None,
                name: "CommandBlock".into(),
                level: 2,
            };
            let outcome = self.run_command_at(&source, line, pos, 0.0, 0.0);
            let (text, success) = match outcome {
                Some(crate::play::commands::dispatch::Outcome::Feedback(value))
                | Some(crate::play::commands::dispatch::Outcome::Broadcast(value)) => {
                    (Some(self.command_text(&value)), 1)
                }
                Some(crate::play::commands::dispatch::Outcome::Silent) => (None, 1),
                None => (Some(format!("Unknown command: {line}")), 0),
            };
            if let Some(e) = self.sim().minecarts.lock().unwrap().get_mut(&id) {
                e.success_count = success;
                if e.track_output {
                    e.last_output = text;
                }
            }
        }
        for (id, pos) in primed {
            self.announce_prime(id, pos);
        }
        let mut gone = Vec::new();
        for (id, rider) in ejected {
            if rider.is_nil() {
                gone.push(id);
            } else {
                self.unlink_rider(rider);
                packets.push((id, play::set_passengers(self.ids.set_passengers, id, &[])));
            }
        }
        if !packets.is_empty() {
            self.hub.send_many_about(packets);
        }
        self.hub.remove_tracked(&gone);
        // Водитель едет с вагонеткой: сессия по этой позиции грузит чанки.
        for (uuid, pos) in riders {
            self.moved(
                uuid,
                Move {
                    pos: Some(pos),
                    rot: None,
                    on_ground: true,
                },
            );
        }
        for pos in pressed {
            self.press_detector_rail(pos);
        }
        for (id, pos) in hoppers {
            self.minecart_hopper(id, pos);
        }
        for (id, pos, speed, lit) in blasts {
            self.explode_minecart(id, pos, speed, lit);
        }
    }

    /// `MinecartHopper.suckInItems`: контейнер над вагонеткой (клетка на
    /// 1.5 выше её низа), без него — предметы в зоне над ней; затем
    /// предметы, задевающие саму вагонетку.
    fn minecart_hopper(&self, id: i32, pos: [f64; 3]) {
        let Some(slots) = self.minecart_slots(id) else {
            return;
        };
        let mut me = Cont {
            parts: vec![cart_pos(id)],
            slots,
            furnace: false,
            name: Variant::Hopper.name().into(),
        };
        let full = me
            .slots
            .iter()
            .all(|s| s.is_some_and(|s| s.count >= self.max_stack(s.item)));
        if full {
            return;
        }
        let cell = [
            pos[0].floor() as i32,
            (pos[1] + 1.5).floor() as i32,
            pos[2].floor() as i32,
        ];
        let mut moved = match self.cont_get(cell) {
            Some(mut a) => self.hopper_pull(&mut a, &mut me),
            None => {
                // `getSuckAabb` (0, 11/16, 0)–(1, 2, 1) со сдвигом на (x−½, y, z−½).
                let lo = [pos[0] - 0.5, pos[1] + 0.6875, pos[2] - 0.5];
                let hi = [pos[0] + 0.5, pos[1] + 2.0, pos[2] + 0.5];
                self.hopper_suck(cell, &mut me, |d| item_in(lo, hi, d))
            }
        };
        if !moved {
            let h = crate::minecart::WIDTH / 2.0 + 0.25;
            let lo = [pos[0] - h, pos[1], pos[2] - h];
            let hi = [pos[0] + h, pos[1] + crate::minecart::HEIGHT, pos[2] + h];
            let own = [
                pos[0].floor() as i32,
                pos[1].floor() as i32,
                pos[2].floor() as i32,
            ];
            moved = self.hopper_suck(own, &mut me, |d| item_in(lo, hi, d));
        }
        if moved {
            self.cont_put(&me);
        }
    }

    /// `MinecartTNT.explode`: сила растёт со скоростью; без `tnt_explodes`
    /// подожжённая просто исчезает, неподожжённая остаётся.
    fn explode_minecart(&self, id: i32, pos: [f64; 3], speed_sq: f64, lit: bool) {
        let explodes = self.vars.flag("tnt_explodes");
        if !explodes && !lit {
            return;
        }
        let Some(e) = self.sim().minecarts.lock().unwrap().remove(&id) else {
            return;
        };
        self.remove_minecart(id, e, false);
        if explodes {
            let k = speed_sq.sqrt().min(5.0);
            let power = TNT_POWER + TNT_SPEED_FACTOR * (rand::random::<f64>() * 1.5 * k) as f32;
            self.explode(pos, power, lit);
        }
    }

    /// `DetectorRailBlock.entityInside` → `checkPressed`: включить и
    /// перепроверять раз в 20 тиков.
    fn press_detector_rail(&self, pos: [i32; 3]) {
        let pl = &self.survival.placement;
        let Some(s) = self.block(pos) else { return };
        if !crate::redstone::is_detector_rail(self.state_name(s))
            || pl.prop(s, "powered") == Some("true")
        {
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

    /// Поля `Entity`/`AbstractMinecart` из NBT (`loadAdditional`): `Motion`,
    /// `Rotation`, `UUID`, `CustomName`, `Items` и данные варианта.
    fn minecart_from_nbt(&self, variant: Variant, pos: [f64; 3], c: &Compound) -> MinecartEntry {
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
        let yaw = list("Rotation").first().copied().unwrap_or(0.0) as f32;
        let mut cart = Cart::new(pos, yaw);
        if let [x, y, z] = list("Motion")[..]
            && [x, y, z].iter().all(|v| v.is_finite())
        {
            cart.vel = [x, y, z];
        }
        let mut e = MinecartEntry::new(cart, variant);
        if let Some(Tag::IntArray(u)) = c.get("UUID")
            && u.len() == 4
        {
            e.uuid = Uuid::from_u128(u.iter().fold(0u128, |acc, &w| acc << 32 | w as u32 as u128));
        }
        if let Some(Tag::String(n)) = c.get("CustomName") {
            e.custom_name = Some(n.clone());
        }
        // `ContainerHelper.loadAllItems`.
        for t in c.get("Items").and_then(Tag::as_list).unwrap_or(&[]) {
            let Some(item) = t.as_compound() else {
                continue;
            };
            let slot = item.get("Slot").and_then(Tag::as_i64).unwrap_or(-1);
            if let Some(dst) = usize::try_from(slot).ok().and_then(|i| e.slots.get_mut(i)) {
                *dst = crate::stack_nbt::from_nbt(item, |n| self.item_id(n));
            }
        }
        if variant.size() > 0 {
            let signal = crate::redstone::fullness(
                e.slots
                    .iter()
                    .flatten()
                    .map(|s| (s.count, self.max_stack(s.item))),
                e.slots.len(),
            );
            e.cart.drag = Drag::Container(signal as u8);
        }
        let int = |k: &str| c.get(k).and_then(Tag::as_i64);
        let float = |k: &str| match c.get(k) {
            Some(Tag::Double(v)) => Some(*v),
            Some(Tag::Float(v)) => Some(*v as f64),
            _ => None,
        };
        match variant {
            Variant::Hopper => e.enabled = int("Enabled").is_none_or(|v| v != 0),
            Variant::Furnace => {
                e.fuel = int("Fuel").unwrap_or(0) as i32;
                e.lit = e.fuel > 0;
                e.cart.push = [float("PushX").unwrap_or(0.0), float("PushZ").unwrap_or(0.0)];
            }
            Variant::Tnt => e.fuse = int("fuse").unwrap_or(-1) as i32,
            Variant::Spawner => e.spawner = SpawnerState::load(c),
            Variant::CommandBlock => {
                e.command = c
                    .get("Command")
                    .and_then(Tag::as_str)
                    .unwrap_or_default()
                    .to_owned();
                e.track_output = c
                    .get("TrackOutput")
                    .and_then(Tag::as_i64)
                    .is_none_or(|v| v != 0);
                e.success_count = int("SuccessCount").unwrap_or(0) as i32;
                e.last_output = c.get("LastOutput").and_then(Tag::as_str).map(str::to_owned);
            }
            _ => {}
        }
        e
    }

    /// Вагонетки из `entities/*.mca`.
    pub(super) fn load_minecart(&self, c: &Compound) -> bool {
        let Some(variant) = (match c.get("id") {
            Some(Tag::String(id)) => Variant::of(id),
            _ => None,
        }) else {
            return false;
        };
        let pos = match c.get("Pos").and_then(Tag::as_list) {
            Some(list) => list
                .iter()
                .filter_map(|t| match t {
                    Tag::Double(v) => Some(*v),
                    Tag::Float(v) => Some(*v as f64),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            None => Vec::new(),
        };
        if pos.len() != 3 || !pos.iter().all(|v| v.is_finite()) {
            return false;
        }
        let e = self.minecart_from_nbt(variant, [pos[0], pos[1], pos[2]], c);
        self.add_minecart(e);
        true
    }

    /// Вагонетки для записи, по чанкам.
    pub(super) fn minecarts_nbt(&self) -> Vec<((i32, i32), Compound)> {
        let carts: Vec<_> = self
            .sim()
            .minecarts
            .lock()
            .unwrap()
            .values()
            .map(|e| {
                let c0 = &e.cart;
                let mut c = Compound::new();
                c.insert("id".into(), Tag::String(e.variant.name().into()));
                c.insert(
                    "Pos".into(),
                    Tag::List(c0.pos.iter().map(|v| Tag::Double(*v)).collect()),
                );
                c.insert(
                    "Motion".into(),
                    Tag::List(c0.vel.iter().map(|v| Tag::Double(*v)).collect()),
                );
                c.insert(
                    "Rotation".into(),
                    Tag::List(vec![Tag::Float(c0.yaw), Tag::Float(0.0)]),
                );
                let u = e.uuid.as_u128();
                c.insert(
                    "UUID".into(),
                    Tag::IntArray(
                        (0..4)
                            .rev()
                            .map(|i| (u >> (32 * i)) as u32 as i32)
                            .collect(),
                    ),
                );
                if let Some(n) = &e.custom_name {
                    c.insert("CustomName".into(), Tag::String(n.clone()));
                }
                match e.variant {
                    Variant::Hopper => {
                        c.insert("Enabled".into(), Tag::Byte(i8::from(e.enabled)));
                    }
                    Variant::Furnace => {
                        c.insert("PushX".into(), Tag::Double(c0.push[0]));
                        c.insert("PushZ".into(), Tag::Double(c0.push[1]));
                        c.insert("Fuel".into(), Tag::Short(e.fuel as i16));
                    }
                    Variant::Tnt => {
                        c.insert("fuse".into(), Tag::Int(e.fuse));
                    }
                    Variant::Spawner => e.spawner.save(&mut c),
                    Variant::CommandBlock => {
                        c.insert("Command".into(), Tag::String(e.command.clone()));
                        c.insert("TrackOutput".into(), Tag::Byte(i8::from(e.track_output)));
                        c.insert("SuccessCount".into(), Tag::Int(e.success_count));
                        if let Some(out) = &e.last_output {
                            c.insert("LastOutput".into(), Tag::String(out.clone()));
                        }
                    }
                    _ => {}
                }
                (
                    (c0.pos[0].floor() as i32 >> 4, c0.pos[2].floor() as i32 >> 4),
                    c,
                    e.slots.clone(),
                )
            })
            .collect();
        // `Items` — после замка: имена предметов берутся из реестра.
        carts
            .into_iter()
            .map(|(chunk, mut c, slots)| {
                if !slots.is_empty() {
                    c.insert("Items".into(), Tag::List(self.items_nbt(&slots)));
                }
                (chunk, c)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_block_minecart_variant_and_drop() {
        assert_eq!(
            Variant::of("minecraft:command_block_minecart"),
            Some(Variant::CommandBlock)
        );
        assert_eq!(
            Variant::CommandBlock.name(),
            "minecraft:command_block_minecart"
        );
        assert_eq!(Variant::CommandBlock.drop_name(), "minecraft:minecart");
        assert_eq!(Variant::CommandBlock.size(), 0);
        assert_eq!(Variant::ALL.len(), 7);
        assert_eq!(
            Variant::of("minecraft:spawner_minecart"),
            Some(Variant::Spawner)
        );
        assert_eq!(Variant::Spawner.drop_name(), "minecraft:minecart");
        assert_eq!(Variant::Spawner.name(), "minecraft:spawner_minecart");
        assert_eq!(SPAWNER_EVENT, 1);
    }

    #[test]
    fn spawner_state_reads_nbt_and_picks_weighted_data() {
        let zombie = Compound::from([("id".into(), Tag::String("minecraft:zombie".into()))]);
        let skeleton = Compound::from([(
            "entity".into(),
            Tag::Compound(Compound::from([(
                "id".into(),
                Tag::String("minecraft:skeleton".into()),
            )])),
        )]);
        let c = Compound::from([
            ("Delay".into(), Tag::Short(7)),
            ("MinSpawnDelay".into(), Tag::Short(20)),
            ("MaxSpawnDelay".into(), Tag::Short(40)),
            ("SpawnCount".into(), Tag::Short(2)),
            ("MaxNearbyEntities".into(), Tag::Short(3)),
            ("RequiredPlayerRange".into(), Tag::Short(9)),
            ("SpawnRange".into(), Tag::Short(2)),
            ("SpawnData".into(), Tag::Compound(zombie)),
            (
                "SpawnPotentials".into(),
                Tag::List(vec![
                    Tag::Compound(Compound::from([
                        ("weight".into(), Tag::Int(1)),
                        ("data".into(), Tag::Compound(skeleton.clone())),
                    ])),
                    Tag::Compound(Compound::from([
                        ("weight".into(), Tag::Int(3)),
                        ("data".into(), Tag::Compound(skeleton)),
                    ])),
                ]),
            ),
        ]);
        let s = SpawnerState::load(&c);
        assert_eq!(
            (
                s.delay,
                s.min_delay,
                s.max_delay,
                s.spawn_count,
                s.max_nearby,
                s.required_player_range,
                s.spawn_range
            ),
            (7, 20, 40, 2, 3, 9, 2)
        );
        let picked = s.next_data(0.0).unwrap();
        let (species, _) = spawner_entity(&picked).unwrap();
        assert_eq!(species, crate::mobs::Species::Skeleton);
        assert_eq!(
            spawner_entity(&s.spawn_data.clone().unwrap()).unwrap().0,
            crate::mobs::Species::Zombie
        );
        let mut back = Compound::new();
        s.save(&mut back);
        assert_eq!(SpawnerState::load(&back).spawn_count, 2);
    }

    #[test]
    fn command_block_activation_respects_delay() {
        assert!(!command_ready(100, 103));
        assert!(command_ready(100, 104));
        assert!(command_ready(100, 105));
        // Стартовое значение не мешает первому срабатыванию.
        assert!(command_ready(i64::MIN / 2, 0));
    }
}
