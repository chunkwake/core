//! Команды над сущностями: `/tp`, `/give`, `/kill`, `/clear`, `/effect`,
//! `/xp`, `/damage`, `/rotate`, `/spawnpoint`, `/defaultgamemode`.

use super::Context;
use super::args;
use super::dispatch::{Call, Outcome, text_component, translate};
use super::selector::Target;
use crate::live::Control;
use serde_json::{Value, json};
use uuid::Uuid;

/// Сколько предметов отдаёт `/give` за раз (ванильный предел).
const MAX_GIVE: i32 = 6400;

/// Ошибка «цель не найдена» ванильными ключами.
fn no_targets() -> Outcome {
    Outcome::Feedback(translate("argument.entity.notfound.player", Vec::new()))
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

/// `/tp` и `/teleport`.
///
/// Первое слово — либо начало координат (`/tp ^ ^5 ^`), либо цель. Раньше
/// координата читалась дважды и первое слово терялось, поэтому `/tp ^ ^5 ^`
/// не добирало трёх чисел. `^` считается от взгляда исполнителя, а без
/// `facing` и углов поворот цели не сбрасывается в ноль.
pub(super) fn teleport(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let words = call.t().tokens();
    let plan = match read_teleport(&words) {
        Ok(plan) => plan,
        Err(text) => return usage(text),
    };
    let (base, look_yaw, look_pitch) = source_pose(ctx, call);
    let targets = match &plan.who {
        None => {
            let Some(uuid) = call.source.uuid else {
                return usage("/tp <location>|<destination>");
            };
            vec![entity_of(ctx, uuid)]
        }
        Some(token) => match ctx.select(call.source, token, true) {
            Ok(list) => list,
            Err(e) => return Outcome::Feedback(e.component()),
        },
    };
    if targets.is_empty() {
        return no_targets();
    }
    match plan.dest {
        Dest::Entity(token) => {
            let Some(dest) = ctx
                .select(call.source, &token, true)
                .ok()
                .and_then(|list| list.into_iter().next())
            else {
                return no_targets();
            };
            let (moved, _) = move_all(ctx, &targets, dest.pos, Some((dest.yaw, dest.pitch)));
            if moved.is_empty() {
                return no_targets();
            }
            success(
                "commands.teleport.success.entity.single",
                "commands.teleport.success.entity.multiple",
                &moved,
                vec![name_of(&moved), json!({"text": dest.name})],
            )
        }
        Dest::Location {
            coords,
            facing,
            rotation,
        } => {
            let basis = args::Basis::from_rotation(look_yaw, look_pitch);
            let pos = basis.resolve(coords, base);
            let turn = match facing {
                Facing::None => rotation
                    .and_then(|(yaw, pitch)| args::rotation(&yaw, &pitch, (look_yaw, look_pitch))),
                Facing::Pos(at) => {
                    let point = basis.resolve(at, base);
                    Some(look_at(pos, point))
                }
                Facing::Entity { token, eyes } => {
                    let Some(dest) = ctx
                        .select(call.source, &token, true)
                        .ok()
                        .and_then(|list| list.into_iter().next())
                    else {
                        return no_targets();
                    };
                    let mut point = dest.pos;
                    if eyes {
                        point[1] += 1.62;
                    }
                    Some(look_at(pos, point))
                }
            };
            let (moved, _) = move_all(ctx, &targets, pos, turn);
            if moved.is_empty() {
                return no_targets();
            }
            success(
                "commands.teleport.success.location.single",
                "commands.teleport.success.location.multiple",
                &moved,
                vec![
                    name_of(&moved),
                    Value::from(pos[0]),
                    Value::from(pos[1]),
                    Value::from(pos[2]),
                ],
            )
        }
    }
}

/// Куда смотрит и откуда считается `~` и `^`: цепочка `/execute`, иначе игрок.
fn source_pose(ctx: &Context<'_>, call: &Call<'_>) -> ([f64; 3], f32, f32) {
    if let Some(origin) = ctx.execution_origin() {
        return origin;
    }
    call.source
        .uuid
        .and_then(|uuid| ctx.pose(uuid))
        .unwrap_or((ctx.spawn(), 0.0, 0.0))
}

/// Углы взгляда из точки на цель.
fn look_at(from: [f64; 3], to: [f64; 3]) -> (f32, f32) {
    let (dx, dy, dz) = (to[0] - from[0], to[1] - from[1], to[2] - from[2]);
    let horizontal = (dx * dx + dz * dz).sqrt();
    let yaw = (-dx).atan2(dz).to_degrees() as f32;
    let pitch = (-dy).atan2(horizontal).to_degrees() as f32;
    (yaw, pitch)
}

/// Разобранный `/tp`.
struct TeleportPlan {
    /// `None` — телепортируем себя.
    who: Option<String>,
    dest: Dest,
}

enum Dest {
    Entity(String),
    Location {
        coords: [args::Coord; 3],
        facing: Facing,
        /// Явный `<yaw> <pitch>`, если нет `facing`.
        rotation: Option<(String, String)>,
    },
}

enum Facing {
    None,
    Pos([args::Coord; 3]),
    Entity { token: String, eyes: bool },
}

/// Разбор строки `/tp` без мира: координаты не теряют первое слово.
fn read_teleport(words: &[&str]) -> Result<TeleportPlan, &'static str> {
    const USAGE: &str = "/tp <location>|<destination> | /tp <targets> <location>|<destination>";
    if words.is_empty() {
        return Err(USAGE);
    }
    if args::is_coord(words[0]) {
        let (coords, facing, rotation) = read_location(words)?;
        return Ok(TeleportPlan {
            who: None,
            dest: Dest::Location {
                coords,
                facing,
                rotation,
            },
        });
    }
    if words.len() == 1 {
        return Ok(TeleportPlan {
            who: None,
            dest: Dest::Entity(words[0].to_owned()),
        });
    }
    if args::is_coord(words[1]) {
        let (coords, facing, rotation) = read_location(&words[1..])?;
        return Ok(TeleportPlan {
            who: Some(words[0].to_owned()),
            dest: Dest::Location {
                coords,
                facing,
                rotation,
            },
        });
    }
    if words.len() != 2 {
        return Err(USAGE);
    }
    Ok(TeleportPlan {
        who: Some(words[0].to_owned()),
        dest: Dest::Entity(words[1].to_owned()),
    })
}

