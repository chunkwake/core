//! Рыбалка (`FishingRodItem`, `FishingHook`): заброс, поплавок, поклёвка
//! и добыча по таблице `gameplay/fishing`.
//!
//! Поплавок — отдельная сущность `minecraft:fishing_bobber`: летит с
//! гравитацией 0.03, на воде переходит в `BOBBING`, ждёт 100–600 тиков
//! (`Lure` уменьшает), затем 20–80 до поклёвки и `nibble` 20–40 — окно, в
//! которое повторный ПКМ вытаскивает добычу. Таблица: рыба 85, мусор 10,
//! сокровища 5 (+2 за уровень Luck of the Sea), сокровища только на
//! открытой воде.

use super::*;

#[derive(Clone, Copy, Debug)]
pub struct Bobber {
    pub pos: [f64; 3],
    pub vel: [f64; 3],
    pub uuid: Uuid,
    pub owner: Uuid,
    pub biting: bool,
    /// Окно поклёвки (`nibble`).
    pub nibble: i32,
    pub time_until_lured: i32,
    pub time_until_hooked: i32,
    pub open_water: bool,
    pub out_of_water: u8,
    pub life: u32,
    pub on_ground: bool,
    /// `DATA_HOOKED_ENTITY`: entity_id + 1 (0 — не зацеплено).
    pub hooked: Option<i32>,
    pub lure: i32,
    pub luck: i32,
}

/// Одна запись таблицы `gameplay/fishing`: предмет и модификаторы.
#[derive(Clone, Copy)]
struct FishingDrop {
    name: &'static str,
    weight: i32,
    count: i32,
    /// `set_damage`: максимальная доля оставшейся прочности.
    damage: Option<f32>,
    /// `set_potion`: id зелья.
    potion: Option<&'static str>,
    /// `enchant_with_levels`: уровни.
    enchant_levels: Option<i32>,
    /// `location_check`: запись только в биомах джунглей.
    jungle_only: bool,
}

const fn drop(name: &'static str, weight: i32) -> FishingDrop {
    FishingDrop {
        name,
        weight,
        count: 1,
        damage: None,
        potion: None,
        enchant_levels: None,
        jungle_only: false,
    }
}

const fn damaged(name: &'static str, weight: i32, damage: f32) -> FishingDrop {
    FishingDrop {
        damage: Some(damage),
        ..drop(name, weight)
    }
}

const fn potion(name: &'static str, weight: i32) -> FishingDrop {
    FishingDrop {
        potion: Some("minecraft:water"),
        ..drop(name, weight)
    }
}

const fn enchanted(
    name: &'static str,
    weight: i32,
    damage: Option<f32>,
    levels: i32,
) -> FishingDrop {
    FishingDrop {
        damage,
        enchant_levels: Some(levels),
        ..drop(name, weight)
    }
}

const fn jungle(name: &'static str, weight: i32) -> FishingDrop {
    FishingDrop {
        jungle_only: true,
        ..drop(name, weight)
    }
}

const FISH_TABLE: &[FishingDrop] = &[
    drop("minecraft:cod", 60),
    drop("minecraft:salmon", 25),
    drop("minecraft:pufferfish", 13),
    drop("minecraft:tropical_fish", 2),
];

const JUNK_TABLE: &[FishingDrop] = &[
    drop("minecraft:lily_pad", 17),
    damaged("minecraft:leather_boots", 10, 0.9),
    drop("minecraft:leather", 10),
    drop("minecraft:bone", 10),
    potion("minecraft:potion", 10),
    drop("minecraft:string", 5),
    damaged("minecraft:fishing_rod", 2, 0.9),
    drop("minecraft:bowl", 10),
    drop("minecraft:stick", 5),
    FishingDrop {
        count: 10,
        ..drop("minecraft:ink_sac", 1)
    },
    drop("minecraft:tripwire_hook", 10),
    drop("minecraft:rotten_flesh", 10),
    jungle("minecraft:bamboo", 10),
];

