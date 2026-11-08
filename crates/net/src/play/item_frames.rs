//! Рамки для предметов (`ItemFrame`, `GlowItemFrame`, `HangingEntity`,
//! `BlockAttachedEntity`, `ItemFrameItem`).
//!
//! Рамка — сущность в блоке `pos`, прижатая к соседу с обратной стороны
//! `facing`. Висит, пока опора твёрдая (`isSolid`, у стены — ещё
//! повторитель или компаратор), в её блоке нет коллизии и на той же стороне
//! нет другой рамки; проверка раз в 100 тиков, как у ванили. Карта в рамке
//! раз в 10 тиков рисует значок рамки на себе и рассылается игрокам
//! (`ServerEntity.sendChanges`). Компаратор за проводником читает поворот
//! предмета (`getAnalogOutput`).
//!
//! Нет: стрел и прочих снарядов по рамке, `EntityType.createDefaultStackConfig`
//! (имя и данные сущности из предмета), предела в 256 рамок на карту.

use super::*;
use crate::maps::MapFrame;

/// `BlockAttachedEntity.CHECK_INTERVAL`.
const CHECK_INTERVAL: i32 = 100;
/// Половина глубины блока минус толщина рамки: центр от середины блока (`createBoundingBox`).
const INSET: f64 = 0.46875;
/// `ServerEntity.sendChanges`: карта в рамке — раз в 10 тиков.
const MAP_INTERVAL: i32 = 10;
/// `NUM_ROTATIONS`.
const ROTATIONS: i32 = 8;

pub(crate) struct ItemFrame {
    pub glow: bool,
    pub uuid: Uuid,
    /// Блок рамки (`block_pos`).
    pub pos: [i32; 3],
    /// Сторона (`Direction`, 3D-номер): куда смотрит лицо рамки.
    pub facing: usize,
    pub item: Slot,
    pub rotation: i32,
    pub drop_chance: f32,
    pub invisible: bool,
    pub fixed: bool,
    custom_name: Option<Tag>,
    /// `ticksSinceLastCheck`.
    since_check: i32,
    /// `ServerEntity.tickCount`.
    ticks: i32,
}

/// Что отдаёт рамка: снято под замком, роняется без него.
struct FrameLoot {
    item: Slot,
    /// Центр, сдвинутый на 0.15 от стены (`spawnAtLocation`).
    at: [f64; 3],
    glow: bool,
    pos: [i32; 3],
    chance: f32,
}

impl ItemFrame {
    /// `dropItem`: снять предмет; закреплённая ничего не отдаёт.
    fn take_loot(&mut self) -> Option<FrameLoot> {
        if self.fixed {
            return None;
        }
        let d = crate::placement::step(self.facing);
        let c = self.center();
        Some(FrameLoot {
            item: self.item.take(),
            at: [c[0] + d[0] as f64 * 0.15, c[1], c[2] + d[2] as f64 * 0.15],
            glow: self.glow,
            pos: self.pos,
            chance: self.drop_chance,
        })
    }

    fn new(glow: bool, pos: [i32; 3], facing: usize) -> Self {
        Self {
            glow,
            uuid: Uuid::from_u128(rand::random()),
            pos,
            facing,
            item: None,
            rotation: 0,
            drop_chance: 1.0,
            invisible: false,
            fixed: false,
            custom_name: None,
            since_check: 0,
            ticks: 0,
        }
    }

    fn kind(&self) -> &'static str {
        if self.glow {
            "minecraft:glow_item_frame"
        } else {
            "minecraft:item_frame"
        }
    }

    fn sound(&self, what: &str) -> String {
        let base = if self.glow {
            "glow_item_frame"
        } else {
            "item_frame"
        };
        format!("minecraft:entity.{base}.{what}")
    }

    /// Центр коробки — позиция сущности.
    pub fn center(&self) -> [f64; 3] {
        center(self.pos, self.facing)
    }

    /// `getYRot`/`getXRot` из `setDirection`.
    fn angles(&self) -> (f32, f32) {
        match self.facing {
            0 => (0.0, 90.0),
            1 => (0.0, -90.0),
            f => (two_d(f) as f32 * 90.0, 0.0),
        }
    }

    /// `getAnalogOutput`: пустая — 0, иначе поворот + 1.
    pub fn analog(&self) -> i32 {
        if self.item.is_none() {
            0
        } else {
            self.rotation % ROTATIONS + 1
        }
    }
}

