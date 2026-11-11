//! Стойки для брони (`ArmorStand`, `ArmorStandItem`).
//!
//! Стойка ставится на верхнюю грань блока (не вниз), занимает 0.5×1.975 блоков
//! и поворачивается на ближайшие 45° от направления игрока. Снаряжение хранится
//! в шести слотах с теми же ID, что в `set_equipment`: 0 — основная рука,
//! 1 — ступни, 2 — ноги, 3 — грудь, 4 — голова, 5 — вторая рука.
//!
//! NBT: `ArmorItems` (ступни, ноги, грудь, голова), `HandItems` (руки),
//! `Invisible`, `Small`, `ShowArms`, `DisabledSlots`, `NoBasePlate`, `Marker`,
//! `Pose` (шесть троек углов). Позы передаются метаданными (индексы 16..21,
//! сериализатор `ROTATIONS` = 9).
//!
//! `DisabledSlots`: бит `ID` закрывает слот для взаимодействия, `ID + 8`
//! запрещает снимать непустой, `ID + 16` — класть в пустой. Руки дополнительно
//! закрыты, пока не включён `ShowArms`.

use super::*;
use play::Meta;

/// `EquipmentSlot` ID в `set_equipment`.
const MAINHAND: usize = 0;
const FEET: usize = 1;
const LEGS: usize = 2;
const CHEST: usize = 3;
const HEAD: usize = 4;
const OFFHAND: usize = 5;

/// `EntityType.Builder.sized(0.5, 1.975)`.
const WIDTH: f64 = 0.5;
const HEIGHT: f64 = 1.975;
/// `BABY_DIMENSIONS = ARMOR_STAND.getDimensions().scale(0.5)`.
const SMALL_HEIGHT: f64 = HEIGHT / 2.0;

/// `ArmorStand.DATA_CLIENT_FLAGS`: после 8 полей `Entity` и 7 полей
/// `LivingEntity` (всего 15).
const META_FLAGS: u8 = 15;
/// `DATA_HEAD_POSE`..`DATA_RIGHT_LEG_POSE`.
const META_POSE: u8 = 16;

const FLAG_SMALL: u8 = 1;
const FLAG_SHOW_ARMS: u8 = 4;
const FLAG_NO_BASE_PLATE: u8 = 8;
const FLAG_MARKER: u8 = 16;

/// `DEFAULT_*_POSE`: голова, тело, левая рука, правая рука, левая нога, правая нога.
const DEFAULT_POSE: [[f32; 3]; 6] = [
    [0.0, 0.0, 0.0],
    [0.0, 0.0, 0.0],
    [-10.0, 0.0, -10.0],
    [-15.0, 0.0, 10.0],
    [-1.0, 0.0, -1.0],
    [1.0, 0.0, 1.0],
];
const POSE_KEYS: [&str; 6] = ["Head", "Body", "LeftArm", "RightArm", "LeftLeg", "RightLeg"];

pub(crate) struct ArmorStand {
    pub uuid: Uuid,
    /// Точка постановки: центр нижней грани (`Vec3.atBottomCenterOf`).
    pub pos: [f64; 3],
    pub yaw: f32,
    /// Снаряжение по ID слота из `set_equipment`.
    pub items: [Slot; 6],
    pub invisible: bool,
    pub small: bool,
    pub show_arms: bool,
    pub no_base_plate: bool,
    pub marker: bool,
    pub disabled_slots: i32,
    pub pose: [[f32; 3]; 6],
    custom_name: Option<Tag>,
    /// `lastHit`: тик последнего удара для «двух ударов подряд».
    last_hit: i64,
}

impl ArmorStand {
    fn new(uuid: Uuid, pos: [f64; 3], yaw: f32) -> Self {
        Self {
            uuid,
            pos,
            yaw,
            items: [None; 6],
            invisible: false,
            small: false,
            show_arms: false,
            no_base_plate: false,
            marker: false,
            disabled_slots: 0,
            pose: DEFAULT_POSE,
            custom_name: None,
            last_hit: -1000,
        }
    }

    fn height(&self) -> f64 {
        if self.marker {
            0.0
        } else if self.small {
            SMALL_HEIGHT
        } else {
            HEIGHT
        }
    }

