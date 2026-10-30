//! Команды над сущностями: `/tp`, `/give`, `/kill`, `/clear`, `/effect`,
//! `/xp`, `/damage`, `/rotate`, `/spawnpoint`, `/defaultgamemode`.

use super::args::{self, Input};
use super::dispatch::{text_component, translate, Call, Outcome};
use super::selector::Target;
use super::Context;
use crate::live::Control;
use serde_json::{json, Value};
use uuid::Uuid;

/// Сколько предметов отдаёт `/give` за раз (ванильный предел).
const MAX_GIVE: i32 = 6400;

/// Ошибка «цель не найдена» ванильными ключами.
fn no_targets() -> Outcome {
    Outcome::Feedback(translate(
        "argument.entity.notfound.player",
        Vec::new(),
    ))
}

/// Успех команды с одной целью или несколькими — ванильные ключи различаются.
fn success(single: &str, many: &str, targets: &[Target], with: Vec<Value>) -> Outcome {
    if targets.len() == 1 {
        Outcome::Feedback(translate(single, with))
    } else {
        let mut with = with;
        with.push(Value::from(targets.len()));
        Outcome::Feedback(translate(many, with))
    }
}

/// `/tp` и `/teleport`: ванильные формы.
pub(super) fn teleport(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(first) = input.word() else {
        return usage("/tp <targets> <location> | /tp <targets> <destination>");
    };
    // Первое слово: координата — телепорт себя, иначе — чья.
    let targets = if args::is_coord(first) {
        let Some(uuid) = call.source.uuid else {
            return usage("/tp <targets> <location>");
        };
        // Координаты начинаются с первого слова: вернём его в разбор.
        input = Input::new(call.rest);
        let _ = input.word();
        vec![entity_of(ctx, uuid)]
    } else {
        match ctx.select(call.source, first, false) {
            Ok(list) => list,
            Err(e) => return Outcome::Feedback(e.component()),
        }
    };
    let Some(anchor) = input.word() else {
        return usage("/tp <targets> <location>");
    };
    // `/tp <who> <player>` — в точку игрока.
    if !args::is_coord(anchor) && !targets.is_empty() {
        let Some(dest) = ctx.targets().iter().find(|t| t.name.eq_ignore_ascii_case(anchor)).cloned()
        else {
            return no_targets();
        };
        let moved = move_all(ctx, &targets, dest.pos, dest.yaw, dest.pitch);
        return success(
            "commands.teleport.success.entity.single",
            "commands.teleport.success.entity.multiple",
            &moved.0,
            vec![name_of(&moved.0), json!({"text": dest.name})],
        );
    }
    // Координаты: три или пять слов.
    let mut tokens = vec![anchor.to_owned()];
    for _ in 0..4 {
        if let Some(word) = input.word() {
            tokens.push(word.to_owned());
        }
    }
    let base = targets
        .first()
        .map(|t| t.pos)
        .unwrap_or_else(|| call.position(ctx));
    let Some(coord) = parse_xyz(&tokens, base) else {
        return usage("/tp <targets> <x> <y> <z> [<yaw> <pitch>]");
    };
    let (yaw, pitch) = match (tokens.get(3), tokens.get(4)) {
        (Some(yaw), Some(pitch)) => {
            let Some((yaw, pitch)) = args::rotation(yaw, pitch, (0.0, 0.0)) else {
                return usage("/tp <targets> <x> <y> <z> [<yaw> <pitch>]");
            };
            (yaw, pitch)
        }
        _ => (0.0, 0.0),
    };
    let (moved, _) = move_all(ctx, &targets, coord, yaw, pitch);
    success(
        "commands.teleport.success.location.single",
        "commands.teleport.success.location.multiple",
        &moved,
        vec![name_of(&moved), Value::from(coord[0]), Value::from(coord[1]), Value::from(coord[2])],
    )
}

/// Три координаты из токенов (если их меньше — ошибка).
fn parse_xyz(tokens: &[String], base: [f64; 3]) -> Option<[f64; 3]> {
    let c = args::coords3(tokens.first()?, tokens.get(1)?, tokens.get(2)?)?;
    Some(super::args::Basis::from_rotation(0.0, 0.0).resolve(c, base))
}

