//! `/particle`: простые частицы (`SimpleParticleType`) пакетом `level_particles`.
//!
//! У `dust`, `block`, `item` и остальных типов с телом пакета данные не читаются:
//! команда отвечает `particle.invalidOptions` и пакет не шлёт.

use super::Context;
use super::args::{self, Basis};
use super::dispatch::{Call, Outcome, translate};
use super::entity::usage;
use super::selector::Target;
use crate::play::PlayPlan;
use serde_json::{Value, json};

/// Типы, у которых после id в пакете есть свои поля.
///
/// `dust` поддержан отдельно: у него три канала цвета и масштаб.
const COMPLEX: &[&str] = &[
    "minecraft:block",
    "minecraft:block_crumble",
    "minecraft:block_marker",
    "minecraft:dragon_breath",
    "minecraft:dust_color_transition",
    "minecraft:dust_pillar",
    "minecraft:effect",
    "minecraft:entity_effect",
    "minecraft:falling_dust",
    "minecraft:flash",
    "minecraft:geyser",
    "minecraft:geyser_base",
    "minecraft:geyser_plume",
    "minecraft:geyser_poof",
    "minecraft:instant_effect",
    "minecraft:item",
    "minecraft:sculk_charge",
    "minecraft:shriek",
    "minecraft:tinted_leaves",
    "minecraft:trail",
    "minecraft:vibration",
];

/// Ближе 32 блоков частица видна всегда, с `force` — до 512.
const NEAR: f64 = 32.0 * 32.0;
const FAR: f64 = 512.0 * 512.0;

impl PlayPlan {
    pub(crate) fn particle_id(&self, name: &str) -> Option<i32> {
        self.particle_types.lock().unwrap().get(name).copied()
    }
}

/// `/particle <name> [pos] [delta speed count] [force|normal] [viewers]`.
pub(super) fn particle(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(token) = input.word() else {
        return usage("/particle <name>");
    };
    let name = args::resource_name(token);
    let Some(id) = ctx.plan.particle_id(&name) else {
        return failed("particle.notFound", vec![Value::String(name)]);
    };
    // Тело частицы: у `dust` это цвет и масштаб, остальные с телом пока не читаются.
    let body = match name.as_str() {
        "minecraft:dust" => match dust_options(&mut input) {
            Ok(Some(body)) => Some(body),
            Ok(None) => return usage("/particle minecraft:dust <r> <g> <b> <scale> [pos] …"),
            Err(outcome) => return outcome,
        },
        other if COMPLEX.contains(&other) => {
            return failed("particle.invalidOptions", vec![Value::String(name)]);
        }
        _ => None,
    };
    let (origin, yaw, pitch) = look(ctx, call);
    let basis = Basis::from_rotation(yaw, pitch);
    let mut pos = origin;
    let mut delta = [0.0f32; 3];
    let mut speed = 0.0f32;
    let mut count = 0i32;
    if peek_coord(&input) {
        let Some(at) = read_vec(&mut input, origin, basis) else {
            return usage("/particle <name> <pos>");
        };
        pos = at;
        if peek_coord(&input) {
            let Some(spread) = read_vec(&mut input, origin, basis) else {
                return usage("/particle <name> <pos> <delta> <speed> <count>");
            };
            delta = [spread[0] as f32, spread[1] as f32, spread[2] as f32];
            let Some(speed_token) = input.word() else {
                return usage("/particle <name> <pos> <delta> <speed> <count>");
            };
            speed = match speed_token.parse::<f32>() {
                Ok(speed) if speed >= 0.0 && speed.is_finite() => speed,
                _ => {
                    return failed(
                        "argument.float.low",
                        vec![json!(0), json!({"text": speed_token})],
                    );
                }
            };
            let Some(count_token) = input.word() else {
                return usage("/particle <name> <pos> <delta> <speed> <count>");
            };
            count = match count_token.parse::<i32>() {
                Ok(count) if count >= 0 => count,
                _ => {
                    return failed(
                        "argument.integer.low",
                        vec![json!(0), json!({"text": count_token})],
                    );
                }
            };
        } else if !input.is_empty() {
            return usage("/particle <name> <pos> <delta> <speed> <count>");
        }
    } else if !input.is_empty() {
        return usage("/particle <name> [pos]");
    }
    let mut force = false;
    if !input.is_empty() {
        force = match input.word() {
            Some("force") => true,
            Some("normal") => false,
            _ => {
                return usage("/particle <name> <pos> <delta> <speed> <count> <force|normal>");
            }
        };
    }
    let viewers = if input.is_empty() {
        ctx.targets()
            .into_iter()
            .filter(|target| target.game_mode.is_some())
            .collect()
    } else {
        let Some(token) = input.word() else {
            return usage("/particle <name> <pos> <delta> <speed> <count> <mode> <viewers>");
        };
        if !input.is_empty() {
            return usage("/particle <name> <pos> <delta> <speed> <count> <mode> <viewers>");
        }
        match players(ctx, call, token) {
            Ok(list) => list,
            Err(outcome) => return outcome,
        }
    };
    let packet = chunkwake_protocol::play::level_particles_with(
        ctx.plan.ids.level_particles,
        id,
        body.as_deref(),
        force,
        pos,
        delta,
        speed,
        count,
    );
    let mut sent = 0;
    for player in &viewers {
        if in_range(player.pos, pos, force) {
            ctx.plan.send_to(player.uuid, packet.clone());
            sent += 1;
        }
    }
    if sent == 0 {
        return failed("commands.particle.failed", Vec::new());
    }
    Outcome::Feedback(translate(
        "commands.particle.success",
        vec![Value::String(name)],
    ))
}

