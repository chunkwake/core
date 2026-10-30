//! `/attribute`: база и модификаторы живых сущностей.
//!
//! Игрок получает набор `Player.createAttributes`, корова и остальные мобы —
//! `Mob.createMobAttributes` с их `movement_speed` и `max_health`.
//! Пакет `update_attributes` забирает ту же базу, поэтому `/attribute base set`
//! не затирается следующей отправкой скорости.

use super::Context;
use super::args;
use super::dispatch::{Call, Outcome, translate};
use super::entity::usage;
use super::selector::{Kind, Target};
use crate::mobs::Species;
use crate::play::PlayPlan;
use crate::survival::{self, OFFHAND};
use chunkwake_protocol::inventory::update_attribute;
use serde_json::{Value, json};
use uuid::Uuid;

/// Правка атрибута командой. `None` у базы — значение сущности по умолчанию.
#[derive(Clone, Debug, Default)]
pub struct AttrState {
    pub base: Option<f64>,
    pub modifiers: Vec<AttrMod>,
}

/// Модификатор: id, величина и операция (`add_value` = 0, база = 1, итог = 2).
#[derive(Clone, Debug)]
pub struct AttrMod {
    pub id: String,
    pub amount: f64,
    pub op: i32,
}

struct Bounds {
    base: f64,
    min: f64,
    max: f64,
}

impl PlayPlan {
    /// База для пакета: правка команды или запасное значение сущности.
    pub(crate) fn attribute_base(&self, uuid: Uuid, name: &str, fallback: f64) -> f64 {
        self.entity_attributes
            .lock()
            .unwrap()
            .get(&uuid)
            .and_then(|map| map.get(name))
            .and_then(|state| state.base)
            .unwrap_or(fallback)
    }

    /// Модификаторы команды после уже собранных (эффекты, чары).
    pub(crate) fn merge_attribute_modifiers(
        &self,
        uuid: Uuid,
        name: &str,
        leading: &[(String, f64, i32)],
    ) -> Vec<(String, f64, i32)> {
        let mut out = leading.to_vec();
        let stored = self.entity_attributes.lock().unwrap();
        let Some(list) = stored.get(&uuid).and_then(|map| map.get(name)) else {
            return out;
        };
        for modifier in &list.modifiers {
            if out.iter().any(|(id, _, _)| id == &modifier.id) {
                continue;
            }
            out.push((modifier.id.clone(), modifier.amount, modifier.op));
        }
        out
    }

    /// Чары надетого набора для одного атрибута.
    pub(crate) fn enchant_attribute_modifiers(
        &self,
        uuid: Uuid,
        name: &str,
    ) -> Vec<(String, f64, i32)> {
        let equipment = {
            let live = self.live.lock().unwrap();
            let Some(player) = live.get(&uuid) else {
                return Vec::new();
            };
            let list = |slot: usize| {
                player
                    .slots
                    .get(slot)
                    .and_then(|stack| *stack)
                    .map(|stack| {
                        chunkwake_protocol::components::get(stack.extra)
                            .enchantments
                            .clone()
                    })
                    .unwrap_or_default()
            };
            let held = 36 + player.selected.min(8);
            [
                ("mainhand", list(held)),
                ("offhand", list(OFFHAND)),
                ("head", list(5)),
                ("chest", list(6)),
                ("legs", list(7)),
                ("feet", list(8)),
            ]
        };
        let view: Vec<(&str, &[(i32, i32)])> = equipment
            .iter()
            .map(|(slot, list)| (*slot, list.as_slice()))
            .collect();
        crate::enchanting::attribute_modifiers(&self.survival.enchanting, &view)
            .remove(name)
            .unwrap_or_default()
    }
}

