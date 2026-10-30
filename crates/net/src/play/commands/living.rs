//! `/tag`, `/swing`, `/spectate`, `/ride`, `/trigger`, `/random`.
//!
//! Это команды про сущности и счёт, которым хватает уже записанного мира:
//! метки селектора, взмах, камера, пассажиры, trigger-счёт и бросок диапазона.

use super::Context;
use super::args;
use super::dispatch::{Call, Outcome, translate};
use super::entity::usage;
use super::selector::Target;
use serde_json::{Value, json};
use uuid::Uuid;

/// `/tag <targets> <add|remove|list> [name]`.
pub(super) fn tag(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(token) = input.word() else {
        return usage("/tag <targets> <add|remove|list> [name]");
    };
    let targets = match ctx.select(call.source, token, true) {
        Ok(list) => list,
        Err(error) => return Outcome::Feedback(error.component()),
    };
    match input.word() {
        Some("list") => list_tags(&targets),
        Some("add") => {
            let Some(name) = input.word() else {
                return usage("/tag <targets> add <name>");
            };
            let changed: Vec<&Target> = targets
                .iter()
                .filter(|target| ctx.plan.add_tag(target.uuid, name))
                .collect();
            if changed.is_empty() {
                return Outcome::Feedback(translate("commands.tag.add.failed", Vec::new()));
            }
            tag_result(
                "commands.tag.add.success.single",
                "commands.tag.add.success.multiple",
                &changed,
                name,
            )
        }
        Some("remove") => {
            let Some(name) = input.word() else {
                return usage("/tag <targets> remove <name>");
            };
            let changed: Vec<&Target> = targets
                .iter()
                .filter(|target| ctx.plan.remove_tag(target.uuid, name))
                .collect();
            if changed.is_empty() {
                return Outcome::Feedback(translate("commands.tag.remove.failed", Vec::new()));
            }
            tag_result(
                "commands.tag.remove.success.single",
                "commands.tag.remove.success.multiple",
                &changed,
                name,
            )
        }
        _ => usage("/tag <targets> <add|remove|list> [name]"),
    }
}

fn list_tags(targets: &[Target]) -> Outcome {
    if targets.len() == 1 {
        let tags = ctx_tags(&targets[0]);
        if tags.is_empty() {
            return Outcome::Feedback(translate(
                "commands.tag.list.single.empty",
                vec![name_of(&targets[0])],
            ));
        }
        return Outcome::Feedback(translate(
            "commands.tag.list.single.success",
            vec![name_of(&targets[0]), json!({"text": tags.join(", ")})],
        ));
    }
    let any = targets.iter().any(|target| !ctx_tags(target).is_empty());
    if !any {
        return Outcome::Feedback(translate(
            "commands.tag.list.multiple.empty",
            vec![Value::from(targets.len() as i32)],
        ));
    }
    Outcome::Feedback(translate(
        "commands.tag.list.multiple.success",
        vec![Value::from(targets.len() as i32)],
    ))
}

fn ctx_tags(target: &Target) -> Vec<String> {
    target.tags.clone()
}

fn tag_result(single: &str, many: &str, targets: &[&Target], name: &str) -> Outcome {
    if targets.len() == 1 {
        Outcome::Feedback(translate(
            single,
            vec![json!({"text": name}), name_of(targets[0])],
        ))
    } else {
        Outcome::Feedback(translate(
            many,
            vec![json!({"text": name}), Value::from(targets.len() as i32)],
        ))
    }
}

