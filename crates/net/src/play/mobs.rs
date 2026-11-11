//! Живые мобы в `PlayPlan`: появление у клиентов, тик, удар игрока, смерть с лутом.
//! Логика самих мобов — в [`crate::mobs`].

use super::*;
use crate::mobs::{self as mb, Mob, Species};

/// Группа спавна: вид, вес, минимум и максимум.
type SpawnEntry = (Option<Species>, u32, u32, u32);
/// Игрок для тика мобов: ноги, имена предметов в руках (основная и вторая), UUID.
type Hands<'a> = ([f64; 3], [Option<&'a str>; 2], u128);

/// Предел детей одного деления. Плагин через `SlimeSplitEvent` не должен
/// заставлять сервер крутить цикл на произвольное число сущностей.
const MAX_SLIME_SPLIT: i32 = 128;

/// Моб и то, что о нём уже знают клиенты.
pub(super) struct MobEntry {
    pub mob: Mob,
    /// Последние `DATA_EFFECT_PARTICLES`/`DATA_EFFECT_AMBIENCE_ID`.
    effect_meta: (Vec<i32>, bool),
    /// Последняя разосланная позиция (квантованная) и повороты.
    sent: [i64; 3],
    sent_rot: (f32, f32, f32),
    moves: u32,
    /// Тики до следующего звука `ambient`.
    ambient: i32,
    /// Убит игроком: лут с `killed_by_player` и опыт.
    killer: Option<(Uuid, i32)>,
    environment: crate::environment::State,
    fall: f64,
    /// Клиентам разослан флаг «агрессивен» (зомби с поднятыми руками).
    aggressive: bool,
    /// Разосланные поля вида: набухание крипера, лазание паука, лук скелета.
    meta: MobMeta,
}

/// Синхронизируемые поля вида (`SynchedEntityData`), которые меняет тик.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct MobMeta {
    swell_dir: i32,
    powered: bool,
    ignited: bool,
    climbing: bool,
    using_item: bool,
    cube_size: i32,
}

impl MobMeta {
    fn of(m: &Mob) -> Self {
        Self {
            swell_dir: m.swell_dir,
            powered: m.powered,
            ignited: m.ignited,
            climbing: m.climbing,
            using_item: m.using_item.is_some(),
            cube_size: m.cube_size,
        }
    }

    /// Поля, отличные от `old` (`None` — все значимые для вида).
    fn fields(self, species: Species, old: Option<Self>) -> Vec<(u8, play::Meta)> {
        use play::Meta;
        let mut out = Vec::new();
        let changed = |f: fn(&Self) -> bool| old.is_none_or(|o| f(&o) != f(&self));
        match species {
            // `Creeper.DATA_SWELL_DIR`, `DATA_IS_POWERED`, `DATA_IS_IGNITED`.
            Species::Creeper => {
                if old.is_none_or(|o| o.swell_dir != self.swell_dir) {
                    out.push((16, Meta::Int(self.swell_dir)));
                }
                if changed(|m| m.powered) {
                    out.push((17, Meta::Bool(self.powered)));
                }
                if changed(|m| m.ignited) {
                    out.push((18, Meta::Bool(self.ignited)));
                }
            }
            // `Spider.DATA_FLAGS_ID`: бит 1 — лезет.
            Species::Spider if changed(|m| m.climbing) => {
                out.push((16, Meta::Byte(u8::from(self.climbing))));
            }
            // `AbstractCubeMob.ID_SIZE` (после полей `AgeableMob` на 16 и 17).
            Species::Slime if old.is_none_or(|o| o.cube_size != self.cube_size) => {
                out.push((18, Meta::Int(self.cube_size)));
            }
            // `LivingEntity.DATA_LIVING_ENTITY_FLAGS`: 1 — тянет предмет основной рукой.
            Species::Skeleton if changed(|m| m.using_item) => {
                out.push((8, Meta::Byte(u8::from(self.using_item))));
            }
            _ => {}
        }
        out
    }
}

impl MobEntry {
    fn new(mob: Mob) -> Self {
        Self {
            meta: MobMeta::of(&mob),
            sent: play::quantize(mob.pos),
            sent_rot: (mob.yaw, mob.pitch, mob.head_yaw),
            mob,
            moves: 0,
            ambient: 80 + rand::random::<i32>().rem_euclid(120),
            killer: None,
            environment: Default::default(),
            fall: 0.0,
            aggressive: false,
            effect_meta: (Vec::new(), false),
        }
    }
}

/// `Player.attack`: сила удара по заряду и признак крита.
#[derive(Clone, Copy, Debug)]
pub(super) struct MeleeHit {
    pub scale: f32,
    pub crit: bool,
    /// `LivingEntity.isFallFlying`: ломает smash-атаку булавы.
    pub gliding: bool,
}

/// `Player.doSweepAttack`: база урона, заряд и Sweeping Edge.
#[derive(Clone, Copy, Debug)]
pub(super) struct SweepHit {
    pub scale: f32,
    pub slot: usize,
    /// Урон предмета без чар (`1 + attack_damage`).
    pub item_damage: f32,
    /// Бонус Остроты (`getEnchantedDamage`).
    pub sharpness: f32,
    pub sweeping: i32,
}

impl PlayPlan {
    fn mob_type(&self, s: Species) -> i32 {
        // Собран при старте по `Species::ALL`; без него (тесты) — по имени.
        self.survival
            .mob_entity_types
            .get(s as usize)
            .or_else(|| self.survival.entity_types.get(s.name()))
            .copied()
            .unwrap_or_default()
    }

    fn mob_packets(&self, id: i32, m: &Mob) -> Vec<Vec<u8>> {
        let name = m
            .custom_name
            .as_ref()
            .map(|n| serde_json::json!({ "text": n }));
        // Флаг детёныша (индекс 16) — у `AgeableMob` и зомби; у крипера и
        // паука на 16 свои поля.
        let ageable = m.species.is_animal() || m.species == Species::Zombie;
        let mut out = vec![
            play::add_entity_moving(
                self.ids.add_entity,
                id,
                Uuid::from_u128(m.uuid),
                self.mob_type(m.species),
                m.pos,
                m.vel,
                m.yaw,
                m.pitch,
                0,
            ),
            play::rotate_head(self.ids.rotate_head, id, m.head_yaw),
            play::mob_data(
                self.ids.set_entity_data,
                id,
                m.health,
                ageable.then_some(m.is_baby()),
                (m.species == Species::Sheep).then_some(m.color | u8::from(m.sheared) << 4),
                name.as_ref(),
            ),
        ];
        let meta = MobMeta::of(m).fields(
            m.species,
            Some(MobMeta {
                swell_dir: -1,
                ..MobMeta::default()
            }),
        );
        if !meta.is_empty() {
            out.push(play::entity_meta(self.ids.set_entity_data, id, &meta));
        }
        // `DATA_EFFECT_PARTICLES`/`DATA_EFFECT_AMBIENCE_ID`: клиент рисует
        // частицы эффектов сам по метаданным.
        let (colors, ambient) = self.effect_particles(&m.effects);
        if !colors.is_empty()
            && let Some(&particle) = self
                .particle_types
                .lock()
                .unwrap()
                .get("minecraft:entity_effect")
        {
            out.push(play::entity_meta(
                self.ids.set_entity_data,
                id,
                &[
                    (10, play::Meta::Particles { particle, colors }),
                    (11, play::Meta::Bool(ambient)),
                ],
            ));
        }
        // `EntityEquipment`: все надетые предметы (`set_equipment`).
        let worn: Vec<(u8, Slot)> = mb::EQUIPMENT_SLOTS
            .iter()
            .enumerate()
            .filter_map(|(i, (_, id))| m.item(i).map(|s| (*id, Some(s))))
            .collect();
        if !worn.is_empty() {
            out.push(play::set_equipment(self.ids.set_equipment, id, &worn));
        }
        out
    }

    /// `Mob.populateDefaultEquipmentSlots` + `Zombie`/`AbstractSkeleton`:
    /// броня по `special_multiplier`, оружие с шансом от сложности. Затем
    /// `populateDefaultEquipmentEnchantments`: чары из `mob_spawn_equipment`.
    fn populate_equipment(&self, m: &mut Mob) {
        let difficulty = self.difficulty().id();
        let special =
            mb::cube::special_multiplier(difficulty, self.day_time.load(Ordering::Relaxed));
        let mut rng = || rand::random::<f32>();
        for (slot, name) in mb::random_equipment(m.species, difficulty, special, &mut rng) {
            if slot >= 2 && m.item(slot).is_some() {
                continue;
            }
            if let Some(item) = self.item_id(name) {
                m.set_item(slot, Some(Stack::new(item, 1)));
            }
        }
        // `Mob.enchantSpawnedEquipment`: оружие — `0.25 × special`, броня —
        // `0.5 × special`; стоимость `EnchantmentsByCostWithDifficulty`
        // (`min_cost 5`, `max_cost_span 17`) и чары тега
        // `#minecraft:on_mob_spawn_equipment`.
        let mut r = crate::enchanting::JavaRandom::new(rand::random::<i64>());
        let span = (special * 17.0) as i32;
        for (slot, chance) in [(0usize, 0.25f32), (2, 0.5), (3, 0.5), (4, 0.5), (5, 0.5)] {
            let Some(stack) = m.item(slot) else { continue };
            if r.next_float() >= chance * special {
                continue;
            }
            let cost = 5 + r.below(span + 1);
            let list = crate::enchanting::mob_spawn_enchantments(
                &self.survival.enchanting,
                self.item_name(stack.item),
                cost,
                &mut r,
            );
            if list.is_empty() {
                continue;
            }
            let mut extra = (*chunkwake_protocol::components::get(stack.extra)).clone();
            extra.enchantments = list;
            m.set_item(
                slot,
                Some(Stack {
                    extra: chunkwake_protocol::components::intern(extra),
                    ..stack
                }),
            );
        }
    }

    /// Новый моб в мире; возвращает ID сущности. `reason` — причина для
    /// `CreatureSpawnEvent` (`None` — без события: спавн по запросу плагина).
    pub(crate) fn spawn_mob(
        &self,
        species: Species,
        pos: [f64; 3],
        yaw: f32,
        baby: bool,
        reason: Option<&str>,
    ) -> Option<i32> {
        self.spawn_mob_with(species, pos, yaw, baby, reason, |_| {})
    }

    /// [`Self::spawn_mob`] с правкой моба до рассылки (`setup`: размер детей
    /// слизня, имя и постоянство от родителя) и `finalizeSpawn`.
    pub(crate) fn spawn_mob_with(
        &self,
        species: Species,
        pos: [f64; 3],
        yaw: f32,
        baby: bool,
        reason: Option<&str>,
        setup: impl FnOnce(&mut Mob),
    ) -> Option<i32> {
        self.spawn_mob_final(species, pos, yaw, baby, reason, true, setup)
    }

    /// `/summon <entity> <pos> <nbt>`: в ванили с NBT `finalize = false`,
    /// поэтому случайного снаряжения нет — только то, что в NBT.
    pub(crate) fn spawn_mob_nbt(
        &self,
        species: Species,
        pos: [f64; 3],
        yaw: f32,
        nbt: &Compound,
    ) -> Option<i32> {
        self.spawn_mob_final(species, pos, yaw, false, Some("COMMAND"), false, |m| {
            self.apply_mob_nbt(m, nbt);
        })
    }

    /// Общий путь спавна: `finalize` включает `finalizeSpawn` (случайное
    /// снаряжение и размер слизня), `setup` правит моба из NBT.
    #[allow(clippy::too_many_arguments)]
    fn spawn_mob_final(
        &self,
        species: Species,
        pos: [f64; 3],
        yaw: f32,
        baby: bool,
        reason: Option<&str>,
        finalize: bool,
        setup: impl FnOnce(&mut Mob),
    ) -> Option<i32> {
        let id = self.next_entity_id.fetch_add(1, Ordering::Relaxed);
        let mut m = Mob::new(species, pos, rand::random());
        (m.yaw, m.head_yaw) = (yaw, yaw);
        if baby {
            m.age = mb::BABY_AGE;
        }
        if species == Species::Sheep {
            m.color = sheep_color(rand::random());
        }
        // `Slime.finalizeSpawn`: случайный размер 1, 2 или 4.
        if finalize && species == Species::Slime {
            let special = mb::cube::special_multiplier(
                self.difficulty().id(),
                self.day_time.load(Ordering::Relaxed),
            );
            m.set_size(
                mb::cube::spawn_size(rand::random(), rand::random(), special),
                true,
            );
        }
        // `Zombie.finalizeSpawn`: шанс ломать двери `specialMultiplier × 0.1`.
        if finalize && m.species == Species::Zombie {
            let special = mb::cube::special_multiplier(
                self.difficulty().id(),
                self.day_time.load(Ordering::Relaxed),
            );
            m.can_break_doors = rand::random::<f32>() < special * 0.1;
        }
        // Внеочередная экипировка мобов (`finalizeSpawn`): броня по сложности,
        // у зомби — случайное оружие, у скелета — лук.
        if finalize && reason != Some("CONVERSION") {
            self.populate_equipment(&mut m);
        }
        // Исчезать вдали от игроков могут только природные: с яйца, из команды
        // и от плагина моб остаётся (`setPersistenceRequired`).
        m.persistent = !matches!(reason, Some("NATURAL" | "CHUNK_GEN"));
        setup(&mut m);
        let packets = self.mob_packets(id, &m);
        let tracked = self.mob_tracked(id, &m);
        self.sim().mobs.lock().unwrap().insert(id, MobEntry::new(m));
        self.mark_work();
        // Моб уже в мире (плагин может спросить о нём), но клиентам — только после события.
        if let Some(reason) = reason
            && self
                .sync_event(Event::CreatureSpawn, || {
                    format!(
                        "{id} {} {} {} {} {reason}",
                        species.short(),
                        pos[0],
                        pos[1],
                        pos[2]
                    )
                })
                .is_some_and(|r| r.trim() == "c")
        {
            self.sim().mobs.lock().unwrap().remove(&id);
            return None;
        }
        if !self.sim().mobs.lock().unwrap().contains_key(&id) {
            return None;
        }
        // Три пакета спавна — одной рассылкой: одно сжатие и одно
        // пробуждение получателей вместо трёх.
        self.hub.spawn_tracked(tracked, packets);
        Some(id)
    }

    /// Слежение за мобами: ID, позиция и дальность вида.
    pub(super) fn track_mobs(&self, out: &mut Vec<Tracked>) {
        for (&id, e) in self.sim().mobs.lock().unwrap().iter() {
            out.push(Tracked {
                id,
                pos: e.mob.pos,
                range: self.tracking_range(self.mob_type(e.mob.species)),
            });
        }
    }

    /// Пакеты спавна моба для нового зрителя (`ServerEntity.sendPairingData`).
    pub(super) fn mob_spawn(&self, id: i32) -> Option<Vec<Vec<u8>>> {
        let mobs = self.sim().mobs.lock().unwrap();
        let e = mobs.get(&id)?;
        let mut out = self.mob_packets(id, &e.mob);
        if e.aggressive {
            out.push(play::mob_flags(self.ids.set_entity_data, id, 4));
        }
        Some(out)
    }

    fn mob_tracked(&self, id: i32, m: &Mob) -> Tracked {
        Tracked {
            id,
            pos: m.pos,
            range: self.tracking_range(self.mob_type(m.species)),
        }
    }