/// `/attribute`.
pub(super) fn attribute(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(who) = input.word() else {
        return usage("/attribute <target> <attribute> <get|base|modifier>");
    };
    let Some(attribute_name) = input.word() else {
        return usage("/attribute <target> <attribute> <get|base|modifier>");
    };
    let Ok(selector) = super::selector::Selector::parse(who) else {
        return failed("argument.entity.invalid", Vec::new());
    };
    let single = selector.expects_single() || selector.kind == Kind::Self_;
    if !single {
        return failed("argument.entity.toomany", Vec::new());
    }
    let targets = match ctx.select(call.source, who, true) {
        Ok(list) => list,
        Err(error) => return Outcome::Feedback(error.component()),
    };
    let Some(target) = targets.first() else {
        return failed("argument.entity.notfound.entity", Vec::new());
    };
    if targets.len() != 1 {
        return failed("argument.entity.toomany", Vec::new());
    }
    let name = match location(attribute_name) {
        Some(name) => name,
        None => return invalid_resource(attribute_name),
    };
    if !ctx.plan.survival.attributes.contains_key(&name) {
        return invalid_resource(&name);
    }
    let Some(bounds) = bounds_of(target, &name) else {
        if profile(target).is_none() {
            return failed(
                "commands.attribute.failed.entity",
                vec![entity_name(target)],
            );
        }
        return failed(
            "commands.attribute.failed.no_attribute",
            vec![entity_name(target), json!({"text": name})],
        );
    };
    match input.word() {
        Some("get") => get_value(ctx, target, &name, &bounds, &mut input, false),
        Some("base") => base_command(ctx, target, &name, &bounds, &mut input),
        Some("modifier") => modifier_command(ctx, target, &name, &bounds, &mut input),
        _ => usage("/attribute <target> <attribute> <get|base|modifier>"),
    }
}

fn base_command(
    ctx: &Context<'_>,
    target: &Target,
    name: &str,
    bounds: &Bounds,
    input: &mut args::Input<'_>,
) -> Outcome {
    match input.word() {
        Some("get") => get_value(ctx, target, name, bounds, input, true),
        Some("set") => set_base(ctx, target, name, bounds, input),
        Some("reset") => reset_base(ctx, target, name, bounds),
        _ => usage("/attribute <target> <attribute> base <get|set|reset>"),
    }
}

fn modifier_command(
    ctx: &Context<'_>,
    target: &Target,
    name: &str,
    bounds: &Bounds,
    input: &mut args::Input<'_>,
) -> Outcome {
    match input.word() {
        Some("add") => add_modifier(ctx, target, name, bounds, input),
        Some("remove") => remove_modifier(ctx, target, name, input),
        Some("value") => match input.word() {
            Some("get") => modifier_value(ctx, target, name, input),
            _ => usage("/attribute <target> <attribute> modifier value get <id>"),
        },
        _ => usage("/attribute <target> <attribute> modifier <add|remove|value>"),
    }
}

fn get_value(
    ctx: &Context<'_>,
    target: &Target,
    name: &str,
    bounds: &Bounds,
    input: &mut args::Input<'_>,
    base_only: bool,
) -> Outcome {
    let Some(scale) = scale_of(input) else {
        return usage("/attribute <target> <attribute> get [scale]");
    };
    let base = ctx.plan.attribute_base(target.uuid, name, bounds.base);
    let number = if base_only {
        base.clamp(bounds.min, bounds.max)
    } else {
        let mods = visible_modifiers(ctx.plan, target, name);
        combine(base, &mods, bounds)
    };
    let shown = number * scale;
    let key = if base_only {
        "commands.attribute.base_value.get.success"
    } else {
        "commands.attribute.value.get.success"
    };
    Outcome::Feedback(translate(
        key,
        vec![
            json!({"text": name}),
            entity_name(target),
            Value::from(shown),
        ],
    ))
}