    fn center(&self) -> [f64; 3] {
        [self.pos[0], self.pos[1] + self.height() / 2.0, self.pos[2]]
    }

    fn boxed(&self) -> [[f64; 3]; 2] {
        let w = if self.marker {
            0.0
        } else if self.small {
            WIDTH / 4.0
        } else {
            WIDTH / 2.0
        };
        [
            [self.pos[0] - w, self.pos[1], self.pos[2] - w],
            [
                self.pos[0] + w,
                self.pos[1] + self.height(),
                self.pos[2] + w,
            ],
        ]
    }

    fn client_flags(&self) -> u8 {
        (u8::from(self.small) * FLAG_SMALL)
            | (u8::from(self.show_arms) * FLAG_SHOW_ARMS)
            | (u8::from(self.no_base_plate) * FLAG_NO_BASE_PLATE)
            | (u8::from(self.marker) * FLAG_MARKER)
    }

    /// `isDisabled(slot)`.
    fn is_disabled(&self, slot: usize) -> bool {
        self.disabled_slots & (1 << slot) != 0
            || (slot == MAINHAND || slot == OFFHAND) && !self.show_arms
    }

    /// `getClickedSlot(location)`: какую часть стойки нажали по высоте.
    fn clicked_slot(&self, y: f64) -> usize {
        let y = y / if self.small { 0.5 } else { 1.0 };
        if y >= 0.1 {
            let d = if self.small { 0.8 } else { 0.45 };
            if y < 0.1 + d && self.items[FEET].is_some() {
                return FEET;
            }
        }
        let d = if self.small { 0.3 } else { 0.0 };
        if y >= 0.9 + d {
            let d2 = if self.small { 1.0 } else { 0.7 };
            if y < 0.9 + d2 && self.items[CHEST].is_some() {
                return CHEST;
            }
        }
        if y >= 0.4 {
            let d3 = if self.small { 1.0 } else { 0.8 };
            if y < 0.4 + d3 && self.items[LEGS].is_some() {
                return LEGS;
            }
        }
        if y >= 1.6 && self.items[HEAD].is_some() {
            return HEAD;
        }
        if self.items[MAINHAND].is_some() {
            return MAINHAND;
        }
        if self.items[OFFHAND].is_none() {
            return MAINHAND;
        }
        OFFHAND
    }
}

/// `Direction` ID для грани клика.
fn direction_offset(face: i32) -> Option<[i32; 3]> {
    play::direction_offset(face)
}

/// `Mth.wrapDegrees`.
fn wrap_degrees(mut v: f32) -> f32 {
    v %= 360.0;
    if v >= 180.0 {
        v -= 360.0;
    }
    if v < -180.0 {
        v += 360.0;
    }
    v
}

/// `ArmorStandItem.useOn`: ближайшие 45° от направления игрока.
fn yaw_from_player(yaw: f32) -> f32 {
    ((wrap_degrees(yaw - 180.0) + 22.5) / 45.0).floor() * 45.0
}

fn intersects(a: [[f64; 3]; 2], b: [[f64; 3]; 2]) -> bool {
    (0..3).all(|i| a[0][i] < b[1][i] && a[1][i] > b[0][i])
}

/// Целые блоки, пересекающиеся с коробкой.
fn blocks_in(b: [[f64; 3]; 2]) -> Vec<[i32; 3]> {
    let lo: [i32; 3] = std::array::from_fn(|i| b[0][i].floor() as i32);
    let hi: [i32; 3] = std::array::from_fn(|i| b[1][i].ceil() as i32 - 1);
    let mut out = Vec::new();
    for x in lo[0]..=hi[0] {
        for y in lo[1]..=hi[1] {
            for z in lo[2]..=hi[2] {
                out.push([x, y, z]);
            }
        }
    }
    out
}

impl PlayPlan {
    /// ID слота `set_equipment` для предмета (`getEquipmentSlotForItem`).
    fn equip_slot(&self, item: i32) -> Option<usize> {
        match self.survival.items.get(item as usize).and_then(|i| i.equip) {
            Some("feet") => Some(FEET),
            Some("legs") => Some(LEGS),
            Some("chest") => Some(CHEST),
            Some("head") => Some(HEAD),
            Some("offhand") => Some(OFFHAND),
            _ => None,
        }
    }