/// Перемещает цели, возвращает сдвинутые и пропущенные.
fn move_all(
    ctx: &Context<'_>,
    targets: &[Target],
    pos: [f64; 3],
    yaw: f32,
    pitch: f32,
) -> (Vec<Target>, Vec<Uuid>) {
    let mut moved = Vec::new();
    let mut skipped = Vec::new();
    for target in targets {
        if target.game_mode.is_none() {
            skipped.push(target.uuid);
            continue;
        }
        if ctx.plan.teleport_to(target.uuid, pos, yaw, pitch) {
            moved.push(target.clone());
        } else {
            skipped.push(target.uuid);
        }
    }
    (moved, skipped)
}

/// Имя одной цели или «N entities».
fn name_of(targets: &[Target]) -> Value {
    match targets {
        [one] => json!({"text": one.name}),
        _ => json!({"text": format!("{} entities", targets.len())}),
    }
}

/// Цель по UUID (для `@s` в консоли — ошибка).
fn entity_of(ctx: &Context<'_>, uuid: Uuid) -> Target {
    ctx.targets()
        .into_iter()
        .find(|t| t.uuid == uuid)
        .unwrap_or(Target {
            uuid,
            name: "Server".into(),
            entity_id: -1,
            pos: ctx.spawn(),
            yaw: 0.0,
            pitch: 0.0,
            game_mode: Some(0),
            tags: Vec::new(),
        })
}

/// `/give <targets> <item> [count]`.
pub(super) fn give(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(first) = input.word() else {
        return usage("/give <targets> <item> [count]");
    };
    let targets = match ctx.select(call.source, first, false) {
        Ok(list) => list,
        Err(e) => return Outcome::Feedback(e.component()),
    };
    let Some(item) = input.word() else {
        return usage("/give <targets> <item> [count]");
    };
    let count = match input.word() {
        Some(token) => match args::int(token, 1, MAX_GIVE) {
            Some(n) => n,
            None => return usage("/give <targets> <item> [1-6400]"),
        },
        None => 1,
    };
    // `item_stack` может прийти с компонентами: `bread[...]` — берём только имя.
    let name = item.split(['[', '{']).next().unwrap_or("");
    let name = args::resource_name(name);
    let Some(&name_id) = ctx.survival().item_ids.get(&name) else {
        return Outcome::Feedback(translate(
            "argument.item.id.invalid",
            vec![Value::String(name)],
        ));
    };
    let mut given = 0;
    for target in &targets {
        if ctx.plan.give_item(target.uuid, name_id, count) {
            given += 1;
        }
    }
    if given == 0 {
        return no_targets();
    }
    let item_text = translate(&format!("item.{}", name.replace(':', ".")), Vec::new());
    success(
        "commands.give.success.single",
        "commands.give.success.multiple",
        &targets[..given.min(targets.len())],
        vec![item_text, Value::from(count)],
    )
}

/// `/kill [targets]`.
pub(super) fn kill(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let token = input.word().unwrap_or("@s");
    let targets = match ctx.select(call.source, token, true) {
        Ok(list) => list,
        Err(e) => return Outcome::Feedback(e.component()),
    };
    let mut killed = 0;
    for target in &targets {
        if ctx.plan.kill_target(target) {
            killed += 1;
        }
    }
    if killed == 0 {
        return no_targets();
    }
    success(
        "commands.kill.success.single",
        "commands.kill.success.multiple",
        &targets[..killed.min(targets.len())],
        Vec::new(),
    )
}

/// `/clear [targets] [item] [max]`.
pub(super) fn clear(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let token = input.word().unwrap_or("@s");
    let targets = match ctx.select(call.source, token, true) {
        Ok(list) => list,
        Err(e) => return Outcome::Feedback(e.component()),
    };
    let Some(first) = targets.first().cloned() else {
        return no_targets();
    };
    let removed = ctx.plan.clear_inventory(first.uuid);
    match targets.len() {
        1 => Outcome::Feedback(translate(
            "commands.clear.success.single",
            vec![Value::from(removed), json!({"text": first.name})],
        )),
        n => Outcome::Feedback(translate(
            "commands.clear.success.multiple",
            vec![Value::from(removed * n as i32), Value::from(n)],
        )),
    }
}