    /// Тик мобов: ИИ, физика, рассылка движений, звуки, исчезновение мёртвых.
    pub(super) fn tick_mobs(&self) {
        let loaded = std::mem::take(&mut *self.sim().mobs_loaded.lock().unwrap());
        if !loaded.is_empty() {
            // Все пакеты появления с диска — одной рассылкой.
            for e in loaded {
                let id = self.next_entity_id.fetch_add(1, Ordering::Relaxed);
                let packets = self.mob_packets(id, &e.mob);
                let tracked = self.mob_tracked(id, &e.mob);
                self.sim().mobs.lock().unwrap().insert(id, e);
                self.hub.spawn_tracked(tracked, packets);
            }
            self.mark_work();
        }
        if self.sim().mobs.lock().unwrap().is_empty() {
            return;
        }
        // Мобы вне радиуса симуляции спят: как в ванили, где тик идёт только
        // по чанкам рядом с игроками.
        let simulated = self.simulated_set();
        let awake = |pos: [f64; 3]| {
            simulated.is_empty()
                || simulated.contains(&((pos[0].floor() as i32) >> 4, (pos[2].floor() as i32) >> 4))
        };
        // Игроки и еда в руках (основная и вторая). Строки берутся один раз
        // на тик, а не на каждого моба: раньше здесь на моба строился `Vec`
        // с `is_food` по строкам предметов.
        let dim = self.active_dim();
        // Кого монстры могут атаковать: игроки этого мира не в творческом.
        let mut prey: Vec<mb::Prey> = Vec::new();
        let hands: Vec<Hands> = {
            let live = self.live.lock().unwrap();
            let players = self.hub.players.lock().unwrap();
            players
                .values()
                .filter(|p| p.collects && p.dimension == dim)
                .map(|p| {
                    let l = live.get(&p.entry.uuid);
                    if l.is_none_or(|l| l.game_mode != 1) {
                        prey.push(mb::Prey {
                            uuid: p.entry.uuid.as_u128(),
                            pos: p.pos,
                        });
                    }
                    let held = l.map_or([None, None], |l| {
                        [
                            l.slots[36 + l.selected].map(|s| self.item_name(s.item)),
                            l.slots[survival::OFFHAND].map(|s| self.item_name(s.item)),
                        ]
                    });
                    (p.pos, held, p.entry.uuid.as_u128())
                })
                .collect()
        };
        // Кого приманивает еда — по видам, один проход на вид за тик, а не на моба.
        let mut food_by_species: Vec<Vec<[f64; 3]>> = vec![Vec::new(); mb::Species::ALL.len()];
        {
            let mut seen = [false; mb::Species::ALL.len()];
            for (pos, held, _) in &hands {
                for species in mb::Species::ALL {
                    let slot = species as usize;
                    if !seen[slot] && held.iter().flatten().any(|n| species.is_food(n)) {
                        seen[slot] = true;
                        food_by_species[slot].push(*pos);
                    }
                }
            }
        }
        // Все игроки мира: на них смотрят мобы, от них считается исчезновение.
        let watchers: Vec<mb::Prey> = hands
            .iter()
            .map(|h| mb::Prey {
                uuid: h.2,
                pos: h.0,
            })
            .collect();
        let peaceful = self.difficulty() == Difficulty::Peaceful;
        let difficulty = self.difficulty().id();
        let mob_griefing = self.vars.flag("mob_griefing");
        let min_y = self.min_y();
        let weather = self.vars.weather();
        let darken = self.sky_darken_now(dim, weather.rain, weather.thunder);
        let mut strikes = Vec::new();
        let mut outs = Vec::new();
        let mut blasts = Vec::new();
        let mut despawned = Vec::new();
        let mut rng = rand::thread_rng();
        let mut rand = || rand::Rng::r#gen::<f64>(&mut rng);
        let mut packets: Vec<(i32, Vec<u8>)> = Vec::new();
        let mut gone = Vec::new();
        let mut sounds = Vec::new();
        let mut trampling = Vec::new();
        let mut vibrations = Vec::new();
        let now = self.game_time.load(Ordering::Relaxed);
        let border = self.border_at(now);
        {
            let mut mobs = self.sim().mobs.lock().unwrap();
            // Животные до тика: пары (`BreedGoal`) и родители (`FollowParentGoal`).
            let kin: Vec<mb::goals::Kin> = mobs
                .iter()
                .filter(|(_, e)| e.mob.species.is_animal() && e.mob.alive())
                .map(|(&id, e)| e.mob.kin(id))
                .collect();
            // `EatBlockGoal.IS_EDIBLE`: `#edible_for_sheep`.
            let edible = self.survival.block_tags.get("minecraft:edible_for_sheep");
            // Один замок мира на всю фазу: коробки коллизий читаются тысячами раз.
            self.with_world(|w| {
                for (&id, e) in mobs.iter_mut() {
                    // Выгруженный чанк не тикает: иначе исследование мира копит
                    // работу, которую никто не видит.
                    if !awake(e.mob.pos) {
                        continue;
                    }
                    let species = e.mob.species;
                    let monster = species.is_monster();
                    // Замыкания мира — по два указателя, без аллокаций: они лишь
                    // держат ссылку на `WorldView` этой фазы.
                    let shape = |c: [i32; 3]| self.shape_at(w.block(c), c);
                    let water = |c: [i32; 3]| self.water_at(w.block(c));
                    let cell = |c: [i32; 3]| self.cell_at(w.block(c), c);
                    // `getWalkTargetValue` и `EatBlockGoal`: дёрн, трава и яркость.
                    let info = |c: [i32; 3]| {
                        let state = w.block(c);
                        let sky = w.sky_light_at(c).unwrap_or(0);
                        let raw = mb::raw_brightness(sky, w.block_light_at(c).unwrap_or(0), darken);
                        mb::goals::CellInfo {
                            grass_block: state.is_some_and(|s| {
                                chunkwake_world::has_flag(s, chunkwake_world::flags::GRASS)
                            }),
                            short_grass: state.is_some_and(|s| {
                                edible.is_some_and(|t| t.contains(self.state_name(s)))
                            }),
                            brightness: mb::brightness(raw),
                            sky: sky == 15,
                        }
                    };
                    let world = mb::World {
                        shape: &shape,
                        water: &water,
                        cell: Some(&cell),
                        info: Some(&info),
                        min_y,
                    };
                    let sight = |a: [f64; 3], b: [f64; 3]| self.clear_line(w, a, b);
                    let jump_boost =
                        self.survival
                            .effects
                            .get("minecraft:jump_boost")
                            .and_then(|&id| {
                                e.mob
                                    .effects
                                    .iter()
                                    .find(|effect| effect.id == id)
                                    .map(|effect| effect.amplifier)
                            });
                    let mut senses = mb::Senses {
                        players: &watchers,
                        rand: &mut rand,
                        prey: if monster { &prey[..] } else { &[] },
                        // Уже отфильтрованные по виду: строки предметов не сравниваются.
                        tempters: &food_by_species[species as usize][..],
                        kin: if monster { &[] } else { &kin[..] },
                        sight: &sight,
                        difficulty,
                        entity_id: id,
                        bright_outside: darken < 4,
                        mob_griefing,
                        jump_boost,
                    };
                    let was_grounded = e.mob.on_ground;
                    let old_pos = e.mob.pos;
                    let old_y = e.mob.pos[1];
                    e.mob.on_fire = e.environment.burning > 0;
                    if mb::tick(&mut e.mob, &world, &mut senses) {
                        // Крипер взорвался (`discard`): без лута и анимации смерти.
                        let blast = e.mob.out.iter().find_map(|o| match o {
                            mb::goals::Out::Explode(p) => Some(*p),
                            _ => None,
                        });
                        if let Some(power) = blast {
                            blasts.push((id, e.mob.pos, power, species.name()));
                            e.mob.out.clear();
                            continue;
                        }
                        gone.push(id);
                        continue;
                    }
                    let clipped = border.clip_movement(now, old_pos, e.mob.pos, e.mob.size().0);
                    for axis in [0, 2] {
                        if clipped[axis] != e.mob.pos[axis] {
                            e.mob.pos[axis] = clipped[axis];
                            e.mob.vel[axis] = 0.0;
                        }
                    }
                    let descent = (old_y - e.mob.pos[1]).max(0.0);
                    let landed_fall = if !was_grounded && e.mob.on_ground {
                        let fall = e.fall + descent;
                        if rand() < crate::soil::trample_chance(fall) {
                            trampling.push(e.mob.pos);
                        }
                        e.fall = 0.0;
                        Some(fall)
                    } else if !e.mob.on_ground {
                        e.fall += descent;
                        None
                    } else {
                        e.fall = 0.0;
                        None
                    };
                    let (width, height) = e.mob.size();
                    let view =
                        crate::environment::probe(e.mob.pos, width, height, height * 0.85, |p| {
                            self.environment_kind(w.block(p))
                        });
                    if !view.water && !view.lava && landed_fall.is_some_and(|fall| fall > 0.0) {
                        vibrations.push((e.mob.pos, "minecraft:hit_ground"));
                    }
                    // `LivingEntity.causeFallDamage`: в воде и лаве падение
                    // сбрасывается без урона.
                    if view.water || view.lava {
                        e.fall = 0.0;
                    } else if let Some(fall) = landed_fall {
                        let damage = mb::fall_damage(
                            fall,
                            mb::safe_fall_distance(species.safe_fall(), jump_boost),
                        );
                        if damage > 0.0
                            && let Some(died) =
                                mb::hurt(&mut e.mob, damage, mb::Source::default(), 0.0)
                        {
                            packets
                                .push((id, play::hurt_animation(self.ids.hurt_animation, id, 0.0)));
                            packets.push((
                                id,
                                play::mob_data(
                                    self.ids.set_entity_data,
                                    id,
                                    e.mob.health,
                                    None,
                                    None,
                                    None,
                                ),
                            ));
                            sounds.push((
                                e.mob.sound(if died { "death" } else { "hurt" }),
                                e.mob.pos,
                                e.mob.volume(),
                                e.mob.is_baby(),
                                sound_source(species),
                            ));
                            vibrations.push((e.mob.pos, "minecraft:entity_damage"));
                            if died {
                                vibrations.push((e.mob.pos, "minecraft:entity_die"));
                            }
                        }
                    }
                    let old = e.environment;
                    if monster && e.mob.alive() {
                        // Сырая яркость у глаз: монстр на свету быстрее «скучает»
                        // (`Monster.updateNoActionTime`), зомби горит на солнце.
                        let cell = e.mob.eye().map(|v| v.floor() as i32);
                        let sky = w.sky_light_at(cell).unwrap_or(0);
                        let raw =
                            mb::raw_brightness(sky, w.block_light_at(cell).unwrap_or(0), darken);
                        if mb::brightness(raw) > 0.5 {
                            e.mob.no_action = e.mob.no_action.saturating_add(2);
                        }
                        let sees_sky = sky == 15;
                        let wet = view.water || (weather.rain && sees_sky);
                        if species.burns_in_sun()
                            && mb::sun_burn_tick(raw, darken, sees_sky, wet, rand())
                        {
                            // `igniteForSeconds(8)`.
                            e.environment.burning = e.environment.burning.max(160);
                        }
                    }
                    let pulse =
                        e.environment
                            .step(view, self.server_ticks.load(Ordering::Relaxed), false);
                    if old.burning == 0 && e.environment.burning > 0
                        || old.burning > 0 && e.environment.burning == 0
                    {
                        packets.push((
                            id,
                            inv::set_entity_flags(
                                self.ids.set_entity_data,
                                id,
                                u8::from(e.environment.burning > 0),
                            ),
                        ));
                    }
                    if old.air != e.environment.air {
                        packets.push((
                            id,
                            inv::set_entity_air(self.ids.set_entity_data, id, e.environment.air),
                        ));
                    }
                    if old.frozen != e.environment.frozen {
                        packets.push((
                            id,
                            inv::set_entity_frozen(
                                self.ids.set_entity_data,
                                id,
                                e.environment.frozen as i32,
                            ),
                        ));
                    }
                    let moving = e.mob.vel.iter().any(|v| v.abs() > 0.003);
                    let fire_damage = self.vars.flag("fire_damage");
                    // Зелья: регенерация лечит, яд и иссушение бьют раз в
                    // `25 >> amp` / `40 >> amp` тиков (`LivingEntity.tickEffects`).
                    let fx = e.mob.tick_effects(self.effect_kinds());
                    let effect_meta = self.effect_particles(&e.mob.effects);
                    if effect_meta != e.effect_meta {
                        e.effect_meta = effect_meta;
                        let (colors, ambient) = &e.effect_meta;
                        if let Some(&particle) = self
                            .particle_types
                            .lock()
                            .unwrap()
                            .get("minecraft:entity_effect")
                        {
                            packets.push((
                                id,
                                play::entity_meta(
                                    self.ids.set_entity_data,
                                    id,
                                    &[
                                        (
                                            10,
                                            play::Meta::Particles {
                                                particle,
                                                colors: colors.clone(),
                                            },
                                        ),
                                        (11, play::Meta::Bool(*ambient)),
                                    ],
                                ),
                            ));
                        }
                    }
                    if fx.heal > 0.0 && e.mob.alive() {
                        e.mob.health = (e.mob.health + fx.heal).min(e.mob.max_health());
                        packets.push((
                            id,
                            play::mob_data(
                                self.ids.set_entity_data,
                                id,
                                e.mob.health,
                                None,
                                None,
                                None,
                            ),
                        ));
                    }
                    for (hit, amount) in [
                        (pulse.drown && self.vars.flag("drowning_damage"), 2.0),
                        (pulse.freeze && self.vars.flag("freeze_damage"), 1.0),
                        (pulse.lava && fire_damage, 4.0),
                        (pulse.burn && fire_damage && !pulse.lava, 1.0),
                        (pulse.cactus, 1.0),
                        (pulse.berries && moving, 1.0),
                        (pulse.magma && fire_damage, 1.0),
                    ]
                    .into_iter()
                    .chain([
                        // `PoisonMobEffect`: не добивает, пока здоровье > 1.
                        (fx.poison && e.mob.health > 1.0, 1.0),
                        (fx.wither, 1.0),
                    ]) {
                        if hit
                            && let Some(died) =
                                mb::hurt(&mut e.mob, amount, mb::Source::default(), 0.0)
                        {
                            packets
                                .push((id, play::hurt_animation(self.ids.hurt_animation, id, 0.0)));
                            packets.push((
                                id,
                                play::mob_data(
                                    self.ids.set_entity_data,
                                    id,
                                    e.mob.health,
                                    None,
                                    None,
                                    None,
                                ),
                            ));
                            vibrations.push((e.mob.pos, "minecraft:entity_damage"));
                            if died {
                                vibrations.push((e.mob.pos, "minecraft:entity_die"));
                            }
                        }
                    }
                    if let Some(target) = e.mob.strike.take() {
                        strikes.push((
                            species,
                            Uuid::from_u128(target),
                            e.mob.pos,
                            e.mob.attack_damage(),
                        ));
                    }
                    // `Slime.playerTouch` вызывается для каждого игрока,
                    // чья раздутая рамка пересекла слизня.
                    for target in std::mem::take(&mut e.mob.touch_strikes) {
                        strikes.push((
                            species,
                            Uuid::from_u128(target),
                            e.mob.pos,
                            e.mob.attack_damage(),
                        ));
                    }
                    if !e.mob.out.is_empty() {
                        outs.extend(e.mob.out.drain(..).map(|o| (id, o)));
                    }
                    let meta = MobMeta::of(&e.mob);
                    if meta != e.meta {
                        let fields = meta.fields(species, Some(e.meta));
                        e.meta = meta;
                        if !fields.is_empty() {
                            packets.push((
                                id,
                                play::entity_meta(self.ids.set_entity_data, id, &fields),
                            ));
                        }
                    }
                    let aggressive = e.mob.aggressive;
                    if aggressive != e.aggressive {
                        e.aggressive = aggressive;
                        packets.push((
                            id,
                            play::mob_flags(
                                self.ids.set_entity_data,
                                id,
                                if aggressive { 4 } else { 0 },
                            ),
                        ));
                    }
                    // `Mob.checkDespawn`: монстры вдали от игроков и в мирной сложности.
                    let nearest = watchers
                        .iter()
                        .map(|p| mb::dist2(p.pos, e.mob.pos))
                        .min_by(f64::total_cmp);
                    if e.mob.alive() && mb::check_despawn(&mut e.mob, nearest, peaceful, rand()) {
                        despawned.push(id);
                        continue;
                    }
                    let m = &e.mob;
                    if m.alive() && species.info().ambient {
                        e.ambient -= 1;
                        if e.ambient <= 0 {
                            e.ambient = 80 + rand::random::<i32>().rem_euclid(120);
                            sounds.push((
                                format!("{}.ambient", species.sound()),
                                m.pos,
                                species.volume(),
                                m.is_baby(),
                                sound_source(species),
                            ));
                        }
                    }
                    let q = play::quantize(m.pos);
                    let rot = (m.yaw, m.pitch, m.head_yaw);
                    let moved = q != e.sent;
                    let turned =
                        (rot.0 - e.sent_rot.0).abs() > 1.0 || (rot.1 - e.sent_rot.1).abs() > 1.0;
                    if moved || turned {
                        e.moves += 1;
                        let packet = match play::delta(e.sent, q).filter(|_| e.moves % 60 != 0) {
                            Some(d) => play::move_entity_pos_rot(
                                self.ids.move_entity_pos_rot,
                                id,
                                d,
                                m.yaw,
                                m.pitch,
                                m.on_ground,
                            ),
                            None => play::teleport_entity(
                                self.ids.teleport_entity,
                                id,
                                m.pos,
                                m.yaw,
                                m.pitch,
                                m.on_ground,
                            ),
                        };
                        packets.push((id, packet));
                        e.sent = q;
                        (e.sent_rot.0, e.sent_rot.1) = (rot.0, rot.1);
                    }
                    if (rot.2 - e.sent_rot.2).abs() > 1.0 {
                        packets.push((id, play::rotate_head(self.ids.rotate_head, id, m.head_yaw)));
                        e.sent_rot.2 = rot.2;
                    }
                }
            });
        }
        for (pos, event) in vibrations {
            self.emit_vibration(pos, event);
        }
        // Движения и повороты всех мобов тика — одной рассылкой: при стаде,
        // где сдвинулись все, это одна запись в канал вместо сотни.
        self.hub.send_many_about(packets);
        for feet in trampling {
            self.trample_farmland(feet);
        }
        for (name, pos, vol, baby, source) in sounds {
            self.play_sound(None, &name, source, pos, vol, mob_pitch(baby));
        }
        if !despawned.is_empty() {
            {
                let mut mobs = self.sim().mobs.lock().unwrap();
                for id in &despawned {
                    mobs.remove(id);
                }
            }
            self.hub.remove_tracked(&despawned);
        }
        for (species, target, from, damage) in strikes {
            self.mob_strike(species, target, from, damage);
        }
        if !blasts.is_empty() {
            let ids: Vec<i32> = blasts.iter().map(|b| b.0).collect();
            {
                let mut mobs = self.sim().mobs.lock().unwrap();
                for id in &ids {
                    mobs.remove(id);
                }
            }
            self.hub.remove_tracked(&ids);
            for (_, pos, power, kind) in blasts {
                self.explode_mob(pos, power, kind);
            }
        }
        self.apply_mob_outs(outs);
        if !gone.is_empty() {
            // `Slime.remove`: число детей решается (и `SlimeSplitEvent` идёт),
            // пока умерший ещё в мире.
            let splits: Vec<(i32, i32)> = {
                let mobs = self.sim().mobs.lock().unwrap();
                gone.iter()
                    .filter(|id| {
                        mobs.get(id)
                            .is_some_and(|e| e.mob.species == Species::Slime && e.mob.cube_size > 1)
                    })
                    .map(|&id| (id, 2 + rand::random::<u32>().rem_euclid(3) as i32))
                    .collect()
            };
            let splits: Vec<(i32, i32)> = splits
                .into_iter()
                .filter_map(|(id, count)| {
                    let reply =
                        self.sync_event(Event::SlimeSplit, || format!("{id} slime {count}"));
                    match reply.as_deref().map(str::trim) {
                        Some("c") => None,
                        Some(n) if !n.is_empty() => n
                            .parse()
                            .ok()
                            .map(|n: i32| (id, n.clamp(0, MAX_SLIME_SPLIT))),
                        _ => Some((id, count.clamp(0, MAX_SLIME_SPLIT))),
                    }
                })
                .collect();
            let dead: Vec<(i32, MobEntry)> = {
                let mut mobs = self.sim().mobs.lock().unwrap();
                gone.iter()
                    .filter_map(|id| Some((*id, mobs.remove(id)?)))
                    .collect()
            };
            self.hub.remove_tracked(&gone);
            for (id, e) in dead {
                self.mob_loot(id, &e);
                if let Some(&(_, count)) = splits.iter().find(|s| s.0 == id) {
                    self.slime_split(&e.mob, count);
                }
            }
        }
    }

    /// Коробки коллизии блока для моба: `getCollisionShape` 26.3.
    ///
    /// Формы берутся из отчёта `shapes.csv` (`BlockPhysics`), а без него —
    /// старый грубый вариант: всё твёрдое — полный куб, жидкости и
    /// заменяемое — ничего.
    fn shape_at(&self, s: Option<u32>, c: [i32; 3]) -> &[[f64; 6]] {
        let Some(s) = s else {
            return if c[1] < self.min_y() { mb::FULL } else { &[] };
        };
        if let Some(p) = self.survival.physics.as_deref() {
            return p.boxes_of(s);
        }
        // Без `shapes.csv`: всё твёрдое — полный куб. Флаг считается при
        // сборке мира по именам, сравнения строк здесь нет.
        if chunkwake_world::has_flag(s, chunkwake_world::flags::SOLID) {
            mb::FULL
        } else {
            &[]
        }
    }