    fn armor_stand_packets(&self, id: i32, st: &ArmorStand) -> Vec<Vec<u8>> {
        let Some(&ty) = self.survival.entity_types.get("minecraft:armor_stand") else {
            return Vec::new();
        };
        let mut out = vec![play::add_entity_moving(
            self.ids.add_entity,
            id,
            st.uuid,
            ty,
            st.pos,
            [0.0; 3],
            st.yaw,
            0.0,
            0,
        )];
        let flags = st.client_flags();
        if flags != 0 {
            out.push(play::entity_meta(
                self.ids.set_entity_data,
                id,
                &[(META_FLAGS, Meta::Byte(flags))],
            ));
        }
        let poses: Vec<(u8, Meta)> = st
            .pose
            .iter()
            .enumerate()
            .map(|(i, r)| (META_POSE + i as u8, Meta::Rotations(*r)))
            .collect();
        out.push(play::entity_meta(self.ids.set_entity_data, id, &poses));
        if st.invisible {
            out.push(inv::set_entity_flags(self.ids.set_entity_data, id, 0x20));
        }
        let equipped: Vec<(u8, Slot)> = st
            .items
            .iter()
            .enumerate()
            .filter_map(|(slot, s)| s.map(|s| (slot as u8, Some(s))))
            .collect();
        if !equipped.is_empty() {
            out.push(play::set_equipment(self.ids.set_equipment, id, &equipped));
        }
        out
    }

    fn armor_stand_tracked(&self, id: i32, st: &ArmorStand) -> Tracked {
        let ty = self
            .survival
            .entity_types
            .get("minecraft:armor_stand")
            .copied()
            .unwrap_or_default();
        Tracked {
            id,
            pos: st.center(),
            range: self.tracking_range(ty),
        }
    }

    fn add_armor_stand(&self, st: ArmorStand) -> i32 {
        let id = self.next_entity_id.fetch_add(1, Ordering::Relaxed);
        let packets = self.armor_stand_packets(id, &st);
        let tracked = self.armor_stand_tracked(id, &st);
        self.sim().armor_stands.lock().unwrap().insert(id, st);
        self.hub.spawn_tracked(tracked, packets);
        id
    }

    pub(super) fn track_armor_stands(&self, out: &mut Vec<Tracked>) {
        for (&id, st) in self.sim().armor_stands.lock().unwrap().iter() {
            out.push(self.armor_stand_tracked(id, st));
        }
    }

    /// Пакеты стойки для игрока, который только что её увидел.
    pub(super) fn armor_stand_spawn(&self, id: i32) -> Option<Vec<Vec<u8>>> {
        let stands = self.sim().armor_stands.lock().unwrap();
        Some(self.armor_stand_packets(id, stands.get(&id)?))
    }

    pub(super) fn is_armor_stand(&self, id: i32) -> bool {
        self.sim().armor_stands.lock().unwrap().contains_key(&id)
    }

    /// `ArmorStandItem.useOn`: стойка на блок.
    pub(super) fn place_armor_stand(
        &self,
        uuid: Uuid,
        clicked: [i32; 3],
        face: i32,
        adventure: bool,
    ) -> bool {
        // `Direction.DOWN` не подходит.
        if face == 0 {
            return false;
        }
        let Some(d) = direction_offset(face) else {
            return false;
        };
        let Some((_, yaw, _)) = self.player_at(uuid) else {
            return false;
        };
        let pos = [clicked[0] + d[0], clicked[1] + d[1], clicked[2] + d[2]];
        if self.block(pos).is_none() || adventure || self.spawn_blocked(uuid, pos) {
            return false;
        }
        let at = [pos[0] as f64 + 0.5, pos[1] as f64, pos[2] as f64 + 0.5];
        let mut st = ArmorStand::new(Uuid::from_u128(rand::random()), at, yaw_from_player(yaw));
        if !self.armor_stand_fits(&st, None) {
            return false;
        }
        st.last_hit = self.game_time.load(Ordering::Relaxed) - 1000;
        let center = st.center();
        self.play_sound(
            None,
            "minecraft:entity.armor_stand.place",
            crate::sounds::BLOCKS,
            center,
            0.75,
            0.8,
        );
        self.add_armor_stand(st);
        true
    }