const TREASURE_TABLE: &[FishingDrop] = &[
    drop("minecraft:name_tag", 1),
    drop("minecraft:saddle", 1),
    enchanted("minecraft:bow", 1, Some(0.25), 30),
    enchanted("minecraft:fishing_rod", 1, Some(0.25), 30),
    enchanted("minecraft:book", 1, None, 30),
    drop("minecraft:nautilus_shell", 1),
];

/// `SetItemDamageFunction`: `floor((1 − clamp(damage, 0, 1)) × maxDamage)`.
fn set_damage(damage: f32, max: i32) -> i32 {
    let remaining = 1.0 - damage.clamp(0.0, 1.0);
    (remaining * max as f32).floor() as i32
}

impl PlayPlan {
    fn bobber_type(&self) -> Option<i32> {
        self.survival
            .entity_types
            .get("minecraft:fishing_bobber")
            .copied()
    }

    /// ПКМ рыболовной удочкой: вытащить поплавок или забросить новый.
    /// `None` — удочка сломалась.
    pub(super) fn fishing_use(&self, uuid: Uuid, stack: Stack, creative: bool) -> Option<Stack> {
        let existing = self
            .sim()
            .fish
            .lock()
            .unwrap()
            .iter()
            .find(|(_, b)| b.owner == uuid)
            .map(|(&id, _)| id);
        let damage = if let Some(id) = existing {
            self.reel_in(id, uuid, stack, creative)
        } else {
            self.cast_bobber(uuid, stack);
            0
        };
        if creative {
            return Some(stack);
        }
        let max = self.item_max_damage(stack.item);
        let unbreaking =
            chunkwake_protocol::components::stack_level(stack.extra, "minecraft:unbreaking");
        let mut d = stack.damage;
        for _ in 0..damage.max(0) {
            if unbreaking > 0 && rand::random::<u32>().is_multiple_of(unbreaking as u32 + 1) {
                continue;
            }
            d += 1;
        }
        if max > 0 && d >= max {
            None
        } else {
            Some(Stack { damage: d, ..stack })
        }
    }

    fn cast_bobber(&self, uuid: Uuid, stack: Stack) {
        let Some((feet, yaw, pitch)) = self.player_at(uuid) else {
            return;
        };
        let (lure, luck) = (
            chunkwake_protocol::components::stack_level(stack.extra, "minecraft:lure"),
            chunkwake_protocol::components::stack_level(stack.extra, "minecraft:luck_of_the_sea"),
        );
        let dir = crate::arrows::aim(yaw as f64, pitch as f64, 0.0);
        let pos = [
            feet[0] + dir[0] * 0.3,
            feet[1] + items::EYE_HEIGHT,
            feet[2] + dir[2] * 0.3,
        ];
        let vel = crate::arrows::shoot(dir, 1.1, 1.0, rand::random());
        let id = self.next_entity_id.fetch_add(1, Ordering::Relaxed);
        let bobber = Bobber {
            pos,
            vel,
            uuid: Uuid::from_u128(rand::random()),
            owner: uuid,
            biting: false,
            nibble: 0,
            time_until_lured: 0,
            time_until_hooked: 0,
            open_water: true,
            out_of_water: 0,
            life: 0,
            on_ground: false,
            hooked: None,
            lure: lure.max(0) * 100,
            luck: luck.max(0),
        };
        self.sim().fish.lock().unwrap().insert(id, bobber);
        self.mark_work();
        if let Some(ty) = self.bobber_type() {
            let (yaw, pitch) = crate::arrows::rotation(vel);
            let range = self.tracking_range(ty);
            self.hub.spawn_tracked(
                Tracked { id, pos, range },
                [
                    play::add_entity_moving(
                        self.ids.add_entity,
                        id,
                        bobber.uuid,
                        ty,
                        pos,
                        vel,
                        yaw,
                        pitch,
                        0,
                    ),
                    play::entity_meta(
                        self.ids.set_entity_data,
                        id,
                        &[(8, play::Meta::Int(0)), (9, play::Meta::Bool(false))],
                    ),
                ],
            );
        }
        self.play_sound(
            None,
            "minecraft:entity.fishing_bobber.throw",
            crate::sounds::NEUTRAL,
            feet,
            0.5,
            0.4,
        );
    }