/// `/effect give|clear`.
///
/// Подкоманда — литерал дерева (`/effect give …`), поэтому она приходит в
/// [`Call::path`], а не в аргументах.
pub(super) fn effect(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(sub) = call.path.first().map(String::as_str) else {
        return usage("/effect give|clear <targets> [effect] [seconds] [amplifier]");
    };
    let Some(token) = input.word() else {
        return usage("/effect give|clear <targets> [effect] [seconds] [amplifier]");
    };
    let targets = match ctx.select(call.source, token, false) {
        Ok(list) => list,
        Err(e) => return Outcome::Feedback(e.component()),
    };
    let Some(effect) = input.word() else {
        if sub == "clear" {
            for target in &targets {
                ctx.plan.clear_effects(target.uuid, None, true);
            }
            return success(
                "commands.effect.clear.everything.success.single",
                "commands.effect.clear.everything.success.multiple",
                &targets,
                Vec::new(),
            );
        }
        return usage("/effect give <targets> <effect> [seconds] [amplifier]");
    };
    let effect = args::resource_name(effect);
    let Some(&id) = ctx.survival().effects.get(&effect) else {
        return Outcome::Feedback(translate(
            "argument.resource.not_found",
            vec![Value::String(effect), Value::String("minecraft:mob_effect".into())],
        ));
    };
    let effect_text = translate(&format!("effect.{}", effect.replace(':', ".")), Vec::new());
    if sub == "clear" {
        for target in &targets {
            ctx.plan.clear_effects(target.uuid, Some(id), true);
        }
        return success(
            "commands.effect.clear.specific.success.single",
            "commands.effect.clear.specific.success.multiple",
            &targets,
            vec![effect_text],
        );
    }
    // `seconds | infinite`: `infinite` — литерал дерева, поэтому приходит в
    // путь, а не в аргументы; бесконечность у нас — отрицательная длительность.
    let ticks = match (call.path.get(1).map(String::as_str), input.word()) {
        (Some("infinite"), _) => -1,
        (_, Some(token)) => match args::int(token, 1, 1_000_000) {
            Some(n) => n * 20,
            None => return usage("/effect give <targets> <effect> [seconds|infinite] [amplifier]"),
        },
        (_, None) => 30 * 20,
    };
    let amplifier = match input.word() {
        Some(token) => match args::int(token, 0, 255) {
            Some(n) => n,
            None => return usage("/effect give <targets> <effect> [seconds|infinite] [amplifier]"),
        },
        None => 0,
    };
    for target in &targets {
        ctx.plan.give_effect(target.uuid, id, amplifier as u8, ticks);
    }
    success(
        "commands.effect.give.success.single",
        "commands.effect.give.success.multiple",
        &targets,
        vec![effect_text],
    )
}