/// `Direction.get2DDataValue`: юг 0, запад 1, север 2, восток 3; низ и верх −1.
fn two_d(facing: usize) -> i32 {
    match facing {
        3 => 0,
        4 => 1,
        2 => 2,
        5 => 3,
        _ => -1,
    }
}

/// `Vec3.atCenterOf(pos).relative(dir, -0.46875)`.
fn center(pos: [i32; 3], facing: usize) -> [f64; 3] {
    let d = crate::placement::step(facing);
    std::array::from_fn(|i| pos[i] as f64 + 0.5 - d[i] as f64 * INSET)
}

/// `createBoundingBox`: 1/16 по оси стороны, поперёк 0.75 (с картой — 1).
fn frame_box(pos: [i32; 3], facing: usize, map: bool) -> [[f64; 3]; 2] {
    let c = center(pos, facing);
    let axis = facing / 2;
    let across = if map { 1.0 } else { 0.75 };
    let size: [f64; 3] = std::array::from_fn(|i| {
        let a = match i {
            0 => 2,
            1 => 0,
            _ => 1,
        };
        if a == axis { 0.0625 } else { across }
    });
    [
        std::array::from_fn(|i| c[i] - size[i] / 2.0),
        std::array::from_fn(|i| c[i] + size[i] / 2.0),
    ]
}

/// `AABB.intersects`: строго внутри по всем осям.
fn intersects(a: [[f64; 3]; 2], b: [[f64; 3]; 2]) -> bool {
    (0..3).all(|i| a[0][i] < b[1][i] && a[1][i] > b[0][i])
}

impl PlayPlan {
    fn frame_packets(&self, id: i32, f: &ItemFrame) -> Vec<Vec<u8>> {
        let Some(&ty) = self.survival.entity_types.get(f.kind()) else {
            return Vec::new();
        };
        let (yaw, pitch) = f.angles();
        // `getAddEntityPacket`: позиция — блок рамки, данные — 3D-номер стороны.
        let at = f.pos.map(|v| v as f64);
        let mut out = vec![play::add_entity_moving(
            self.ids.add_entity,
            id,
            f.uuid,
            ty,
            at,
            [0.0; 3],
            yaw,
            pitch,
            f.facing as i32,
        )];
        // `getNonDefaultValues`: по умолчанию юг, пусто, поворот 0, видима.
        let (facing, item, rotation, invisible) = (
            (f.facing != 3).then_some(f.facing as i32),
            f.item.is_some().then_some(f.item),
            (f.rotation != 0).then_some(f.rotation),
            f.invisible.then_some(true),
        );
        if facing.is_some() || item.is_some() || rotation.is_some() || invisible.is_some() {
            out.push(inv::item_frame_data(
                self.ids.set_entity_data,
                id,
                facing,
                item,
                rotation,
                invisible,
            ));
        }
        out
    }

    /// `packDirty` после `setItem`: только предмет.
    fn frame_item_data(&self, id: i32, f: &ItemFrame) -> Vec<u8> {
        inv::item_frame_data(self.ids.set_entity_data, id, None, Some(f.item), None, None)
    }

    fn frame_tracked(&self, id: i32, f: &ItemFrame) -> Tracked {
        let ty = self
            .survival
            .entity_types
            .get(f.kind())
            .copied()
            .unwrap_or_default();
        Tracked {
            id,
            // Позиция сущности — центр её коробки (`recalculateBoundingBox`).
            pos: center(f.pos, f.facing as usize),
            range: self.tracking_range(ty),
        }
    }

    pub(super) fn track_item_frames(&self, out: &mut Vec<Tracked>) {
        for (&id, f) in self.sim().item_frames.lock().unwrap().iter() {
            out.push(self.frame_tracked(id, f));
        }
    }

    /// Пакеты спавна рамки для нового зрителя.
    pub(super) fn item_frame_spawn(&self, id: i32) -> Option<Vec<Vec<u8>>> {
        let frames = self.sim().item_frames.lock().unwrap();
        Some(self.frame_packets(id, frames.get(&id)?))
    }

    pub(super) fn is_item_frame(&self, id: i32) -> bool {
        self.sim().item_frames.lock().unwrap().contains_key(&id)
    }