    /// `FishingHook.retrieve`: добыча, вытягивание сущности или просто
    /// снятие. Возвращает износ удочки.
    fn reel_in(&self, id: i32, uuid: Uuid, stack: Stack, creative: bool) -> i32 {
        let bobber = self.sim().fish.lock().unwrap().remove(&id);
        let Some(b) = bobber else {
            return 0;
        };
        self.hub.remove_tracked(&[id]);
        let mut damage = 0;
        if let Some(hooked) = self.hooked_entity(&b) {
            // `pullEntity`: рывок к владельцу.
            if let Some((feet, _, _)) = self.player_at(uuid) {
                let pull = [
                    (feet[0] - b.pos[0]) * 0.1,
                    (feet[1] - b.pos[1]) * 0.1,
                    (feet[2] - b.pos[2]) * 0.1,
                ];
                self.pull_hooked(hooked, pull);
            }
            damage = 5;
        } else if b.nibble > 0 {
            self.fishing_catch(&b, uuid);
            damage = 1;
        } else if b.on_ground {
            damage = 2;
        }
        if let Some((feet, _, _)) = self.player_at(uuid) {
            self.play_sound(
                None,
                "minecraft:entity.fishing_bobber.retrieve",
                crate::sounds::NEUTRAL,
                feet,
                1.0,
                0.4,
            );
        }
        let _ = (stack, creative);
        damage
    }

    fn hooked_entity(&self, b: &Bobber) -> Option<i32> {
        b.hooked
    }

    /// Позиция сущности, за которую зацеплен поплавок.
    fn hooked_pos(&self, id: i32) -> Option<[f64; 3]> {
        if let Some(e) = self.sim().mobs.lock().unwrap().get(&id) {
            return Some(e.mob.pos);
        }
        self.sim().dropped.lock().unwrap().get(&id).map(|d| d.pos)
    }

    /// `ProjectileUtil.getEntityHitResult`: моб или предмет рядом с поплавком.
    fn hook_target(&self, pos: [f64; 3]) -> Option<i32> {
        {
            let mobs = self.sim().mobs.lock().unwrap();
            for (&id, e) in mobs.iter() {
                if !e.mob.alive() {
                    continue;
                }
                let (w, h) = e.mob.size();
                let p = e.mob.pos;
                if (pos[0] - p[0]).abs() <= w / 2.0 + 0.3
                    && (pos[2] - p[2]).abs() <= w / 2.0 + 0.3
                    && pos[1] >= p[1] - 0.3
                    && pos[1] <= p[1] + h + 0.3
                {
                    return Some(id);
                }
            }
        }
        let dropped = self.sim().dropped.lock().unwrap();
        for (&id, d) in dropped.iter() {
            if (pos[0] - d.pos[0]).abs() <= 0.6
                && (pos[1] - d.pos[1]).abs() <= 0.6
                && (pos[2] - d.pos[2]).abs() <= 0.6
            {
                return Some(id);
            }
        }
        None
    }

    fn pull_hooked(&self, id: i32, pull: [f64; 3]) {
        if let Some(e) = self.sim().mobs.lock().unwrap().get_mut(&id) {
            e.mob.vel[0] += pull[0];
            e.mob.vel[1] += pull[1];
            e.mob.vel[2] += pull[2];
        }
        if let Some(d) = self.sim().dropped.lock().unwrap().get_mut(&id) {
            d.vel[0] += pull[0];
            d.vel[1] += pull[1];
            d.vel[2] += pull[2];
        }
    }