    /// `level.noCollision(box)` + `getEntities(null, box).isEmpty()`.
    fn armor_stand_fits(&self, st: &ArmorStand, skip: Option<i32>) -> bool {
        let b = st.boxed();
        for q in blocks_in(b) {
            let Some(state) = self.block(q) else {
                return false;
            };
            if let Some(phys) = self.survival.physics.as_deref() {
                let base = q.map(|v| v as f64);
                for s in phys.boxes_of(state) {
                    let block = [
                        [base[0] + s[0], base[1] + s[1], base[2] + s[2]],
                        [base[0] + s[3], base[1] + s[4], base[2] + s[5]],
                    ];
                    if intersects(b, block) {
                        return false;
                    }
                }
            } else if self.survival.placement.solid(state) {
                return false;
            }
        }
        // Другие стойки.
        if self
            .sim()
            .armor_stands
            .lock()
            .unwrap()
            .iter()
            .any(|(&other, o)| Some(other) != skip && intersects(b, o.boxed()))
        {
            return false;
        }
        // Игроки (у остальных сущностей хитбокс не моделируем).
        let players = self.hub.players.lock().unwrap();
        !players.values().any(|p| {
            intersects(
                b,
                [
                    [p.pos[0] - 0.3, p.pos[1], p.pos[2] - 0.3],
                    [p.pos[0] + 0.3, p.pos[1] + 1.8, p.pos[2] + 0.3],
                ],
            )
        })
    }

    /// `ArmorStand.interactAt`: надеть, снять или обменять предмет.
    /// Возвращает слоты инвентаря игрока, которые нужно переслать.
    pub(super) fn use_armor_stand(
        &self,
        uuid: Uuid,
        target: i32,
        hand: i32,
        at: [f64; 3],
        inv: &mut Inventory,
        creative: bool,
    ) -> Option<Vec<usize>> {
        let (feet, _, _) = self.player_at(uuid)?;
        let hand_slot = Self::hand_index(inv, hand)?;
        let held = inv.slots[hand_slot];
        let mut stands = self.sim().armor_stands.lock().unwrap();
        let st = stands.get_mut(&target)?;
        if st.marker {
            return None;
        }
        let center = st.center();
        let d2: f64 = (0..3).map(|i| (center[i] - feet[i]).powi(2)).sum();
        if d2 > 36.0 {
            return None;
        }
        let mut changed = Vec::new();
        let mut touched = None;
        if let Some(hand_stack) = held {
            let slot = self.equip_slot(hand_stack.item).unwrap_or(MAINHAND);
            if st.is_disabled(slot) {
                return None;
            }
            let old = st.items[slot];
            // `swapItem`: запрет на снятие и на установку в пустой слот.
            if old.is_some() && st.disabled_slots & (1 << (slot + 8)) != 0 {
                return None;
            }
            if old.is_none() && st.disabled_slots & (1 << (slot + 16)) != 0 {
                return None;
            }
            if creative && old.is_none() {
                st.items[slot] = Some(Stack {
                    count: 1,
                    ..hand_stack
                });
            } else if hand_stack.count > 1 {
                // Из стака берётся один предмет, но только в пустой слот.
                if old.is_some() {
                    return None;
                }
                st.items[slot] = Some(Stack {
                    count: 1,
                    ..hand_stack
                });
                if let Some(s) = inv.slots[hand_slot].as_mut() {
                    s.count -= 1;
                    if s.count <= 0 {
                        inv.slots[hand_slot] = None;
                    }
                }
                changed.push(hand_slot);
            } else {
                st.items[slot] = Some(hand_stack);
                inv.slots[hand_slot] = old;
                changed.push(hand_slot);
            }
            touched = Some(slot);
        } else {
            let clicked = st.clicked_slot(at[1]);
            let slot = if st.is_disabled(clicked) {
                MAINHAND
            } else {
                clicked
            };
            if st.disabled_slots & (1 << (slot + 8)) != 0 {
                return None;
            }
            if let Some(old) = st.items[slot].take() {
                inv.slots[hand_slot] = Some(old);
                changed.push(hand_slot);
                touched = Some(slot);
            }
        }
        let packet = touched.map(|slot| {
            play::set_equipment(
                self.ids.set_equipment,
                target,
                &[(slot as u8, st.items[slot])],
            )
        });
        let equipped = touched
            .and_then(|slot| st.items[slot])
            .is_some_and(|s| self.equip_slot(s.item).is_some());
        drop(stands);
        if let Some(packet) = packet {
            self.hub.send_about(target, packet);
            if equipped {
                self.play_sound(
                    None,
                    "minecraft:item.armor.equip_generic",
                    crate::sounds::PLAYERS,
                    center,
                    1.0,
                    1.0,
                );
            }
        }
        Some(changed)
    }

