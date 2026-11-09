//! Живые мобы в `PlayPlan`: появление у клиентов, тик, удар игрока, смерть с лутом.
//! Логика самих мобов — в [`crate::mobs`].

use super::*;
use crate::mobs::{self as mb, Mob, Species};

/// Группа спавна: вид, вес, минимум и максимум.
type SpawnEntry = (Option<Species>, u32, u32, u32);
/// Игрок для тика мобов: ноги, имена предметов в руках (основная и вторая), UUID.
type Hands<'a> = ([f64; 3], [Option<&'a str>; 2], u128);

/// Моб и то, что о нём уже знают клиенты.
pub(super) struct MobEntry {
    pub mob: Mob,
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
}

impl MobMeta {
    fn of(m: &Mob) -> Self {
        Self {
            swell_dir: m.swell_dir,
            powered: m.powered,
            ignited: m.ignited,
            climbing: m.climbing,
            using_item: m.using_item.is_some(),
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
        }
    }
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
        // `AbstractSkeleton.populateDefaultEquipmentSlots`: лук в основной руке.
        if m.species == Species::Skeleton
            && let Some(bow) = self.item_id("minecraft:bow")
        {
            out.push(play::set_equipment(
                self.ids.set_equipment,
                id,
                &[(0, Some(Stack::new(bow, 1)))],
            ));
        }
        out
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
        let id = self.next_entity_id.fetch_add(1, Ordering::Relaxed);
        let mut m = Mob::new(species, pos, rand::random());
        (m.yaw, m.head_yaw) = (yaw, yaw);
        if baby {
            m.age = mb::BABY_AGE;
        }
        if species == Species::Sheep {
            m.color = sheep_color(rand::random());
        }
        // Исчезать вдали от игроков могут только природные: с яйца, из команды
        // и от плагина моб остаётся (`setPersistenceRequired`).
        m.persistent = !matches!(reason, Some("NATURAL" | "CHUNK_GEN"));
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
                    let (width, height) = e.mob.species.size();
                    let view =
                        crate::environment::probe(e.mob.pos, width, height, height * 0.85, |p| {
                            self.environment_kind(w.block(p))
                        });
                    if !view.water && landed_fall.is_some_and(|fall| fall > 0.0) {
                        vibrations.push((e.mob.pos, "minecraft:hit_ground"));
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
                    for (hit, amount) in [
                        (pulse.drown && self.vars.flag("drowning_damage"), 2.0),
                        (pulse.freeze && self.vars.flag("freeze_damage"), 1.0),
                        (pulse.lava && fire_damage, 4.0),
                        (pulse.burn && fire_damage && !pulse.lava, 1.0),
                        (pulse.cactus, 1.0),
                        (pulse.berries && moving, 1.0),
                        (pulse.magma && fire_damage, 1.0),
                    ] {
                        if hit
                            && let Some(died) =
                                mb::hurt(&mut e.mob, amount, mb::Source::default(), 0.0)
                        {
                            packets
                                .push((id, play::hurt_animation(self.ids.hurt_animation, id, 0.0)));
                            vibrations.push((e.mob.pos, "minecraft:entity_damage"));
                            if died {
                                vibrations.push((e.mob.pos, "minecraft:entity_die"));
                            }
                        }
                    }
                    if let Some(target) = e.mob.strike.take() {
                        strikes.push((species, Uuid::from_u128(target), e.mob.pos));
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
        for (species, target, from) in strikes {
            self.mob_strike(species, target, from);
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
            let dead: Vec<(i32, MobEntry)> = {
                let mut mobs = self.sim().mobs.lock().unwrap();
                gone.iter()
                    .filter_map(|id| Some((*id, mobs.remove(id)?)))
                    .collect()
            };
            self.hub.remove_tracked(&gone);
            for (id, e) in dead {
                self.mob_loot(id, &e);
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
        let props = move |k: &str| match k {
            "color" => Some(color.to_owned()),
            "sheared" => Some(sheared.to_string()),
            "on_fire" => Some(on_fire.to_string()),
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
            m.species.xp(m.is_baby(), rand::random())
        } else {
            0
        };
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

    /// Удар игрока по мобу (`Player.attack`). Возвращает слот оружия, если оно износилось.
    pub(super) async fn attack_mob(
        &self,
        uuid: Uuid,
        target: i32,
        inv: &mut Inventory,
        creative: bool,
        sprinting: bool,
    ) -> Option<usize> {
        let (pos, yaw, airborne) = {
            let players = self.hub.players.lock().unwrap();
            let p = players.get(&uuid)?;
            (p.pos, p.yaw, !p.on_ground)
        };
        let slot = inv.held_slot();
        let held = inv.slots[slot];
        let item = held.and_then(|s| self.survival.items.get(s.item as usize));
        let level = |n: &str| {
            held.map_or(0, |s| {
                chunkwake_protocol::components::stack_level(s.extra, n)
            })
        };
        let mut damage = 1.0 + item.map_or(0.0, |i| i.attack);
        let sharp = level("minecraft:sharpness");
        if sharp > 0 {
            damage += 0.5 * sharp as f32 + 0.5;
        }
        let crit = airborne && !sprinting;
        if crit {
            damage *= 1.5;
        }
        let species = self.sim().mobs.lock().unwrap().get(&target)?.mob.species;
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
        let knock = mb::KNOCKBACK
            * (1.0 + level("minecraft:knockback") as f64 + f64::from(u8::from(sprinting)));
        let (result, mpos, species, baby) = {
            let mut mobs = self.sim().mobs.lock().unwrap();
            let e = mobs.get_mut(&target)?;
            // Дальше 3 блоков (+ запас на задержку) не бьём.
            let d = [
                e.mob.pos[0] - pos[0],
                e.mob.pos[1] - pos[1],
                e.mob.pos[2] - pos[2],
            ];
            if d[0] * d[0] + d[1] * d[1] + d[2] * d[2] > 36.0 {
                return None;
            }
            // Обидчика монстр берёт целью сам (`HurtByTargetGoal`), творческого — нет.
            let r = mb::hurt(
                &mut e.mob,
                damage,
                mb::Source::at(pos, Some(uuid.as_u128())),
                knock,
            );
            if r == Some(true) {
                e.killer = Some((uuid, slot as i32));
            }
            (r, e.mob.pos, e.mob.species, e.mob.is_baby())
        };
        let died = result?;
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
        let sound = format!(
            "{}.{}",
            species.sound(),
            if died { "death" } else { "hurt" }
        );
        self.play_sound(
            None,
            &sound,
            sound_source(species),
            mpos,
            species.volume(),
            mob_pitch(baby),
        );
        self.play_sound(
            None,
            if crit {
                "minecraft:entity.player.attack.crit"
            } else {
                "minecraft:entity.player.attack.strong"
            },
            crate::sounds::PLAYERS,
            pos,
            1.0,
            1.0,
        );
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
pub(super) fn mob_to_nbt(m: &Mob) -> Compound {
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
        }
        Species::Sheep => {
            c.insert("Sheared".into(), Tag::Byte(i8::from(m.sheared)));
            c.insert("Color".into(), Tag::Byte(m.color as i8));
        }
        Species::Chicken => {
            c.insert("EggLayTime".into(), Tag::Int(m.egg_time));
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
    c
}

/// Обратно из NBT; чужие виды — `None`.
pub(super) fn mob_from_nbt(c: &Compound) -> Option<Mob> {
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
    if let Some(Tag::Float(h)) = c.get("Health") {
        m.health = h.clamp(0.0, species.max_health());
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
                if self.load_boat(c) || self.load_minecart(c) || self.load_item_frame(c) {
                    written.insert(pos);
                    continue;
                }
                let Some(m) = mob_from_nbt(c) else { continue };
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
            by_chunk.entry(pos).or_default().push(mob_to_nbt(&e.mob));
        }
        for (pos, c) in self
            .boats_nbt()
            .into_iter()
            .chain(self.minecarts_nbt())
            .chain(self.item_frames_nbt())
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
                // вид uuid x y z yaw pitch здоровье макс возраст стрижена цвет мёртв любовь\nимя
                format!(
                    "{} {} {x} {y} {z} {} {} {} {} {} {} {} {} {}\n{}",
                    m.species.short(),
                    Uuid::from_u128(m.uuid),
                    m.yaw,
                    m.pitch,
                    m.health,
                    m.species.max_health(),
                    m.age,
                    u8::from(m.sheared),
                    m.color,
                    u8::from(!m.alive()),
                    m.love,
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
                            m.health = h.clamp(0.0, m.species.max_health());
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
    /// Затемнение неба сейчас (`Level.skyDarken`): из угла солнца по таймлайну
    /// дня и погоды. В Аду и Краю времени суток нет — «светло снаружи» там не
    /// бывает, поэтому 11, как ночью.
    fn sky_darken_now(&self, dim: Dimension, rain: bool, thunder: bool) -> i32 {
        if dim != Dimension::Overworld {
            return 11;
        }
        let day = self.day_time.load(Ordering::Relaxed).rem_euclid(24000) as f32;
        mb::sky_darken(
            super::sun_angle(day),
            f32::from(u8::from(rain)),
            f32::from(u8::from(thunder)),
        )
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

    fn mob_strike(&self, species: Species, target: Uuid, from: [f64; 3]) {
        // `Creeper.doHurtTarget`: удар засчитан, урона нет — он взрывается.
        if species == Species::Creeper {
            return;
        }
        let damage = survival::mob_damage_for(self.difficulty(), species.attack_damage());
        if damage <= 0.0 {
            return;
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
                let cap = mb::mob_cap(cat, chunks.len());
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
    fn mob_nbt_roundtrip() {
        let mut m = Mob::new(
            Species::Sheep,
            [1.5, 64.0, -3.25],
            0x0123_4567_89ab_cdef_0011_2233_4455_6677,
        );
        (m.yaw, m.pitch, m.health, m.age, m.sheared, m.color) = (90.0, 10.0, 5.0, -100, true, 14);
        let back = mob_from_nbt(&mob_to_nbt(&m)).unwrap();
        assert_eq!(back.species, Species::Sheep);
        assert_eq!(back.uuid, m.uuid);
        assert_eq!(back.pos, m.pos);
        assert_eq!((back.yaw, back.health, back.age), (90.0, 5.0, -100));
        assert!(back.sheared);
        assert_eq!(back.color, 14);
    }

    #[test]
    fn feeding_speeds_up_baby() {
        assert_eq!(feed_baby(mb::BABY_AGE), mb::BABY_AGE + 2400);
        assert_eq!(feed_baby(-10), -10);
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