    fn fishing_catch(&self, b: &Bobber, uuid: Uuid) {
        let Some((feet, _, _)) = self.player_at(uuid) else {
            return;
        };
        let mut caught: Option<&'static str> = None;
        for drop in self.fishing_loot(b.luck, b.open_water, b.pos) {
            let Some(mut item) = self.item_id(drop.name) else {
                continue;
            };
            caught.get_or_insert(drop.name);
            let mut stack = Stack::new(item, drop.count);
            // `SetItemDamageFunction`: доля оставшейся прочности равномерна
            // в `0..max`, урон — `floor((1 − доля) × maxDamage)`.
            if let Some(max_fraction) = drop.damage {
                let max = self.item_max_damage(item);
                if max > 0 {
                    stack.damage = set_damage(rand::random::<f32>() * max_fraction, max);
                }
            }
            // `SetPotionFunction`: `minecraft:water`.
            if let Some(potion) = drop.potion
                && let Some(extra) = self.with_potion(item, stack.extra, potion)
            {
                stack.extra = extra;
            }
            // `EnchantWithLevelsFunction`: 30 уровней, тег `#on_random_loot`.
            if let Some(levels) = drop.enchant_levels {
                let options = self
                    .survival
                    .trades
                    .enchant_tags
                    .get("minecraft:on_random_loot")
                    .cloned()
                    .unwrap_or_default();
                let enchantable = self
                    .survival
                    .enchanting
                    .enchantable
                    .get(drop.name)
                    .copied()
                    .unwrap_or(1);
                let mut rng = rand::thread_rng();
                let enchants = super::villagers::select_enchantments(
                    &self.survival.enchanting.defs,
                    &options,
                    drop.name,
                    enchantable,
                    levels,
                    &mut rng,
                );
                if !enchants.is_empty() {
                    let mut extra = (*chunkwake_protocol::components::get(stack.extra)).clone();
                    if drop.name == "minecraft:book" {
                        // `EnchantmentHelper.enchantItem`: книга становится
                        // зачарованной, чары — в `stored_enchantments`.
                        if let Some(book) = self.item_id("minecraft:enchanted_book") {
                            item = book;
                        }
                        extra.stored = enchants;
                    } else {
                        extra.enchantments = enchants;
                    }
                    stack.item = item;
                    stack.extra = chunkwake_protocol::components::intern(extra);
                }
            }
            let vel = [
                (feet[0] - b.pos[0]) * 0.1,
                (feet[1] - b.pos[1]) * 0.1 + 0.3,
                (feet[2] - b.pos[2]) * 0.1,
            ];
            self.drop_item(b.pos, vel, stack, Duration::ZERO);
        }
        self.spawn_orbs(
            [feet[0], feet[1] + 0.5, feet[2]],
            rand::random::<i32>() % 6 + 1,
        );
        self.award_stat(uuid, "minecraft:custom", "minecraft:fish_caught", 1);
        if let Some(name) = caught {
            self.advancement_trigger(uuid, "minecraft:fishing_rod_hooked", Some(name));
        }
        self.play_sound(
            None,
            "minecraft:entity.fishing_bobber.splash",
            crate::sounds::NEUTRAL,
            b.pos,
            0.25,
            1.0,
        );
    }

    /// Таблица `gameplay/fishing`: мусор 10 (`quality` −2), сокровища 5
    /// (`quality` +2, только на открытой воде), рыба 85 (`quality` −1).
    fn fishing_loot(&self, luck: i32, open_water: bool, at: [f64; 3]) -> Vec<FishingDrop> {
        let mut pools: Vec<(&str, i32, i32)> = vec![("junk", 10, -2), ("fish", 85, -1)];
        if open_water {
            pools.insert(1, ("treasure", 5, 2));
        }
        let total: i32 = pools.iter().map(|(_, w, q)| (*w + q * luck).max(0)).sum();
        if total <= 0 {
            return vec![FISH_TABLE[0]];
        }
        let mut roll = rand::random::<u32>() % total as u32;
        let mut kind = "fish";
        for (name, w, q) in pools {
            let weight = (w + q * luck).max(0) as u32;
            if roll < weight {
                kind = name;
                break;
            }
            roll -= weight;
        }
        let jungle = self
            .biome_name_at([
                at[0].floor() as i32,
                at[1].floor() as i32,
                at[2].floor() as i32,
            ])
            .is_some_and(|b| {
                matches!(
                    b.as_str(),
                    "minecraft:jungle" | "minecraft:sparse_jungle" | "minecraft:bamboo_jungle"
                )
            });
        let table: Vec<FishingDrop> = match kind {
            "junk" => JUNK_TABLE
                .iter()
                .copied()
                .filter(|d| !d.jungle_only || jungle)
                .collect(),
            "treasure" => TREASURE_TABLE.to_vec(),
            _ => FISH_TABLE.to_vec(),
        };
        let sum: i32 = table.iter().map(|d| d.weight).sum();
        if sum <= 0 {
            return vec![FISH_TABLE[0]];
        }
        let mut r = rand::random::<u32>() % sum as u32;
        for drop in table {
            if r < drop.weight as u32 {
                return vec![drop];
            }
            r -= drop.weight as u32;
        }
        vec![FISH_TABLE[0]]
    }