/// `/xp add|set|query`: алиас `/experience`, подкоманда идёт в [`Call::path`].
pub(super) fn xp(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    use crate::xp::XpChange;
    let mut input = call.t();
    let Some(sub) = call.path.first().map(String::as_str) else {
        return usage("/xp add|set|query <targets> <amount> [points|levels]");
    };
    let Some(token) = input.word() else {
        return usage("/xp add|set|query <targets> <amount> [points|levels]");
    };
    let targets = match ctx.select(call.source, token, false) {
        Ok(list) => list,
        Err(e) => return Outcome::Feedback(e.component()),
    };
    let Some(first) = targets.first().cloned() else {
        return no_targets();
    };
    if sub == "query" {
        let unit = input.word().unwrap_or("points");
        let Some((level, points)) = ctx.plan.xp_of(first.uuid) else {
            return no_targets();
        };
        let (key, n) = if unit == "levels" {
            ("levels", level)
        } else {
            ("points", points)
        };
        return Outcome::Feedback(translate(
            &format!("commands.experience.query.{key}"),
            vec![json!({"text": first.name}), Value::from(n)],
        ));
    }
    let Some(amount) = input.word().and_then(|t| t.parse::<i32>().ok()) else {
        return usage("/xp add|set <targets> <amount> [points|levels]");
    };
    let levels = match input.word().unwrap_or("points") {
        "points" => false,
        "levels" => true,
        _ => return usage("/xp add|set <targets> <amount> [points|levels]"),
    };
    let change = match (sub, levels) {
        ("add", false) => XpChange::Points(amount),
        ("add", true) => XpChange::Levels(amount),
        ("set", false) => XpChange::SetPoints(amount),
        ("set", true) if amount >= 0 => XpChange::SetLevel(amount),
        _ => return usage("/xp add|set <targets> <amount> [points|levels]"),
    };
    for target in &targets {
        ctx.plan.push_control(target.uuid, Control::Xp(change.clone()));
    }
    let unit = if levels { "levels" } else { "points" };
    let kind = if targets.len() == 1 { "single" } else { "multiple" };
    Outcome::Feedback(translate(
        &format!("commands.experience.{sub}.{unit}.success.{kind}"),
        vec![
            Value::from(amount),
            if targets.len() == 1 {
                json!({"text": first.name})
            } else {
                Value::from(targets.len())
            },
        ],
    ))
}

/// `/damage <target> <amount> [type] [at <pos> | by <entity>]`.
pub(super) fn damage(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(token) = input.word() else {
        return usage("/damage <target> <amount> [type]");
    };
    let targets = match ctx.select(call.source, token, true) {
        Ok(list) => list,
        Err(e) => return Outcome::Feedback(e.component()),
    };
    let Some(amount) = input.word().and_then(|t| t.parse::<f32>().ok()) else {
        return usage("/damage <target> <amount> [type]");
    };
    let cause = match input.word() {
        Some(type_id) => {
            let name = args::resource_name(type_id);
            crate::survival::Cause::from_damage_type(&name)
        }
        None => crate::survival::Cause::Generic,
    };
    let Some(first) = targets.first().cloned() else {
        return no_targets();
    };
    if ctx.plan.push_control(first.uuid, Control::Hurt(amount, cause)) {
        success(
            "commands.damage.success",
            "commands.damage.success",
            &targets,
            vec![Value::from(amount), json!({"text": first.name})],
        )
    } else {
        no_targets()
    }
}

/// `/rotate <target> <rotation>`.
pub(super) fn rotate(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(token) = input.word() else {
        return usage("/rotate <target> <yaw> <pitch>");
    };
    let targets = match ctx.select(call.source, token, false) {
        Ok(list) => list,
        Err(e) => return Outcome::Feedback(e.component()),
    };
    let (Some(yaw), Some(pitch)) = (input.word(), input.word()) else {
        return usage("/rotate <target> <yaw> <pitch>");
    };
    let Some(first) = targets.first().cloned() else {
        return no_targets();
    };
    let base = ctx.pose(first.uuid).map_or((0.0, 0.0), |(_, y, p)| (y, p));
    let Some((yaw, pitch)) = args::rotation(yaw, pitch, base) else {
        return usage("/rotate <target> <yaw> <pitch>");
    };
    let pos = first.pos;
    for target in &targets {
        ctx.plan.teleport_to(target.uuid, pos, yaw, pitch);
    }
    success(
        "commands.rotate.success",
        "commands.rotate.success",
        &targets,
        vec![name_of(&targets)],
    )
}