/// Три координаты и необязательные `facing` или углы.
/// Разобранная цель: координаты, поворот и (для `facing entity`) имя.
type LocationArg = ([args::Coord; 3], Facing, Option<(String, String)>);

fn read_location(words: &[&str]) -> Result<LocationArg, &'static str> {
    const USAGE: &str = "/tp <x> <y> <z> [<yaw> <pitch>|facing <pos>|facing entity <target>]";
    if words.len() < 3 {
        return Err(USAGE);
    }
    let Some(coords) = args::coords3(words[0], words[1], words[2]) else {
        return Err(USAGE);
    };
    let local = coords
        .iter()
        .filter(|c| matches!(c, args::Coord::Local(_)))
        .count();
    if local != 0 && local != 3 {
        return Err("/tp: ^ must be used for all three coordinates");
    }
    let tail = &words[3..];
    if tail.is_empty() {
        return Ok((coords, Facing::None, None));
    }
    if tail[0] == "facing" {
        if tail.get(1) == Some(&"entity") {
            let Some(token) = tail.get(2) else {
                return Err(USAGE);
            };
            let eyes = match tail.get(3) {
                None | Some(&"feet") => false,
                Some(&"eyes") => true,
                Some(_) => return Err(USAGE),
            };
            if tail.len() > 4 {
                return Err(USAGE);
            }
            return Ok((
                coords,
                Facing::Entity {
                    token: (*token).to_owned(),
                    eyes,
                },
                None,
            ));
        }
        if tail.len() != 4 {
            return Err(USAGE);
        }
        let Some(at) = args::coords3(tail[1], tail[2], tail[3]) else {
            return Err(USAGE);
        };
        return Ok((coords, Facing::Pos(at), None));
    }
    if tail.len() != 2 {
        return Err(USAGE);
    }
    Ok((
        coords,
        Facing::None,
        Some((tail[0].to_owned(), tail[1].to_owned())),
    ))
}