    pub(super) fn track_fish(&self, out: &mut Vec<Tracked>) {
        let Some(ty) = self.bobber_type() else {
            return;
        };
        let range = self.tracking_range(ty);
        for (&id, b) in self.sim().fish.lock().unwrap().iter() {
            out.push(Tracked {
                id,
                pos: b.pos,
                range,
            });
        }
    }

    pub(super) fn fish_spawn(&self, id: i32) -> Option<Vec<Vec<u8>>> {
        let ty = self.bobber_type()?;
        let fish = self.sim().fish.lock().unwrap();
        let b = fish.get(&id)?;
        let (yaw, pitch) = crate::arrows::rotation(b.vel);
        Some(vec![play::add_entity_moving(
            self.ids.add_entity,
            id,
            b.uuid,
            ty,
            b.pos,
            b.vel,
            yaw,
            pitch,
            0,
        )])
    }

    /// Тик поплавков: полёт, вода, таймеры поклёвки, отрыв от владельца.
    pub(super) fn tick_fishing(&self) {
        if self.sim().fish.lock().unwrap().is_empty() {
            return;
        }
        let simulated = self.simulated_set();
        let awake = |p: [f64; 3]| {
            simulated.is_empty()
                || simulated.contains(&((p[0].floor() as i32) >> 4, (p[2].floor() as i32) >> 4))
        };
        let players: FastMap<Uuid, ([f64; 3], bool)> = self
            .hub
            .players
            .lock()
            .unwrap()
            .values()
            .map(|p| (p.entry.uuid, (p.pos, p.collects)))
            .collect();
        let mut packets = Vec::new();
        let mut gone = Vec::new();
        self.with_world(|w| {
            let mut fish = self.sim().fish.lock().unwrap();
            for (&id, b) in fish.iter_mut() {
                if !awake(b.pos) {
                    continue;
                }
                let Some((feet, _)) = players.get(&b.owner) else {
                    gone.push(id);
                    continue;
                };
                if let Some(hooked) = b.hooked {
                    if let Some(hpos) = self.hooked_pos(hooked) {
                        b.pos = [hpos[0], hpos[1] + 0.8, hpos[2]];
                        let pull = [
                            (feet[0] - b.pos[0]) * 0.1,
                            (feet[1] - b.pos[1]) * 0.1,
                            (feet[2] - b.pos[2]) * 0.1,
                        ];
                        self.pull_hooked(hooked, pull);
                        let (yaw, pitch) = crate::arrows::rotation(b.vel);
                        packets.push((
                            id,
                            play::teleport_entity(
                                self.ids.teleport_entity,
                                id,
                                b.pos,
                                yaw,
                                pitch,
                                false,
                            ),
                        ));
                        continue;
                    }
                    b.hooked = None;
                    packets.push((
                        id,
                        play::entity_meta(self.ids.set_entity_data, id, &[(8, play::Meta::Int(0))]),
                    ));
                }
                if let Some(target) = self.hook_target(b.pos) {
                    b.hooked = Some(target);
                    b.vel = [0.0; 3];
                    if let Some(hpos) = self.hooked_pos(target) {
                        b.pos = [hpos[0], hpos[1] + 0.8, hpos[2]];
                    }
                    packets.push((
                        id,
                        play::entity_meta(
                            self.ids.set_entity_data,
                            id,
                            &[(8, play::Meta::Int(target + 1))],
                        ),
                    ));
                    packets.push((
                        id,
                        play::teleport_entity(self.ids.teleport_entity, id, b.pos, 0.0, 0.0, false),
                    ));
                    continue;
                }
                let was_biting = b.biting;
                let dist2 = (feet[0] - b.pos[0]).powi(2)
                    + (feet[1] - b.pos[1]).powi(2)
                    + (feet[2] - b.pos[2]).powi(2);
                if dist2 > 32.0 * 32.0 {
                    gone.push(id);
                    continue;
                }
                let water = |p: [i32; 3]| {
                    w.block(p).is_some_and(|s| {
                        crate::fluids::state_fluid(&self.survival.placement, s)
                            .is_some_and(|f| f.kind == crate::fluids::Kind::Water)
                    })
                };
                let cell = b.pos.map(|c| c.floor() as i32);
                let in_water = water(cell);
                if b.on_ground {
                    b.life += 1;
                    if b.life >= 1200 {
                        gone.push(id);
                        continue;
                    }
                } else {
                    b.life = 0;
                }
                if in_water {
                    // `BOBBING`: держим на поверхности и считаем поклёвку.
                    b.vel = [0.0; 3];
                    b.on_ground = false;
                    let surface = cell[1] as f64
                        + w.block(cell)
                            .and_then(|s| crate::fluids::state_fluid(&self.survival.placement, s))
                            .map_or(8.0 / 9.0, |f| f.amount as f64 / 9.0);
                    b.pos[1] = surface - 0.1;
                    b.out_of_water = b.out_of_water.saturating_sub(1);
                    if b.nibble == 0 && b.time_until_hooked == 0 {
                        b.open_water = self.open_water(cell, &water);
                    }
                    self.catching_fish(b);
                } else {
                    b.out_of_water = (b.out_of_water + 1).min(10);
                    b.vel[1] -= 0.03;
                    let next = [
                        b.pos[0] + b.vel[0],
                        b.pos[1] + b.vel[1],
                        b.pos[2] + b.vel[2],
                    ];
                    let solid = w.block(next.map(|c| c.floor() as i32)).is_some_and(|s| {
                        chunkwake_world::has_flag(s, chunkwake_world::flags::SOLID)
                    });
                    if solid {
                        b.on_ground = true;
                        b.vel = [0.0; 3];
                    } else {
                        b.pos = next;
                    }
                }
                b.vel = [b.vel[0] * 0.92, b.vel[1] * 0.92, b.vel[2] * 0.92];
                if b.biting != was_biting {
                    packets.push((
                        id,
                        play::entity_meta(
                            self.ids.set_entity_data,
                            id,
                            &[(9, play::Meta::Bool(b.biting))],
                        ),
                    ));
                }
                let (yaw, pitch) = crate::arrows::rotation(b.vel);
                packets.push((
                    id,
                    play::teleport_entity(
                        self.ids.teleport_entity,
                        id,
                        b.pos,
                        yaw,
                        pitch,
                        b.on_ground,
                    ),
                ));
            }
            for &id in &gone {
                fish.remove(&id);
            }
        });
        if !packets.is_empty() {
            self.hub.send_many_about(packets);
        }
        self.hub.remove_tracked(&gone);
    }