fn set_base(
    ctx: &Context<'_>,
    target: &Target,
    name: &str,
    bounds: &Bounds,
    input: &mut args::Input<'_>,
) -> Outcome {
    let Some(token) = input.word() else {
        return usage("/attribute <target> <attribute> base set <value>");
    };
    let Some(value) = token.parse::<f64>().ok() else {
        return usage("/attribute <target> <attribute> base set <value>");
    };
    if input.word().is_some() {
        return usage("/attribute <target> <attribute> base set <value>");
    }
    let value = value.clamp(bounds.min, bounds.max);
    edit(ctx.plan, target.uuid, name, |state| {
        state.base = Some(value)
    });
    publish(ctx.plan, target, name, bounds);
    Outcome::Feedback(translate(
        "commands.attribute.base_value.set.success",
        vec![
            json!({"text": name}),
            entity_name(target),
            Value::from(value),
        ],
    ))
}

fn reset_base(ctx: &Context<'_>, target: &Target, name: &str, bounds: &Bounds) -> Outcome {
    edit(ctx.plan, target.uuid, name, |state| state.base = None);
    publish(ctx.plan, target, name, bounds);
    Outcome::Feedback(translate(
        "commands.attribute.base_value.reset.success",
        vec![
            json!({"text": name}),
            entity_name(target),
            Value::from(bounds.base),
        ],
    ))
}

fn add_modifier(
    ctx: &Context<'_>,
    target: &Target,
    name: &str,
    bounds: &Bounds,
    input: &mut args::Input<'_>,
) -> Outcome {
    let (Some(id_token), Some(amount_token), Some(op_token)) =
        (input.word(), input.word(), input.word())
    else {
        return usage(
            "/attribute <target> <attribute> modifier add <id> <value> <add_value|add_multiplied_base|add_multiplied_total>",
        );
    };
    let Some(id) = location(id_token) else {
        return invalid_resource(id_token);
    };
    let Some(amount) = amount_token.parse::<f64>().ok() else {
        return usage(
            "/attribute <target> <attribute> modifier add <id> <value> <add_value|add_multiplied_base|add_multiplied_total>",
        );
    };
    let Some(op) = operation(op_token) else {
        return usage(
            "/attribute <target> <attribute> modifier add <id> <value> <add_value|add_multiplied_base|add_multiplied_total>",
        );
    };
    if input.word().is_some() {
        return usage(
            "/attribute <target> <attribute> modifier add <id> <value> <add_value|add_multiplied_base|add_multiplied_total>",
        );
    }
    let duplicate = ctx
        .plan
        .entity_attributes
        .lock()
        .unwrap()
        .get(&target.uuid)
        .and_then(|map| map.get(name))
        .is_some_and(|state| state.modifiers.iter().any(|modifier| modifier.id == id));
    if duplicate {
        return failed(
            "commands.attribute.failed.modifier_already_present",
            vec![
                json!({"text": id}),
                json!({"text": name}),
                entity_name(target),
            ],
        );
    }
    edit(ctx.plan, target.uuid, name, |state| {
        state.modifiers.push(AttrMod {
            id: id.clone(),
            amount,
            op,
        });
    });
    publish(ctx.plan, target, name, bounds);
    Outcome::Feedback(translate(
        "commands.attribute.modifier.add.success",
        vec![
            json!({"text": id}),
            json!({"text": name}),
            entity_name(target),
        ],
    ))
}