    /// Клетка поиска пути: из таблицы состояний, без неё — по форме и воде.
    fn cell_at(&self, s: Option<u32>, c: [i32; 3]) -> mb::path::Cell {
        use mb::path::{Cell, PathType};
        let Some(s) = s else {
            return if c[1] < self.min_y() {
                Cell {
                    kind: PathType::Blocked,
                    flags: Cell::SOLID,
                }
            } else {
                Cell::AIR_CELL
            };
        };
        if let Some(t) = self.survival.path_cells.as_deref()
            && let Some(&cell) = t.get(s as usize)
        {
            return cell;
        }
        if self.water_at(Some(s)) {
            Cell {
                kind: PathType::Water,
                flags: Cell::LAND,
            }
        } else if self.shape_at(Some(s), c).is_empty() {
            Cell::AIR_CELL
        } else {
            Cell {
                kind: PathType::Blocked,
                flags: Cell::SOLID,
            }
        }
    }

    /// Вода ли в блоке (`state_fluid`).
    fn water_at(&self, s: Option<u32>) -> bool {
        s.and_then(|s| crate::fluids::state_fluid(&self.survival.placement, s))
            .is_some_and(|f| f.kind == crate::fluids::Kind::Water)
    }

    /// Лут и опыт умершего моба (`LivingEntity.dropAllDeathLoot`).
    fn mob_loot(&self, id: i32, e: &MobEntry) {
        let m = &e.mob;
        // Детёныши животных ничего не роняют; детёныш зомби — как взрослый.
        if m.is_baby() && m.species.is_animal() {
            return;
        }
        let weapon = e.killer.and_then(|(_, slot)| (slot >= 0).then_some(slot));
        let held: Slot = e.killer.and_then(|(uuid, _)| {
            let live = self.live.lock().unwrap();
            let l = live.get(&uuid)?;
            weapon.and_then(|s| l.slots[s as usize])
        });
        let level = |n: &str| {
            held.map_or(0, |s| {
                chunkwake_protocol::components::stack_level(s.extra, n)
            })
        };
        let tool = held.map(|s| self.item_name(s.item));
        let color = SHEEP_COLORS[m.color as usize % 16];
        let sheared = m.sheared;
        let on_fire = e.environment.burning > 0;
        let size = m.cube_size;
        let props = move |k: &str| match k {
            "color" => Some(color.to_owned()),
            "sheared" => Some(sheared.to_string()),
            "on_fire" => Some(on_fire.to_string()),
            "size" => Some(size.to_string()),
            _ => None,
        };
        let smelt = |name: &str| {
            let item = self.item_id(name)?;
            let c = self.survival.recipes.cooking(0, item)?;
            Some(self.item_name(c.result.0).to_owned())
        };
        let no_prop = |_: &str| None;
        let ctx = crate::loot::Ctx {
            prop: &no_prop,
            block: m.species.loot(),
            tool,
            level: &level,
            entity: Some(&props),
            killed_by_player: e.killer.is_some(),
            smelt: &smelt,
        };
        let center = [m.pos[0], m.pos[1] + m.size().1 / 2.0, m.pos[2]];
        let mut drops: Vec<Stack> =
            crate::loot::roll(&self.survival.loot, &ctx, &mut rand::thread_rng())
                .into_iter()
                .filter_map(|(name, count)| {
                    let item = self.item_id(&name)?;
                    Some(Stack::new(item, count.min(self.max_stack(item).max(1))))
                })
                .collect();
        let mut xp = if e.killer.is_some() {
            m.xp(rand::random())
        } else {
            0
        };
        // `Mob.dropCustomDeathLoot`: экипировка по `drop_chances` (с бонусом
        // Добычи 0.01 за уровень); без убийцы — только «сохраняемые» (> 1).
        let killed_by_player = e.killer.is_some();
        let looting = level("minecraft:looting");
        for (slot, _) in mb::EQUIPMENT_SLOTS.iter().enumerate() {
            let Some(stack) = m.item(slot) else { continue };
            let mut chance = m.drop_chance(slot);
            if chance == 0.0 {
                continue;
            }
            let preserve = chance > 1.0;
            if !killed_by_player && !preserve {
                continue;
            }
            chance += 0.01 * looting as f32;
            if rand::random::<f32>() >= chance {
                continue;
            }
            let mut stack = stack;
            if !preserve {
                let max_damage = self
                    .survival
                    .items
                    .get(stack.item as usize)
                    .map_or(0, |i| i.max_damage);
                if max_damage > 0 {
                    let span = (max_damage - 3).max(1) as u32;
                    let inner = rand::random::<u32>() % span;
                    let back = rand::random::<u32>() % (1 + inner);
                    stack.damage = max_damage - back as i32;
                }
            }
            drops.push(stack);
        }
        // `EntityDeathEvent`: плагин меняет дроп и опыт.
        if let Some(reply) = self.sync_event(Event::MobDeath, || {
            let mut arg = format!(
                "{id} {} {} {} {} {xp}",
                m.species.short(),
                m.pos[0],
                m.pos[1],
                m.pos[2]
            );
            for s in &drops {
                arg.push('\n');
                arg.push_str(&crate::stack_nbt::to_line(*s, self.item_name(s.item)));
            }
            arg
        }) && !reply.trim().is_empty()
        {
            let mut lines = reply.lines();
            xp = lines
                .next()
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(xp);
            drops = live::parse_stacks(lines, |n| self.item_id(n));
        }
        for stack in drops {
            let vel = items::pop_velocity(rand::random(), rand::random());
            self.drop_item(center, vel, stack, items::BLOCK_DELAY);
        }
        if xp > 0 {
            self.spawn_orbs(m.pos, xp);
        }
    }

    /// Поля мобов из NBT: `/summon <entity> <pos> <nbt>` и
    /// `/data merge entity`. Повторяет `LivingEntity.readAdditionalSaveData`
    /// и `Mob.readAdditionalSaveData`: `Pos`/`Rotation` применяет вызывающий.
    /// Возвращает изменённые слоты снаряжения для `set_equipment`.
    pub(super) fn apply_mob_nbt(&self, m: &mut Mob, nbt: &Compound) -> Vec<(u8, Option<Stack>)> {
        if let Some(health) = nbt
            .get("Health")
            .and_then(crate::play::commands::snbt::number)
        {
            m.health = (health as f32).clamp(0.0, m.max_health());
        }
        if let Some(name) = nbt.get("CustomName") {
            m.custom_name = Some(custom_name_text(name));
        }
        if let Some(Tag::Byte(baby)) = nbt.get("IsBaby") {
            m.age = if *baby != 0 { mb::BABY_AGE } else { 0 };
        }
        if let Some(age) = nbt.get("Age").and_then(Tag::as_i64) {
            m.age = age as i32;
        }
        if let Some(Tag::Byte(persistent)) = nbt.get("PersistenceRequired") {
            m.persistent = *persistent != 0;
        }
        if m.species == Species::Zombie
            && let Some(Tag::Byte(breaks)) = nbt.get("CanBreakDoors")
        {
            m.can_break_doors = *breaks != 0;
        }
        // `active_effects` заменяет список целиком (`readAdditionalSaveData`).
        if let Some(Tag::List(list)) = nbt.get("active_effects") {
            m.effects.clear();
            for tag in list {
                let Tag::Compound(e) = tag else { continue };
                let Some(Tag::String(name)) = e.get("id") else {
                    continue;
                };
                let Some(&id) = self.survival.effects.get(name) else {
                    continue;
                };
                let amplifier = match e.get("amplifier") {
                    Some(Tag::Byte(v)) => *v as u8,
                    _ => 0,
                };
                let duration = match e.get("duration") {
                    Some(Tag::Int(v)) => *v,
                    _ => 0,
                };
                if duration == 0 {
                    continue;
                }
                m.add_effect(crate::survival::Effect {
                    id,
                    amplifier,
                    duration,
                    ambient: matches!(e.get("ambient"), Some(Tag::Byte(v)) if *v != 0),
                    particles: !matches!(e.get("show_particles"), Some(Tag::Byte(v)) if *v == 0),
                    show_icon: !matches!(e.get("show_icon"), Some(Tag::Byte(v)) if *v == 0),
                    age: 0,
                });
            }
        }
        let mut changed = Vec::new();
        if let Some(Tag::Compound(eq)) = nbt.get("equipment") {
            for (i, (slot, id)) in mb::EQUIPMENT_SLOTS.iter().enumerate() {
                let Some(tag) = eq.get(*slot) else { continue };
                let stack = match tag {
                    Tag::Compound(c) if c.contains_key("id") => {
                        crate::stack_nbt::from_nbt(c, |n| self.item_id(n))
                    }
                    _ => None,
                };
                m.set_item(i, stack);
                changed.push((*id, m.item(i)));
            }
        }
        if let Some(Tag::Compound(chances)) = nbt.get("drop_chances") {
            for (i, (slot, _)) in mb::EQUIPMENT_SLOTS.iter().enumerate() {
                match chances.get(*slot) {
                    Some(Tag::Float(v)) => m.drop_chances[i] = (*v).max(0.0),
                    Some(Tag::Double(v)) => m.drop_chances[i] = (*v as f32).max(0.0),
                    _ => {}
                }
            }
        }
        changed
    }

    /// Урон удара игрока: `base × (0.2 + scale²·0.8)`, крит ×1.5,
    /// бонус чар (`magicBoost`) масштабируется отдельно.
    pub(super) fn melee_damage(base: f32, bonus: f32, scale: f32, crit: bool) -> f32 {
        let scale = scale.clamp(0.0, 1.0);
        let mut damage = base * (0.2 + scale * scale * 0.8);
        if crit {
            damage *= 1.5;
        }
        damage + bonus * scale
    }

    /// `Player.doSweepAttack`: `1 + sweeping_damage_ratio × item_damage`
    /// плюс бонус чар, затем всё умножается на заряд. Sweeping Edge даёт
    /// `level / (level + 1)` (`minecraft:sweeping_edge` в данных 26.3).
    /// `MaceItem.getAttackDamageBonus` без чар: 4×падение до 3 блоков,
    /// 12 + 2×(h−3) до 8, дальше 22 + (h−8).
    pub(super) fn mace_smash_bonus(fall: f32) -> f32 {
        if fall <= 3.0 {
            4.0 * fall
        } else if fall <= 8.0 {
            12.0 + 2.0 * (fall - 3.0)
        } else {
            22.0 + fall - 8.0
        }
    }

    /// Бонус булавы с Density: `getAttackDamageBonus` плюс
    /// `smash_damage_per_fallen_block × fall` (`0.5 × уровень`).
    pub(super) fn mace_smash_damage(fall: f32, density: i32) -> f32 {
        Self::mace_smash_bonus(fall) + 0.5 * density.max(0) as f32 * fall
    }

    pub(super) fn sweep_damage(item_damage: f32, sharpness: f32, sweeping: i32, scale: f32) -> f32 {
        let ratio = if sweeping > 0 {
            f64::from(sweeping) / f64::from(sweeping + 1)
        } else {
            0.0
        };
        let base = 1.0 + ratio as f32 * item_damage + sharpness;
        base * scale.clamp(0.0, 1.0)
    }

    /// Заряд и крит удара: `Player.attack` передаёт их в урон и эффекты.
    pub(super) fn melee_hit(
        &self,
        uuid: Uuid,
        held: Slot,
        vitals: &Vitals,
        passenger: bool,
        gliding: bool,
    ) -> MeleeHit {
        let speed = 4.0
            + held
                .and_then(|s| self.survival.items.get(s.item as usize))
                .map_or(0.0, |i| i.attack_speed);
        let scale = vitals.attack_scale(speed);
        let (pos, on_ground) = self
            .hub
            .players
            .lock()
            .unwrap()
            .get(&uuid)
            .map_or(([0.0; 3], true), |p| (p.pos, p.on_ground));
        // `fallDistance > 0`: крит только на падении, не на подъёме прыжка.
        let crit = scale > 0.9
            && vitals.fall_distance() > 0.0
            && !on_ground
            && !self.standing_in_water(pos)
            && !self.on_climbable(pos)
            && self.effect_amp(vitals, "minecraft:blindness").is_none()
            && !vitals.sprinting
            && !passenger;
        MeleeHit {
            scale,
            crit,
            gliding,
        }
    }

    /// `Entity.onClimbable`: лестницы, лианы и подмостки.
    fn on_climbable(&self, feet: [f64; 3]) -> bool {
        let p = [
            feet[0].floor() as i32,
            feet[1].floor() as i32,
            feet[2].floor() as i32,
        ];
        [p, [p[0], p[1] + 1, p[2]]].iter().any(|&q| {
            self.block(q).is_some_and(|s| {
                matches!(
                    self.state_name(s),
                    "minecraft:ladder"
                        | "minecraft:vine"
                        | "minecraft:scaffolding"
                        | "minecraft:twisting_vines"
                        | "minecraft:twisting_vines_plant"
                        | "minecraft:weeping_vines"
                        | "minecraft:weeping_vines_plant"
                        | "minecraft:cave_vines"
                        | "minecraft:cave_vines_plant"
                )
            })
        })
    }

    /// Удар игрока по мобу (`Player.attack`). Возвращает слот оружия, если оно износилось.
    /// `MaceItem.smashAttack`: звук, частицы и отбрасывание живых в радиусе
    /// 3.5 блока; сила `(3.5 − d) × 0.7`, на падении > 5 — вдвое.
    fn mace_smash(&self, target: i32, user: [f64; 3], fall: f32, on_ground: bool) {
        let heavy = fall > 5.0;
        let sound = if on_ground {
            if heavy {
                "minecraft:item.mace.smash_ground_heavy"
            } else {
                "minecraft:item.mace.smash_ground"
            }
        } else {
            "minecraft:item.mace.smash_air"
        };
        self.play_sound(None, sound, crate::sounds::PLAYERS, user, 1.0, 1.0);
        let mut mobs = self.sim().mobs.lock().unwrap();
        let Some(at) = mobs.get(&target).map(|e| e.mob.pos) else {
            return;
        };
        self.hub.send(
            None,
            play::level_event(
                self.ids.level_event,
                2013,
                [
                    at[0].floor() as i32,
                    at[1].floor() as i32,
                    at[2].floor() as i32,
                ],
                750,
                false,
            ),
        );
        let multiplier = if heavy { 2.0 } else { 1.0 };
        for (&id, e) in mobs.iter_mut() {
            if id == target || !e.mob.alive() {
                continue;
            }
            let d = [
                e.mob.pos[0] - at[0],
                e.mob.pos[1] - at[1],
                e.mob.pos[2] - at[2],
            ];
            let dist = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            if dist > 3.5 || dist < 1e-4 {
                continue;
            }
            let power = (3.5 - dist) * 0.7 * multiplier;
            e.mob.vel[0] += d[0] / dist * power;
            e.mob.vel[1] += 0.7;
            e.mob.vel[2] += d[2] / dist * power;
        }
    }

    pub(super) async fn attack_mob(
        &self,
        uuid: Uuid,
        target: i32,
        inv: &mut Inventory,
        creative: bool,
        vitals: &mut Vitals,
        hit: MeleeHit,
    ) -> Option<usize> {
        let scale = hit.scale;
        let crit = hit.crit;
        let (pos, yaw, on_ground) = {
            let players = self.hub.players.lock().unwrap();
            let p = players.get(&uuid)?;
            (p.pos, p.yaw, p.on_ground)
        };
        let slot = inv.held_slot();
        let held = inv.slots[slot];
        let item = held.and_then(|s| self.survival.items.get(s.item as usize));
        let level = |n: &str| {
            held.map_or(0, |s| {
                chunkwake_protocol::components::stack_level(s.extra, n)
            })
        };
        let base = 1.0 + item.map_or(0.0, |i| i.attack);
        let sharp = level("minecraft:sharpness");
        let bonus = if sharp > 0 {
            0.5 * sharp as f32 + 0.5
        } else {
            0.0
        };
        let mut damage = Self::melee_damage(base, bonus, scale, crit);
        // `MaceItem`: бонус за падение не масштабируется зарядом, но крит
        // умножает его вместе с базовым уроном.
        let mace = held.is_some_and(|s| self.item_name(s.item) == "minecraft:mace");
        let fall = vitals.fall_distance() as f32;
        let smash = mace && fall > 1.5 && !hit.gliding;
        if smash {
            let density = level("minecraft:density");
            let bonus = Self::mace_smash_damage(fall, density);
            damage += bonus * if crit { 1.5 } else { 1.0 };
        }
        let full = scale > 0.9;
        let sprinting = vitals.sprinting;
        let sprint_knock = sprinting && full;
        let sword = held.is_some_and(|s| {
            self.survival
                .item_tags
                .get("minecraft:swords")
                .is_some_and(|set| set.contains(self.item_name(s.item)))
        });
        let sweep = full && !crit && !sprint_knock && on_ground && sword;
        let sweeping = level("minecraft:sweeping_edge");
        let species = self.sim().mobs.lock().unwrap().get(&target)?.mob.species;
        {
            let mobs = self.sim().mobs.lock().unwrap();
            let e = mobs.get(&target)?;
            // Дальше 3 блоков (+ запас на задержку) не бьём.
            let d = [
                e.mob.pos[0] - pos[0],
                e.mob.pos[1] - pos[1],
                e.mob.pos[2] - pos[2],
            ];
            if d[0] * d[0] + d[1] * d[1] + d[2] * d[2] > 36.0 {
                return None;
            }
        }
        // `Player.onAttack`: заряд сбрасывается до расчёта урона.
        vitals.reset_attack();
        match self
            .plugin_event(Event::MobDamage, uuid, || {
                format!("{target} {} {damage} {}", species.short(), u8::from(crit))
            })
            .await
            .as_deref()
            .map(str::trim)
        {
            Some("c") => return None,
            Some(r) if r.starts_with("d ") => damage = r[2..].parse().unwrap_or(damage),
            _ => {}
        }
        if damage <= 0.0 {
            return None;
        }
        // `LivingEntity.getKnockback` = (атрибут + чары) / 2; база 0.4 уже
        // применяется в `hurt`, это — добавка `causeExtraKnockback`.
        let extra =
            0.25 * level("minecraft:knockback") as f64 + if sprint_knock { 0.5 } else { 0.0 };
        let (result, mpos, species, baby, sound, target_ground) = {
            let mut mobs = self.sim().mobs.lock().unwrap();
            let e = mobs.get_mut(&target)?;
            // Обидчика монстр берёт целью сам (`HurtByTargetGoal`), творческого — нет.
            let r = mb::hurt(
                &mut e.mob,
                damage,
                mb::Source::at(pos, Some(uuid.as_u128())),
                mb::KNOCKBACK,
            );
            if r.is_some() && extra > 0.0 {
                let dx = pos[0] - e.mob.pos[0];
                let dz = pos[2] - e.mob.pos[2];
                mb::knockback(&mut e.mob, extra, dx, dz);
            }
            if r == Some(true) {
                e.killer = Some((uuid, slot as i32));
            }
            let died = r == Some(true);
            let sound = (
                e.mob.sound(if died { "death" } else { "hurt" }),
                e.mob.volume(),
            );
            (
                r,
                e.mob.pos,
                e.mob.species,
                e.mob.is_baby(),
                sound,
                e.mob.on_ground,
            )
        };
        let died = result?;
        if smash {
            self.mace_smash(target, pos, fall, target_ground);
            vitals.reset_fall(pos);
        }
        self.emit_vibration(mpos, "minecraft:entity_damage");
        if died {
            self.emit_vibration(mpos, "minecraft:entity_die");
        }
        self.hub.send_about(
            target,
            play::hurt_animation(self.ids.hurt_animation, target, yaw),
        );
        let packets = {
            let mobs = self.sim().mobs.lock().unwrap();
            mobs.get(&target).map(|e| self.mob_packets(target, &e.mob))
        };
        if let Some(data) = packets.and_then(|p| p.into_iter().nth(2)) {
            self.hub.send_about(target, data);
        }
        self.play_sound(
            None,
            &sound.0,
            sound_source(species),
            mpos,
            sound.1,
            mob_pitch(baby),
        );
        if sprint_knock {
            self.play_sound(
                None,
                "minecraft:entity.player.attack.knockback",
                crate::sounds::PLAYERS,
                pos,
                1.0,
                1.0,
            );
        }
        if crit {
            self.play_sound(
                None,
                "minecraft:entity.player.attack.crit",
                crate::sounds::PLAYERS,
                pos,
                1.0,
                1.0,
            );
            self.hub
                .send_about(target, play::animate(self.ids.animate, target, 1));
        } else if !sweep {
            self.play_sound(
                None,
                if full {
                    "minecraft:entity.player.attack.strong"
                } else {
                    "minecraft:entity.player.attack.weak"
                },
                crate::sounds::PLAYERS,
                pos,
                1.0,
                1.0,
            );
        } else {
            self.play_sound(
                None,
                "minecraft:entity.player.attack.sweep",
                crate::sounds::PLAYERS,
                pos,
                1.0,
                1.0,
            );
            self.sweep_attack(
                uuid,
                target,
                pos,
                yaw,
                SweepHit {
                    scale,
                    slot,
                    item_damage: base,
                    sharpness: bonus,
                    sweeping,
                },
            );
        }
        // `EnchantmentHelper` бонус (Острота и т.п.) — «магический крит».
        if bonus > 0.0 {
            self.hub
                .send_about(target, play::animate(self.ids.animate, target, 2));
        }
        if died {
            self.hub
                .send_about(target, inv::entity_event(self.ids.entity_event, target, 3));
        }
        if creative {
            return None;
        }
        let mut s = inv.slots[slot]?;
        if self.wear_one(&mut s) {
            inv.slots[slot] = None;
        } else {
            inv.slots[slot] = Some(s);
        }
        Some(slot)
    }