    fn add_item_frame(&self, f: ItemFrame) -> i32 {
        let id = self.next_entity_id.fetch_add(1, Ordering::Relaxed);
        let packets = self.frame_packets(id, &f);
        let tracked = self.frame_tracked(id, &f);
        self.sim().item_frames.lock().unwrap().insert(id, f);
        self.hub.spawn_tracked(tracked, packets);
        id
    }

    /// `HangingEntity.survives` для рамки (`ItemFrame.survives`).
    fn frame_survives(&self, id: Option<i32>, pos: [i32; 3], facing: usize, fixed: bool) -> bool {
        if fixed {
            return true;
        }
        let pop = frame_box(pos, facing, false);
        // `hasLevelCollision(getPopBox())`: коллизия блока рамки.
        if let Some(s) = self.block(pos)
            && let Some(phys) = self.survival.physics.as_deref()
        {
            let base = pos.map(|v| v as f64);
            let hit = phys.boxes_of(s).iter().any(|b| {
                intersects(
                    pop,
                    [
                        [base[0] + b[0], base[1] + b[1], base[2] + b[2]],
                        [base[0] + b[3], base[1] + b[4], base[2] + b[5]],
                    ],
                )
            });
            if hit {
                return false;
            }
        }
        let d = crate::placement::step(facing);
        let support = [pos[0] - d[0], pos[1] - d[1], pos[2] - d[2]];
        let s = self.block(support).unwrap_or(0);
        let diode = matches!(
            self.state_name(s),
            "minecraft:repeater" | "minecraft:comparator"
        );
        if !self.survival.placement.solid(s) && !(facing >= 2 && diode) {
            return false;
        }
        // `canCoexist(true)`: другая рамка на той же стороне в коробке.
        let frames = self.sim().item_frames.lock().unwrap();
        !frames.iter().any(|(&other, o)| {
            Some(other) != id
                && o.facing == facing
                && intersects(
                    pop,
                    frame_box(o.pos, o.facing, self.map_id_of_slot(o.item).is_some()),
                )
        })
    }

    fn map_id_of_slot(&self, s: Slot) -> Option<i32> {
        s.and_then(|s| self.map_id_of(s))
    }

    /// `ItemFrameItem.useOn`: рамка в блок перед кликнутой гранью.
    /// `true` — поставлена (предмет тратится вне творческого).
    pub(super) fn place_item_frame(
        &self,
        uuid: Uuid,
        item: &str,
        clicked: [i32; 3],
        face: i32,
        adventure: bool,
    ) -> bool {
        let glow = match item {
            "minecraft:item_frame" => false,
            "minecraft:glow_item_frame" => true,
            _ => return false,
        };
        let Some(d) = play::direction_offset(face) else {
            return false;
        };
        let facing = face.clamp(0, 5) as usize;
        let pos = [clicked[0] + d[0], clicked[1] + d[1], clicked[2] + d[2]];
        // `mayPlace`: высота мира и `mayUseItemAt` (не приключение, не защита спавна).
        if self.block(pos).is_none() || adventure || self.spawn_blocked(uuid, pos) {
            return false;
        }
        if !self.frame_survives(None, pos, facing, false) {
            return false;
        }
        let f = ItemFrame::new(glow, pos, facing);
        self.play_sound(
            None,
            &f.sound("place"),
            crate::sounds::NEUTRAL,
            f.center(),
            1.0,
            1.0,
        );
        self.add_item_frame(f);
        true
    }

    /// `ItemFrame.interact`: в пустую — предмет из руки, в занятую — поворот.
    /// `Some(true)` — рука тратит один предмет; `None` — не рамка или мимо.
    pub(super) fn use_item_frame(&self, target: i32, held: Slot) -> Option<bool> {
        let mut frames = self.sim().item_frames.lock().unwrap();
        let f = frames.get_mut(&target)?;
        if f.fixed {
            return None;
        }
        let (sound, spend, packet) = if f.item.is_none() {
            let held = held?;
            if self
                .map_id_of(held)
                .is_some_and(|id| self.map_has_too_many_decorations(id))
            {
                return None;
            }
            f.item = Some(Stack { count: 1, ..held });
            ("add_item", true, self.frame_item_data(target, f))
        } else {
            f.rotation = (f.rotation + 1) % ROTATIONS;
            (
                "rotate_item",
                false,
                inv::item_frame_data(
                    self.ids.set_entity_data,
                    target,
                    None,
                    None,
                    Some(f.rotation),
                    None,
                ),
            )
        };
        let (center, name, pos) = (f.center(), f.sound(sound), f.pos);
        drop(frames);
        self.hub.send_about(target, packet);
        self.play_sound(None, &name, crate::sounds::NEUTRAL, center, 1.0, 1.0);
        self.redstone_update(&crate::redstone::around(pos));
        Some(spend)
    }