fn remove_modifier(
    ctx: &Context<'_>,
    target: &Target,
    name: &str,
    input: &mut args::Input<'_>,
) -> Outcome {
    let Some(id_token) = input.word() else {
        return usage("/attribute <target> <attribute> modifier remove <id>");
    };
    let Some(id) = location(id_token) else {
        return invalid_resource(id_token);
    };
    let Some(bounds) = bounds_of(target, name) else {
        return failed(
            "commands.attribute.failed.no_attribute",
            vec![entity_name(target), json!({"text": name})],
        );
    };
    let removed = {
        let mut map = ctx.plan.entity_attributes.lock().unwrap();
        let Some(state) = map
            .get_mut(&target.uuid)
            .and_then(|attrs| attrs.get_mut(name))
        else {
            return failed(
                "commands.attribute.failed.no_modifier",
                vec![
                    json!({"text": name}),
                    entity_name(target),
                    json!({"text": id}),
                ],
            );
        };
        let before = state.modifiers.len();
        state.modifiers.retain(|modifier| modifier.id != id);
        state.modifiers.len() != before
    };
    if !removed {
        return failed(
            "commands.attribute.failed.no_modifier",
            vec![
                json!({"text": name}),
                entity_name(target),
                json!({"text": id}),
            ],
        );
    }
    publish(ctx.plan, target, name, &bounds);
    Outcome::Feedback(translate(
        "commands.attribute.modifier.remove.success",
        vec![
            json!({"text": id}),
            json!({"text": name}),
            entity_name(target),
        ],
    ))
}

fn modifier_value(
    ctx: &Context<'_>,
    target: &Target,
    name: &str,
    input: &mut args::Input<'_>,
) -> Outcome {
    let Some(id_token) = input.word() else {
        return usage("/attribute <target> <attribute> modifier value get <id>");
    };
    let Some(id) = location(id_token) else {
        return invalid_resource(id_token);
    };
    let Some(scale) = scale_of(input) else {
        return usage("/attribute <target> <attribute> modifier value get <id> [scale]");
    };
    let stored = ctx.plan.entity_attributes.lock().unwrap();
    let amount = stored
        .get(&target.uuid)
        .and_then(|map| map.get(name))
        .and_then(|state| {
            state
                .modifiers
                .iter()
                .find(|modifier| modifier.id == id)
                .map(|modifier| modifier.amount)
        });
    drop(stored);
    let Some(amount) = amount else {
        return failed(
            "commands.attribute.failed.no_modifier",
            vec![
                json!({"text": name}),
                entity_name(target),
                json!({"text": id}),
            ],
        );
    };
    Outcome::Feedback(translate(
        "commands.attribute.modifier.value.get.success",
        vec![
            json!({"text": id}),
            json!({"text": name}),
            entity_name(target),
            Value::from(amount * scale),
        ],
    ))
}

fn visible_modifiers(plan: &PlayPlan, target: &Target, name: &str) -> Vec<(String, f64, i32)> {
    let mut mods = effect_modifiers(plan, target.uuid, name);
    mods.extend(plan.enchant_attribute_modifiers(target.uuid, name));
    plan.merge_attribute_modifiers(target.uuid, name, &mods)
}

fn effect_modifiers(plan: &PlayPlan, uuid: Uuid, name: &str) -> Vec<(String, f64, i32)> {
    let amp = |effect: &str| {
        let id = *plan.survival.effects.get(effect)?;
        plan.live
            .lock()
            .unwrap()
            .get(&uuid)?
            .effects
            .iter()
            .find(|active| active.id == id)
            .map(|active| active.amplifier)
    };
    match name {
        "minecraft:movement_speed" => {
            survival::speed_modifiers(false, amp("minecraft:speed"), amp("minecraft:slowness"))
        }
        "minecraft:jump_strength" => survival::jump_modifiers(amp("minecraft:jump_boost")),
        "minecraft:gravity" => survival::gravity_modifiers(amp("minecraft:slow_falling").is_some()),
        _ => Vec::new(),
    }
}

fn publish(plan: &PlayPlan, target: &Target, name: &str, bounds: &Bounds) {
    let Some(&id) = plan.survival.attributes.get(name) else {
        return;
    };
    if target.entity_id < 0 {
        return;
    }
    let mods = visible_modifiers(plan, target, name);
    let base = plan.attribute_base(target.uuid, name, bounds.base);
    let packet = update_attribute(
        plan.ids.update_attributes,
        target.entity_id,
        id,
        base,
        &mods,
    );
    if target.game_mode.is_some() {
        plan.send_to(target.uuid, packet);
    } else {
        plan.broadcast_packet(packet);
    }
}