/// `/spawnpoint [targets] [pos] [angle]`.
pub(super) fn spawnpoint(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let token = input.word().unwrap_or("@s");
    let targets = match ctx.select(call.source, token, false) {
        Ok(list) => list,
        Err(e) => return Outcome::Feedback(e.component()),
    };
    let Some(first) = targets.first().cloned() else {
        return no_targets();
    };
    let pos = match (input.word(), input.word(), input.word()) {
        (Some(x), Some(y), Some(z)) => {
            let Some(c) = args::coords3(x, y, z) else {
                return usage("/spawnpoint [targets] [pos] [angle]");
            };
            Some(args::block_pos(
                c,
                first.pos,
                args::Basis::from_rotation(first.yaw, first.pitch),
            ))
        }
        _ => None,
    };
    for target in &targets {
        ctx.plan.set_player_spawn(target.uuid, pos);
    }
    match pos {
        Some([x, y, z]) => Outcome::Feedback(translate(
            "commands.spawnpoint.success.single",
            vec![
                name_of(&targets),
                Value::from(x),
                Value::from(y),
                Value::from(z),
            ],
        )),
        None => Outcome::Feedback(translate(
            "commands.spawnpoint.success.single",
            vec![name_of(&targets)],
        )),
    }
}

/// `/defaultgamemode <mode>`.
pub(super) fn default_gamemode(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(token) = input.word() else {
        return usage("/defaultgamemode <mode>");
    };
    let Some(mode) = args::gamemode(token) else {
        return Outcome::Feedback(translate("argument.gamemode.invalid", Vec::new()));
    };
    ctx.plan.set_default_game_mode(mode);
    Outcome::Feedback(translate(
        "commands.defaultgamemode.success",
        vec![translate(
            &format!("gameMode.{}", gamemode_name(mode)),
            Vec::new(),
        )],
    ))
}

/// Имя режима для ключей перевода.
pub(super) fn gamemode_name(mode: u8) -> &'static str {
    match mode {
        0 => "survival",
        1 => "creative",
        2 => "adventure",
        _ => "spectator",
    }
}

/// `/gamemode <mode> [targets]`.
///
/// Режим — аргумент `minecraft:gamemode` (принимает и `creative`, и `1`),
/// поэтому он приходит первым словом, а не литералом пути.
pub(super) fn gamemode(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(token) = input.word() else {
        return usage("/gamemode <mode> [targets]");
    };
    let Some(mode) = args::gamemode(token) else {
        return Outcome::Feedback(translate("argument.gamemode.invalid", Vec::new()));
    };
    let token = input.word().unwrap_or("@s");
    let targets = match ctx.select(call.source, token, false) {
        Ok(list) => list,
        Err(e) => return Outcome::Feedback(e.component()),
    };
    let mut changed = 0;
    for target in &targets {
        if ctx.plan.push_control(target.uuid, Control::GameMode(mode)) {
            changed += 1;
        }
    }
    if changed == 0 {
        return no_targets();
    }
    let mode_text = translate(&format!("gameMode.{}", gamemode_name(mode)), Vec::new());
    // Себе — `success.self`, другому — `success.single` с именем.
    if targets.len() == 1 && Some(targets[0].uuid) == call.source.uuid {
        Outcome::Feedback(translate(
            "commands.gamemode.success.self",
            vec![mode_text],
        ))
    } else {
        success(
            "commands.gamemode.success.other",
            "commands.gamemode.success.other",
            &targets,
            vec![name_of(&targets), mode_text],
        )
    }
}

/// Общее сообщение об использовании команды.
pub(super) fn usage(text: &str) -> Outcome {
    Outcome::Feedback(text_component(text))
}

/// Узел-аргумент, чьи дети подключаются позже (`/give <targets> <item>`).
fn args_group(
    name: &str,
    level: i32,
    parser: chunkwake_protocol::text::Parser,
    handler: super::dispatch::Handler,
) -> super::dispatch::Spec {
    super::dispatch::Spec::arg_group(name, level, parser, handler)
}