    /// Удар игрока (`skipAttackInteraction` → `hurtServer`): первый удар
    /// снимает предмет, следующий ломает рамку. В творческом ничего не падает.
    pub(super) fn attack_item_frame(&self, uuid: Uuid, target: i32, creative: bool) -> Option<()> {
        let feet = self.player_at(uuid)?.0;
        let (pos, center, fixed, has_item) = {
            let frames = self.sim().item_frames.lock().unwrap();
            let f = frames.get(&target)?;
            (f.pos, f.center(), f.fixed, f.item.is_some())
        };
        let d: f64 = (0..3).map(|i| (center[i] - feet[i]).powi(2)).sum();
        if d > 36.0 || self.spawn_blocked(uuid, pos) {
            return Some(());
        }
        // `canHurtWhenFixed`: закреплённую ломает только творческий, без дропа.
        if fixed && !creative {
            return Some(());
        }
        if has_item && !fixed {
            // `hurtServer`: `dropItem` (предмет падает сразу), звук, а данные
            // рамки уходят следующим тиком.
            let (loot, packet, center, sound) = {
                let mut frames = self.sim().item_frames.lock().unwrap();
                let f = frames.get_mut(&target)?;
                let loot = f.take_loot()?;
                (
                    loot,
                    self.frame_item_data(target, f),
                    f.center(),
                    f.sound("remove_item"),
                )
            };
            let pos = loot.pos;
            self.frame_drop(target, loot, false, creative);
            self.play_sound(None, &sound, crate::sounds::NEUTRAL, center, 1.0, 1.0);
            self.hub.send_about(target, packet);
            self.redstone_update(&crate::redstone::around(pos));
            return Some(());
        }
        self.break_item_frame(target, creative);
        Some(())
    }

    /// `kill` + `dropItem(attacker)`: рамка исчезает, звук поломки, затем
    /// рамка и предмет на землю.
    fn break_item_frame(&self, id: i32, creative: bool) {
        let Some(mut f) = self.sim().item_frames.lock().unwrap().remove(&id) else {
            return;
        };
        self.hub.remove_tracked(&[id]);
        self.play_sound(
            None,
            &f.sound("break"),
            crate::sounds::NEUTRAL,
            f.center(),
            1.0,
            1.0,
        );
        if let Some(loot) = f.take_loot() {
            self.frame_drop(id, loot, true, creative);
        }
        self.redstone_update(&crate::redstone::around(f.pos));
    }

    /// `ItemFrame.dropItem(level, entity, withFrame)`: предмет из рамки (с
    /// шансом `ItemDropChance`) и, если ломается, сама рамка. Карта при этом
    /// теряет значок рамки.
    fn frame_drop(&self, id: i32, loot: FrameLoot, with_frame: bool, creative: bool) {
        if let Some(map) = self.map_id_of_slot(loot.item) {
            self.map_removed_from_frame(map, loot.pos, id);
        }
        // `entity_drops` выключен или ломает творческий — ничего не падает.
        if creative || !self.vars.flag("entity_drops") {
            return;
        }
        let pop = |s: Stack| {
            let vel = items::pop_velocity(rand::random(), rand::random());
            self.drop_item(loot.at, vel, s, items::BLOCK_DELAY);
        };
        if with_frame
            && let Some(frame) = self.item_id(if loot.glow {
                "minecraft:glow_item_frame"
            } else {
                "minecraft:item_frame"
            })
        {
            pop(Stack::new(frame, 1));
        }
        if let Some(s) = loot.item
            && rand::random::<f32>() < loot.chance
        {
            pop(s);
        }
    }