    /// Удар: первый — толчок и звук, второй в течение 5 тиков (или любой в
    /// творческом) — стойка ломается.
    pub(super) fn attack_armor_stand(&self, uuid: Uuid, target: i32, creative: bool) -> Option<()> {
        let feet = self.player_at(uuid)?.0;
        let now = self.game_time.load(Ordering::Relaxed);
        let (center, brk) = {
            let mut stands = self.sim().armor_stands.lock().unwrap();
            let st = stands.get_mut(&target)?;
            let center = st.center();
            let d2: f64 = (0..3).map(|i| (center[i] - feet[i]).powi(2)).sum();
            if d2 > 36.0 || st.marker {
                return Some(());
            }
            let brk = creative || now - st.last_hit <= 5;
            if !brk {
                st.last_hit = now;
            }
            (center, brk)
        };
        if brk {
            self.break_armor_stand(target, creative);
        } else {
            self.play_sound(
                None,
                "minecraft:entity.armor_stand.hit",
                crate::sounds::NEUTRAL,
                center,
                0.3,
                1.0,
            );
            self.hub
                .send_about(target, inv::entity_event(self.ids.entity_event, target, 32));
        }
        Some(())
    }

    /// `brokenByPlayer` + `brokenByAnything`: звук, предмет стойки и снаряжение.
    fn break_armor_stand(&self, id: i32, creative: bool) {
        let Some(st) = self.sim().armor_stands.lock().unwrap().remove(&id) else {
            return;
        };
        self.hub.remove_tracked(&[id]);
        self.play_sound(
            None,
            "minecraft:entity.armor_stand.break",
            crate::sounds::NEUTRAL,
            st.center(),
            1.0,
            1.0,
        );
        if creative || !self.vars.flag("entity_drops") {
            return;
        }
        let pop = |at: [f64; 3], stack: Stack| {
            let vel = items::pop_velocity(rand::random(), rand::random());
            self.drop_item(at, vel, stack, items::BLOCK_DELAY);
        };
        if let Some(item) = self.item_id("minecraft:armor_stand") {
            let mut stack = Stack::new(item, 1);
            if let Some(name) = &st.custom_name {
                stack.extra =
                    chunkwake_protocol::components::intern(chunkwake_protocol::components::Extra {
                        custom_name: Some(crate::stack_nbt::tag_to_json(name).to_string()),
                        ..Default::default()
                    });
            }
            pop([st.pos[0], st.pos[1] + 0.5, st.pos[2]], stack);
        }
        for s in st.items.iter().flatten() {
            pop([st.pos[0], st.pos[1] + 1.0, st.pos[2]], *s);
        }
    }

    pub(super) fn pick_armor_stand(&self, id: i32) -> Option<(Stack, [[f64; 3]; 2])> {
        let stands = self.sim().armor_stands.lock().unwrap();
        let st = stands.get(&id)?;
        if st.marker {
            return None;
        }
        let item = self.item_id("minecraft:armor_stand")?;
        Some((Stack::new(item, 1), st.boxed()))
    }