fn edit(plan: &PlayPlan, uuid: Uuid, name: &str, change: impl FnOnce(&mut AttrState)) {
    let mut map = plan.entity_attributes.lock().unwrap();
    change(
        map.entry(uuid)
            .or_default()
            .entry(name.to_owned())
            .or_default(),
    );
}

fn combine(base: f64, mods: &[(String, f64, i32)], bounds: &Bounds) -> f64 {
    let mut added = base;
    let mut multiplied_base = 0.0;
    let mut multiplied_total = 1.0;
    for (_, amount, op) in mods {
        match *op {
            0 => added += amount,
            1 => multiplied_base += amount,
            _ => multiplied_total *= 1.0 + amount,
        }
    }
    (added * (1.0 + multiplied_base) * multiplied_total).clamp(bounds.min, bounds.max)
}

fn scale_of(input: &mut args::Input<'_>) -> Option<f64> {
    match input.word() {
        None => Some(1.0),
        Some(token) => token.parse().ok().filter(|_| input.word().is_none()),
    }
}

fn operation(token: &str) -> Option<i32> {
    Some(match token {
        "add_value" => 0,
        "add_multiplied_base" => 1,
        "add_multiplied_total" => 2,
        _ => return None,
    })
}

enum Profile {
    Player,
    Mob(Species),
}

fn profile(target: &Target) -> Option<Profile> {
    if target.game_mode.is_some() {
        Some(Profile::Player)
    } else {
        Species::from_name(&target.name).map(Profile::Mob)
    }
}

fn bounds_of(target: &Target, name: &str) -> Option<Bounds> {
    let profile = profile(target)?;
    if !entity_has(&profile, name) {
        return None;
    }
    let mut bounds = registry_bounds(name)?;
    bounds.base = match (&profile, name) {
        (Profile::Player, "minecraft:movement_speed") => survival::BASE_MOVEMENT_SPEED,
        (Profile::Player, "minecraft:jump_strength") => survival::BASE_JUMP_STRENGTH,
        (Profile::Player, "minecraft:gravity") => survival::BASE_GRAVITY,
        (
            Profile::Player,
            "minecraft:waypoint_transmit_range" | "minecraft:waypoint_receive_range",
        ) => 60_000_000.0,
        (Profile::Player, "minecraft:attack_damage") => 1.0,
        (Profile::Mob(species), "minecraft:movement_speed") => species.speed(),
        (Profile::Mob(species), "minecraft:max_health") => f64::from(species.max_health()),
        (Profile::Mob(_), "minecraft:follow_range") => 16.0,
        _ => bounds.base,
    };
    Some(bounds)
}

fn entity_has(profile: &Profile, name: &str) -> bool {
    if living(name) {
        return true;
    }
    match profile {
        Profile::Player => player_only(name),
        Profile::Mob(_) => name == "minecraft:follow_range",
    }
}

fn living(name: &str) -> bool {
    matches!(
        name,
        "minecraft:air_drag_modifier"
            | "minecraft:armor"
            | "minecraft:armor_toughness"
            | "minecraft:attack_knockback"
            | "minecraft:below_name_distance"
            | "minecraft:bounciness"
            | "minecraft:burning_time"
            | "minecraft:camera_distance"
            | "minecraft:entity_interaction_range"
            | "minecraft:explosion_knockback_resistance"
            | "minecraft:fall_damage_multiplier"
            | "minecraft:friction_modifier"
            | "minecraft:gravity"
            | "minecraft:jump_strength"
            | "minecraft:knockback_resistance"
            | "minecraft:max_absorption"
            | "minecraft:max_health"
            | "minecraft:movement_efficiency"
            | "minecraft:movement_speed"
            | "minecraft:name_tag_distance"
            | "minecraft:oxygen_bonus"
            | "minecraft:safe_fall_distance"
            | "minecraft:scale"
            | "minecraft:step_height"
            | "minecraft:water_movement_efficiency"
            | "minecraft:waypoint_transmit_range"
    )
}