/// `/swing [targets] [mainhand|offhand] [animation] [duration]`.
pub(super) fn swing(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let targets = if input.rest().split_whitespace().next().is_some()
        && !matches!(
            input.rest().split_whitespace().next(),
            Some("mainhand" | "offhand")
        ) {
        let Some(token) = input.word() else {
            return usage("/swing [targets] [mainhand|offhand]");
        };
        match ctx.select(call.source, token, true) {
            Ok(list) => list,
            Err(error) => return Outcome::Feedback(error.component()),
        }
    } else {
        match self_target(ctx, call) {
            Some(target) => vec![target],
            None => {
                return Outcome::Feedback(translate("commands.swing.failed.notliving", Vec::new()));
            }
        }
    };
    let hand = match input.word() {
        None => 0,
        Some("mainhand") => 0,
        Some("offhand") => 1,
        Some(_) => return usage("/swing [targets] [mainhand|offhand] [animation] [duration]"),
    };
    let kind = match input.word() {
        None => 1,
        Some(token) => match swing_kind(token) {
            Some(kind) => kind,
            None => {
                return Outcome::Feedback(translate(
                    "argument.resource.invalid",
                    vec![Value::String(token.to_owned())],
                ));
            }
        },
    };
    let duration = match input.word() {
        None => 6,
        Some(token) => match args::time_ticks(token).filter(|ticks| *ticks >= 1) {
            Some(ticks) => i32::try_from(ticks).unwrap_or(i32::MAX),
            None => return usage("/swing … [duration]"),
        },
    };
    let living: Vec<&Target> = targets
        .iter()
        .filter(|target| ctx.plan.is_living(target))
        .collect();
    if living.is_empty() {
        return Outcome::Feedback(translate("commands.swing.failed.notliving", Vec::new()));
    }
    for target in &living {
        ctx.plan.swing(target.entity_id, hand, kind, duration);
    }
    if living.len() == 1 {
        Outcome::Feedback(translate(
            "commands.swing.success.single",
            vec![name_of(living[0])],
        ))
    } else {
        Outcome::Feedback(translate(
            "commands.swing.success.multiple",
            vec![Value::from(living.len() as i32)],
        ))
    }
}

fn swing_kind(token: &str) -> Option<i32> {
    Some(match token.strip_prefix("minecraft:").unwrap_or(token) {
        "none" => 0,
        "whack" => 1,
        "stab" => 2,
        _ => return None,
    })
}

/// `/spectate [target] [player]`.
pub(super) fn spectate(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let target = match input.word() {
        None => None,
        Some(token) => match one_entity(ctx, call, token, true) {
            Ok(target) => Some(target),
            Err(outcome) => return outcome,
        },
    };
    let player = match input.word() {
        None => None,
        Some(token) => match one_entity(ctx, call, token, false) {
            Ok(target) => Some(target),
            Err(outcome) => return outcome,
        },
    };
    let Some(spectator) = player.or_else(|| self_target(ctx, call)) else {
        return Outcome::Feedback(translate("commands.spectate.not_spectator", Vec::new()));
    };
    if spectator.game_mode != Some(3) {
        return Outcome::Feedback(translate("commands.spectate.not_spectator", Vec::new()));
    }
    let Some(watch) = target else {
        ctx.plan.set_camera(spectator.uuid, spectator.entity_id);
        return Outcome::Feedback(translate("commands.spectate.success.stopped", Vec::new()));
    };
    if watch.uuid == spectator.uuid {
        return Outcome::Feedback(translate("commands.spectate.self", Vec::new()));
    }
    if !ctx.plan.is_living(&watch) {
        return Outcome::Feedback(translate("commands.spectate.cannot_spectate", Vec::new()));
    }
    ctx.plan.set_camera(spectator.uuid, watch.entity_id);
    Outcome::Feedback(translate(
        "commands.spectate.success.started",
        vec![name_of(&watch), name_of(&spectator)],
    ))
}

/// `/ride <target> <mount <vehicle>|dismount>`.
pub(super) fn ride(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(token) = input.word() else {
        return usage("/ride <target> <mount <vehicle>|dismount>");
    };
    let target = match one_entity(ctx, call, token, true) {
        Ok(target) => target,
        Err(outcome) => return outcome,
    };
    match input.word() {
        Some("dismount") => dismount(ctx, &target),
        Some("mount") => {
            let Some(token) = input.word() else {
                return usage("/ride <target> mount <vehicle>");
            };
            let vehicle = match one_entity(ctx, call, token, true) {
                Ok(target) => target,
                Err(outcome) => return outcome,
            };
            mount(ctx, &target, &vehicle)
        }
        _ => usage("/ride <target> <mount <vehicle>|dismount>"),
    }
}

fn dismount(ctx: &Context<'_>, target: &Target) -> Outcome {
    let Some(vehicle) = ctx.plan.unlink_rider(target.uuid) else {
        return Outcome::Feedback(translate("commands.ride.not_riding", Vec::new()));
    };
    send_vehicle(ctx, vehicle);
    Outcome::Feedback(translate(
        "commands.ride.dismount.success",
        vec![name_of(target)],
    ))
}