    /// `ServerExplosion.hurtEntities`: взрыв ломает стойки с дропом.
    pub(super) fn explode_armor_stands(&self, center: [f64; 3], power: f32) {
        let reach = power as f64 * 2.0;
        let hit: Vec<i32> = self
            .sim()
            .armor_stands
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, st)| {
                let c = st.center();
                (0..3).map(|i| (c[i] - center[i]).powi(2)).sum::<f64>() <= reach * reach
            })
            .map(|(&id, _)| id)
            .collect();
        for id in hit {
            self.break_armor_stand(id, false);
        }
    }

    pub(super) fn load_armor_stand(&self, c: &Compound) -> bool {
        if c.get("id").and_then(Tag::as_str) != Some("minecraft:armor_stand") {
            return false;
        }
        let Some(Tag::List(pos)) = c.get("Pos") else {
            return false;
        };
        if pos.len() != 3 {
            return false;
        }
        let p: Vec<f64> = pos
            .iter()
            .map(|t| match t {
                Tag::Double(v) => *v,
                Tag::Float(v) => *v as f64,
                _ => 0.0,
            })
            .collect();
        let yaw = match c.get("Rotation") {
            Some(Tag::List(r)) if !r.is_empty() => match &r[0] {
                Tag::Float(v) => *v,
                Tag::Double(v) => *v as f32,
                _ => 0.0,
            },
            _ => 0.0,
        };
        let mut uuid = Uuid::from_u128(rand::random());
        if let Some(Tag::IntArray(u)) = c.get("UUID")
            && u.len() == 4
        {
            uuid = Uuid::from_u128(u.iter().fold(0u128, |acc, &w| acc << 32 | w as u32 as u128));
        }
        let mut st = ArmorStand::new(uuid, [p[0], p[1], p[2]], yaw);
        st.invisible = flag(c, "Invisible");
        st.small = flag(c, "Small");
        st.show_arms = flag(c, "ShowArms");
        st.no_base_plate = flag(c, "NoBasePlate");
        st.marker = flag(c, "Marker");
        st.disabled_slots = c.get("DisabledSlots").and_then(Tag::as_i64).unwrap_or(0) as i32;
        st.custom_name = c.get("CustomName").cloned();
        if let Some(Tag::Compound(pose)) = c.get("Pose") {
            for (i, key) in POSE_KEYS.iter().enumerate() {
                if let Some(Tag::List(r)) = pose.get(*key)
                    && r.len() == 3
                {
                    st.pose[i] = [num(&r[0]), num(&r[1]), num(&r[2])];
                }
            }
        }
        let mut items = [None; 6];
        read_items(
            c.get("ArmorItems"),
            &mut items,
            &[FEET, LEGS, CHEST, HEAD],
            self,
        );
        read_items(c.get("HandItems"), &mut items, &[MAINHAND, OFFHAND], self);
        st.items = items;
        self.add_armor_stand(st);
        true
    }

    pub(super) fn armor_stands_nbt(&self) -> Vec<((i32, i32), Compound)> {
        self.sim()
            .armor_stands
            .lock()
            .unwrap()
            .values()
            .map(|st| {
                let mut c = Compound::new();
                c.insert("id".into(), Tag::String("minecraft:armor_stand".into()));
                c.insert(
                    "Pos".into(),
                    Tag::List(st.pos.iter().map(|v| Tag::Double(*v)).collect()),
                );
                c.insert("Motion".into(), Tag::List(vec![Tag::Double(0.0); 3]));
                c.insert(
                    "Rotation".into(),
                    Tag::List(vec![Tag::Float(st.yaw), Tag::Float(0.0)]),
                );
                let u = st.uuid.as_u128();
                c.insert(
                    "UUID".into(),
                    Tag::IntArray(
                        (0..4)
                            .rev()
                            .map(|i| (u >> (32 * i)) as u32 as i32)
                            .collect(),
                    ),
                );
                c.insert("Invisible".into(), Tag::Byte(i8::from(st.invisible)));
                c.insert("Small".into(), Tag::Byte(i8::from(st.small)));
                c.insert("ShowArms".into(), Tag::Byte(i8::from(st.show_arms)));
                c.insert("NoBasePlate".into(), Tag::Byte(i8::from(st.no_base_plate)));
                c.insert("Marker".into(), Tag::Byte(i8::from(st.marker)));
                c.insert("DisabledSlots".into(), Tag::Int(st.disabled_slots));
                if let Some(name) = &st.custom_name {
                    c.insert("CustomName".into(), name.clone());
                }
                let mut pose = Compound::new();
                for (i, key) in POSE_KEYS.iter().enumerate() {
                    pose.insert(
                        (*key).into(),
                        Tag::List(st.pose[i].iter().map(|v| Tag::Float(*v)).collect()),
                    );
                }
                c.insert("Pose".into(), Tag::Compound(pose));
                c.insert(
                    "ArmorItems".into(),
                    Tag::List(
                        [FEET, LEGS, CHEST, HEAD]
                            .iter()
                            .map(|&slot| stack_tag(self, st.items[slot]))
                            .collect(),
                    ),
                );
                c.insert(
                    "HandItems".into(),
                    Tag::List(
                        [MAINHAND, OFFHAND]
                            .iter()
                            .map(|&slot| stack_tag(self, st.items[slot]))
                            .collect(),
                    ),
                );
                let chunk = (st.pos[0].floor() as i32 >> 4, st.pos[2].floor() as i32 >> 4);
                (chunk, c)
            })
            .collect()
    }
}