fn player_only(name: &str) -> bool {
    matches!(
        name,
        "minecraft:attack_damage"
            | "minecraft:attack_speed"
            | "minecraft:block_break_speed"
            | "minecraft:block_interaction_range"
            | "minecraft:luck"
            | "minecraft:mining_efficiency"
            | "minecraft:sneaking_speed"
            | "minecraft:submerged_mining_speed"
            | "minecraft:sweeping_damage_ratio"
            | "minecraft:waypoint_receive_range"
    )
}

fn registry_bounds(name: &str) -> Option<Bounds> {
    let (base, min, max) = match name {
        "minecraft:air_drag_modifier" => (1.0, 0.0, 2048.0),
        "minecraft:armor" => (0.0, 0.0, 30.0),
        "minecraft:armor_toughness" => (0.0, 0.0, 20.0),
        "minecraft:attack_damage" => (2.0, 0.0, 2048.0),
        "minecraft:attack_knockback" => (0.0, 0.0, 5.0),
        "minecraft:attack_speed" => (4.0, 0.0, 1024.0),
        "minecraft:below_name_distance" => (10.0, 0.0, 512.0),
        "minecraft:block_break_speed" => (1.0, 0.0, 1024.0),
        "minecraft:block_interaction_range" => (4.5, 0.0, 64.0),
        "minecraft:bounciness" => (0.0, 0.0, 1.0),
        "minecraft:burning_time" => (1.0, 0.0, 1024.0),
        "minecraft:camera_distance" => (4.0, 0.0, 32.0),
        "minecraft:entity_interaction_range" => (3.0, 0.0, 64.0),
        "minecraft:explosion_knockback_resistance" => (0.0, 0.0, 1.0),
        "minecraft:fall_damage_multiplier" => (1.0, 0.0, 100.0),
        "minecraft:follow_range" => (32.0, 0.0, 2048.0),
        "minecraft:friction_modifier" => (1.0, 0.0, 2048.0),
        "minecraft:gravity" => (0.08, -1.0, 1.0),
        "minecraft:jump_strength" => (0.41999998688697815, 0.0, 32.0),
        "minecraft:knockback_resistance" => (0.0, -2.0, 1.0),
        "minecraft:luck" => (0.0, -1024.0, 1024.0),
        "minecraft:max_absorption" => (0.0, 0.0, 2048.0),
        "minecraft:max_health" => (20.0, 1.0, 1024.0),
        "minecraft:mining_efficiency" => (0.0, 0.0, 1024.0),
        "minecraft:movement_efficiency" => (0.0, 0.0, 1.0),
        "minecraft:movement_speed" => (0.7, 0.0, 1024.0),
        "minecraft:name_tag_distance" => (64.0, 0.0, 512.0),
        "minecraft:oxygen_bonus" => (0.0, 0.0, 1024.0),
        "minecraft:safe_fall_distance" => (3.0, -1024.0, 1024.0),
        "minecraft:scale" => (1.0, 0.0625, 16.0),
        "minecraft:sneaking_speed" => (0.3, 0.0, 1.0),
        "minecraft:step_height" => (0.6, 0.0, 10.0),
        "minecraft:submerged_mining_speed" => (0.2, 0.0, 20.0),
        "minecraft:sweeping_damage_ratio" => (0.0, 0.0, 1.0),
        "minecraft:water_movement_efficiency" => (0.0, 0.0, 1.0),
        "minecraft:waypoint_receive_range" | "minecraft:waypoint_transmit_range" => {
            (0.0, 0.0, 60_000_000.0)
        }
        _ => return None,
    };
    Some(Bounds { base, min, max })
}