fn mount(ctx: &Context<'_>, target: &Target, vehicle: &Target) -> Outcome {
    if vehicle.game_mode.is_some() {
        return Outcome::Feedback(translate(
            "commands.ride.mount.failure.cant_ride_players",
            Vec::new(),
        ));
    }
    if !ctx.plan.is_living(vehicle) {
        return Outcome::Feedback(translate("commands.ride.mount.failure.generic", Vec::new()));
    }
    if rides_toward(ctx, vehicle.uuid, target.uuid) {
        return Outcome::Feedback(translate("commands.ride.mount.failure.loop", Vec::new()));
    }
    if ctx.plan.vehicle_of(target.uuid).is_some() {
        return Outcome::Feedback(translate("commands.ride.already_riding", Vec::new()));
    }
    ctx.plan.link_rider(target.uuid, vehicle.uuid);
    send_vehicle(ctx, vehicle.uuid);
    Outcome::Feedback(translate(
        "commands.ride.mount.success",
        vec![name_of(target), name_of(vehicle)],
    ))
}

/// `true`, если `from`, поднимаясь по транспортам, доходит до `stop`.
fn rides_toward(ctx: &Context<'_>, from: Uuid, stop: Uuid) -> bool {
    if from == stop {
        return true;
    }
    let mut cursor = Some(from);
    for _ in 0..32 {
        let Some(current) = cursor else {
            return false;
        };
        if current == stop {
            return true;
        }
        cursor = ctx.plan.vehicle_of(current);
    }
    false
}

fn send_vehicle(ctx: &Context<'_>, vehicle: Uuid) {
    let passengers = ctx.plan.passengers_of(vehicle);
    let Some(vehicle_target) = ctx.plan.command_target(vehicle) else {
        return;
    };
    let ids: Vec<i32> = passengers
        .iter()
        .filter_map(|uuid| ctx.plan.command_target(*uuid).map(|t| t.entity_id))
        .collect();
    ctx.plan.send_passengers(vehicle_target.entity_id, &ids);
}

/// `/trigger <objective> [add|set <value>]`.
pub(super) fn trigger(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let Some(uuid) = call.source.uuid else {
        return Outcome::Feedback(translate("commands.trigger.failed.invalid", Vec::new()));
    };
    if ctx.pose(uuid).is_none() && ctx.plan.command_target(uuid).is_none() {
        return Outcome::Feedback(translate("commands.trigger.failed.invalid", Vec::new()));
    }
    let mut input = call.t();
    let Some(objective) = input.word() else {
        return usage("/trigger <objective> [add|set <value>]");
    };
    let (add, value, key) = match input.word() {
        None => (true, 1, "commands.trigger.simple.success"),
        Some("add") => {
            let Some(value) = input.word().and_then(|token| token.parse::<i32>().ok()) else {
                return usage("/trigger <objective> add <value>");
            };
            (true, value, "commands.trigger.add.success")
        }
        Some("set") => {
            let Some(value) = input.word().and_then(|token| token.parse::<i32>().ok()) else {
                return usage("/trigger <objective> set <value>");
            };
            (false, value, "commands.trigger.set.success")
        }
        Some(_) => return usage("/trigger <objective> [add|set <value>]"),
    };
    match ctx
        .plan
        .pull_trigger(&call.source.name, objective, value, add)
    {
        Ok(next) => Outcome::Feedback(translate(
            key,
            vec![Value::from(next), json!({"text": objective})],
        )),
        Err(true) => Outcome::Feedback(translate(
            "arguments.objective.notFound",
            vec![json!({"text": objective})],
        )),
        Err(false) => {
            let kind = ctx.plan.scoreboard.lock().unwrap().criterion(objective);
            let key = if matches!(kind, Some(super::scoreboard::Criterion::Trigger) | None) {
                "commands.trigger.failed.unprimed"
            } else {
                "commands.trigger.failed.invalid"
            };
            Outcome::Feedback(translate(key, vec![json!({"text": objective})]))
        }
    }
}

/// `/random <roll|value|reset> …`.
pub(super) fn random(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    match call.path.first().map(String::as_str) {
        Some("roll") => roll(ctx, &mut input, true),
        Some("value") => roll(ctx, &mut input, false),
        Some("reset") => reset_random(ctx, call, &mut input),
        _ => usage("/random <roll|value <range>|reset>"),
    }
}