/// `/experience add|set|query`: узел повторяется для `xp` (алиас) и самого
/// `experience`, как в ванили.
pub(super) fn specs() -> Vec<super::dispatch::Spec> {
    use super::dispatch::Spec;
    use super::registry::{ent, float, int, GAMEMASTERS};
    let players = || ent(true, false);
    let single_entity = || ent(false, true);
    vec![
        // give <targets> <item> [count]
        Spec::group(
            "give",
            GAMEMASTERS,
            vec![
                args_group("targets", GAMEMASTERS, players(), give).with_children(vec![
                    Spec::arg_leaf("item", GAMEMASTERS, super::registry::item_stack(), give)
                        .with_children(vec![Spec::arg_leaf(
                            "count",
                            GAMEMASTERS,
                            int(1, MAX_GIVE),
                            give,
                        )]),
                ]),
            ],
        )
        .with_handler(give),
        // effect give|clear <targets> <effect> …
        Spec::group(
            "effect",
            GAMEMASTERS,
            vec![                Spec::group(
                    "give",
                    GAMEMASTERS,
                    vec![
                        args_group("targets", GAMEMASTERS, players(), effect).with_children(vec![
                            Spec::arg_leaf("effect", GAMEMASTERS, super::registry::mob_effect(), effect)
                                .with_children(vec![
                                    Spec::arg_leaf("seconds", GAMEMASTERS, int(1, 1_000_000), effect)
                                        .with_children(vec![Spec::arg_leaf(
                                            "amplifier",
                                            GAMEMASTERS,
                                            int(0, 255),
                                            effect,
                                        )]),
                                    Spec::exec(
                                        "infinite",
                                        GAMEMASTERS,
                                        effect,
                                        vec![Spec::arg_leaf(
                                            "amplifier",
                                            GAMEMASTERS,
                                            int(0, 255),
                                            effect,
                                        )],
                                    ),
                                ]),
                        ]),
                    ],
                ),
                Spec::exec(
                    "clear",
                    GAMEMASTERS,
                    effect,
                    vec![
                        Spec::arg_leaf("targets", GAMEMASTERS, players(), effect).with_children(vec![
                            Spec::arg_leaf("effect", GAMEMASTERS, super::registry::mob_effect(), effect),
                        ]),
                    ],
                ),
            ],
        )
        .with_handler(effect),
        // gamemode <gamemode> [target]
        Spec::group(
            "gamemode",
            GAMEMASTERS,
            vec![
                Spec::arg_leaf("gamemode", GAMEMASTERS, super::registry::gamemode(), gamemode)
                    .with_children(vec![Spec::arg_leaf("target", GAMEMASTERS, players(), gamemode)]),
            ],
        )
        .with_handler(gamemode),
        // defaultgamemode <gamemode>
        Spec::group(
            "defaultgamemode",
            GAMEMASTERS,
            vec![Spec::arg_leaf(
                "gamemode",
                GAMEMASTERS,
                super::registry::gamemode(),
                default_gamemode,
            )],
        )
        .with_handler(default_gamemode),
        // experience и алиас xp
        experience_spec("experience"),
        Spec::alias("xp", GAMEMASTERS, "experience"),
        // kill [targets]: корень исполняем (`/kill` — себя), как в ванили.
        Spec::exec(
            "kill",
            GAMEMASTERS,
            kill,
            vec![Spec::arg_leaf("targets", GAMEMASTERS, ent(false, false), kill)],
        ),
        // clear [targets] [item] [maxCount]: корень исполняем.
        Spec::exec(
            "clear",
            GAMEMASTERS,
            clear,
            vec![
                Spec::arg_leaf("targets", GAMEMASTERS, players(), clear).with_children(vec![
                    Spec::arg_leaf("item", GAMEMASTERS, super::registry::item_predicate(), clear)
                        .with_children(vec![Spec::arg_leaf(
                            "maxCount",
                            GAMEMASTERS,
                            int(0, i32::MAX),
                            clear,
                        )]),
                ]),
            ],
        ),
        // teleport и алиас tp
        tp_spec("teleport"),
        Spec::alias("tp", GAMEMASTERS, "teleport"),        // damage <target> <amount> [damageType]
        Spec::group(
            "damage",
            GAMEMASTERS,
            vec![
                args_group("target", GAMEMASTERS, single_entity(), damage).with_children(vec![
                    Spec::arg_leaf("amount", GAMEMASTERS, float(0.0, 3.4e38), damage).with_children(
                        vec![Spec::arg_leaf(
                            "damageType",
                            GAMEMASTERS,
                            super::registry::damage_type(),
                            damage,
                        )],
                    ),
                ]),
            ],
        ),
        // rotate <target> <rotation>
        Spec::group(
            "rotate",
            GAMEMASTERS,
            vec![
                args_group("target", GAMEMASTERS, single_entity(), rotate).with_children(vec![
                    Spec::arg_leaf("rotation", GAMEMASTERS, super::registry::rotation(), rotate),
                ]),
            ],
        ),
        // spawnpoint [targets] [pos] [rotation]
        Spec::exec(
            "spawnpoint",
            GAMEMASTERS,
            spawnpoint,
            vec![
                Spec::arg_leaf("targets", GAMEMASTERS, players(), spawnpoint).with_children(vec![
                    Spec::arg_leaf("pos", GAMEMASTERS, super::registry::block_pos(), spawnpoint)
                        .with_children(vec![Spec::arg_leaf(
                            "rotation",
                            GAMEMASTERS,
                            super::registry::rotation(),
                            spawnpoint,
                        )]),
                ]),
            ],
        ),
    ]
}

