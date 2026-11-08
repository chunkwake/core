//! Живые мобы в `PlayPlan`: появление у клиентов, тик, удар игрока, смерть с лутом.
//! Логика самих мобов — в [`crate::mobs`].

use super::*;
use crate::mobs::{self as mb, Mob, Species};

/// Группа спавна: вид, вес, минимум и максимум.
type SpawnEntry = (Option<Species>, u32, u32, u32);

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
}

impl MobEntry {
    fn new(mob: Mob) -> Self {
        Self {
            sent: play::quantize(mob.pos),
            sent_rot: (mob.yaw, mob.pitch, mob.head_yaw),
            mob,
            moves: 0,
            ambient: 80 + rand::random::<i32>().rem_euclid(120),
            killer: None,
            environment: Default::default(),
            fall: 0.0,
        }
    }
}

impl PlayPlan {
    fn mob_type(&self, s: Species) -> i32 {
        self.survival.mob_entity_types[s as usize]
    }

    fn mob_packets(&self, id: i32, m: &Mob) -> [Vec<u8>; 3] {
        let name = m
            .custom_name
            .as_ref()
            .map(|n| serde_json::json!({ "text": n }));
        [
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
                m.is_baby(),
                (m.species == Species::Sheep).then_some(m.color | u8::from(m.sheared) << 4),
                name.as_ref(),
            ),
        ]
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
        let packets = self.mob_packets(id, &m);
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
        self.hub.send_many(packets);
        Some(id)
    }

    /// Все мобы для зашедшего игрока.
    pub(super) fn mob_frames(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for (&id, e) in self.sim().mobs.lock().unwrap().iter() {
            for p in self.mob_packets(id, &e.mob) {
                out.extend(self.hub.frame(&p));
            }
        }
        out
    }

    /// Тик мобов: ИИ, физика, рассылка движений, звуки, исчезновение мёртвых.
    pub(super) fn tick_mobs(&self) {
        let loaded = std::mem::take(&mut *self.sim().mobs_loaded.lock().unwrap());
        if !loaded.is_empty() {
            // Все пакеты появления с диска — одной рассылкой.
            let mut frames = Vec::new();
            for e in loaded {
                let id = self.next_entity_id.fetch_add(1, Ordering::Relaxed);
                frames.extend(self.mob_packets(id, &e.mob));
                self.sim().mobs.lock().unwrap().insert(id, e);
            }
            self.hub.send_many(frames);
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
        let hands: Vec<([f64; 3], [Option<&str>; 2])> = {
            let live = self.live.lock().unwrap();
            let players = self.hub.players.lock().unwrap();
            players
                .values()
                .filter(|p| p.collects)
                .map(|p| {
                    let held = live.get(&p.entry.uuid).map_or([None, None], |l| {
                        [
                            l.slots[36 + l.selected].map(|s| self.item_name(s.item)),
                            l.slots[survival::OFFHAND].map(|s| self.item_name(s.item)),
                        ]
                    });
                    (p.pos, held)
                })
                .collect()
        };
        // Кого приманивает еда — по видам, один проход на вид за тик, а не на моба.
        let mut food_by_species: Vec<Vec<[f64; 3]>> = vec![Vec::new(); mb::Species::ALL.len()];
        {
            let mut seen = [false; mb::Species::ALL.len()];
            for (pos, held) in &hands {
                for species in mb::Species::ALL {
                    let slot = species as usize;
                    if !seen[slot] && held.iter().flatten().any(|n| species.is_food(n)) {
                        seen[slot] = true;
                        food_by_species[slot].push(*pos);
                    }
                }
            }
        }
        let physics = self.survival.physics.as_deref();
        let _ = physics;
        let mut rng = rand::thread_rng();
        let mut rand = || rand::Rng::r#gen::<f64>(&mut rng);
        let mut packets = Vec::new();
        let mut gone = Vec::new();
        let mut sounds = Vec::new();
        let mut trampling = Vec::new();
        let mut vibrations = Vec::new();
        let now = self.game_time.load(Ordering::Relaxed);
        let border = self.border_at(now);
        {
            let mut mobs = self.sim().mobs.lock().unwrap();
            // Один замок мира на всю фазу: коробки коллизий читаются тысячами раз.
            self.with_world(|w| {
                for (&id, e) in mobs.iter_mut() {
                    // Выгруженный чанк не тикает: иначе исследование мира копит
                    // работу, которую никто не видит.
                    if !awake(e.mob.pos) {
                        continue;
                    }
                    let species = e.mob.species;
                    // Замыкания мира — по два указателя, без аллокаций: они лишь
                    // держат ссылку на `WorldView` этой фазы.
                    let shape = |c: [i32; 3]| self.shape_at(w.block(c), c);
                    let water = |c: [i32; 3]| self.water_at(w.block(c));
                    let world = mb::World {
                        shape: &shape,
                        water: &water,
                    };
                    let mut senses = mb::Senses {
                        // Уже отфильтрованные по виду: моб видит только тех, кто
                        // держит его еду, и строки предметов не сравниваются.
                        players: &food_by_species[species as usize],
                        rand: &mut rand,
                    };
                    let was_grounded = e.mob.on_ground;
                    let old_pos = e.mob.pos;
                    let old_y = e.mob.pos[1];
                    if mb::tick(&mut e.mob, &world, &mut senses) {
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
                    let pulse =
                        e.environment
                            .step(view, self.server_ticks.load(Ordering::Relaxed), false);
                    if old.burning == 0 && e.environment.burning > 0
                        || old.burning > 0 && e.environment.burning == 0
                    {
                        packets.push(inv::set_entity_flags(
                            self.ids.set_entity_data,
                            id,
                            u8::from(e.environment.burning > 0),
                        ));
                    }
                    if old.air != e.environment.air {
                        packets.push(inv::set_entity_air(
                            self.ids.set_entity_data,
                            id,
                            e.environment.air,
                        ));
                    }
                    if old.frozen != e.environment.frozen {
                        packets.push(inv::set_entity_frozen(
                            self.ids.set_entity_data,
                            id,
                            e.environment.frozen as i32,
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
                        if hit && mb::hurt(&mut e.mob, amount, None, 0.0).is_some() {
                            packets.push(play::hurt_animation(self.ids.hurt_animation, id, 0.0));
                            vibrations.push((e.mob.pos, "minecraft:entity_damage"));
                        }
                    }
                    let m = &e.mob;
                    if m.alive() {
                        e.ambient -= 1;
                        if e.ambient <= 0 {
                            e.ambient = 80 + rand::random::<i32>().rem_euclid(120);
                            sounds.push((
                                format!("{}.ambient", species.sound()),
                                m.pos,
                                species.volume(),
                                m.is_baby(),
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
                        packets.push(packet);
                        e.sent = q;
                        (e.sent_rot.0, e.sent_rot.1) = (rot.0, rot.1);
                    }
                    if (rot.2 - e.sent_rot.2).abs() > 1.0 {
                        packets.push(play::rotate_head(self.ids.rotate_head, id, m.head_yaw));
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
        self.hub.send_many(packets);
        for feet in trampling {
            self.trample_farmland(feet);
        }
        for (name, pos, vol, baby) in sounds {
            self.play_sound(
                None,
                &name,
                crate::sounds::NEUTRAL,
                pos,
                vol,
                mob_pitch(baby),
            );
        }
        if !gone.is_empty() {
            let dead: Vec<(i32, MobEntry)> = {
                let mut mobs = self.sim().mobs.lock().unwrap();
                gone.iter()
                    .filter_map(|id| Some((*id, mobs.remove(id)?)))
                    .collect()
            };
            self.hub
                .send(None, play::remove_entities(self.ids.remove_entities, &gone));
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

    /// Вода ли в блоке (`state_fluid`).
    fn water_at(&self, s: Option<u32>) -> bool {
        s.and_then(|s| crate::fluids::state_fluid(&self.survival.placement, s))
            .is_some_and(|f| f.kind == crate::fluids::Kind::Water)
    }

    /// Лут и опыт умершего моба (`LivingEntity.dropAllDeathLoot`).
    fn mob_loot(&self, id: i32, e: &MobEntry) {
        let m = &e.mob;
        if m.is_baby() {
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
        let props = move |k: &str| match k {
            "color" => Some(color.to_owned()),
            "sheared" => Some(sheared.to_string()),
            "on_fire" => Some("false".to_owned()),
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
            1 + rand::random::<i32>().rem_euclid(3)
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
            let r = mb::hurt(&mut e.mob, damage, Some(pos), knock);
            if r == Some(true) {
                e.killer = Some((uuid, slot as i32));
            }
            (r, e.mob.pos, e.mob.species, e.mob.is_baby())
        };
        let died = result?;
        self.emit_vibration(mpos, "minecraft:entity_damage");
        self.hub.send(
            None,
            play::hurt_animation(self.ids.hurt_animation, target, yaw),
        );
        let packets = {
            let mobs = self.sim().mobs.lock().unwrap();
            mobs.get(&target).map(|e| self.mob_packets(target, &e.mob))
        };
        if let Some([_, _, data]) = packets {
            self.hub.send(None, data);
        }
        let sound = format!(
            "{}.{}",
            species.sound(),
            if died { "death" } else { "hurt" }
        );
        self.play_sound(
            None,
            &sound,
            crate::sounds::NEUTRAL,
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
                .send(None, inv::entity_event(self.ids.entity_event, target, 3));
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
                        .send(None, inv::entity_event(self.ids.entity_event, target, 18));
                }
            }
            Act::Shear(pos, color) => {
                self.shear_drops(target, pos, color, crate::sounds::PLAYERS);
                if !creative {
                    let mut s = stack;
                    inv.slots[slot] = (!self.wear_one(&mut s)).then_some(s);
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
        if let Some([_, _, d]) = data {
            self.hub.send(None, d);
        }
    }

    /// Поведение животных поверх ИИ: пары, яйца куриц, овцы щиплют траву,
    /// детёныши взрослеют.
    pub(super) fn tick_animals(&self) {
        let mut eggs = Vec::new();
        let mut grazed = Vec::new();
        let mut grown = Vec::new();
        let mut births = Vec::new();
        let simulated = self.simulated_set();
        {
            let mut mobs = self.sim().mobs.lock().unwrap();
            if mobs.is_empty() {
                return;
            }
            let lovers: Vec<(i32, Species, [f64; 3])> = mobs
                .iter()
                .filter(|(_, e)| e.mob.alive() && e.mob.love > 0 && e.mob.age == 0)
                .map(|(&id, e)| (id, e.mob.species, e.mob.pos))
                .collect();
            let mut paired = HashSet::new();
            for &(a, sa, pa) in &lovers {
                if paired.contains(&a) {
                    continue;
                }
                // `BreedGoal`: ближайший влюблённый того же вида в 8 блоках.
                let Some(&(b, _, pb)) = lovers
                    .iter()
                    .filter(|(b, sb, pb)| {
                        *b != a && *sb == sa && !paired.contains(b) && mb::dist2(pa, *pb) < 64.0
                    })
                    .min_by(|x, y| mb::dist2(pa, x.2).total_cmp(&mb::dist2(pa, y.2)))
                else {
                    continue;
                };
                paired.insert(a);
                paired.insert(b);
                if mb::dist2(pa, pb) < 9.0 {
                    for id in [a, b] {
                        if let Some(e) = mobs.get_mut(&id) {
                            e.mob.love = 0;
                            e.mob.age = 6000;
                        }
                    }
                    let at = [
                        (pa[0] + pb[0]) / 2.0,
                        pa[1].min(pb[1]),
                        (pa[2] + pb[2]) / 2.0,
                    ];
                    births.push((sa, at, a, b));
                } else {
                    for (id, to) in [(a, pb), (b, pa)] {
                        if let Some(e) = mobs.get_mut(&id) {
                            e.mob.goal = mb::Goal::Walk {
                                to,
                                speed: sa.speed(),
                                ticks: 20,
                            };
                            e.mob.look = Some(([to[0], to[1] + 0.5, to[2]], 2));
                        }
                    }
                }
            }
            for (&id, e) in mobs.iter_mut() {
                let m = &mut e.mob;
                if !m.alive() {
                    continue;
                }
                // Вне симуляции животное не стареет, не несётся и не ест.
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
                // `EatBlockGoal`: взрослая 1/1000, детёныш 1/50; жуёт 40 тиков.
                if m.species == Species::Sheep {
                    if m.eating > 0 {
                        m.eating -= 1;
                        if m.eating == 4 {
                            let below = [
                                m.pos[0].floor() as i32,
                                (m.pos[1] - 0.5).floor() as i32,
                                m.pos[2].floor() as i32,
                            ];
                            grazed.push((id, below));
                        }
                    } else if m.on_ground && m.goal == mb::Goal::Idle && m.panic == 0 {
                        let chance = if m.is_baby() { 50 } else { 1000 };
                        if rand::random::<u32>().is_multiple_of(chance) {
                            m.eating = 40;
                        }
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
        let dirt = self.survival.block_defaults.get("minecraft:dirt").copied();
        for (id, below) in grazed {
            let Some(d) = dirt else {
                break;
            };
            if !self.is_grass(below) {
                continue;
            }
            {
                let _edit = self.world_edit.lock().unwrap();
                self.set_block(below, d);
            }
            self.hub
                .send(None, inv::entity_event(self.ids.entity_event, id, 10));
            if let Some(e) = self.sim().mobs.lock().unwrap().get_mut(&id) {
                // `Sheep.ate`: шерсть отрастает, детёныш растёт на минуту.
                e.mob.sheared = false;
                if e.mob.is_baby() {
                    e.mob.age = (e.mob.age + 1200).min(0);
                }
            }
            self.refresh_mob(id);
        }
        for id in grown {
            self.refresh_mob(id);
        }
        for (species, pos, mother, father) in births {
            let yaw = rand::random::<f32>() * 360.0;
            let Some(child) = self.spawn_mob(species, pos, yaw, true, Some("BREEDING")) else {
                continue;
            };
            let mut xp = 1 + (rand::random::<u32>() % 7) as i32;
            let reply = self.sync_event(Event::Breed, || format!("{child} {mother} {father} {xp}"));
            match reply.as_deref().map(str::trim) {
                Some("c") => {
                    self.sim().mobs.lock().unwrap().remove(&child);
                    self.hub.send(
                        None,
                        play::remove_entities(self.ids.remove_entities, &[child]),
                    );
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
    match m.species {
        Species::Sheep => {
            c.insert("Sheared".into(), Tag::Byte(i8::from(m.sheared)));
            c.insert("Color".into(), Tag::Byte(m.color as i8));
        }
        Species::Chicken => {
            c.insert("EggLayTime".into(), Tag::Int(m.egg_time));
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
                self.sim().mobs_loaded.lock().unwrap().push(MobEntry::new(m));
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
        for e in self.sim().mobs.lock().unwrap().values().filter(|e| e.mob.alive()) {
            let pos = (
                (e.mob.pos[0].floor() as i32) >> 4,
                (e.mob.pos[2].floor() as i32) >> 4,
            );
            by_chunk.entry(pos).or_default().push(mob_to_nbt(&e.mob));
        }
        for (pos, c) in self.boats_nbt().into_iter().chain(self.minecarts_nbt()).chain(self.item_frames_nbt()) {
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
                        self.hub
                            .send(None, play::remove_entities(self.ids.remove_entities, &[id]));
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
                            died =
                                mb::hurt(m, num(3).unwrap_or(0.0) as f32, None, 0.0) == Some(true);
                        }
                        Some("pos") => {
                            if let (Some(x), Some(y), Some(z)) = (num(3), num(4), num(5)) {
                                m.pos = [x, y, z];
                                m.vel = [0.0; 3];
                                m.yaw = num(6).unwrap_or(m.yaw as f64) as f32;
                                m.pitch = num(7).unwrap_or(m.pitch as f64) as f32;
                                m.head_yaw = m.yaw;
                                m.goal = mb::Goal::Idle;
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
                    self.hub.send(
                        None,
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
                        .send(None, inv::entity_event(self.ids.entity_event, id, 3));
                }
                self.refresh_mob(id);
                "1".into()
            }
            _ => String::new(),
        }
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