fn roll(ctx: &Context<'_>, input: &mut args::Input<'_>, announce: bool) -> Outcome {
    let Some(token) = input.word() else {
        return usage("/random <roll|value> <range> [sequence]");
    };
    let (lo, hi) = match int_span(token) {
        Ok(span) => span,
        Err(true) => {
            return Outcome::Feedback(translate(
                "commands.random.error.range_too_large",
                Vec::new(),
            ));
        }
        Err(false) => {
            return Outcome::Feedback(translate(
                "commands.random.error.range_too_small",
                Vec::new(),
            ));
        }
    };
    let sequence = input.word().map(args::resource_name);
    let value = ctx.plan.random_int(sequence.as_deref(), lo, hi);
    let key = if announce {
        "commands.random.roll"
    } else {
        "commands.random.sample.success"
    };
    let outcome = Outcome::Feedback(translate(
        key,
        vec![Value::from(value), Value::from(lo), Value::from(hi)],
    ));
    if announce {
        Outcome::Broadcast(match outcome {
            Outcome::Feedback(value) => value,
            other => return other,
        })
    } else {
        outcome
    }
}

fn reset_random(ctx: &Context<'_>, call: &Call<'_>, input: &mut args::Input<'_>) -> Outcome {
    let all = call.path.get(1).map(String::as_str) == Some("*");
    let name = if all {
        None
    } else {
        let Some(name) = input.word() else {
            return usage("/random reset <sequence|*> [seed]");
        };
        Some(args::resource_name(name))
    };
    let salt = match input.word() {
        None => 0,
        Some(token) => match token.parse::<i32>() {
            Ok(value) => i64::from(value),
            Err(_) => return usage("/random reset <sequence|*> [seed]"),
        },
    };
    let include_world = match input.word() {
        None => true,
        Some("true") => true,
        Some("false") => false,
        Some(_) => return usage("/random reset … [includeWorldSeed] [includeSequenceId]"),
    };
    let include_id = match input.word() {
        None => true,
        Some("true") => true,
        Some("false") => false,
        Some(_) => return usage("/random reset … [includeWorldSeed] [includeSequenceId]"),
    };
    ctx.plan
        .reset_random(name.as_deref(), salt, include_world, include_id);
    match name {
        None => Outcome::Feedback(translate("commands.random.reset.all.success", Vec::new())),
        Some(name) => Outcome::Feedback(translate(
            "commands.random.reset.success",
            vec![Value::String(name)],
        )),
    }
}

/// `Ok` — включительные границы. `Err(true)` — диапазон слишком велик, `Err(false)` — пуст.
fn int_span(token: &str) -> Result<(i32, i32), bool> {
    let Some((lo, hi)) = token.split_once("..") else {
        let value = token.parse::<i32>().map_err(|_| false)?;
        return Ok((value, value));
    };
    let lo = if lo.is_empty() {
        None
    } else {
        Some(lo.parse::<i32>().map_err(|_| false)?)
    };
    let hi = if hi.is_empty() {
        None
    } else {
        Some(hi.parse::<i32>().map_err(|_| false)?)
    };
    match (lo, hi) {
        (Some(lo), Some(hi)) if lo > hi => Err(false),
        (Some(lo), Some(hi)) => Ok((lo, hi)),
        _ => Err(true),
    }
}

fn one_entity(
    ctx: &Context<'_>,
    call: &Call<'_>,
    token: &str,
    allow_entities: bool,
) -> Result<Target, Outcome> {
    match ctx.select(call.source, token, allow_entities) {
        Ok(list) if list.len() == 1 => Ok(list.into_iter().next().unwrap()),
        Ok(_) => Err(Outcome::Feedback(translate(
            "argument.entity.toomany",
            Vec::new(),
        ))),
        Err(error) => Err(Outcome::Feedback(error.component())),
    }
}

fn self_target(ctx: &Context<'_>, call: &Call<'_>) -> Option<Target> {
    let uuid = call.source.uuid?;
    ctx.plan
        .command_targets()
        .into_iter()
        .find(|target| target.uuid == uuid)
}

fn name_of(target: &Target) -> Value {
    json!({"text": target.name})
}