    /// Взрыв (`ServerExplosion.hurtEntities`): незакреплённые рамки в радиусе
    /// `2·сила` ломаются целиком.
    pub(super) fn explode_item_frames(&self, center: [f64; 3], power: f32) {
        let reach = power as f64 * 2.0;
        let hit: Vec<i32> = self
            .sim()
            .item_frames
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, f)| {
                let c = f.center();
                !f.fixed && (0..3).map(|i| (c[i] - center[i]).powi(2)).sum::<f64>() <= reach * reach
            })
            .map(|(&id, _)| id)
            .collect();
        for id in hit {
            self.break_item_frame(id, false);
        }
    }

    /// `ComparatorBlock.getItemFrame`: единственная рамка в блоке `p` стороной `facing`.
    pub(super) fn item_frame_signal(&self, p: [i32; 3], facing: usize) -> Option<i32> {
        let frames = self.sim().item_frames.lock().unwrap();
        let block = [p.map(|v| v as f64), p.map(|v| v as f64 + 1.0)];
        let mut found = frames.values().filter(|f| {
            f.facing == facing
                && intersects(
                    block,
                    frame_box(f.pos, f.facing, self.map_id_of_slot(f.item).is_some()),
                )
        });
        let one = found.next()?;
        found.next().is_none().then(|| one.analog())
    }

    /// Тик рамок в симулируемых чанках: проверка опоры раз в 100 тиков,
    /// карта в рамке раз в 10.
    pub(super) fn tick_item_frames(&self) {
        if self.sim().item_frames.lock().unwrap().is_empty() {
            return;
        }
        let simulated = self.simulated_set();
        let mut check = Vec::new();
        let mut maps = Vec::new();
        {
            let mut frames = self.sim().item_frames.lock().unwrap();
            for (&id, f) in frames.iter_mut() {
                if !simulated.is_empty() && !simulated.contains(&(f.pos[0] >> 4, f.pos[2] >> 4)) {
                    continue;
                }
                if f.ticks % MAP_INTERVAL == 0
                    && let Some(map) = f.item.and_then(|s| self.map_id_of(s))
                {
                    let frame = MapFrame {
                        pos: f.pos,
                        rotation: two_d(f.facing) * 90,
                        entity_id: id,
                    };
                    maps.push((map, frame));
                }
                f.ticks += 1;
                f.since_check += 1;
                if f.since_check >= CHECK_INTERVAL {
                    f.since_check = 0;
                    check.push((id, f.pos, f.facing, f.fixed));
                }
            }
        }
        for (map, frame) in maps {
            self.tick_framed_map(map, frame);
        }
        for (id, pos, facing, fixed) in check {
            if !self.frame_survives(Some(id), pos, facing, fixed) {
                self.break_item_frame(id, false);
            }
        }
    }

    /// Рамки чанка `entities/*.mca`.
    pub(super) fn load_item_frame(&self, c: &Compound) -> bool {
        let glow = match c.get("id") {
            Some(Tag::String(k)) if k == "minecraft:item_frame" => false,
            Some(Tag::String(k)) if k == "minecraft:glow_item_frame" => true,
            _ => return false,
        };
        let pos = match c.get("block_pos") {
            Some(Tag::IntArray(p)) if p.len() == 3 => [p[0], p[1], p[2]],
            _ => match c.get("Pos") {
                Some(Tag::List(l)) if l.len() == 3 => {
                    let v: Vec<f64> = l
                        .iter()
                        .map(|t| match t {
                            Tag::Double(v) => *v,
                            Tag::Float(v) => *v as f64,
                            _ => 0.0,
                        })
                        .collect();
                    [
                        v[0].floor() as i32,
                        v[1].floor() as i32,
                        v[2].floor() as i32,
                    ]
                }
                _ => return false,
            },
        };
        let byte = |k: &str| c.get(k).and_then(Tag::as_i64);
        let facing = byte("Facing").filter(|f| (0..6).contains(f)).unwrap_or(0) as usize;
        let mut f = ItemFrame::new(glow, pos, facing);
        if let Some(Tag::IntArray(u)) = c.get("UUID")
            && u.len() == 4
        {
            f.uuid = Uuid::from_u128(u.iter().fold(0u128, |acc, &w| acc << 32 | w as u32 as u128));
        }
        f.item = c
            .get("Item")
            .and_then(Tag::as_compound)
            .and_then(|i| crate::stack_nbt::from_nbt(i, |n| self.item_id(n)));
        f.rotation = byte("ItemRotation")
            .unwrap_or(0)
            .rem_euclid(ROTATIONS as i64) as i32;
        f.drop_chance = match c.get("ItemDropChance") {
            Some(Tag::Float(v)) => *v,
            Some(Tag::Double(v)) => *v as f32,
            _ => 1.0,
        };
        f.invisible = byte("Invisible").is_some_and(|v| v != 0);
        f.fixed = byte("Fixed").is_some_and(|v| v != 0);
        f.custom_name = c.get("CustomName").cloned();
        self.add_item_frame(f);
        true
    }

    /// Рамки для записи `entities/*.mca`, по чанкам.
    pub(super) fn item_frames_nbt(&self) -> Vec<((i32, i32), Compound)> {
        self.sim()
            .item_frames
            .lock()
            .unwrap()
            .values()
            .map(|f| {
                let mut c = Compound::new();
                let center = f.center();
                let (yaw, pitch) = f.angles();
                c.insert("id".into(), Tag::String(f.kind().into()));
                c.insert(
                    "Pos".into(),
                    Tag::List(center.iter().map(|v| Tag::Double(*v)).collect()),
                );
                c.insert("Motion".into(), Tag::List(vec![Tag::Double(0.0); 3]));
                c.insert(
                    "Rotation".into(),
                    Tag::List(vec![Tag::Float(yaw), Tag::Float(pitch)]),
                );
                let u = f.uuid.as_u128();
                c.insert(
                    "UUID".into(),
                    Tag::IntArray(
                        (0..4)
                            .rev()
                            .map(|i| (u >> (32 * i)) as u32 as i32)
                            .collect(),
                    ),
                );
                c.insert("block_pos".into(), Tag::IntArray(f.pos.to_vec()));
                c.insert("Facing".into(), Tag::Byte(f.facing as i8));
                if let Some(s) = f.item
                    && let Some(item) = crate::stack_nbt::to_nbt(s, self.item_name(s.item))
                {
                    c.insert("Item".into(), Tag::Compound(item));
                }
                c.insert("ItemRotation".into(), Tag::Byte(f.rotation as i8));
                c.insert("ItemDropChance".into(), Tag::Float(f.drop_chance));
                c.insert("Invisible".into(), Tag::Byte(f.invisible as i8));
                c.insert("Fixed".into(), Tag::Byte(f.fixed as i8));
                if let Some(n) = &f.custom_name {
                    c.insert("CustomName".into(), n.clone());
                }
                let chunk = (
                    (center[0].floor() as i32) >> 4,
                    (center[2].floor() as i32) >> 4,
                );
                (chunk, c)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wall_frame_sits_flush_against_its_support() {
        // Рамка смотрит на юг: опора — блок севернее, коробка у северной грани.
        let b = frame_box([0, 64, 0], 3, false);
        assert_eq!(b[0][2], 0.0);
        assert!((b[1][2] - 0.0625).abs() < 1e-9);
        assert!((b[0][0] - 0.125).abs() < 1e-9 && (b[1][0] - 0.875).abs() < 1e-9);
        // На полу (верх): тонкая по Y у нижней грани.
        let f = frame_box([0, 64, 0], 1, true);
        assert_eq!(f[0][1], 64.0);
        assert!((f[1][1] - 64.0625).abs() < 1e-9);
        assert_eq!((f[0][0], f[1][0]), (0.0, 1.0));
    }

    #[test]
    fn neighbouring_frames_do_not_overlap_even_with_maps() {
        let a = frame_box([0, 64, 0], 3, true);
        let b = frame_box([1, 64, 0], 3, false);
        assert!(!intersects(a, b));
        assert!(intersects(
            frame_box([0, 64, 0], 3, false),
            frame_box([0, 64, 0], 3, true)
        ));
    }

    #[test]
    fn analog_is_rotation_plus_one_and_zero_when_empty() {
        let mut f = ItemFrame::new(false, [0, 0, 0], 3);
        assert_eq!(f.analog(), 0);
        f.item = Some(Stack::new(1, 1));
        assert_eq!(f.analog(), 1);
        f.rotation = 7;
        assert_eq!(f.analog(), 8);
    }

    #[test]
    fn horizontal_yaw_follows_two_d_value() {
        let yaw = |facing| ItemFrame::new(false, [0, 0, 0], facing).angles();
        assert_eq!(yaw(3), (0.0, 0.0));
        assert_eq!(yaw(4), (90.0, 0.0));
        assert_eq!(yaw(2), (180.0, 0.0));
        assert_eq!(yaw(5), (270.0, 0.0));
        assert_eq!(yaw(1), (0.0, -90.0));
        assert_eq!(yaw(0), (0.0, 90.0));
    }
}
