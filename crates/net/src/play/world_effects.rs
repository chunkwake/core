//! Мировые эффекты Bukkit API: `World.strikeLightning`/`strikeLightningEffect`,
//! `World.createExplosion` и `World.spawnParticle`. Запросы идут через
//! `PlayPlan::world_request`:
//!
//! - `fx lightning <x> <y> <z> <effect>` → `<id>` (id сущности молнии);
//! - `fx explosion <x> <y> <z> <power> <fire> <break-blocks>` → `1`;
//! - `fx particle <name> <x> <y> <z> <count> <dx> <dy> <dz> <speed> <force> [body-hex]`
//!   → `1` (пакет `level_particles` зрителям в текущем измерении).
//!
//! Молния создаётся в заданной точке, без перенацеливания на громоотвод:
//! так ведёт себя `CraftWorld.strikeLightning0`. `effect=1` — только визуал,
//! без урона, огня и удара по мобам.

use super::*;

/// Разобранный `spawnParticle`: тип частицы и параметры пакета.
pub(super) struct ParticleRequest {
    id: i32,
    pos: [f64; 3],
    delta: [f32; 3],
    speed: f32,
    count: i32,
    force: bool,
    /// Тело сложной частицы (`dust`: ARGB + scale, `block`: VarInt состояния).
    body: Option<Vec<u8>>,
}

impl PlayPlan {
    pub(super) fn world_effect_request(&self, line: &str) -> String {
        let f: Vec<&str> = line.split(' ').collect();
        let num = |i: usize| f.get(i).and_then(|v| v.parse::<f64>().ok());
        match f.first().copied() {
            Some("lightning") => {
                let (Some(x), Some(y), Some(z)) = (num(1), num(2), num(3)) else {
                    return String::new();
                };
                let effect = matches!(f.get(4), Some(&"1"));
                self.strike_lightning_api(
                    [x.floor() as i32, y.floor() as i32, z.floor() as i32],
                    effect,
                )
                .map_or_else(String::new, |id| id.to_string())
            }
            Some("explosion") => {
                let (Some(x), Some(y), Some(z), Some(power)) = (num(1), num(2), num(3), num(4))
                else {
                    return "0".into();
                };
                let fire = matches!(f.get(5), Some(&"1"));
                let break_blocks = matches!(f.get(6), Some(&"1"));
                self.explode_api([x, y, z], power as f32, fire, break_blocks);
                "1".into()
            }
            Some("particle") => {
                let mut tokens = line.split_whitespace().skip(1);
                let Some(request) = parse_particle(self, &mut tokens) else {
                    return "0".into();
                };
                if tokens.next().is_some() {
                    return "0".into();
                }
                let packet = self.particle_packet(&request);
                let dim = self.active_dim();
                let viewers: Vec<Uuid> = self
                    .hub
                    .players
                    .lock()
                    .unwrap()
                    .values()
                    .filter(|p| {
                        p.dimension == dim
                            && commands::particle::in_range(p.pos, request.pos, request.force)
                    })
                    .map(|p| p.entry.uuid)
                    .collect();
                for uuid in viewers {
                    self.send_to(uuid, packet.clone());
                }
                "1".into()
            }
            _ => String::new(),
        }
    }

    /// Пакет `level_particles` по разобранному запросу.
    pub(super) fn particle_packet(&self, request: &ParticleRequest) -> Vec<u8> {
        play::level_particles_with(
            self.ids.level_particles,
            request.id,
            request.body.as_deref(),
            request.force,
            request.pos,
            request.delta,
            request.speed,
            request.count,
        )
    }

    /// `World.strikeLightning*`: молния в точку, без перенацеливания на громоотвод.
    fn strike_lightning_api(&self, at: [i32; 3], effect: bool) -> Option<i32> {
        let id = self.next_entity_id.fetch_add(1, Ordering::Relaxed);
        let pos = [at[0] as f64 + 0.5, at[1] as f64, at[2] as f64 + 0.5];
        self.hub.send(
            None,
            play::add_entity_moving(
                self.ids.add_entity,
                id,
                Uuid::from_u128(rand::random()),
                self.survival.lightning_entity_type,
                pos,
                [0.0; 3],
                0.0,
                0.0,
                0,
            ),
        );
        self.sim()
            .lightning
            .lock()
            .unwrap()
            .push((id, self.game_time.load(Ordering::Relaxed) + 2));
        self.mark_work();
        self.play_sound(
            None,
            "minecraft:entity.lightning_bolt.thunder",
            3,
            pos,
            10_000.0,
            0.8,
        );
        self.play_sound(
            None,
            "minecraft:entity.lightning_bolt.impact",
            3,
            pos,
            2.0,
            0.8,
        );
        if !effect {
            let players: Vec<_> = self
                .hub
                .players
                .lock()
                .unwrap()
                .values()
                .filter(|p| p.collects)
                .map(|p| (p.entry.uuid, p.pos))
                .collect();
            for (uuid, feet) in players {
                let dx = feet[0] - pos[0];
                let dz = feet[2] - pos[2];
                if dx * dx + dz * dz <= 9.0 && (feet[1] - pos[1]).abs() <= 6.0 {
                    self.push_control(uuid, Control::Hurt(5.0, Cause::Fire, Some(pos), 0.0));
                }
            }
            self.lightning_mobs(pos);
            self.ignite_at(at, 0);
        }
        Some(id)
    }

    /// `World.createExplosion`: без Bukkit-событий пока; `break_blocks=false` сохраняет блоки.
    fn explode_api(&self, center: [f64; 3], power: f32, fire: bool, break_blocks: bool) {
        self.explode_full(center, power, false, fire, None, None, break_blocks, None);
    }
}

/// `particle <name> <x> <y> <z> <count> <dx> <dy> <dz> <speed> <force> [body-hex]`.
pub(super) fn parse_particle<'a>(
    plan: &PlayPlan,
    tokens: &mut impl Iterator<Item = &'a str>,
) -> Option<ParticleRequest> {
    let raw = tokens.next()?;
    let name = if raw.contains(':') {
        raw.to_owned()
    } else {
        format!("minecraft:{raw}")
    };
    let Some(id) = plan.particle_id(&name) else {
        tracing::debug!(particle = %name, "spawnParticle: unknown particle");
        return None;
    };
    let mut number = || tokens.next()?.parse::<f64>().ok().filter(|v| v.is_finite());
    let pos = [number()?, number()?, number()?];
    let count = tokens.next()?.parse::<i32>().ok().filter(|v| *v >= 0)?;
    let mut offset = || tokens.next()?.parse::<f32>().ok().filter(|v| v.is_finite());
    let delta = [offset()?, offset()?, offset()?];
    let speed = tokens
        .next()?
        .parse::<f32>()
        .ok()
        .filter(|v| v.is_finite() && *v >= 0.0)?;
    let force = match tokens.next()? {
        "1" | "true" | "force" => true,
        "0" | "false" | "normal" => false,
        _ => return None,
    };
    let body = match tokens.next() {
        Some(hex) => Some(decode_hex(hex)?),
        None => None,
    };
    Some(ParticleRequest {
        id,
        pos,
        delta,
        speed,
        count,
        force,
        body,
    })
}

/// Тело частицы из hex-строки (байты уже в порядке сетевого кодека).
fn decode_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || text.len() > 4096 {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() / 2);
    for pair in text.as_bytes().as_chunks::<2>().0 {
        let pair = std::str::from_utf8(pair).ok()?;
        out.push(u8::from_str_radix(pair, 16).ok()?);
    }
    Some(out)
}