    /// Применить один эффект зелья к мобу (`AreaEffectCloud`): `None` —
    /// не instant, добавляет `active_effects`; иначе лечит/ранит.
    pub(super) fn mob_potion_effect(
        &self,
        id: i32,
        effect: survival::Effect,
        instant: survival::Instant,
        strength: f64,
    ) -> bool {
        let mut hit = None;
        {
            let mut mobs = self.sim().mobs.lock().unwrap();
            let Some(entry) = mobs.get_mut(&id) else {
                return false;
            };
            if !entry.mob.alive() {
                return false;
            }
            let undead = entry.mob.species.is_undead();
            let amount = |base: f32| crate::potions::splash_instant_amount(base, strength);
            let hurt = |entry: &mut MobEntry, amount: f32| {
                mb::hurt(&mut entry.mob, amount, mb::Source::default(), 0.0)
            };
            let mut died_now = None;
            match instant {
                survival::Instant::Heal(base) => {
                    if undead {
                        died_now = hurt(entry, amount(base));
                    } else {
                        entry.mob.health =
                            (entry.mob.health + amount(base)).min(entry.mob.max_health());
                    }
                }
                survival::Instant::Harm(base) => {
                    if undead {
                        entry.mob.health =
                            (entry.mob.health + amount(base)).min(entry.mob.max_health());
                    } else {
                        died_now = hurt(entry, amount(base));
                    }
                }
                survival::Instant::Eat(..) => return false,
                survival::Instant::None => {
                    entry.mob.add_effect(effect);
                    return true;
                }
            }
            if let Some(died) = died_now {
                hit = Some((died, entry.mob.sound(if died { "death" } else { "hurt" })));
            }
        }
        if let Some((died, sound)) = hit {
            let mobs = self.sim().mobs.lock().unwrap();
            if let Some(entry) = mobs.get(&id) {
                let pos = entry.mob.pos;
                let volume = entry.mob.volume();
                let baby = entry.mob.is_baby();
                let species = entry.mob.species;
                drop(mobs);
                self.play_sound(
                    None,
                    &sound,
                    sound_source(species),
                    pos,
                    volume,
                    mob_pitch(baby),
                );
                self.hub
                    .send_about(id, play::hurt_animation(self.ids.hurt_animation, id, 0.0));
                self.refresh_mob(id);
                self.emit_vibration(pos, "minecraft:entity_damage");
                if died {
                    self.emit_vibration(pos, "minecraft:entity_die");
                    self.hub
                        .send_about(id, inv::entity_event(self.ids.entity_event, id, 3));
                }
            }
        }
        true
    }

    /// `ThrownSplashPotion.onHitAsPotion` для мобов: те же эффекты и
    /// масштаб по расстоянию, но в `Mob.effects`.
    pub(super) fn splash_mobs(&self, effects: &[survival::Effect], pos: [f64; 3]) {
        let instant: Vec<(survival::Effect, survival::Instant)> = effects
            .iter()
            .map(|e| {
                (
                    *e,
                    survival::instant_effect(
                        self.effect_name(e.id).unwrap_or_default(),
                        e.amplifier,
                    ),
                )
            })
            .collect();
        let mut hits = Vec::new();
        {
            let mut mobs = self.sim().mobs.lock().unwrap();
            for (&id, entry) in mobs.iter_mut() {
                if !entry.mob.alive() {
                    continue;
                }
                let p = entry.mob.pos;
                let (_, height) = entry.mob.size();
                let distance = ((p[0] - pos[0]).powi(2)
                    + (p[1] + height * 0.5 - pos[1]).powi(2)
                    + (p[2] - pos[2]).powi(2))
                .sqrt();
                if distance > 4.0 {
                    continue;
                }
                let strength = crate::potions::splash_strength(distance, false);
                let undead = entry.mob.species.is_undead();
                for (effect, instant) in &instant {
                    let hurt = |entry: &mut MobEntry, amount: f32| {
                        mb::hurt(&mut entry.mob, amount, mb::Source::default(), 0.0)
                    };
                    match *instant {
                        survival::Instant::Heal(amount) => {
                            let amount = crate::potions::splash_instant_amount(amount, strength);
                            if undead {
                                if let Some(died) = hurt(entry, amount) {
                                    hits.push((
                                        id,
                                        (
                                            entry.mob.sound(if died { "death" } else { "hurt" }),
                                            entry.mob.pos,
                                            entry.mob.volume(),
                                            entry.mob.is_baby(),
                                            sound_source(entry.mob.species),
                                        ),
                                        died,
                                    ));
                                }
                            } else {
                                entry.mob.health =
                                    (entry.mob.health + amount).min(entry.mob.max_health());
                            }
                            continue;
                        }
                        survival::Instant::Harm(amount) => {
                            let amount = crate::potions::splash_instant_amount(amount, strength);
                            if undead {
                                entry.mob.health =
                                    (entry.mob.health + amount).min(entry.mob.max_health());
                            } else if let Some(died) = hurt(entry, amount) {
                                hits.push((
                                    id,
                                    (
                                        entry.mob.sound(if died { "death" } else { "hurt" }),
                                        entry.mob.pos,
                                        entry.mob.volume(),
                                        entry.mob.is_baby(),
                                        sound_source(entry.mob.species),
                                    ),
                                    died,
                                ));
                            }
                            continue;
                        }
                        survival::Instant::None => {}
                        survival::Instant::Eat(..) => continue,
                    }
                    let mut effect = *effect;
                    if effect.duration > 1 {
                        effect.duration = (effect.duration as f64 * strength).round() as i32;
                    }
                    if effect.duration < 20 {
                        continue;
                    }
                    entry.mob.add_effect(effect);
                }
            }
        }
        for (id, (name, p, volume, baby, source), died) in hits {
            self.hub
                .send_about(id, play::hurt_animation(self.ids.hurt_animation, id, 0.0));
            self.refresh_mob(id);
            self.play_sound(None, &name, source, p, volume, mob_pitch(baby));
            self.emit_vibration(p, "minecraft:entity_damage");
            if died {
                self.emit_vibration(p, "minecraft:entity_die");
                self.hub
                    .send_about(id, inv::entity_event(self.ids.entity_event, id, 3));
            }
        }
    }

    /// `Player.doSweepAttack`: `1 + sweeping_damage_ratio × база` урона всем
    /// рядом с целью, всё умножается на заряд.
    fn sweep_attack(&self, uuid: Uuid, target: i32, pos: [f64; 3], yaw: f32, sweep: SweepHit) {
        let amount = Self::sweep_damage(
            sweep.item_damage,
            sweep.sharpness,
            sweep.sweeping,
            sweep.scale,
        );
        let slot = sweep.slot;
        let mut hits = Vec::new();
        {
            let mut mobs = self.sim().mobs.lock().unwrap();
            let Some((tpos, tsize)) = mobs.get(&target).map(|e| (e.mob.pos, e.mob.size())) else {
                return;
            };
            for (&id, e) in mobs.iter_mut() {
                if id == target || !e.mob.alive() {
                    continue;
                }
                let p = e.mob.pos;
                let size = e.mob.size();
                // AABB цели, расширенный на (1.0, 0.25, 1.0).
                if (p[0] - tpos[0]).abs() > tsize.0 / 2.0 + 1.0 + size.0 / 2.0
                    || (p[2] - tpos[2]).abs() > tsize.0 / 2.0 + 1.0 + size.0 / 2.0
                    || p[1] > tpos[1] + tsize.1 + 0.25
                    || p[1] + size.1 < tpos[1] - 0.25
                {
                    continue;
                }
                let d2: f64 = (0..3).map(|i| (p[i] - pos[i]).powi(2)).sum();
                if d2 >= 9.0 {
                    continue;
                }
                // `LivingEntity.hurtServer` затем `knockback(0.4)`.
                let r = mb::hurt(
                    &mut e.mob,
                    amount,
                    mb::Source::at(pos, Some(uuid.as_u128())),
                    mb::KNOCKBACK,
                );
                if let Some(died) = r {
                    if died {
                        e.killer = Some((uuid, slot as i32));
                    }
                    hits.push((
                        id,
                        (
                            e.mob.sound(if died { "death" } else { "hurt" }),
                            p,
                            e.mob.volume(),
                            e.mob.is_baby(),
                            sound_source(e.mob.species),
                        ),
                        died,
                    ));
                }
            }
        }
        for (id, (name, p, volume, baby, source), died) in hits {
            self.hub
                .send_about(id, play::hurt_animation(self.ids.hurt_animation, id, yaw));
            self.refresh_mob(id);
            self.play_sound(None, &name, source, p, volume, mob_pitch(baby));
            self.emit_vibration(p, "minecraft:entity_damage");
            if died {
                self.emit_vibration(p, "minecraft:entity_die");
            }
        }
        if let Some(&particle) = self
            .particle_types
            .lock()
            .unwrap()
            .get("minecraft:sweep_attack")
        {
            let a = f64::from(yaw).to_radians();
            let at = [pos[0] - a.sin(), pos[1] + 0.5, pos[2] + a.cos()];
            self.hub.send(
                None,
                play::level_particles(
                    self.ids.level_particles,
                    particle,
                    false,
                    at,
                    [-(a.sin() as f32), 0.0, a.cos() as f32],
                    0.0,
                    0,
                ),
            );
        }
    }

    /// Яйцо призыва `<вид>_spawn_egg` по клику на блок: моб на грани `face`.
    pub(super) fn use_spawn_egg(&self, item: &str, block: [i32; 3], face: i32, yaw: f32) -> bool {
        let Some(species) = item
            .strip_prefix("minecraft:")
            .and_then(|n| n.strip_suffix("_spawn_egg"))
            .and_then(Species::from_name)
        else {
            return false;
        };
        let d = play::direction_offset(face).unwrap_or([0, 1, 0]);
        let at = [block[0] + d[0], block[1] + d[1], block[2] + d[2]];
        let pos = [at[0] as f64 + 0.5, at[1] as f64, at[2] as f64 + 0.5];
        self.spawn_mob(species, pos, yaw + 180.0, false, Some("SPAWNER_EGG"));
        true
    }
}

/// Категория звуков моба (`Mob.getSoundSource`): монстры — `HOSTILE`.
fn sound_source(species: Species) -> i32 {
    if species.is_monster() {
        crate::sounds::HOSTILE
    } else {
        crate::sounds::NEUTRAL
    }
}

/// Звук моба: детёныш выше на 0.5 (`LivingEntity.getVoicePitch`).
fn mob_pitch(baby: bool) -> f32 {
    let r = (rand::random::<f32>() - rand::random::<f32>()) * 0.2;
    if baby { r + 1.5 } else { r + 1.0 }
}

pub(super) const SHEEP_COLORS: [&str; 16] = [
    "white",
    "orange",
    "magenta",
    "light_blue",
    "yellow",
    "lime",
    "pink",
    "gray",
    "light_gray",
    "cyan",
    "purple",
    "blue",
    "brown",
    "green",
    "red",
    "black",
];

/// `Sheep.getRandomSheepColor`: белая 81.836%, серые/чёрная по 5%, коричневая 3%, розовая 0.164%.
/// Текст из компонента `minecraft:custom_name` (строка или `{"text": …}`).
fn tag_name(json: &str) -> Option<String> {
    match serde_json::from_str::<serde_json::Value>(json).ok()? {
        serde_json::Value::String(s) => Some(s),
        serde_json::Value::Object(o) => o
            .get("text")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        _ => None,
    }
}

fn sheep_color(r: f64) -> u8 {
    let r = r * 100.0;
    if r < 5.0 {
        15
    } else if r < 10.0 {
        7
    } else if r < 15.0 {
        8
    } else if r < 18.0 {
        12
    } else if r < 18.164 {
        6
    } else {
        0
    }
}

/// `Animal.getSpeedUpSecondsWhenFeeding`: еда ускоряет рост детёныша на 10 %.
fn feed_baby(age: i32) -> i32 {
    let secs = (-age / 20) as f32 * 0.1;
    (age + secs as i32 * 20).min(0)
}

impl PlayPlan {
    /// Правый клик по мобу (`Mob.interact` → `Animal.mobInteract`): еда, ножницы, ведро.
    /// Возвращает слоты, которые надо переслать; `None` — клик ни на что не повлиял.
    pub(super) async fn interact_mob(
        &self,
        uuid: Uuid,
        target: i32,
        hand: i32,
        inv: &mut Inventory,
        creative: bool,
    ) -> Option<Vec<usize>> {
        let slot = Self::hand_index(inv, hand)?;
        let species = self.sim().mobs.lock().unwrap().get(&target)?.mob.species;
        let hand_name = if hand == 1 { "OFF_HAND" } else { "HAND" };
        if self
            .cancelled(Event::InteractEntity, uuid, || {
                format!("{target} {} {hand_name}", species.short())
            })
            .await
        {
            return None;
        }
        let stack = inv.slots[slot]?;
        let name = self.item_name(stack.item).to_owned();
        if name == "minecraft:shears"
            && species == Species::Sheep
            && self
                .cancelled(Event::Shear, uuid, || format!("{target} {hand_name}"))
                .await
        {
            return None;
        }
        enum Act {
            Love,
            Grow,
            Shear([f64; 3], u8),
            Milk([f64; 3]),
            Dye([f64; 3]),
            Name([f64; 3]),
            Ignite([f64; 3], bool),
        }
        let act = {
            let mut mobs = self.sim().mobs.lock().unwrap();
            let m = &mut mobs.get_mut(&target)?.mob;
            if !m.alive() {
                return None;
            }
            if m.species.is_food(&name) {
                if m.is_baby() {
                    m.age = feed_baby(m.age);
                    Act::Grow
                } else if m.age == 0 && m.love == 0 {
                    m.love = 600;
                    Act::Love
                } else {
                    return None;
                }
            } else if name == "minecraft:shears"
                && m.species == Species::Sheep
                && !m.sheared
                && !m.is_baby()
            {
                m.sheared = true;
                Act::Shear(m.pos, m.color)
            } else if name == "minecraft:bucket" && m.species == Species::Cow && !m.is_baby() {
                Act::Milk(m.pos)
            } else if m.species == Species::Sheep
                && !m.sheared
                && let Some(color) = name
                    .strip_prefix("minecraft:")
                    .and_then(|n| n.strip_suffix("_dye"))
                    .and_then(|c| {
                        chunkwake_protocol::components::DYE_COLORS
                            .iter()
                            .position(|d| *d == c)
                    })
            {
                let color = color as u8;
                if color == m.color {
                    return None;
                }
                m.color = color;
                Act::Dye(m.pos)
            } else if name == "minecraft:name_tag" {
                let extra = chunkwake_protocol::components::get(stack.extra);
                let json = extra.custom_name.as_deref()?;
                let text = tag_name(json)?;
                m.custom_name = Some(text);
                m.persistent = true;
                Act::Name(m.pos)
            } else if m.species == Species::Creeper
                && matches!(
                    name.as_str(),
                    "minecraft:flint_and_steel" | "minecraft:fire_charge"
                )
            {
                // `Creeper.mobInteract`: `#creeper_igniters` поджигают.
                m.ignited = true;
                Act::Ignite(m.pos, name == "minecraft:fire_charge")
            } else {
                return None;
            }
        };
        let mut slots = vec![slot];
        match act {
            Act::Love | Act::Grow => {
                if !creative {
                    inv.consume_at(slot);
                }
                if matches!(act, Act::Love) {
                    self.hub
                        .send_about(target, inv::entity_event(self.ids.entity_event, target, 18));
                }
            }
            Act::Shear(pos, color) => {
                self.shear_drops(target, pos, color, crate::sounds::PLAYERS);
                if !creative {
                    let mut s = stack;
                    inv.slots[slot] = (!self.wear_one(&mut s)).then_some(s);
                }
            }
            Act::Ignite(pos, charge) => {
                let sound = if charge {
                    "minecraft:item.firecharge.use"
                } else {
                    "minecraft:item.flintandsteel.use"
                };
                let pitch = rand::random::<f32>() * 0.4 + 0.8;
                self.play_sound(None, sound, crate::sounds::HOSTILE, pos, 1.0, pitch);
                if !creative {
                    let mut s = stack;
                    inv.slots[slot] = if charge {
                        (s.count > 1).then(|| Stack {
                            count: s.count - 1,
                            ..s
                        })
                    } else {
                        (!self.wear_one(&mut s)).then_some(s)
                    };
                }
            }
            Act::Milk(pos) => {
                self.play_sound(
                    None,
                    "minecraft:entity.cow.milk",
                    crate::sounds::PLAYERS,
                    pos,
                    1.0,
                    1.0,
                );
                let milk = Stack::new(self.item_id("minecraft:milk_bucket")?, 1);
                // `ItemUtils.createFilledResult`: в креативе ведро остаётся, молоко — если его нет.
                if creative || stack.count > 1 {
                    if !creative {
                        inv.consume_at(slot);
                    }
                    if !(creative && inv.slots.iter().flatten().any(|s| s.item == milk.item))
                        && let Some(left) = inv.add_stack(milk, 1)
                        && let Some((feet, yaw, pitch)) = self.player_at(uuid)
                    {
                        self.throw(feet, yaw, pitch, left);
                    }
                    slots = (0..inv.slots.len()).collect();
                } else {
                    inv.slots[slot] = Some(milk);
                }
            }
            Act::Dye(pos) => {
                self.refresh_mob(target);
                self.play_sound(
                    Some(uuid),
                    "minecraft:item.dye.use",
                    crate::sounds::PLAYERS,
                    pos,
                    1.0,
                    1.0,
                );
                inv.consume_at(slot);
            }
            Act::Name(pos) => {
                self.refresh_mob(target);
                self.play_sound(
                    Some(uuid),
                    "minecraft:item.name_tag.use",
                    crate::sounds::PLAYERS,
                    pos,
                    1.0,
                    1.0,
                );
                inv.consume_at(slot);
            }
        }
        Some(slots)
    }