/// Узлы, сверенные с `commands.json` 26.3.
pub(super) fn specs() -> Vec<super::dispatch::Spec> {
    use super::dispatch::Spec;
    use super::registry::{
        EVERYONE, GAMEMASTERS, boolean, ent, int, int_range, objective, resource_location, time,
        word,
    };
    use chunkwake_protocol::text::Parser;

    let hand = |name: &str| {
        Spec::exec(
            name,
            GAMEMASTERS,
            swing,
            vec![
                Spec::arg_leaf(
                    "animation",
                    GAMEMASTERS,
                    Parser::Raw("minecraft:swing_animation"),
                    swing,
                )
                .with_children(vec![Spec::arg_leaf(
                    "duration",
                    GAMEMASTERS,
                    time(1),
                    swing,
                )]),
            ],
        )
    };
    let range =
        |handler: super::dispatch::Handler| {
            Spec::arg_leaf("range", EVERYONE, int_range(), handler).with_children(vec![
                Spec::arg_leaf("sequence", EVERYONE, resource_location(), handler),
            ])
        };
    let reset_seed = || {
        Spec::arg_leaf("seed", EVERYONE, int(i32::MIN, i32::MAX), random).with_children(vec![
            Spec::arg_leaf("includeWorldSeed", EVERYONE, boolean(), random).with_children(vec![
                Spec::arg_leaf("includeSequenceId", EVERYONE, boolean(), random),
            ]),
        ])
    };
    vec![
        Spec::group(
            "tag",
            GAMEMASTERS,
            vec![Spec::arg(
                "targets",
                GAMEMASTERS,
                ent(false, false),
                vec![
                    Spec::group(
                        "add",
                        GAMEMASTERS,
                        vec![Spec::arg_leaf("name", GAMEMASTERS, word(), tag)],
                    ),
                    Spec::literal_leaf("list", GAMEMASTERS, tag),
                    Spec::group(
                        "remove",
                        GAMEMASTERS,
                        vec![Spec::arg_leaf("name", GAMEMASTERS, word(), tag)],
                    ),
                ],
            )],
        )
        .with_handler(tag),
        Spec::exec(
            "swing",
            GAMEMASTERS,
            swing,
            vec![
                Spec::arg_leaf("targets", GAMEMASTERS, ent(false, false), swing)
                    .with_children(vec![hand("mainhand"), hand("offhand")]),
            ],
        ),
        Spec::exec(
            "spectate",
            GAMEMASTERS,
            spectate,
            vec![
                Spec::arg_leaf("target", GAMEMASTERS, ent(false, true), spectate).with_children(
                    vec![Spec::arg_leaf(
                        "player",
                        GAMEMASTERS,
                        ent(true, true),
                        spectate,
                    )],
                ),
            ],
        ),
        Spec::group(
            "ride",
            GAMEMASTERS,
            vec![Spec::arg(
                "target",
                GAMEMASTERS,
                ent(false, true),
                vec![
                    Spec::literal_leaf("dismount", GAMEMASTERS, ride),
                    Spec::group(
                        "mount",
                        GAMEMASTERS,
                        vec![Spec::arg_leaf(
                            "vehicle",
                            GAMEMASTERS,
                            ent(false, true),
                            ride,
                        )],
                    ),
                ],
            )],
        )
        .with_handler(ride),
        Spec::group(
            "trigger",
            EVERYONE,
            vec![
                Spec::arg_leaf("objective", EVERYONE, objective(), trigger).with_children(vec![
                    Spec::group(
                        "add",
                        EVERYONE,
                        vec![Spec::arg_leaf(
                            "value",
                            EVERYONE,
                            int(i32::MIN, i32::MAX),
                            trigger,
                        )],
                    ),
                    Spec::group(
                        "set",
                        EVERYONE,
                        vec![Spec::arg_leaf(
                            "value",
                            EVERYONE,
                            int(i32::MIN, i32::MAX),
                            trigger,
                        )],
                    ),
                ]),
            ],
        )
        .with_handler(trigger),
        Spec::group(
            "random",
            EVERYONE,
            vec![
                Spec::group(
                    "reset",
                    EVERYONE,
                    vec![
                        Spec::exec("*", EVERYONE, random, vec![reset_seed()]),
                        Spec::arg_leaf("sequence", EVERYONE, resource_location(), random)
                            .with_children(vec![reset_seed()]),
                    ],
                ),
                Spec::group("roll", EVERYONE, vec![range(random)]),
                Spec::group("value", EVERYONE, vec![range(random)]),
            ],
        )
        .with_handler(random),
    ]
}