fn location(token: &str) -> Option<String> {
    let name = args::resource_name(token);
    let (namespace, path) = name.split_once(':')?;
    let ok = |part: &str| {
        !part.is_empty()
            && part.bytes().all(|b| {
                b.is_ascii_lowercase()
                    || b.is_ascii_digit()
                    || matches!(b, b'_' | b'.' | b'/' | b'-')
            })
    };
    (ok(namespace) && ok(path)).then_some(name)
}

fn entity_name(target: &Target) -> Value {
    json!({"text": target.name})
}

fn invalid_resource(name: &str) -> Outcome {
    failed(
        "argument.resource.not_found",
        vec![
            Value::String(name.to_owned()),
            Value::String("minecraft:attribute".into()),
        ],
    )
}

fn failed(key: &str, with: Vec<Value>) -> Outcome {
    let mut value = translate(key, with);
    if let Some(object) = value.as_object_mut() {
        object.insert("color".into(), Value::String("red".into()));
    }
    Outcome::Feedback(value)
}

/// Узлы сверены с `commands.json` 26.3.
pub(super) fn specs() -> Vec<super::dispatch::Spec> {
    use super::dispatch::Spec;
    use super::registry::{GAMEMASTERS, double, ent, resource_location};
    let scale = || Spec::arg_leaf("scale", GAMEMASTERS, double(f64::MIN, f64::MAX), attribute);
    let id = |exec: bool| {
        if exec {
            Spec::arg_leaf("id", GAMEMASTERS, resource_location(), attribute)
        } else {
            Spec::arg("id", GAMEMASTERS, resource_location(), Vec::new())
        }
    };
    let ops = ["add_multiplied_base", "add_multiplied_total", "add_value"]
        .into_iter()
        .map(|op| Spec::literal_leaf(op, GAMEMASTERS, attribute))
        .collect();
    let value = Spec::arg("value", GAMEMASTERS, double(f64::MIN, f64::MAX), ops);
    let add_id = id(false).with_children(vec![value]);
    let remove_id = id(true);
    let modifier_get_id = id(true).with_children(vec![scale()]);
    vec![
        Spec::group(
            "attribute",
            GAMEMASTERS,
            vec![Spec::arg(
                "target",
                GAMEMASTERS,
                ent(false, true),
                vec![Spec::arg(
                    "attribute",
                    GAMEMASTERS,
                    chunkwake_protocol::text::Parser::Resource("minecraft:attribute"),
                    vec![
                        Spec::group(
                            "base",
                            GAMEMASTERS,
                            vec![
                                Spec::exec("get", GAMEMASTERS, attribute, vec![scale()]),
                                Spec::literal_leaf("reset", GAMEMASTERS, attribute),
                                Spec::group(
                                    "set",
                                    GAMEMASTERS,
                                    vec![Spec::arg_leaf(
                                        "value",
                                        GAMEMASTERS,
                                        double(f64::MIN, f64::MAX),
                                        attribute,
                                    )],
                                ),
                            ],
                        ),
                        Spec::exec("get", GAMEMASTERS, attribute, vec![scale()]),
                        Spec::group(
                            "modifier",
                            GAMEMASTERS,
                            vec![
                                Spec::group("add", GAMEMASTERS, vec![add_id]),
                                Spec::group("remove", GAMEMASTERS, vec![remove_id]),
                                Spec::group(
                                    "value",
                                    GAMEMASTERS,
                                    vec![Spec::group("get", GAMEMASTERS, vec![modifier_get_id])],
                                ),
                            ],
                        ),
                    ],
                )],
            )],
        )
        .with_handler(attribute),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifiers_follow_attribute_math() {
        let bounds = Bounds {
            base: 0.1,
            min: 0.0,
            max: 1024.0,
        };
        let mods = vec![
            ("add".into(), 1.0, 0),
            ("base".into(), 1.0, 1),
            ("total".into(), 0.5, 2),
        ];
        // (0.1 + 1) * (1 + 1) * 1.5 = 3.3
        assert!((combine(0.1, &mods, &bounds) - 3.3).abs() < 1e-9);
    }
}