/// Тело `dust`: три канала 0..1 и масштаб.
///
/// `Ok(None)` — числа не прочитались (это уже позиция), `Err` — вне диапазона.
fn dust_options(input: &mut args::Input<'_>) -> Result<Option<Vec<u8>>, Outcome> {
    let mut channels = [0u8; 3];
    for (index, channel) in channels.iter_mut().enumerate() {
        let Some(token) = input.peek_word() else {
            return Ok(None);
        };
        let Ok(value) = token.parse::<f32>() else {
            return Ok(None);
        };
        if !(0.0..=1.0).contains(&value) || !value.is_finite() {
            return Err(failed(
                "argument.float.range",
                vec![json!(0.0), json!(1.0), json!({"text": token}), json!(index)],
            ));
        }
        input.word();
        *channel = (value * 255.0).round() as u8;
    }
    let Some(scale_token) = input.word() else {
        return Ok(None);
    };
    let Ok(scale) = scale_token.parse::<f32>() else {
        return Ok(None);
    };
    if !scale.is_finite() || scale <= 0.0 {
        return Err(failed(
            "argument.float.low",
            vec![json!(0), json!({"text": scale_token})],
        ));
    }
    // `ARGB.color`: непрозрачный цвет из трёх каналов.
    let argb = 0xFF00_0000u32
        | (u32::from(channels[0]) << 16)
        | (u32::from(channels[1]) << 8)
        | u32::from(channels[2]);
    Ok(Some(chunkwake_protocol::play::dust_body(
        argb as i32,
        scale,
    )))
}

fn peek_coord(input: &args::Input<'_>) -> bool {
    input
        .rest()
        .split_whitespace()
        .next()
        .is_some_and(args::is_coord)
}

fn read_vec(input: &mut args::Input<'_>, base: [f64; 3], basis: Basis) -> Option<[f64; 3]> {
    let x = input.word()?;
    let y = input.word()?;
    let z = input.word()?;
    let coords = args::coords3(x, y, z)?;
    Some(basis.resolve(coords, base))
}

fn look(ctx: &Context<'_>, call: &Call<'_>) -> ([f64; 3], f32, f32) {
    if let Some(origin) = ctx.execution_origin() {
        return origin;
    }
    if let Some(uuid) = call.source.uuid
        && let Some(pose) = ctx.pose(uuid)
    {
        return pose;
    }
    (ctx.spawn(), 0.0, 0.0)
}

fn in_range(player: [f64; 3], at: [f64; 3], force: bool) -> bool {
    let dx = player[0] - at[0];
    let dy = player[1] - at[1];
    let dz = player[2] - at[2];
    let distance = dx * dx + dy * dy + dz * dz;
    distance < NEAR || (force && distance < FAR)
}

fn players(ctx: &Context<'_>, call: &Call<'_>, token: &str) -> Result<Vec<Target>, Outcome> {
    let targets = match ctx.select(call.source, token, false) {
        Ok(list) => list,
        Err(error) => return Err(Outcome::Feedback(error.component())),
    };
    let players: Vec<Target> = targets
        .into_iter()
        .filter(|target| target.game_mode.is_some())
        .collect();
    if players.is_empty() {
        return Err(failed("argument.entity.notfound.player", Vec::new()));
    }
    Ok(players)
}

fn failed(key: &str, with: Vec<Value>) -> Outcome {
    let mut value = translate(key, with);
    if let Some(object) = value.as_object_mut() {
        object.insert("color".into(), json!("red"));
    }
    Outcome::Feedback(value)
}

pub(super) fn specs() -> Vec<super::dispatch::Spec> {
    use super::dispatch::Spec;
    use super::registry::{GAMEMASTERS, ent, float, int, vec3};
    use chunkwake_protocol::text::Parser;
    let viewers = || Spec::arg_leaf("viewers", GAMEMASTERS, ent(true, false), particle);
    vec![
        Spec::group(
            "particle",
            GAMEMASTERS,
            vec![
                Spec::arg_leaf("name", GAMEMASTERS, Parser::Particle, particle).with_children(
                    vec![
                        Spec::arg_leaf("pos", GAMEMASTERS, vec3(), particle).with_children(vec![
                            Spec::arg(
                                "delta",
                                GAMEMASTERS,
                                vec3(),
                                vec![Spec::arg(
                                    "speed",
                                    GAMEMASTERS,
                                    float(0.0, f32::MAX as f64),
                                    vec![
                                        Spec::arg_leaf(
                                            "count",
                                            GAMEMASTERS,
                                            int(0, i32::MAX),
                                            particle,
                                        )
                                        .with_children(
                                            vec![
                                                Spec::exec(
                                                    "force",
                                                    GAMEMASTERS,
                                                    particle,
                                                    vec![viewers()],
                                                ),
                                                Spec::exec(
                                                    "normal",
                                                    GAMEMASTERS,
                                                    particle,
                                                    vec![viewers()],
                                                ),
                                            ],
                                        ),
                                    ],
                                )],
                            ),
                        ]),
                    ],
                ),
            ],
        )
        .with_handler(particle),
    ]
}