    /// `Sheep.shear`: звук, 1–3 шерсти своего цвета, новые метаданные.
    fn shear_drops(&self, id: i32, pos: [f64; 3], color: u8, source: i32) {
        self.play_sound(None, "minecraft:entity.sheep.shear", source, pos, 1.0, 1.0);
        let wool = format!("minecraft:{}_wool", SHEEP_COLORS[color as usize % 16]);
        if let Some(wool) = self.item_id(&wool) {
            for _ in 0..1 + rand::random::<u32>() % 3 {
                let vel = [
                    (rand::random::<f64>() - rand::random::<f64>()) * 0.1,
                    rand::random::<f64>() * 0.05,
                    (rand::random::<f64>() - rand::random::<f64>()) * 0.1,
                ];
                let at = [pos[0], pos[1] + 1.0, pos[2]];
                self.drop_item(at, vel, Stack::new(wool, 1), items::BLOCK_DELAY);
            }
        }
        self.refresh_mob(id);
    }

    /// Раздатчик с ножницами (`ShearsDispenseItemBehavior`): первая взрослая
    /// нестриженая овца в блоке `front`. `true` — постриг.
    pub(super) fn dispense_shear(&self, front: [i32; 3]) -> bool {
        let lo = front.map(f64::from);
        let hit = {
            let mut mobs = self.sim().mobs.lock().unwrap();
            mobs.iter_mut().find_map(|(&id, e)| {
                let m = &mut e.mob;
                let (w, h) = m.size();
                let inside = m.pos[0] + w / 2.0 > lo[0]
                    && m.pos[0] - w / 2.0 < lo[0] + 1.0
                    && m.pos[2] + w / 2.0 > lo[2]
                    && m.pos[2] - w / 2.0 < lo[2] + 1.0
                    && m.pos[1] + h > lo[1]
                    && m.pos[1] < lo[1] + 1.0;
                (inside && m.alive() && m.species == Species::Sheep && !m.sheared && !m.is_baby())
                    .then(|| {
                        m.sheared = true;
                        (id, m.pos, m.color)
                    })
            })
        };
        let Some((id, pos, color)) = hit else {
            return false;
        };
        self.shear_drops(id, pos, color, crate::sounds::BLOCKS);
        true
    }

    /// Раздатчик с яйцом призыва: моб в центре блока перед ним.
    pub(super) fn dispense_egg(&self, item: &str, front: [i32; 3], yaw: f32) -> bool {
        let Some(species) = item
            .strip_prefix("minecraft:")
            .and_then(|n| n.strip_suffix("_spawn_egg"))
            .and_then(Species::from_name)
        else {
            return false;
        };
        let pos = [
            front[0] as f64 + 0.5,
            front[1] as f64,
            front[2] as f64 + 0.5,
        ];
        self.spawn_mob(species, pos, yaw, false, Some("DISPENSE_EGG"));
        true
    }

    /// Разослать метаданные моба заново (стрижка, рост).
    fn refresh_mob(&self, id: i32) {
        let data = {
            let mobs = self.sim().mobs.lock().unwrap();
            mobs.get(&id).map(|e| self.mob_packets(id, &e.mob))
        };
        if let Some(d) = data.and_then(|p| p.into_iter().nth(2)) {
            self.hub.send_about(id, d);
        }
    }

    /// Поведение животных поверх ИИ: яйца куриц и взросление детёнышей.
    pub(super) fn tick_animals(&self) {
        let mut eggs = Vec::new();
        let mut grown = Vec::new();
        let simulated = self.simulated_set();
        {
            let mut mobs = self.sim().mobs.lock().unwrap();
            for (&id, e) in mobs.iter_mut() {
                let m = &mut e.mob;
                if !m.alive() {
                    continue;
                }
                // Вне симуляции животное не стареет и не несётся.
                if !simulated.is_empty()
                    && !simulated.contains(&(
                        (m.pos[0].floor() as i32) >> 4,
                        (m.pos[2].floor() as i32) >> 4,
                    ))
                {
                    continue;
                }
                if m.age == -1 {
                    grown.push(id);
                }
                // `Chicken.aiStep`: яйцо раз в 6000–12000 тиков.
                if m.species == Species::Chicken && !m.is_baby() {
                    m.egg_time -= 1;
                    if m.egg_time <= 0 {
                        m.egg_time = 6000 + (rand::random::<u32>() % 6000) as i32;
                        eggs.push(m.pos);
                    }
                }
            }
        }
        for pos in eggs {
            self.play_sound(
                None,
                "minecraft:entity.chicken.egg",
                crate::sounds::NEUTRAL,
                pos,
                1.0,
                mob_pitch(false),
            );
            if let Some(egg) = self.item_id("minecraft:egg") {
                self.drop_item(pos, [0.0; 3], Stack::new(egg, 1), items::BLOCK_DELAY);
            }
        }
        for id in grown {
            self.refresh_mob(id);
        }
    }

    /// `BreakDoorGoal`: дверь сломана — обе половины, `block_update` зрителям,
    /// звук удара (`levelEvent 1021`) и частицы разрушения (`2001`).
    fn break_mob_door(&self, pos: [i32; 3]) {
        let Some(state) = self.block(pos) else {
            return;
        };
        let name = self.state_name(state).to_owned();
        if !name.ends_with("_door") {
            return;
        }
        let Some(offset) = self.survival.placement.partner(state) else {
            return;
        };
        let other = [pos[0] + offset[0], pos[1] + offset[1], pos[2] + offset[2]];
        let Some(air) = self.survival.block_defaults.get("minecraft:air").copied() else {
            return;
        };
        let mut changed = vec![(pos, state)];
        if let Some(s) = self.block(other)
            && self.survival.placement.name(s) == Some(name.as_str())
        {
            changed.push((other, s));
        }
        let _edit = self.world_edit.lock().unwrap();
        for (p, _) in changed {
            self.set_block(p, air);
            self.hub.send(
                None,
                play::block_update(self.ids.block_update, p, air as i32),
            );
        }
        // Ваниль: `levelEvent 2001` только в точке `removeBlock` (верхняя
        // половина), вторая убирается `updateShape`.
        self.hub.send(
            None,
            play::level_event(self.ids.level_event, 2001, pos, state as i32, false),
        );
        self.hub.send(
            None,
            play::level_event(self.ids.level_event, 1021, pos, 0, false),
        );
    }

    /// События целей за тик ([`mb::goals::Out`]): пары, еда овцы, зов на обидчика.
    fn apply_mob_outs(&self, outs: Vec<(i32, mb::goals::Out)>) {
        use mb::goals::Out;
        let mut births = Vec::new();
        for (id, out) in outs {
            match out {
                // `Animal.spawnChildFromBreeding`: оба на 5 минут без любви,
                // детёныш на месте того, у кого сработала цель.
                Out::Breed(partner) => {
                    let mut mobs = self.sim().mobs.lock().unwrap();
                    let ready =
                        |e: Option<&MobEntry>| e.is_some_and(|e| e.mob.alive() && e.mob.love > 0);
                    if !ready(mobs.get(&id)) || !ready(mobs.get(&partner)) {
                        continue;
                    }
                    let mut at = None;
                    for k in [id, partner] {
                        if let Some(e) = mobs.get_mut(&k) {
                            e.mob.love = 0;
                            e.mob.age = 6000;
                            at.get_or_insert((e.mob.species, e.mob.pos));
                        }
                    }
                    if let Some((species, pos)) = at {
                        births.push((species, pos, id, partner));
                    }
                }
                // `Creeper.tick`: шипение и игровое событие `prime_fuse`.
                Out::Primed => {
                    let pos = self.sim().mobs.lock().unwrap().get(&id).map(|e| e.mob.pos);
                    if let Some(pos) = pos {
                        self.play_sound(
                            None,
                            "minecraft:entity.creeper.primed",
                            crate::sounds::HOSTILE,
                            pos,
                            1.0,
                            0.5,
                        );
                        self.emit_vibration(pos, "minecraft:prime_fuse");
                    }
                }
                // Взрыв разобран в тике: моб уже убран.
                Out::Explode(_) => {}
                // `CubeMobMoveControl.tick` и `Slime.tick`: прыжок (`getSoundPitch`:
                // ×1.4 у маленького, ×0.8 у большого) и шлепок при приземлении.
                Out::CubeSound { jump } => {
                    let mob = self
                        .sim()
                        .mobs
                        .lock()
                        .unwrap()
                        .get(&id)
                        .map(|e| (e.mob.pos, e.mob.cube_size, e.mob.volume()));
                    let Some((pos, size, volume)) = mob else {
                        continue;
                    };
                    let small = if size <= 1 { "_small" } else { "" };
                    let r = (rand::random::<f32>() - rand::random::<f32>()) * 0.2 + 1.0;
                    let name = if jump {
                        format!("minecraft:entity.slime.jump{small}")
                    } else {
                        format!("minecraft:entity.slime.squish{small}")
                    };
                    let pitch = mb::cube::sound_pitch(jump, size, r);
                    self.play_sound(None, &name, crate::sounds::HOSTILE, pos, volume, pitch);
                }
                Out::Shoot { target, power } => self.mob_shoot(id, Uuid::from_u128(target), power),
                // `EatBlockGoal.start`: анимация опущенной головы.
                Out::EatStart => self
                    .hub
                    .send_about(id, inv::entity_event(self.ids.entity_event, id, 10)),
                // `EatBlockGoal.tick`: трава ломается, дёрн становится землёй
                // (если `mob_griefing`); овца ест в любом случае (`Sheep.ate`).
                Out::Eat(pos, plant) => {
                    if self.vars.flag("mob_griefing") {
                        let to = if plant {
                            self.survival.block_defaults.get("minecraft:air").copied()
                        } else if self.is_grass(pos) {
                            self.survival.block_defaults.get("minecraft:dirt").copied()
                        } else {
                            None
                        };
                        if let Some(state) = to {
                            let _edit = self.world_edit.lock().unwrap();
                            self.set_block(pos, state);
                        }
                    }
                    if let Some(e) = self.sim().mobs.lock().unwrap().get_mut(&id) {
                        e.mob.sheared = false;
                        if e.mob.is_baby() {
                            e.mob.age = (e.mob.age + 1200).min(0);
                        }
                    }
                    self.refresh_mob(id);
                }
                // `BreakDoorGoal.tick`: звук удара по деревянной двери и взмах.
                Out::DoorHit(pos) => {
                    self.hub.send(
                        None,
                        play::level_event(self.ids.level_event, 1019, pos, 0, false),
                    );
                    self.hub.send_about(
                        id,
                        play::swing_animation(self.ids.swing_animation, id, 0, 1, 6),
                    );
                }
                // `Level.destroyBlockProgress`: сетка ломания у зрителей.
                Out::DoorBreakProgress { pos, progress } => {
                    self.hub.send(
                        None,
                        play::block_destruction(
                            self.ids.block_destruction,
                            id,
                            pos,
                            progress as i8,
                        ),
                    );
                }
                Out::DoorBroken(pos) => self.break_mob_door(pos),
                // `HurtByTargetGoal.alertOthers`: свои того же вида рядом без цели.
                Out::Alert(attacker) => {
                    let mut mobs = self.sim().mobs.lock().unwrap();
                    let Some((species, pos)) = mobs.get(&id).map(|e| (e.mob.species, e.mob.pos))
                    else {
                        continue;
                    };
                    let r = species.follow_range();
                    for (&other, e) in mobs.iter_mut() {
                        let m = &mut e.mob;
                        if other != id
                            && m.species == species
                            && m.alive()
                            && m.target.is_none()
                            && (m.pos[0] - pos[0]).abs() < r + 1.0
                            && (m.pos[1] - pos[1]).abs() < 11.0
                            && (m.pos[2] - pos[2]).abs() < r + 1.0
                        {
                            m.target = Some(attacker);
                        }
                    }
                }
            }
        }
        for (species, pos, mother, father) in births {
            let Some(child) = self.spawn_mob(species, pos, 0.0, true, Some("BREEDING")) else {
                continue;
            };
            let mut xp = 1 + (rand::random::<u32>() % 7) as i32;
            let reply = self.sync_event(Event::Breed, || format!("{child} {mother} {father} {xp}"));
            match reply.as_deref().map(str::trim) {
                Some("c") => {
                    self.sim().mobs.lock().unwrap().remove(&child);
                    self.hub.remove_tracked(&[child]);
                    continue;
                }
                Some(v) => xp = v.parse().unwrap_or(xp),
                None => {}
            }
            if xp > 0 {
                self.spawn_orbs(pos, xp);
            }
        }
    }
}

/// `creature` у равнин — для плоского мира без климата.
const PLAINS_CREATURES: [(Species, u32, u32); 4] = [
    (Species::Sheep, 12, 4),
    (Species::Pig, 10, 4),
    (Species::Chicken, 10, 4),
    (Species::Cow, 8, 4),
];

/// `creature_world_gen_spawn_probability` по умолчанию.
const CREATURE_GEN_CHANCE: f32 = 0.1;