/// Перемещает цели, возвращает сдвинутые и пропущенные.
fn move_all(
    ctx: &Context<'_>,
    targets: &[Target],
    pos: [f64; 3],
    turn: Option<(f32, f32)>,
) -> (Vec<Target>, Vec<Uuid>) {
    let mut moved = Vec::new();
    let mut skipped = Vec::new();
    for target in targets {
        if target.game_mode.is_none() {
            skipped.push(target.uuid);
            continue;
        }
        // Без `facing` и углов оставляем текущий поворот, а не смотрим на юг.
        let (yaw, pitch) = turn.unwrap_or((target.yaw, target.pitch));
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
    ctx.plan.command_target(uuid).unwrap_or(Target {
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
    let Some(item) = input.balanced() else {
        return usage("/give <targets> <item> [count]");
    };
    let count = match input.word() {
        Some(token) => match args::int(token, 1, MAX_GIVE) {
            Some(n) => n,
            None => return usage("/give <targets> <item> [1-6400]"),
        },
        None => 1,
    };
    let Some(parsed) = super::item::parse_item(item, &ctx.survival().item_ids) else {
        return Outcome::Feedback(translate(
            "argument.item.id.invalid",
            vec![Value::String(item.to_owned())],
        ));
    };
    let name = item.split(['[', '{']).next().unwrap_or(item);
    let name = args::resource_name(name);
    let extra = parsed
        .potion
        .as_deref()
        .and_then(|p| ctx.plan.with_potion(parsed.id, parsed.extra, p))
        .unwrap_or(parsed.extra);
    let mut given = 0;
    for target in &targets {
        if ctx.plan.give_item(
            target.uuid,
            parsed.id,
            count,
            extra,
            parsed.damage.unwrap_or(0),
        ) {
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
    if targets.is_empty() {
        return no_targets();
    }
    let item = match input.balanced() {
        None => None,
        Some(token) if token.starts_with('#') => {
            match super::item::tag_ids(&ctx.survival().item_tags, &ctx.survival().item_ids, token) {
                Some(ids) => Some((ids, 0, None)),
                None => {
                    return Outcome::Feedback(translate(
                        "argument.item.tag.unknown",
                        vec![Value::String(token.to_owned())],
                    ));
                }
            }
        }
        Some(token) => {
            let Some(parsed) = super::item::parse_item(token, &ctx.survival().item_ids) else {
                return Outcome::Feedback(translate(
                    "argument.item.id.invalid",
                    vec![Value::String(token.to_owned())],
                ));
            };
            Some((vec![parsed.id], parsed.extra, parsed.damage))
        }
    };
    let max = match input.word() {
        None => i32::MAX,
        Some(token) => match args::int(token, 0, i32::MAX) {
            Some(n) => n,
            None => return usage("/clear [targets] [item] [maxCount]"),
        },
    };
    let mut removed = 0;
    for target in &targets {
        removed += match &item {
            Some((ids, extra, damage)) => ctx.plan.clear_items(
                target.uuid,
                ids,
                *extra,
                *damage,
                max.saturating_sub(removed),
            ),
            None => ctx.plan.clear_inventory(target.uuid),
        };
    }
    if item.is_some() && removed == 0 {
        return Outcome::Feedback(translate(
            "commands.clear.failed.single",
            vec![json!({"text": targets[0].name})],
        ));
    }
    let first = &targets[0];
    match targets.len() {
        1 => Outcome::Feedback(translate(
            "commands.clear.success.single",
            vec![Value::from(removed), json!({"text": first.name})],
        )),
        n => Outcome::Feedback(translate(
            "commands.clear.success.multiple",
            vec![Value::from(removed), Value::from(n)],
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
            vec![
                Value::String(effect),
                Value::String("minecraft:mob_effect".into()),
            ],
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
        (_, Some("infinite")) => -1,
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
    // `hideParticles`: ванильный флаг прячет частицы, у эффекта поле наоборот.
    let particles = match input.word() {
        None => true,
        Some("true") => false,
        Some("false") => true,
        Some(_) => {
            return usage(
                "/effect give <targets> <effect> [seconds|infinite] [amplifier] [hideParticles]",
            );
        }
    };
    for target in &targets {
        ctx.plan
            .give_effect(target.uuid, id, amplifier as u8, ticks, particles);
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
        ctx.plan.push_control(target.uuid, Control::Xp(change));
    }
    let unit = if levels { "levels" } else { "points" };
    let kind = if targets.len() == 1 {
        "single"
    } else {
        "multiple"
    };
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
    if ctx
        .plan
        .push_control(first.uuid, Control::Hurt(amount, cause))
    {
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

/// `/spawnpoint [targets] [pos] [rotation]` (`SetSpawnCommand`): точка
/// принудительная, в измерении источника; без позиции — блок цели.
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
    let base = (first.yaw, first.pitch);
    let pos = match (input.word(), input.word(), input.word()) {
        (Some(x), Some(y), Some(z)) => {
            let Some(c) = args::coords3(x, y, z) else {
                return usage("/spawnpoint [targets] [pos] [rotation]");
            };
            args::block_pos(c, first.pos, args::Basis::from_rotation(base.0, base.1))
        }
        (None, _, _) => first.pos.map(|v| v.floor() as i32),
        _ => return usage("/spawnpoint [targets] [pos] [rotation]"),
    };
    // `Mth.wrapDegrees(yaw)`, `clamp(pitch, -90, 90)`; без угла — ноль.
    let angle = match (input.word(), input.word()) {
        (Some(y), Some(p)) => match args::rotation(y, p, base) {
            Some((y, p)) => [wrap_degrees(y), p.clamp(-90.0, 90.0)],
            None => return usage("/spawnpoint [targets] [pos] [rotation]"),
        },
        (None, _) => [0.0; 2],
        _ => return usage("/spawnpoint [targets] [pos] [rotation]"),
    };
    let dim = ctx.plan.active_dimension();
    for target in &targets {
        ctx.plan
            .set_player_spawn(target.uuid, Some(pos), angle, dim);
    }
    let [x, y, z] = pos;
    let mut args = vec![
        Value::from(x),
        Value::from(y),
        Value::from(z),
        Value::from(angle[0]),
        Value::from(angle[1]),
        Value::from(dim.key()),
    ];
    let key = if let [one] = targets.as_slice() {
        args.push(json!({"text": one.name}));
        "commands.spawnpoint.success.single"
    } else {
        args.push(Value::from(targets.len()));
        "commands.spawnpoint.success.multiple"
    };
    Outcome::Feedback(translate(key, args))
}

/// `Mth.wrapDegrees`: угол в `[-180, 180)`.
fn wrap_degrees(a: f32) -> f32 {
    let w = a % 360.0;
    if w >= 180.0 {
        w - 360.0
    } else if w < -180.0 {
        w + 360.0
    } else {
        w
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
        Outcome::Feedback(translate("commands.gamemode.success.self", vec![mode_text]))
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
    use super::registry::{GAMEMASTERS, ent, float, int};
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
            vec![
                Spec::group(
                    "give",
                    GAMEMASTERS,
                    vec![
                        args_group("targets", GAMEMASTERS, players(), effect).with_children(vec![
                            Spec::arg_leaf(
                                "effect",
                                GAMEMASTERS,
                                super::registry::mob_effect(),
                                effect,
                            )
                            .with_children(vec![
                                Spec::arg_leaf("seconds", GAMEMASTERS, int(1, 1_000_000), effect)
                                    .with_children(vec![
                                        Spec::arg_leaf(
                                            "amplifier",
                                            GAMEMASTERS,
                                            int(0, 255),
                                            effect,
                                        )
                                        .with_children(
                                            vec![Spec::arg_leaf(
                                                "hideParticles",
                                                GAMEMASTERS,
                                                super::registry::boolean(),
                                                effect,
                                            )],
                                        ),
                                    ]),
                                Spec::exec(
                                    "infinite",
                                    GAMEMASTERS,
                                    effect,
                                    vec![
                                        Spec::arg_leaf(
                                            "amplifier",
                                            GAMEMASTERS,
                                            int(0, 255),
                                            effect,
                                        )
                                        .with_children(
                                            vec![Spec::arg_leaf(
                                                "hideParticles",
                                                GAMEMASTERS,
                                                super::registry::boolean(),
                                                effect,
                                            )],
                                        ),
                                    ],
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
                        Spec::arg_leaf("targets", GAMEMASTERS, players(), effect).with_children(
                            vec![Spec::arg_leaf(
                                "effect",
                                GAMEMASTERS,
                                super::registry::mob_effect(),
                                effect,
                            )],
                        ),
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
                Spec::arg_leaf(
                    "gamemode",
                    GAMEMASTERS,
                    super::registry::gamemode(),
                    gamemode,
                )
                .with_children(vec![Spec::arg_leaf(
                    "target",
                    GAMEMASTERS,
                    players(),
                    gamemode,
                )]),
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
            vec![Spec::arg_leaf(
                "targets",
                GAMEMASTERS,
                ent(false, false),
                kill,
            )],
        ),
        // clear [targets] [item] [maxCount]: корень исполняем.
        Spec::exec(
            "clear",
            GAMEMASTERS,
            clear,
            vec![
                Spec::arg_leaf("targets", GAMEMASTERS, players(), clear).with_children(vec![
                    Spec::arg_leaf(
                        "item",
                        GAMEMASTERS,
                        super::registry::item_predicate(),
                        clear,
                    )
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
        Spec::alias("tp", GAMEMASTERS, "teleport"), // damage <target> <amount> [damageType]
        Spec::group(
            "damage",
            GAMEMASTERS,
            vec![
                args_group("target", GAMEMASTERS, single_entity(), damage).with_children(vec![
                    Spec::arg_leaf("amount", GAMEMASTERS, float(0.0, 3.4e38), damage)
                        .with_children(vec![Spec::arg_leaf(
                            "damageType",
                            GAMEMASTERS,
                            super::registry::damage_type(),
                            damage,
                        )]),
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
    use super::registry::{GAMEMASTERS, arg, ent, int};
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
    use super::registry::{GAMEMASTERS, anchor, ent, vec3};
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
        let rotation = Spec::arg_leaf(
            "rotation",
            GAMEMASTERS,
            super::registry::rotation(),
            teleport,
        );
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

#[cfg(test)]
mod tests {
    use super::read_teleport;

    #[test]
    fn local_teleport_keeps_all_three_coordinates() {
        // Раньше первое слово съедалось, и `/tp ^ ^5 ^` не добирало тройки.
        let plan = read_teleport(&["^", "^5", "^"]).unwrap();
        assert!(plan.who.is_none());
        let super::Dest::Location {
            coords,
            facing,
            rotation,
        } = plan.dest
        else {
            panic!("ожидались координаты");
        };
        assert!(matches!(facing, super::Facing::None));
        assert!(rotation.is_none());
        assert!(
            coords
                .iter()
                .all(|c| matches!(c, super::args::Coord::Local(_)))
        );
        assert!(matches!(coords[1], super::args::Coord::Local(v) if (v - 5.0).abs() < 1e-6));
    }

    #[test]
    fn facing_entity_is_not_a_rotation() {
        let plan = read_teleport(&["^", "^5", "^", "facing", "entity", "qtless", "eyes"]).unwrap();
        let super::Dest::Location { facing, .. } = plan.dest else {
            panic!("ожидались координаты");
        };
        match facing {
            super::Facing::Entity { token, eyes } => {
                assert_eq!(token, "qtless");
                assert!(eyes);
            }
            _ => panic!("ожидался facing entity"),
        }
    }

    #[test]
    fn named_destination_is_not_coordinates() {
        let plan = read_teleport(&["Alice"]).unwrap();
        assert!(matches!(plan.dest, super::Dest::Entity(name) if name == "Alice"));
        let plan = read_teleport(&["Bob", "Alice"]).unwrap();
        assert_eq!(plan.who.as_deref(), Some("Bob"));
    }
}