/// `/experience add|set|query`: узел повторяется для `xp` (алиас) и самого
/// `experience`, как в ванили.
fn experience_spec(name: &str) -> super::dispatch::Spec {
    use super::dispatch::Spec;
    use super::registry::{arg, ent, int, GAMEMASTERS};
    let units = || {
        vec![
            Spec::literal_leaf("levels", GAMEMASTERS, xp),
            Spec::literal_leaf("points", GAMEMASTERS, xp),
        ]
    };
    Spec::group(
        name,
        GAMEMASTERS,
        vec![
            Spec::group(
                "add",
                GAMEMASTERS,
                vec![
                    arg("target", GAMEMASTERS, ent(true, false), xp).with_children(vec![
                        Spec::arg_leaf("amount", GAMEMASTERS, int(i32::MIN, i32::MAX), xp)
                            .with_children(units()),
                    ]),
                ],
            ),
            Spec::group(
                "set",
                GAMEMASTERS,
                vec![
                    arg("target", GAMEMASTERS, ent(true, false), xp).with_children(vec![
                        Spec::arg_leaf("amount", GAMEMASTERS, int(0, i32::MAX), xp)
                            .with_children(units()),
                    ]),
                ],
            ),
            Spec::group(
                "query",
                GAMEMASTERS,
                vec![arg("target", GAMEMASTERS, ent(true, true), xp).with_children(units())],
            ),
        ],
    )
    .with_handler(xp)
}

/// `/teleport`: ветки `<location>`, `<destination>` и `<targets> …` — как в
/// ванили, поэтому клиент принимает и `/tp ~ ~ ~`, и `/tp Alice`.
fn tp_spec(name: &str) -> super::dispatch::Spec {
    use super::dispatch::Spec;
    use super::registry::{anchor, ent, vec3, GAMEMASTERS};
    let location = || {
        let facing_pos = Spec::arg_leaf("facingLocation", GAMEMASTERS, vec3(), teleport);
        let facing_entity = Spec::group(
            "entity",
            GAMEMASTERS,
            vec![
                Spec::arg_leaf("facingEntity", GAMEMASTERS, ent(false, true), teleport)
                    .with_children(vec![Spec::arg_leaf(
                        "facingAnchor",
                        GAMEMASTERS,
                        anchor(),
                        teleport,
                    )]),
            ],
        );
        let facing = Spec::group("facing", GAMEMASTERS, vec![facing_entity, facing_pos]);
        let rotation = Spec::arg_leaf("rotation", GAMEMASTERS, super::registry::rotation(), teleport);
        Spec::arg_leaf("location", GAMEMASTERS, vec3(), teleport)
            .with_children(vec![facing, rotation])
    };
    Spec::group(
        name,
        GAMEMASTERS,
        vec![
            location(),
            Spec::arg_leaf("destination", GAMEMASTERS, ent(false, true), teleport),
            args_group("targets", GAMEMASTERS, ent(false, false), teleport).with_children(vec![
                location(),
                Spec::arg_leaf("destination", GAMEMASTERS, ent(false, true), teleport),
            ]),
        ],
    )
    .with_handler(teleport)
}