fn list_f64(t: Option<&Tag>) -> Vec<f64> {
    match t {
        Some(Tag::List(l)) => l
            .iter()
            .filter_map(|v| match v {
                Tag::Double(d) => Some(*d),
                Tag::Float(f) => Some(*f as f64),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Моб в NBT сущности (`Entity.saveWithoutId` + `Animal`/`Sheep`/`Chicken`).
/// `equipment` (26.3): слот -> `ItemStack` только для непустых.
pub(super) fn equipment_to_nbt(m: &Mob, item_names: &[String]) -> Option<Tag> {
    let mut map = Compound::new();
    for (i, (slot, _)) in mb::EQUIPMENT_SLOTS.iter().enumerate() {
        let Some(s) = m.item(i) else { continue };
        let name = item_names
            .get(s.item as usize)
            .filter(|n| !n.is_empty())
            .map_or("minecraft:air", String::as_str);
        if let Some(c) = crate::stack_nbt::to_nbt(s, name) {
            map.insert((*slot).to_owned(), Tag::Compound(c));
        }
    }
    (!map.is_empty()).then_some(Tag::Compound(map))
}

/// `drop_chances`: только отличия от 0.085 (`DropChances.filterDefaultValues`).
pub(super) fn drop_chances_to_nbt(m: &Mob) -> Option<Tag> {
    let mut map = Compound::new();
    for (i, (slot, _)) in mb::EQUIPMENT_SLOTS.iter().enumerate() {
        let chance = m.drop_chance(i);
        if chance != mb::DEFAULT_DROP_CHANCE {
            map.insert((*slot).to_owned(), Tag::Float(chance));
        }
    }
    (!map.is_empty()).then_some(Tag::Compound(map))
}

/// Чтение `equipment`/`drop_chances` (26.3) с запасным старым
/// `HandItems`/`ArmorItems`/`DropChances`/`*DropChances`.
pub(super) fn equipment_from_nbt(
    m: &mut Mob,
    c: &Compound,
    items: &std::collections::HashMap<String, i32>,
) {
    let resolve = |n: &str| -> Option<i32> { items.get(n).copied() };
    let read_stack = |tag: Option<&Tag>| -> Option<Stack> {
        match tag {
            Some(Tag::Compound(item)) if item.contains_key("id") => {
                crate::stack_nbt::from_nbt(item, resolve)
            }
            _ => None,
        }
    };
    if let Some(Tag::Compound(eq)) = c.get("equipment") {
        for (i, (slot, _)) in mb::EQUIPMENT_SLOTS.iter().enumerate() {
            if let Some(tag) = eq.get(*slot) {
                m.set_item(i, read_stack(Some(tag)));
            }
        }
    } else {
        let mut legacy = |key: &str, order: &[usize]| {
            let Some(Tag::List(list)) = c.get(key) else {
                return;
            };
            for (n, &i) in order.iter().enumerate() {
                m.set_item(i, read_stack(list.get(n)));
            }
        };
        legacy("HandItems", &[0, 1]);
        legacy("ArmorItems", &[2, 3, 4, 5]);
    }
    if let Some(Tag::Compound(chances)) = c.get("drop_chances") {
        for (i, (slot, _)) in mb::EQUIPMENT_SLOTS.iter().enumerate() {
            match chances.get(*slot) {
                Some(Tag::Float(v)) => m.drop_chances[i] = (*v).max(0.0),
                Some(Tag::Double(v)) => m.drop_chances[i] = (*v as f32).max(0.0),
                _ => {}
            }
        }
    } else {
        let mut legacy = |key: &str, order: &[usize]| {
            let Some(Tag::List(list)) = c.get(key) else {
                return;
            };
            for (n, &i) in order.iter().enumerate() {
                match list.get(n) {
                    Some(Tag::Float(v)) => m.drop_chances[i] = (*v).max(0.0),
                    Some(Tag::Double(v)) => m.drop_chances[i] = (*v as f32).max(0.0),
                    _ => {}
                }
            }
        };
        legacy("HandDropChances", &[0, 1]);
        legacy("ArmorDropChances", &[2, 3, 4, 5]);
        legacy("DropChances", &[0, 1, 2, 3, 4, 5]);
    }
}

/// `CustomName` из NBT: строка — литерал, у компонента берём `text`.
fn custom_name_text(tag: &Tag) -> String {
    match tag {
        Tag::String(s) => s.clone(),
        Tag::Compound(c) => match c.get("text") {
            Some(Tag::String(s)) => s.clone(),
            _ => crate::stack_nbt::tag_to_json(tag).to_string(),
        },
        _ => crate::stack_nbt::tag_to_json(tag).to_string(),
    }
}

pub(super) fn mob_to_nbt(
    m: &Mob,
    effects: &std::collections::HashMap<String, i32>,
    item_names: &[String],
) -> Compound {
    let mut c = Compound::new();
    let u = m.uuid;
    c.insert("id".into(), Tag::String(m.species.name().to_owned()));
    c.insert(
        "Pos".into(),
        Tag::List(m.pos.iter().map(|v| Tag::Double(*v)).collect()),
    );
    c.insert(
        "Motion".into(),
        Tag::List(m.vel.iter().map(|v| Tag::Double(*v)).collect()),
    );
    c.insert(
        "Rotation".into(),
        Tag::List(vec![Tag::Float(m.yaw), Tag::Float(m.pitch)]),
    );
    c.insert(
        "UUID".into(),
        Tag::IntArray(
            (0..4)
                .rev()
                .map(|i| (u >> (32 * i)) as u32 as i32)
                .collect(),
        ),
    );
    c.insert("Health".into(), Tag::Float(m.health));
    c.insert("OnGround".into(), Tag::Byte(i8::from(m.on_ground)));
    c.insert("Age".into(), Tag::Int(m.age));
    c.insert("InLove".into(), Tag::Int(m.love));
    if let Some(name) = &m.custom_name {
        c.insert("CustomName".into(), Tag::String(name.clone()));
    }
    c.insert(
        "PersistenceRequired".into(),
        Tag::Byte(i8::from(m.persistent)),
    );
    match m.species {
        Species::Zombie => {
            c.insert("IsBaby".into(), Tag::Byte(i8::from(m.is_baby())));
            c.insert(
                "CanBreakDoors".into(),
                Tag::Byte(i8::from(m.can_break_doors)),
            );
        }
        Species::Sheep => {
            c.insert("Sheared".into(), Tag::Byte(i8::from(m.sheared)));
            c.insert("Color".into(), Tag::Byte(m.color as i8));
        }
        Species::Chicken => {
            c.insert("EggLayTime".into(), Tag::Int(m.egg_time));
        }
        // `AbstractCubeMob.addAdditionalSaveData`.
        Species::Slime => {
            c.insert("Size".into(), Tag::Int(m.cube_size - 1));
            c.insert("wasOnGround".into(), Tag::Byte(i8::from(m.was_on_ground)));
        }
        // `Creeper.addAdditionalSaveData`.
        Species::Creeper => {
            c.insert("powered".into(), Tag::Byte(i8::from(m.powered)));
            c.insert("Fuse".into(), Tag::Short(m.max_swell as i16));
            c.insert(
                "ExplosionRadius".into(),
                Tag::Byte(m.explosion_radius as i8),
            );
            c.insert("ignited".into(), Tag::Byte(i8::from(m.ignited)));
        }
        _ => {}
    }
    if !m.effects.is_empty() {
        let list = m
            .effects
            .iter()
            .filter_map(|e| {
                let name = effects.iter().find(|(_, id)| **id == e.id)?.0.clone();
                let mut c = Compound::new();
                c.insert("id".into(), Tag::String(name));
                c.insert("amplifier".into(), Tag::Byte(e.amplifier as i8));
                c.insert("duration".into(), Tag::Int(e.duration));
                c.insert("ambient".into(), Tag::Byte(i8::from(e.ambient)));
                c.insert("show_particles".into(), Tag::Byte(i8::from(e.particles)));
                c.insert("show_icon".into(), Tag::Byte(i8::from(e.show_icon)));
                Some(Tag::Compound(c))
            })
            .collect();
        c.insert("active_effects".into(), Tag::List(list));
    }
    if let Some(tag) = equipment_to_nbt(m, item_names) {
        c.insert("equipment".into(), tag);
    }
    if let Some(tag) = drop_chances_to_nbt(m) {
        c.insert("drop_chances".into(), tag);
    }
    c
}

/// Обратно из NBT; чужие виды — `None`.
pub(super) fn mob_from_nbt(
    c: &Compound,
    effects: &std::collections::HashMap<String, i32>,
    items: &std::collections::HashMap<String, i32>,
) -> Option<Mob> {
    let Some(Tag::String(id)) = c.get("id") else {
        return None;
    };
    let species = Species::from_name(id)?;
    let pos: [f64; 3] = list_f64(c.get("Pos")).try_into().ok()?;
    let uuid = match c.get("UUID") {
        Some(Tag::IntArray(a)) if a.len() == 4 => {
            a.iter().fold(0u128, |acc, &v| acc << 32 | v as u32 as u128)
        }
        _ => rand::random(),
    };
    let mut m = Mob::new(species, pos, uuid);
    if let Ok(v) = <[f64; 3]>::try_from(list_f64(c.get("Motion"))) {
        m.vel = v;
    }
    if let [yaw, pitch] = list_f64(c.get("Rotation"))[..] {
        (m.yaw, m.head_yaw, m.pitch) = (yaw as f32, yaw as f32, pitch as f32);
    }
    let int = |k: &str| match c.get(k) {
        Some(Tag::Int(v)) => Some(*v),
        _ => None,
    };
    let byte = |k: &str| match c.get(k) {
        Some(Tag::Byte(v)) => Some(*v),
        _ => None,
    };
    if species == Species::Slime {
        // `readAdditionalSaveData`: `setSize(Size + 1, false)`.
        m.set_size(int("Size").unwrap_or(0) + 1, false);
        m.was_on_ground = byte("wasOnGround").is_some_and(|b| b != 0);
    }
    if let Some(Tag::Float(h)) = c.get("Health") {
        m.health = h.clamp(0.0, m.max_health());
    }
    m.on_ground = byte("OnGround").is_some_and(|b| b != 0);
    m.age = int("Age").unwrap_or(0);
    m.love = int("InLove").unwrap_or(0);
    m.sheared = byte("Sheared").is_some_and(|b| b != 0);
    m.color = byte("Color").unwrap_or(0) as u8 & 15;
    if let Some(t) = int("EggLayTime") {
        m.egg_time = t;
    }
    m.persistent = byte("PersistenceRequired").is_some_and(|b| b != 0);
    // Зомби: детёныш навсегда (`IsBaby`), возраста у него нет.
    if species == Species::Zombie {
        m.age = if byte("IsBaby").is_some_and(|b| b != 0) {
            mb::BABY_AGE
        } else {
            0
        };
        m.love = 0;
        m.can_break_doors = byte("CanBreakDoors").is_some_and(|b| b != 0);
    }
    if species == Species::Creeper {
        m.powered = byte("powered").is_some_and(|b| b != 0);
        if let Some(Tag::Short(f)) = c.get("Fuse") {
            m.max_swell = i32::from(*f);
        }
        if let Some(r) = byte("ExplosionRadius") {
            m.explosion_radius = i32::from(r);
        }
        m.ignited = byte("ignited").is_some_and(|b| b != 0);
    }
    if let Some(Tag::String(n)) = c.get("CustomName") {
        m.custom_name = Some(n.clone());
    }
    if let Some(Tag::List(list)) = c.get("active_effects") {
        for tag in list {
            let Tag::Compound(e) = tag else { continue };
            let Some(Tag::String(name)) = e.get("id") else {
                continue;
            };
            let Some(&id) = effects.get(name) else {
                continue;
            };
            let amp = match e.get("amplifier") {
                Some(Tag::Byte(v)) => *v as u8,
                _ => 0,
            };
            let duration = match e.get("duration") {
                Some(Tag::Int(v)) => *v,
                _ => 0,
            };
            if duration == 0 {
                continue;
            }
            m.add_effect(crate::survival::Effect {
                id,
                amplifier: amp,
                duration,
                ambient: matches!(e.get("ambient"), Some(Tag::Byte(v)) if *v != 0),
                particles: !matches!(e.get("show_particles"), Some(Tag::Byte(v)) if *v == 0),
                show_icon: !matches!(e.get("show_icon"), Some(Tag::Byte(v)) if *v == 0),
                age: 0,
            });
        }
    }
    equipment_from_nbt(&mut m, c, items);
    (m.health > 0.0).then_some(m)
}

impl PlayPlan {
    /// Мобы региона из `entities/*.mca`: чанки с записью уже заселены.
    pub(super) fn load_mobs(&self, chunks: Vec<((i32, i32), Compound)>) {
        let mut populated = self.sim().populated.lock().unwrap();
        let mut written = self.sim().entity_written.lock().unwrap();
        for (pos, root) in chunks {
            populated.insert(pos);
            let Some(Tag::List(list)) = root.get("Entities") else {
                continue;
            };
            for e in list {
                let Tag::Compound(c) = e else { continue };
                if self.load_boat(c)
                    || self.load_minecart(c)
                    || self.load_item_frame(c)
                    || self.load_painting(c)
                    || self.load_armor_stand(c)
                {
                    written.insert(pos);
                    continue;
                }
                let Some(m) = mob_from_nbt(c, &self.survival.effects, &self.survival.item_ids)
                else {
                    continue;
                };
                written.insert(pos);
                self.sim()
                    .mobs_loaded
                    .lock()
                    .unwrap()
                    .push(MobEntry::new(m));
            }
        }
    }

    /// Заселяет новый чанк (`NaturalSpawner.spawnMobsForChunkGeneration`), один раз.
    pub(super) fn populate_chunk(&self, cx: i32, cz: i32) {
        if self.sim().populated.lock().unwrap().contains(&(cx, cz)) {
            return;
        }
        // Регион с диска помечает свои чанки заселёнными.
        self.ensure_region(cx, cz);
        // Рельефа ещё нет: не сажать мобов на плоский шаблон (они оказывались
        // в океане, когда настоящий чанк приходил позже).
        if self.terrain().is_some()
            && !self
                .dim_state()
                .edited
                .read()
                .unwrap()
                .contains_key(&(cx, cz))
        {
            return;
        }
        if !self.sim().populated.lock().unwrap().insert((cx, cz)) {
            return;
        }
        self.sim().entity_new.lock().unwrap().insert((cx, cz));
        // Список `creature` биома в центре чанка; океан и реки — пустые.
        // Неизвестные виды остаются в весах, их группа просто пропускается.
        let (chance, list): (f32, Vec<SpawnEntry>) = match self
            .chunk_biome(cx, cz)
            .and_then(|b| self.survival.creature_spawns.get(b as usize))
        {
            Some(b) => (
                b.chance,
                b.entries
                    .iter()
                    .map(|(n, w, lo, hi)| (Species::from_name(n), *w, *lo, *hi))
                    .collect(),
            ),
            None if self.survival.creature_spawns.is_empty() => (
                CREATURE_GEN_CHANCE,
                PLAINS_CREATURES
                    .iter()
                    .map(|&(s, w, c)| (Some(s), w, c, c))
                    .collect(),
            ),
            None => return,
        };
        let total: u32 = list.iter().map(|c| c.1).sum();
        if total == 0 {
            return;
        }
        let mut rng = rand::thread_rng();
        use rand::Rng;
        while rng.r#gen::<f32>() < chance {
            let mut pick = rng.gen_range(0..total);
            let &(species, _, lo, hi) = list
                .iter()
                .find(|c| {
                    let hit = pick < c.1;
                    pick = pick.saturating_sub(c.1);
                    hit
                })
                .unwrap_or(&list[0]);
            let count = rng.gen_range(lo..=hi.max(lo));
            let Some(species) = species else { continue };
            let (mut x, mut z) = (
                cx * 16 + rng.gen_range(0..16),
                cz * 16 + rng.gen_range(0..16),
            );
            let (x0, z0) = (x, z);
            for _ in 0..count {
                // Разброс группы ±5 блоков внутри чанка.
                for _ in 0..4 {
                    let Some(y) = self.top_solid(x, z) else { break };
                    if self.is_grass([x, y, z]) {
                        let pos = [x as f64 + 0.5, (y + 1) as f64, z as f64 + 0.5];
                        self.spawn_mob(
                            species,
                            pos,
                            rng.r#gen::<f32>() * 360.0,
                            false,
                            Some("CHUNK_GEN"),
                        );
                        break;
                    }
                    x = (x + rng.gen_range(0..5) - rng.gen_range(0..5))
                        .clamp(cx * 16, cx * 16 + 15);
                    z = (z + rng.gen_range(0..5) - rng.gen_range(0..5))
                        .clamp(cz * 16, cz * 16 + 15);
                }
                x = (x0 + rng.gen_range(-5..=5)).clamp(cx * 16, cx * 16 + 15);
                z = (z0 + rng.gen_range(-5..=5)).clamp(cz * 16, cz * 16 + 15);
            }
        }
    }

    /// Биом (сетевой ID) в центре чанка на высоте поверхности.
    fn chunk_biome(&self, cx: i32, cz: i32) -> Option<u32> {
        let y = self.top_solid(cx * 16 + 8, cz * 16 + 8)?;
        let world = self.dim_state().edited.read().unwrap();
        let column = world.get(&(cx, cz)).map_or(self.template(), |e| &e.0);
        let ry = ((y - self.min_y()).max(0) as usize).min(column.sections.len() * 16 - 1);
        Some(column.sections[ry / 16].biome_at(2, (ry % 16) / 4, 2))
    }

    /// `animals_spawnable_on` / `EatBlockGoal`: блок травы (любое состояние).
    fn is_grass(&self, pos: [i32; 3]) -> bool {
        self.block(pos)
            .is_some_and(|s| chunkwake_world::has_flag(s, chunkwake_world::flags::GRASS))
    }

    /// Верхний непрозрачный для спавна блок столбца. Один проход по столбцу
    /// под одним замком: `block()` на каждый из 384 блоков брал мьютекс и
    /// пересчитывал ключ, а при заселении чанка это делается на каждую попытку.
    fn top_solid(&self, x: i32, z: i32) -> Option<i32> {
        let world = self.dim_state().edited.read().unwrap();
        let key = (x >> 4, z >> 4);
        let template = self.template();
        let column = world.get(&key).map_or(template, |e| &e.0);
        let sky = |s: u32| {
            !matches!(
                self.state_name(s),
                "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air" | ""
            )
        };
        let (lx, lz) = ((x & 15) as usize, (z & 15) as usize);
        let height = template.height();
        (0..height)
            .rev()
            .find(|&ry| sky(column.block(lx, ry, lz)))
            .map(|ry| ry as i32 + self.min_y())
    }

    /// Чанки `entities/*.mca` измерения `dim` к записи. Измерение задаётся явно:
    /// поток сохранения не знает «текущего» мира, а общее активное измерение
    /// меняется при каждом переходе игрока — сущности Ада уходили в файлы
    /// оверворлда и затирали его списки.
    pub fn unsaved_entities_in(&self, dim: Dimension) -> Vec<((i32, i32), Vec<Compound>)> {
        with_dim(dim, || self.unsaved_entities())
    }

    /// После записи сущностей измерения `dim`.
    pub fn mark_entities_saved_in(&self, dim: Dimension, saved: &[((i32, i32), Vec<Compound>)]) {
        with_dim(dim, || self.mark_entities_saved(saved));
    }

    /// Чанки `entities/*.mca` к записи: где есть мобы, где они были и новые заселённые.
    pub fn unsaved_entities(&self) -> Vec<((i32, i32), Vec<Compound>)> {
        let mut by_chunk: HashMap<(i32, i32), Vec<Compound>> = HashMap::new();
        for e in self
            .sim()
            .mobs
            .lock()
            .unwrap()
            .values()
            .filter(|e| e.mob.alive())
        {
            let pos = (
                (e.mob.pos[0].floor() as i32) >> 4,
                (e.mob.pos[2].floor() as i32) >> 4,
            );
            by_chunk.entry(pos).or_default().push(mob_to_nbt(
                &e.mob,
                &self.survival.effects,
                &self.item_names,
            ));
        }
        for (pos, c) in self
            .boats_nbt()
            .into_iter()
            .chain(self.minecarts_nbt())
            .chain(self.item_frames_nbt())
            .chain(self.paintings_nbt())
            .chain(self.armor_stands_nbt())
        {
            by_chunk.entry(pos).or_default().push(c);
        }
        for &pos in self.sim().entity_written.lock().unwrap().iter() {
            by_chunk.entry(pos).or_default();
        }
        for &pos in self.sim().entity_new.lock().unwrap().iter() {
            by_chunk.entry(pos).or_default();
        }
        by_chunk.into_iter().collect()
    }

    /// После записи: запомнить, где теперь лежат мобы.
    pub fn mark_entities_saved(&self, saved: &[((i32, i32), Vec<Compound>)]) {
        let mut written = self.sim().entity_written.lock().unwrap();
        written.clear();
        written.extend(saved.iter().filter(|(_, l)| !l.is_empty()).map(|(p, _)| *p));
        let mut new = self.sim().entity_new.lock().unwrap();
        for (p, _) in saved {
            new.remove(p);
        }
    }
}

impl PlayPlan {
    /// Запросы плагинов к мобам:
    /// `spawn вид x y z yaw` → id; `get id` → строка состояния (пусто — моба нет);
    /// `list` → строки `id вид x y z`; `set id поле значение…` → `1`/`0`.
    pub(super) fn mob_request(&self, line: &str) -> String {
        let f: Vec<&str> = line.split(' ').collect();
        let num = |i: usize| f.get(i).and_then(|v| v.parse::<f64>().ok());
        match f.first().copied() {
            Some("spawn") => {
                let (Some(species), Some(x), Some(y), Some(z)) = (
                    f.get(1).and_then(|n| Species::from_name(n)),
                    num(2),
                    num(3),
                    num(4),
                ) else {
                    return String::new();
                };
                let yaw = num(5).unwrap_or(0.0) as f32;
                self.spawn_mob(species, [x, y, z], yaw, false, None)
                    .map_or(String::new(), |id| id.to_string())
            }
            // `list после`: мобы с ID больше `после` по возрастанию, кусок до 7 КБ.
            Some("list") => {
                let after = f
                    .get(1)
                    .and_then(|v| v.parse::<i32>().ok())
                    .unwrap_or(i32::MIN);
                let mobs = self.sim().mobs.lock().unwrap();
                let mut ids: Vec<i32> = mobs
                    .iter()
                    .filter(|(id, e)| **id > after && e.mob.alive())
                    .map(|(id, _)| *id)
                    .collect();
                ids.sort_unstable();
                let mut out = String::new();
                for id in ids {
                    let e = &mobs[&id];
                    let [x, y, z] = e.mob.pos;
                    let line = format!("{id} {} {x} {y} {z}\n", e.mob.species.short());
                    if out.len() + line.len() > 7000 {
                        break;
                    }
                    out.push_str(&line);
                }
                out
            }
            Some("get") => {
                let Some(id) = f.get(1).and_then(|v| v.parse::<i32>().ok()) else {
                    return String::new();
                };
                let mobs = self.sim().mobs.lock().unwrap();
                let Some(e) = mobs.get(&id) else {
                    return String::new();
                };
                let m = &e.mob;
                let [x, y, z] = m.pos;
                // вид uuid x y z yaw pitch здоровье макс возраст стрижена цвет мёртв любовь размер\nимя
                format!(
                    "{} {} {x} {y} {z} {} {} {} {} {} {} {} {} {} {}\n{}",
                    m.species.short(),
                    Uuid::from_u128(m.uuid),
                    m.yaw,
                    m.pitch,
                    m.health,
                    m.max_health(),
                    m.age,
                    u8::from(m.sheared),
                    m.color,
                    u8::from(!m.alive()),
                    m.love,
                    m.cube_size,
                    m.custom_name.as_deref().unwrap_or("")
                )
            }
            Some("set") => {
                let Some(id) = f.get(1).and_then(|v| v.parse::<i32>().ok()) else {
                    return "0".into();
                };
                if f.get(2) == Some(&"remove") {
                    let gone = self.sim().mobs.lock().unwrap().remove(&id).is_some();
                    if gone {
                        self.hub.remove_tracked(&[id]);
                    }
                    return if gone { "1" } else { "0" }.into();
                }
                let mut moved = None;
                let mut died = false;
                {
                    let mut mobs = self.sim().mobs.lock().unwrap();
                    let Some(e) = mobs.get_mut(&id) else {
                        return "0".into();
                    };
                    let m = &mut e.mob;
                    match f.get(2).copied() {
                        Some("health") => {
                            let h = num(3).unwrap_or(0.0) as f32;
                            m.health = h.clamp(0.0, m.max_health());
                            if m.health <= 0.0 && m.alive() {
                                m.death_time = 1;
                                died = true;
                            }
                        }
                        Some("damage") => {
                            died = mb::hurt(
                                m,
                                num(3).unwrap_or(0.0) as f32,
                                mb::Source::default(),
                                0.0,
                            ) == Some(true);
                        }
                        Some("pos") => {
                            if let (Some(x), Some(y), Some(z)) = (num(3), num(4), num(5)) {
                                m.pos = [x, y, z];
                                m.vel = [0.0; 3];
                                m.yaw = num(6).unwrap_or(m.yaw as f64) as f32;
                                m.pitch = num(7).unwrap_or(m.pitch as f64) as f32;
                                m.head_yaw = m.yaw;
                                m.nav.stop();
                                e.sent = play::quantize(m.pos);
                                moved = Some((m.pos, m.yaw, m.pitch, m.on_ground));
                            }
                        }
                        Some("age") => m.age = num(3).unwrap_or(0.0) as i32,
                        Some("love") => m.love = num(3).unwrap_or(0.0) as i32,
                        Some("sheared") => m.sheared = f.get(3) == Some(&"1"),
                        Some("color") => m.color = num(3).unwrap_or(0.0) as u8 & 15,
                        // `CraftSlime.setSize`: `setSize(size, true)`.
                        Some("size") if m.species == Species::Slime => {
                            m.set_size(num(3).unwrap_or(1.0) as i32, true);
                        }
                        Some("name") => {
                            let name = f[3..].join(" ");
                            m.custom_name = (!name.is_empty()).then_some(name);
                        }
                        _ => return "0".into(),
                    }
                }
                if let Some((pos, yaw, pitch, ground)) = moved {
                    self.hub.send_about(
                        id,
                        play::teleport_entity(
                            self.ids.teleport_entity,
                            id,
                            pos,
                            yaw,
                            pitch,
                            ground,
                        ),
                    );
                    self.hub
                        .send(None, play::rotate_head(self.ids.rotate_head, id, yaw));
                }
                if died {
                    self.hub
                        .send_about(id, inv::entity_event(self.ids.entity_event, id, 3));
                }
                self.refresh_mob(id);
                "1".into()
            }
            _ => String::new(),
        }
    }
}

/// Освещение для спавна монстров в этом тике и измерении.
#[derive(Clone, Copy)]
struct SpawnLight {
    darken: i32,
    thundering: bool,
    rules: mb::MonsterLight,
    /// Точка спавна мира: ближе 24 блоков к ней никто не появляется.
    world_spawn: Option<[f64; 3]>,
}

impl PlayPlan {
    /// Затемнение неба сейчас (`Level.skyDarken`): по таймлайну дня и погоде. В Аду и Краю времени суток нет — «светло снаружи» там не
    /// бывает, поэтому 11, как ночью.
    fn sky_darken_now(&self, dim: Dimension, rain: bool, thunder: bool) -> i32 {
        if dim != Dimension::Overworld {
            return 11;
        }
        let day = self.day_time.load(Ordering::Relaxed).rem_euclid(24000) as f32;
        mb::sky_darken(day, f32::from(u8::from(rain)), f32::from(u8::from(thunder)))
    }

    /// Прямая видимость (`ClipContext.Block.COLLIDER`): точки отрезка через
    /// каждые 0.2 блока не попадают в коробки коллизии. Стекло видимость
    /// закрывает, трава и цветы — нет.
    fn clear_line(&self, w: &WorldView<'_>, from: [f64; 3], to: [f64; 3]) -> bool {
        let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let steps = (len / 0.2).ceil().max(1.0) as usize;
        for i in 1..steps {
            let t = i as f64 / steps as f64;
            let p = [from[0] + d[0] * t, from[1] + d[1] * t, from[2] + d[2] * t];
            let cell = p.map(|v| v.floor() as i32);
            let local = [
                p[0] - f64::from(cell[0]),
                p[1] - f64::from(cell[1]),
                p[2] - f64::from(cell[2]),
            ];
            let hit = self
                .shape_at(w.block(cell), cell)
                .iter()
                .any(|b| (0..3).all(|a| local[a] >= b[a] && local[a] <= b[a + 3]));
            if hit {
                return false;
            }
        }
        true
    }

    /// `Entity.thunderHit` для мобов: 5 урона, огонь на 8 секунд,
    /// крипер заряжается. Область — `AABB(pos).inflate(3)` вверх до `y + 9`.
    pub(super) fn lightning_mobs(&self, at: [f64; 3]) {
        let mut hits = Vec::new();
        {
            let mut mobs = self.sim().mobs.lock().unwrap();
            for (&id, e) in mobs.iter_mut() {
                if !e.mob.alive() {
                    continue;
                }
                let mpos = e.mob.pos;
                if (mpos[0] - at[0]).powi(2) + (mpos[2] - at[2]).powi(2) > 9.0 {
                    continue;
                }
                if mpos[1] < at[1] - 3.0 || mpos[1] > at[1] + 9.0 {
                    continue;
                }
                let died = mb::hurt(&mut e.mob, 5.0, mb::Source::default(), 0.0) == Some(true);
                e.environment.burning = e.environment.burning.max(160);
                if e.mob.species == Species::Creeper {
                    e.mob.powered = true;
                }
                hits.push((
                    id,
                    (
                        e.mob.sound(if died { "death" } else { "hurt" }),
                        mpos,
                        e.mob.volume(),
                        e.mob.is_baby(),
                        sound_source(e.mob.species),
                    ),
                    died,
                ));
            }
        }
        for (id, (name, pos, volume, baby, source), died) in hits {
            self.hub
                .send_about(id, play::hurt_animation(self.ids.hurt_animation, id, 0.0));
            self.refresh_mob(id);
            self.play_sound(None, &name, source, pos, volume, mob_pitch(baby));
            self.emit_vibration(pos, "minecraft:entity_damage");
            if died {
                self.emit_vibration(pos, "minecraft:entity_die");
            }
        }
    }

    /// Удар моба по игроку (`Mob.doHurtTarget`): атрибут `attack_damage` со
    /// сложностью, затем отбрасывание 0.4 от моба (`LivingEntity.knockback`).
    /// Броню, сопротивление и событие урона плагинам считает сессия игрока.
    /// `AbstractSkeleton.performRangedAttack`: стрела от глаз (−0.1) в треть
    /// роста цели с поправкой на дальность, сила 1.6, разброс `14 − 4 × сложность`,
    /// урон `2 × сила лука + triangle(0.11 × сложность, 0.57425)`.
    fn mob_shoot(&self, id: i32, target: Uuid, power: f32) {
        let Some(eye) = self
            .sim()
            .mobs
            .lock()
            .unwrap()
            .get(&id)
            .map(|e| e.mob.eye())
        else {
            return;
        };
        let Some(feet) = self.hub.players.lock().unwrap().get(&target).map(|p| p.pos) else {
            return;
        };
        let from = [eye[0], eye[1] - 0.1, eye[2]];
        let dx = feet[0] - from[0];
        let dy = feet[1] + mb::PLAYER_SIZE.1 / 3.0 - from[1];
        let dz = feet[2] - from[2];
        let d = (dx * dx + dz * dz).sqrt();
        let difficulty = self.difficulty().id();
        let spread = f64::from(14 - i32::from(difficulty) * 4);
        let vel = crate::arrows::shoot([dx, dy + d * 0.2, dz], 1.6, spread, rand::random());
        let mut arrow = crate::arrows::Arrow::flight(from, vel, false);
        let uuid = self.sim().mobs.lock().unwrap().get(&id).map(|e| e.mob.uuid);
        arrow.owner = uuid.map(Uuid::from_u128);
        arrow.left_owner = false;
        let triangle = f64::from(difficulty) * 0.11
            + 0.57425 * (rand::random::<f64>() - rand::random::<f64>());
        arrow.base_damage = f64::from(power) * 2.0 + triangle;
        // `Pickup.DISALLOWED`: стрелу моба не подобрать.
        arrow.creative_only = true;
        self.launch(self.survival.arrow_entity_type, arrow);
        self.play_sound(
            None,
            "minecraft:entity.skeleton.shoot",
            crate::sounds::HOSTILE,
            from,
            1.0,
            1.0 / (rand::random::<f32>() * 0.4 + 0.8),
        );
    }

    /// `Slime.remove` у умершего больше 1: 2–4 детей вдвое меньше
    /// (`convertTo SPLIT_ON_DEATH`: имя и постоянство переходят к ним).
    fn slime_split(&self, m: &Mob, count: i32) {
        let count = count.clamp(0, MAX_SLIME_SPLIT);
        let size = m.cube_size / 2;
        let width = m.size().0 as f32;
        for [dx, dz] in mb::cube::split_offsets(count, f64::from(width)) {
            let pos = [m.pos[0] + dx, m.pos[1] + 0.5, m.pos[2] + dz];
            let yaw = rand::random::<f32>() * 360.0;
            let (name, persistent) = (m.custom_name.clone(), m.persistent);
            self.spawn_mob_with(Species::Slime, pos, yaw, false, Some("SLIME_SPLIT"), |c| {
                c.set_size(size, true);
                c.custom_name = name;
                c.persistent = persistent;
            });
        }
    }

    fn mob_strike(&self, species: Species, target: Uuid, from: [f64; 3], attack: f32) {
        // `Creeper.doHurtTarget`: удар засчитан, урона нет — он взрывается.
        if species == Species::Creeper {
            return;
        }
        let mut damage = survival::mob_damage_for(self.difficulty(), attack);
        // `isInvulnerableTo` раньше счёта неуязвимости: незагрузившегося
        // клиента удар не задевает и `lastHurt` не меняет.
        if damage <= 0.0 || !self.client_loaded(target) {
            return;
        }
        // `LivingEntity.hurtServer`: 10 тиков после удара (`invulnerableTime`
        // > 10) проходит только превышение над прошлым ударом. Слизень бьёт
        // касанием каждый тик — без этого он наносил бы 20 ударов в секунду.
        {
            let now = self.game_time.load(Ordering::Relaxed);
            let mut hits = self.mob_hits.lock().unwrap();
            match hits.get(&target).copied() {
                Some((at, last)) if now.wrapping_sub(at) < 10 => {
                    if damage <= last {
                        return;
                    }
                    hits.insert(target, (at, damage));
                    damage -= last;
                }
                _ => {
                    hits.insert(target, (now, damage));
                }
            }
            if hits.len() > 256 {
                hits.retain(|_, (at, _)| now.wrapping_sub(*at) < 10);
            }
        }
        let player = {
            let players = self.hub.players.lock().unwrap();
            players
                .get(&target)
                .map(|p| (p.entity_id, p.pos, p.on_ground))
        };
        let Some((entity, pos, on_ground)) = player else {
            return;
        };
        if !self.push_control(target, Control::Hurt(damage, Cause::Mob(species.name()))) {
            return;
        }
        // `AbstractCubeMob.dealDamage`: звук удара после прошедшего урона.
        if species == Species::Slime {
            let pitch = (rand::random::<f32>() - rand::random::<f32>()) * 0.2 + 1.0;
            self.play_sound(
                None,
                "minecraft:entity.slime.attack",
                crate::sounds::HOSTILE,
                from,
                1.0,
                pitch,
            );
        }
        let d = [from[0] - pos[0], from[2] - pos[2]];
        let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
        if len > 1e-4 {
            let vel = [
                -d[0] / len * mb::KNOCKBACK,
                if on_ground { mb::KNOCKBACK } else { 0.0 },
                -d[1] / len * mb::KNOCKBACK,
            ];
            self.hub.send(
                None,
                play::set_entity_motion(self.ids.set_entity_motion, entity, vel),
            );
        }
    }

    /// Естественный спавн по тику (`NaturalSpawner.spawnForChunk`): чанки
    /// симуляции в 128 блоках от игрока, лимит категории `max × чанки / 289`
    /// по мобам, которые могут исчезнуть. Монстры — каждый тик (не в мирной
    /// сложности и при `spawn_monsters`), существа — раз в 400 тиков; всё при
    /// `spawn_mobs`. Мир читается под одним замком, мобы появляются после.
    pub(super) fn tick_natural_spawn(&self) {
        use rand::Rng;
        use rand::seq::SliceRandom;
        if !self.vars.flag("spawn_mobs") {
            return;
        }
        let now = self.game_time.load(Ordering::Relaxed);
        let monsters =
            self.difficulty() != Difficulty::Peaceful && self.vars.flag("spawn_monsters");
        let creatures = now.rem_euclid(mb::CREATURE_SPAWN_PERIOD as i64) == 0;
        if !monsters && !creatures {
            return;
        }
        let dim = self.active_dim();
        let players: Vec<[f64; 3]> = {
            let players = self.hub.players.lock().unwrap();
            players
                .values()
                .filter(|p| p.collects && p.dimension == dim)
                .map(|p| p.pos)
                .collect()
        };
        if players.is_empty() {
            return;
        }
        let simulated = self.simulated_set();
        let near = mb::DESPAWN_DISTANCE * mb::DESPAWN_DISTANCE;
        let mut chunks: Vec<(i32, i32)> = simulated
            .iter()
            .copied()
            .filter(|&(cx, cz)| {
                let (x, z) = (f64::from(cx * 16 + 8), f64::from(cz * 16 + 8));
                players
                    .iter()
                    .any(|p| (p[0] - x).powi(2) + (p[2] - z).powi(2) < near)
            })
            .collect();
        if chunks.is_empty() {
            return;
        }
        let slot = |c: mb::Category| match c {
            mb::Category::Monster => 0,
            mb::Category::Creature => 1,
        };
        let mut counts = [0usize; 2];
        for e in self.sim().mobs.lock().unwrap().values() {
            let m = &e.mob;
            let chunk = (
                (m.pos[0].floor() as i32) >> 4,
                (m.pos[2].floor() as i32) >> 4,
            );
            if m.alive() && !m.persistent && simulated.contains(&chunk) {
                counts[slot(m.species.category())] += 1;
            }
        }
        let weather = self.vars.weather();
        let light = SpawnLight {
            darken: self.sky_darken_now(dim, weather.rain, weather.thunder),
            thundering: weather.thunder,
            rules: if dim == Dimension::Nether {
                mb::MonsterLight::NETHER
            } else {
                mb::MonsterLight::OVERWORLD
            },
            world_spawn: if dim == Dimension::Overworld {
                self.vars.world_spawn()
            } else {
                None
            },
        };
        let player_chunks: Vec<(i32, i32)> = players
            .iter()
            .map(|p| ((p[0].floor() as i32) >> 4, (p[2].floor() as i32) >> 4))
            .collect();
        let cap_chunks = mb::spawn_cap_chunks(&player_chunks);
        let mut rng = rand::thread_rng();
        chunks.shuffle(&mut rng);
        let mut plan: Vec<(Species, [i32; 3])> = Vec::new();
        self.with_world(|w| {
            for cat in mb::Category::ALL {
                let on = match cat {
                    mb::Category::Monster => monsters,
                    mb::Category::Creature => creatures,
                };
                if !on {
                    continue;
                }
                let cap = mb::mob_cap(cat, cap_chunks);
                for &chunk in &chunks {
                    if counts[slot(cat)] >= cap {
                        break;
                    }
                    let found = self.spawn_in_chunk(w, cat, chunk, light, &players);
                    counts[slot(cat)] += found.len();
                    plan.extend(found);
                }
            }
        });
        for (species, at) in plan {
            // `Zombie.getSpawnAsBabyOdds`: 5 %.
            let baby = species == Species::Zombie && rng.r#gen::<f64>() < 0.05;
            let pos = [
                f64::from(at[0]) + 0.5,
                f64::from(at[1]),
                f64::from(at[2]) + 0.5,
            ];
            self.spawn_mob(
                species,
                pos,
                rng.r#gen::<f32>() * 360.0,
                baby,
                Some("NATURAL"),
            );
        }
    }

    /// Одна попытка категории в чанке (`spawnCategoryForChunk`): случайная
    /// точка от дна мира до поверхности + 1, список вида по биому этой точки.
    fn spawn_in_chunk(
        &self,
        w: &WorldView<'_>,
        cat: mb::Category,
        (cx, cz): (i32, i32),
        light: SpawnLight,
        players: &[[f64; 3]],
    ) -> Vec<(Species, [i32; 3])> {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let (x, z) = (
            cx * 16 + rng.gen_range(0..16),
            cz * 16 + rng.gen_range(0..16),
        );
        let Some(top) = self.surface_y(w, x, z) else {
            return Vec::new();
        };
        let start = [x, rng.gen_range(self.min_y()..=top + 1), z];
        // Внутри полного блока (`isRedstoneConductor`) чанк пропускается.
        if w.block(start)
            .is_some_and(|s| mb::full_block(self.shape_at(Some(s), start)))
        {
            return Vec::new();
        }
        let table = match cat {
            mb::Category::Monster => &self.survival.monster_spawns,
            mb::Category::Creature => &self.survival.creature_spawns,
        };
        let entries: Vec<mb::SpawnEntry> = self
            .biome_in(w, start)
            .and_then(|b| table.get(b as usize))
            .map(|b| {
                b.entries
                    .iter()
                    .map(|(name, weight, min, max)| mb::SpawnEntry {
                        species: Species::from_name(name),
                        weight: *weight,
                        min: *min,
                        max: *max,
                    })
                    .collect()
            })
            .unwrap_or_default();
        if entries.iter().all(|e| e.species.is_none()) {
            return Vec::new();
        }
        let mut nearest = |p: [i32; 3]| {
            let c = [
                f64::from(p[0]) + 0.5,
                f64::from(p[1]),
                f64::from(p[2]) + 0.5,
            ];
            players
                .iter()
                .map(|q| mb::dist2(*q, c))
                .min_by(f64::total_cmp)
        };
        let mut fit_rng = rand::thread_rng();
        let mut fits = |species: Species, pos: [i32; 3], _: f64| {
            self.spawn_fits(w, species, pos, light, &mut fit_rng)
        };
        let mut roll = || rng.r#gen::<f64>();
        mb::spawn_packs(start, &entries, &mut nearest, &mut fits, &mut roll)
    }

    /// Место для моба (`SpawnPlacementTypes.ON_GROUND`, `noCollision` рамки
    /// и правила вида): прочная опора, не запрещённый пол, свободные клетка и
    /// клетка над ней без жидкости и опасных блоков, место под рамку; монстру —
    /// темнота (`isDarkEnoughToSpawn`), животному — трава и свет больше 8.
    fn spawn_fits(
        &self,
        w: &WorldView<'_>,
        species: Species,
        pos: [i32; 3],
        light: SpawnLight,
        rng: &mut impl rand::Rng,
    ) -> bool {
        let feet = [
            f64::from(pos[0]) + 0.5,
            f64::from(pos[1]),
            f64::from(pos[2]) + 0.5,
        ];
        if let Some(spawn) = light.world_spawn
            && mb::dist2(spawn, feet) < mb::MIN_SPAWN_DISTANCE * mb::MIN_SPAWN_DISTANCE
        {
            return false;
        }
        let below = [pos[0], pos[1] - 1, pos[2]];
        let Some(floor) = w.block(below) else {
            return false;
        };
        if !mb::sturdy_top(self.shape_at(Some(floor), below))
            || mb::spawn_floor_forbidden(self.state_name(floor))
        {
            return false;
        }
        for cell in [pos, [pos[0], pos[1] + 1, pos[2]]] {
            let Some(s) = w.block(cell) else {
                return false;
            };
            if mb::full_block(self.shape_at(Some(s), cell))
                || crate::fluids::state_fluid(&self.survival.placement, s).is_some()
                || mb::spawn_cell_forbidden(self.state_name(s))
            {
                return false;
            }
        }
        let (width, height) = species.size();
        let shape = |c: [i32; 3]| self.shape_at(w.block(c), c);
        if !mb::box_free(feet, width, height, &shape) {
            return false;
        }
        let sky = w.sky_light_at(pos).unwrap_or(0);
        let block = w.block_light_at(pos).unwrap_or(0);
        // `Slime.checkSlimeSpawnRules`: темнота не нужна. Болото на высоте
        // 51–69 — по шансу луны и свету; иначе слизневый чанк ниже y 40.
        if species == Species::Slime {
            let swamp = self.biome_in(w, pos).is_some_and(|b| {
                let names = &self.survival.biome_names;
                let tags = self
                    .survival
                    .biome_tags
                    .get("minecraft:allows_surface_slime_spawns");
                names
                    .get(b as usize)
                    .is_some_and(|n| tags.is_some_and(|t| t.contains(n)))
            });
            if swamp && pos[1] > 50 && pos[1] < 70 {
                let chance = mb::cube::surface_spawn_chance(self.day_time.load(Ordering::Relaxed));
                if rng.r#gen::<f32>() < chance
                    && i32::from(mb::raw_brightness(sky, block, light.darken))
                        <= rng.gen_range(0..8)
                {
                    return true;
                }
            }
            let roll = rng.gen_range(0..10) == 0;
            return roll
                && mb::cube::slime_chunk(pos[0] >> 4, pos[2] >> 4, self.seed)
                && pos[1] < 40;
        }
        match species.category() {
            mb::Category::Monster => {
                let mut roll = || rng.r#gen::<f64>();
                mb::dark_enough(
                    sky,
                    block,
                    light.darken,
                    light.thundering,
                    light.rules,
                    &mut roll,
                )
            }
            mb::Category::Creature => {
                chunkwake_world::has_flag(floor, chunkwake_world::flags::GRASS)
                    && mb::bright_enough(sky, block)
            }
        }
    }

    /// Верхний не воздушный блок столбца (карта высот `WORLD_SURFACE`).
    fn surface_y(&self, w: &WorldView<'_>, x: i32, z: i32) -> Option<i32> {
        let min_y = self.min_y();
        let top = min_y + self.template().height() as i32 - 1;
        (min_y..=top).rev().find(|&y| {
            w.block([x, y, z]).is_some_and(|s| {
                !matches!(
                    self.state_name(s),
                    "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air" | ""
                )
            })
        })
    }

    /// Биом (сетевой ID) в клетке: палитра 4×4×4 секции.
    fn biome_in(&self, w: &WorldView<'_>, [x, y, z]: [i32; 3]) -> Option<u32> {
        let ry = (y - self.min_y()).max(0) as usize;
        let column = match w.columns.get(&(x >> 4, z >> 4)) {
            Some((column, _)) => column,
            None => self.template(),
        };
        let section = column.sections.get(ry / 16)?;
        Some(section.biome_at(
            ((x & 15) / 4) as usize,
            (ry % 16) / 4,
            ((z & 15) / 4) as usize,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mace_smash_bonus_matches_vanilla() {
        assert_eq!(PlayPlan::mace_smash_bonus(0.0), 0.0);
        assert_eq!(PlayPlan::mace_smash_bonus(2.0), 8.0);
        assert_eq!(PlayPlan::mace_smash_bonus(3.0), 12.0);
        assert_eq!(PlayPlan::mace_smash_bonus(5.0), 16.0);
        assert_eq!(PlayPlan::mace_smash_bonus(8.0), 22.0);
        assert_eq!(PlayPlan::mace_smash_bonus(10.0), 24.0);
        assert_eq!(PlayPlan::mace_smash_damage(5.0, 0), 16.0);
        assert_eq!(PlayPlan::mace_smash_damage(5.0, 3), 23.5);
    }

    #[test]
    fn mob_nbt_roundtrip() {
        let mut m = Mob::new(
            Species::Sheep,
            [1.5, 64.0, -3.25],
            0x0123_4567_89ab_cdef_0011_2233_4455_6677,
        );
        (m.yaw, m.pitch, m.health, m.age, m.sheared, m.color) = (90.0, 10.0, 5.0, -100, true, 14);
        let back = mob_from_nbt(
            &mob_to_nbt(&m, &std::collections::HashMap::new(), &[]),
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        )
        .unwrap();
        assert_eq!(back.species, Species::Sheep);
        assert_eq!(back.uuid, m.uuid);
        assert_eq!(back.pos, m.pos);
        assert_eq!((back.yaw, back.health, back.age), (90.0, 5.0, -100));
        assert!(back.sheared);
        assert_eq!(back.color, 14);
    }

    #[test]
    fn mob_nbt_keeps_active_effects() {
        let mut m = Mob::new(Species::Cow, [0.5, 64.0, 0.5], 7);
        m.add_effect(crate::survival::Effect::plain(2, 1, 120));
        let names = std::collections::HashMap::from([("minecraft:poison".to_owned(), 2)]);
        let nbt = mob_to_nbt(&m, &names, &[]);
        let list = match nbt.get("active_effects") {
            Some(Tag::List(list)) => list,
            _ => panic!("нет active_effects"),
        };
        let Tag::Compound(e) = &list[0] else {
            panic!("элемент не compound")
        };
        assert_eq!(
            e.get("id"),
            Some(&Tag::String("minecraft:poison".to_owned()))
        );
        assert_eq!(e.get("duration"), Some(&Tag::Int(120)));
        let back = mob_from_nbt(&nbt, &names, &std::collections::HashMap::new()).unwrap();
        assert_eq!(back.effects.len(), 1);
        assert_eq!((back.effects[0].id, back.effects[0].amplifier), (2, 1));
        assert_eq!(back.effects[0].duration, 120);
    }

    #[test]
    fn mob_nbt_keeps_can_break_doors() {
        let mut m = Mob::new(Species::Zombie, [0.5, 64.0, 0.5], 9);
        m.can_break_doors = true;
        let nbt = mob_to_nbt(&m, &std::collections::HashMap::new(), &[]);
        let back = mob_from_nbt(
            &nbt,
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        )
        .unwrap();
        assert!(back.can_break_doors);
    }

    #[test]
    fn mob_nbt_keeps_equipment_and_drop_chances() {
        let mut m = Mob::new(Species::Zombie, [0.5, 64.0, 0.5], 9);
        m.set_item(0, Some(Stack::new(2, 1)));
        m.set_item(5, Some(Stack::new(1, 1)));
        m.drop_chances[5] = 2.0;
        m.drop_chances[0] = 0.0;
        let names = [
            "minecraft:air".to_owned(),
            "minecraft:iron_helmet".to_owned(),
            "minecraft:iron_sword".to_owned(),
        ];
        let items = std::collections::HashMap::from([
            ("minecraft:iron_helmet".to_owned(), 1),
            ("minecraft:iron_sword".to_owned(), 2),
        ]);
        let nbt = mob_to_nbt(&m, &std::collections::HashMap::new(), &names);
        let Some(Tag::Compound(eq)) = nbt.get("equipment") else {
            panic!("нет equipment")
        };
        assert!(eq.contains_key("mainhand"));
        assert!(eq.contains_key("head"));
        let Some(Tag::Compound(chances)) = nbt.get("drop_chances") else {
            panic!("нет drop_chances")
        };
        assert_eq!(chances.get("head"), Some(&Tag::Float(2.0)));
        assert_eq!(chances.get("mainhand"), Some(&Tag::Float(0.0)));

        let back = mob_from_nbt(&nbt, &std::collections::HashMap::new(), &items).unwrap();
        assert_eq!(back.item(0).map(|s| s.item), Some(2));
        assert_eq!(back.item(5).map(|s| s.item), Some(1));
        assert_eq!(back.drop_chance(5), 2.0);
        assert_eq!(back.drop_chance(0), 0.0);
        // Незаданные слоты остаются пустыми, шанс — по умолчанию.
        assert_eq!(back.item(4), None);
        assert_eq!(back.drop_chance(4), crate::mobs::DEFAULT_DROP_CHANCE);
    }

    #[test]
    fn mob_nbt_reads_legacy_hand_and_armor_items() {
        let mut root = Compound::new();
        root.insert("id".into(), Tag::String("minecraft:zombie".into()));
        root.insert(
            "Pos".into(),
            Tag::List(vec![Tag::Double(0.0), Tag::Double(64.0), Tag::Double(0.0)]),
        );
        let sword = Tag::Compound(Compound::from([
            ("id".into(), Tag::String("minecraft:iron_sword".into())),
            ("count".into(), Tag::Int(1)),
        ]));
        let helmet = Tag::Compound(Compound::from([
            ("id".into(), Tag::String("minecraft:iron_helmet".into())),
            ("count".into(), Tag::Int(1)),
        ]));
        root.insert(
            "HandItems".into(),
            Tag::List(vec![sword, Tag::Compound(Compound::new())]),
        );
        root.insert(
            "ArmorItems".into(),
            Tag::List(vec![
                Tag::Compound(Compound::new()),
                Tag::Compound(Compound::new()),
                Tag::Compound(Compound::new()),
                helmet,
            ]),
        );
        root.insert(
            "DropChances".into(),
            Tag::List(vec![
                Tag::Float(0.0),
                Tag::Float(0.0),
                Tag::Float(0.0),
                Tag::Float(0.0),
                Tag::Float(0.0),
                Tag::Float(2.0),
            ]),
        );
        let items = std::collections::HashMap::from([
            ("minecraft:iron_helmet".to_owned(), 1),
            ("minecraft:iron_sword".to_owned(), 2),
        ]);
        let back = mob_from_nbt(&root, &std::collections::HashMap::new(), &items).unwrap();
        assert_eq!(back.item(0).map(|s| s.item), Some(2));
        assert_eq!(back.item(5).map(|s| s.item), Some(1));
        assert_eq!(back.drop_chance(5), 2.0);
    }

    #[test]
    fn slime_nbt_roundtrip_keeps_size_and_health() {
        let mut m = Mob::new(
            Species::Slime,
            [1.5, 64.0, -3.25],
            0x0123_4567_89ab_cdef_0011_2233_4455_6677,
        );
        m.set_size(4, true);
        m.was_on_ground = true;
        let back = mob_from_nbt(
            &mob_to_nbt(&m, &std::collections::HashMap::new(), &[]),
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        )
        .unwrap();
        assert_eq!(back.cube_size, 4);
        assert_eq!(back.max_health(), 16.0);
        assert_eq!(back.health, 16.0);
        assert!(back.was_on_ground);
    }

    #[test]
    fn feeding_speeds_up_baby() {
        assert_eq!(feed_baby(mb::BABY_AGE), mb::BABY_AGE + 2400);
        assert_eq!(feed_baby(-10), -10);
    }

    #[test]
    fn name_tag_text_is_extracted() {
        assert_eq!(tag_name("\"Бурёнка\"").as_deref(), Some("Бурёнка"));
        assert_eq!(
            tag_name(r#"{"text":"Бурёнка","color":"gold"}"#).as_deref(),
            Some("Бурёнка")
        );
        assert_eq!(tag_name("7"), None);
    }

    #[test]
    fn melee_damage_scales_with_charge_and_crit() {
        let near = |a: f32, b: f32| (a - b).abs() < 1e-5;
        // Полный заряд: 6 урона железного меча без бонусов.
        assert!(near(PlayPlan::melee_damage(6.0, 0.0, 1.0, false), 6.0));
        // Пустой заряд: только 0.2 от базы.
        assert!(near(PlayPlan::melee_damage(6.0, 0.0, 0.0, false), 1.2));
        // Крит — до бонуса чар.
        assert!(near(PlayPlan::melee_damage(6.0, 0.0, 1.0, true), 9.0));
        // Острота при полном заряде прибавляется целиком.
        assert!(near(PlayPlan::melee_damage(6.0, 1.5, 1.0, false), 7.5));
        // При неполном — пропорционально заряду (`magicBoost`).
        let half = PlayPlan::melee_damage(6.0, 1.5, 0.5, false);
        assert!(near(half, 6.0 * 0.4 + 0.75));
    }

    #[test]
    fn sweep_damage_uses_ratio_and_charge() {
        let near = |a: f32, b: f32| (a - b).abs() < 1e-5;
        // Без Sweeping Edge: 1 × заряд, как раньше.
        assert!(near(PlayPlan::sweep_damage(6.0, 0.0, 0, 1.0), 1.0));
        assert!(near(PlayPlan::sweep_damage(6.0, 0.0, 0, 0.5), 0.5));
        // Уровень 3: ratio 3/4, железный меч — 1 + 4.5 = 5.5.
        assert!(near(PlayPlan::sweep_damage(6.0, 0.0, 3, 1.0), 5.5));
        // Острота прибавляется до умножения на заряд.
        assert!(near(PlayPlan::sweep_damage(6.0, 1.5, 3, 1.0), 7.0));
        assert!(near(PlayPlan::sweep_damage(6.0, 1.5, 3, 0.5), 3.5));
    }

    #[test]
    fn sheep_colors_mostly_white() {
        assert_eq!(sheep_color(0.5), 0);
        assert_eq!(sheep_color(0.01), 15);
        assert_eq!(sheep_color(0.181), 6);
    }

    /// Дельта считается от последней разосланной позиции: сумма дельт равна
    /// смещению моба, а не удваивает его. Именно здесь была бы скорость «в 2-3 раза».
    #[test]
    fn move_deltas_sum_to_displacement() {
        let mut sent = play::quantize([0.5, 64.0, 0.5]);
        let mut pos = [0.5, 64.0, 0.5];
        let mut decoded = [0.5, 64.0, 0.5];
        // Шаг 0.44 блока за тик — установившаяся скорость коровы.
        for _ in 0..20 {
            pos[0] += 0.44;
            let q = play::quantize(pos);
            let delta = play::delta(sent, q).expect("короткий шаг помещается в short");
            sent = q;
            for i in 0..3 {
                decoded[i] += f64::from(delta[i]) / 4096.0;
            }
        }
        assert!(
            (decoded[0] - pos[0]).abs() < 1e-3,
            "клиент прошёл {:.4}, моб сместился {:.4}",
            decoded[0],
            pos[0]
        );
    }
}