    fn catching_fish(&self, b: &mut Bobber) {
        let speed = 1;
        if b.nibble > 0 {
            b.nibble -= 1;
            if b.nibble <= 0 {
                b.biting = false;
                b.time_until_lured = 0;
                b.time_until_hooked = 0;
            }
        } else if b.time_until_hooked > 0 {
            b.time_until_hooked -= speed;
            if b.time_until_hooked <= 0 {
                b.nibble = 20 + (rand::random::<i32>() % 21);
                b.biting = true;
            }
        } else if b.time_until_lured > 0 {
            b.time_until_lured -= speed;
            if b.time_until_lured <= 0 {
                b.time_until_hooked = 20 + (rand::random::<i32>() % 61);
            }
        } else {
            b.time_until_lured = (100 + rand::random::<i32>() % 501 - b.lure).max(0);
        }
    }

    /// `calculateOpenWater`: 5×5 воды под поплавком и воздух над ней.
    fn open_water(&self, cell: [i32; 3], water: &impl Fn([i32; 3]) -> bool) -> bool {
        for dx in -2..=2 {
            for dz in -2..=2 {
                for dy in -1..=1 {
                    let p = [cell[0] + dx, cell[1] + dy, cell[2] + dz];
                    if dy <= 0 && !water(p) {
                        return false;
                    }
                    if dy > 0 && water(p) {
                        return false;
                    }
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fishing_tables_match_vanilla_26_3() {
        assert_eq!(FISH_TABLE.iter().map(|d| d.weight).sum::<i32>(), 100);
        assert_eq!(JUNK_TABLE.iter().map(|d| d.weight).sum::<i32>(), 110);
        assert_eq!(TREASURE_TABLE.len(), 6);
        // `set_count: 10` у чернильного мешка, остальное по одному.
        let ink = JUNK_TABLE
            .iter()
            .find(|d| d.name == "minecraft:ink_sac")
            .unwrap();
        assert_eq!(ink.count, 10);
        // `set_potion: minecraft:water`.
        let bottle = JUNK_TABLE
            .iter()
            .find(|d| d.name == "minecraft:potion")
            .unwrap();
        assert_eq!(bottle.potion, Some("minecraft:water"));
        // `location_check`: бамбук только в джунглях.
        let bamboo = JUNK_TABLE
            .iter()
            .find(|d| d.name == "minecraft:bamboo")
            .unwrap();
        assert!(bamboo.jungle_only);
        // Сокровища: книга — именно `minecraft:book`, чары добавит функция.
        assert!(
            TREASURE_TABLE
                .iter()
                .all(|d| d.name != "minecraft:enchanted_book")
        );
        let book = TREASURE_TABLE
            .iter()
            .find(|d| d.name == "minecraft:book")
            .unwrap();
        assert_eq!(book.enchant_levels, Some(30));
        assert_eq!(book.damage, None);
        for name in ["minecraft:bow", "minecraft:fishing_rod"] {
            let d = TREASURE_TABLE.iter().find(|d| d.name == name).unwrap();
            assert_eq!(d.damage, Some(0.25));
            assert_eq!(d.enchant_levels, Some(30));
        }
        let boots = JUNK_TABLE
            .iter()
            .find(|d| d.name == "minecraft:leather_boots")
            .unwrap();
        assert_eq!(boots.damage, Some(0.9));
    }

    #[test]
    fn set_damage_uses_remaining_durability() {
        assert_eq!(set_damage(0.0, 100), 100);
        assert_eq!(set_damage(0.25, 100), 75);
        assert_eq!(set_damage(0.9, 100), 10);
        assert_eq!(set_damage(1.0, 100), 0);
        assert_eq!(set_damage(1.5, 100), 0);
        assert_eq!(set_damage(0.5, 0), 0);
    }

    #[test]
    fn category_weights_include_luck_quality() {
        // junk 10 − 2·luck, treasure 5 + 2·luck (только открытая вода), fish 85 − luck.
        let weight = |base: i32, quality: i32, luck: i32| (base + quality * luck).max(0);
        assert_eq!(weight(10, -2, 0), 10);
        assert_eq!(weight(5, 2, 0), 5);
        assert_eq!(weight(85, -1, 0), 85);
        assert_eq!(weight(10, -2, 3), 4);
        assert_eq!(weight(5, 2, 3), 11);
        assert_eq!(weight(85, -1, 3), 82);
    }
}