/// `getBooleanOr` для NBT.
fn flag(c: &Compound, key: &str) -> bool {
    c.get(key).and_then(Tag::as_i64).unwrap_or(0) != 0
}

fn num(t: &Tag) -> f32 {
    match t {
        Tag::Float(v) => *v,
        Tag::Double(v) => *v as f32,
        _ => 0.0,
    }
}

/// Пустой слот — пустой компаунд, как `ItemStack.OPTIONAL_CODEC`.
fn stack_tag(plan: &PlayPlan, slot: Slot) -> Tag {
    match slot.and_then(|s| crate::stack_nbt::to_nbt(s, plan.item_name(s.item))) {
        Some(c) => Tag::Compound(c),
        None => Tag::Compound(Compound::new()),
    }
}

fn read_items(list: Option<&Tag>, out: &mut [Slot; 6], slots: &[usize], plan: &PlayPlan) {
    let Some(Tag::List(list)) = list else {
        return;
    };
    for (i, &slot) in slots.iter().enumerate() {
        if let Some(Tag::Compound(c)) = list.get(i)
            && c.contains_key("id")
        {
            out[slot] = crate::stack_nbt::from_nbt(c, |n| plan.item_id(n));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stand() -> ArmorStand {
        ArmorStand::new(Uuid::nil(), [0.0, 64.0, 0.0], 0.0)
    }

    #[test]
    fn clicked_slot_follows_armor_heights() {
        let mut st = stand();
        st.items = [
            None,
            Some(Stack::new(1, 1)),
            Some(Stack::new(1, 1)),
            Some(Stack::new(1, 1)),
            Some(Stack::new(1, 1)),
            None,
        ];
        assert_eq!(st.clicked_slot(0.2), FEET);
        assert_eq!(st.clicked_slot(0.7), LEGS);
        assert_eq!(st.clicked_slot(1.0), CHEST);
        assert_eq!(st.clicked_slot(1.7), HEAD);
        // Пустая рука без снаряжения выше головы — основная рука.
        st.items[HEAD] = None;
        assert_eq!(st.clicked_slot(1.7), MAINHAND);
    }

    #[test]
    fn disabled_slots_use_equipment_ids() {
        let mut st = stand();
        st.disabled_slots = (1 << HEAD) | (1 << (FEET + 8)) | (1 << (CHEST + 16));
        assert!(st.is_disabled(HEAD));
        assert!(!st.is_disabled(LEGS));
        // Руки закрыты, пока не включён ShowArms.
        assert!(st.is_disabled(MAINHAND));
        st.show_arms = true;
        assert!(!st.is_disabled(MAINHAND));
    }

    #[test]
    fn yaw_snaps_to_45_degrees() {
        // `Mth.floor((wrapDegrees(yaw - 180) + 22.5) / 45) * 45`.
        assert_eq!(yaw_from_player(0.0), -180.0);
        assert_eq!(yaw_from_player(180.0), 0.0);
        assert_eq!(yaw_from_player(202.5), 45.0);
        assert_eq!(yaw_from_player(135.0), -45.0);
    }
}
