//! Команды мира: `/setblock`, `/fill`, `/clone`, `/summon`, `/weather`,
//! `/gamerule`, `/time` живёт в [`super::meta`], `/forceload`, `/seed` тоже.

use super::Context;
use super::args;
use super::dispatch::{Call, Outcome, coordinates, translate};
use super::entity::usage;
use serde_json::{Value, json};

/// Предел `/fill` за раз (ванильный `MAX_BLOCK_MODIFICATIONS`, уменьшенный:
/// один тик не должен превращаться в замирание мира).
const FILL_LIMIT: i64 = 32_768;

/// Как `/setblock` обходится со старым блоком.
enum SetMode {
    Replace,
    Keep,
    Destroy,
    Strict,
}

/// `/setblock <pos> <block> [destroy|keep|replace|strict]`.
pub(super) fn setblock(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let (Some(x), Some(y), Some(z)) = (input.word(), input.word(), input.word()) else {
        return usage("/setblock <x> <y> <z> <block> [destroy|keep|replace|strict]");
    };
    let Some(coords) = args::coords3(x, y, z) else {
        return Outcome::Feedback(translate("argument.pos.invalid", Vec::new()));
    };
    let base = call.position(ctx);
    let pos = args::block_pos(coords, base, args::Basis::from_rotation(0.0, 0.0));
    let Some(spec) = input.word() else {
        return usage("/setblock <x> <y> <z> <block> [destroy|keep|replace|strict]");
    };
    let Some(state) = ctx.plan.parse_block_state(spec) else {
        return Outcome::Feedback(translate(
            "argument.block.id.invalid",
            vec![Value::String(spec.to_owned())],
        ));
    };
    let mode = match input.word() {
        None | Some("replace") => SetMode::Replace,
        Some("keep") => SetMode::Keep,
        Some("destroy") => SetMode::Destroy,
        Some("strict") => SetMode::Strict,
        Some(_) => return usage("/setblock <x> <y> <z> <block> [destroy|keep|replace|strict]"),
    };
    if !ctx.plan.in_world(pos) {
        return Outcome::Feedback(translate("argument.pos.outofbounds", Vec::new()));
    }
    let before = ctx.plan.block_name_at(pos);
    if matches!(mode, SetMode::Keep) && before.as_deref().is_some_and(|n| !n.ends_with(":air")) {
        return Outcome::Feedback(translate("commands.setblock.failed", Vec::new()));
    }
    let broke =
        matches!(mode, SetMode::Destroy) && before.as_deref().is_some_and(|n| !n.ends_with(":air"));
    if broke {
        ctx.plan.break_drops(pos);
    }
    let placed = ctx.plan.place_block(pos, state);
    // `strict` и обычная постановка успешны, только если состояние сменилось.
    // `destroy` успешен и когда блок сломан и поставлен заново тем же состоянием.
    let ok = match mode {
        SetMode::Destroy => placed == Some(true) || broke,
        SetMode::Replace | SetMode::Keep | SetMode::Strict => placed == Some(true),
    };
    if ok {
        Outcome::Feedback(translate(
            "commands.setblock.success",
            vec![
                Value::from(pos[0]),
                Value::from(pos[1]),
                Value::from(pos[2]),
            ],
        ))
    } else {
        Outcome::Feedback(translate("commands.setblock.failed", Vec::new()))
    }
}

/// `/fill <from> <to> <block> [destroy|hollow|keep|outline|replace]`.
pub(super) fn fill(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let tokens = (
        input.word(),
        input.word(),
        input.word(),
        input.word(),
        input.word(),
        input.word(),
    );
    let (Some(x1), Some(y1), Some(z1), Some(x2), Some(y2), Some(z2)) = tokens else {
        return usage("/fill <from> <to> <block> [destroy|hollow|keep|outline|replace]");
    };
    let (Some(from), Some(to)) = (args::coords3(x1, y1, z1), args::coords3(x2, y2, z2)) else {
        return Outcome::Feedback(translate("argument.pos.invalid", Vec::new()));
    };
    let base = call.position(ctx);
    let basis = args::Basis::from_rotation(0.0, 0.0);
    let from = args::block_pos(from, base, basis);
    let to = args::block_pos(to, base, basis);
    let Some(spec) = input.word() else {
        return usage("/fill <from> <to> <block> [destroy|hollow|keep|outline|replace]");
    };
    let Some(state) = ctx.plan.parse_block_state(spec) else {
        return Outcome::Feedback(translate(
            "argument.block.id.invalid",
            vec![Value::String(spec.to_owned())],
        ));
    };
    let (mode, filter) = match input.word() {
        None => (FillMode::Replace, None),
        Some("destroy") => (FillMode::Destroy, None),
        Some("hollow") => (FillMode::Hollow, None),
        Some("keep") => (FillMode::Keep, None),
        Some("outline") => (FillMode::Outline, None),
        Some("strict") => (FillMode::Strict, None),
        Some("replace") => match input.word() {
            None => (FillMode::Replace, None),
            Some(token) if fill_mode(token).is_some() => (fill_mode(token).unwrap(), None),
            Some(token) => {
                let mode = match input.word() {
                    None => FillMode::Replace,
                    Some(next) => match fill_mode(next) {
                        Some(mode) => mode,
                        None => {
                            return usage(
                                "/fill <from> <to> <block> [replace|destroy|hollow|keep|outline|strict]",
                            );
                        }
                    },
                };
                (mode, Some(token.to_owned()))
            }
        },
        Some(_) => {
            return usage("/fill <from> <to> <block> [replace|destroy|hollow|keep|outline|strict]");
        }
    };
    if !ctx.plan.in_world(from) || !ctx.plan.in_world(to) {
        return Outcome::Feedback(translate("argument.pos.outofbounds", Vec::new()));
    }
    let volume = (i64::from(to[0] - from[0]).abs() + 1)
        * (i64::from(to[1] - from[1]).abs() + 1)
        * (i64::from(to[2] - from[2]).abs() + 1);
    if volume > FILL_LIMIT {
        return Outcome::Feedback(translate(
            "commands.fill.toobig",
            vec![Value::from(FILL_LIMIT), Value::from(volume)],
        ));
    }
    let changed = ctx
        .plan
        .fill_region(from, to, state, mode, filter.as_deref());
    if matches!(mode, FillMode::Strict) && changed == 0 {
        return Outcome::Feedback(translate("commands.fill.failed", Vec::new()));
    }
    Outcome::Feedback(translate(
        "commands.fill.success",
        vec![Value::from(changed)],
    ))
}

/// `/fillbiome`.
pub(super) fn fillbiome(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let tokens = (
        input.word(),
        input.word(),
        input.word(),
        input.word(),
        input.word(),
        input.word(),
    );
    let (Some(x1), Some(y1), Some(z1), Some(x2), Some(y2), Some(z2)) = tokens else {
        return usage("/fillbiome <from> <to> <biome> [replace <filter>]");
    };
    let (Some(from_c), Some(to_c)) = (args::coords3(x1, y1, z1), args::coords3(x2, y2, z2)) else {
        return failed("argument.pos.invalid", Vec::new());
    };
    let base = call.position(ctx);
    let basis = args::Basis::from_rotation(0.0, 0.0);
    let from = args::block_pos(from_c, base, basis);
    let to = args::block_pos(to_c, base, basis);
    let Some(biome_token) = input.word() else {
        return usage("/fillbiome <from> <to> <biome> [replace <filter>]");
    };
    let filter = match input.word() {
        None => None,
        Some("replace") => {
            let Some(token) = input.word() else {
                return usage("/fillbiome <from> <to> <biome> replace <filter>");
            };
            Some(token.to_owned())
        }
        Some(_) => return usage("/fillbiome <from> <to> <biome> [replace <filter>]"),
    };
    if input.word().is_some() {
        return usage("/fillbiome <from> <to> <biome> [replace <filter>]");
    }
    if !ctx.plan.in_world(from) || !ctx.plan.in_world(to) {
        return failed("argument.pos.outofbounds", Vec::new());
    }
    let volume = (i64::from(to[0] - from[0]).abs() + 1)
        * (i64::from(to[1] - from[1]).abs() + 1)
        * (i64::from(to[2] - from[2]).abs() + 1);
    let limit = ctx.vars().number("max_block_modifications").max(0);
    if volume > limit {
        return failed(
            "commands.fillbiome.toobig",
            vec![Value::from(limit), Value::from(volume)],
        );
    }
    let biome_name = args::resource_name(biome_token);
    if biome_token.starts_with('#') {
        return failed(
            "argument.resource.not_found",
            vec![
                Value::String(biome_name),
                Value::String("minecraft:worldgen/biome".into()),
            ],
        );
    }
    let Some(biome) = biome_id(ctx, &biome_name) else {
        return failed(
            "argument.resource.not_found",
            vec![
                Value::String(biome_name),
                Value::String("minecraft:worldgen/biome".into()),
            ],
        );
    };
    let filter = match filter.as_deref() {
        None => None,
        Some(token) => match biome_filter(ctx, token) {
            Ok(ids) => Some(ids),
            Err(outcome) => return outcome,
        },
    };
    match ctx.plan.fill_biomes(from, to, biome, filter.as_ref()) {
        Err(key) => failed(key, Vec::new()),
        Ok(0) => failed("commands.fillbiome.no_changes", Vec::new()),
        Ok(count) => Outcome::Feedback(translate(
            "commands.fillbiome.success.count",
            vec![
                Value::from(count),
                Value::from(from[0]),
                Value::from(from[1]),
                Value::from(from[2]),
                Value::from(to[0]),
                Value::from(to[1]),
                Value::from(to[2]),
            ],
        )),
    }
}

fn biome_id(ctx: &Context<'_>, name: &str) -> Option<u32> {
    ctx.plan
        .survival
        .biome_names
        .iter()
        .position(|known| known == name)
        .map(|index| index as u32)
}

fn biome_filter(ctx: &Context<'_>, token: &str) -> Result<std::collections::HashSet<u32>, Outcome> {
    let mut ids = std::collections::HashSet::new();
    if let Some(tag) = token.strip_prefix('#') {
        let tag = args::resource_name(tag);
        let Some(names) = ctx.plan.survival.biome_tags.get(&tag) else {
            return Err(failed(
                "argument.resource_tag.not_found",
                vec![
                    Value::String(tag),
                    Value::String("minecraft:worldgen/biome".into()),
                ],
            ));
        };
        for name in names {
            if let Some(id) = biome_id(ctx, name) {
                ids.insert(id);
            }
        }
        return Ok(ids);
    }
    let name = args::resource_name(token);
    let Some(id) = biome_id(ctx, &name) else {
        return Err(failed(
            "argument.resource.not_found",
            vec![
                Value::String(name),
                Value::String("minecraft:worldgen/biome".into()),
            ],
        ));
    };
    ids.insert(id);
    Ok(ids)
}

pub(super) fn failed(key: &str, with: Vec<Value>) -> Outcome {
    let mut value = translate(key, with);
    if let Some(object) = value.as_object_mut() {
        object.insert("color".into(), Value::String("red".into()));
    }
    Outcome::Feedback(value)
}

fn fill_mode(token: &str) -> Option<FillMode> {
    Some(match token {
        "replace" => FillMode::Replace,
        "destroy" => FillMode::Destroy,
        "hollow" => FillMode::Hollow,
        "keep" => FillMode::Keep,
        "outline" => FillMode::Outline,
        "strict" => FillMode::Strict,
        _ => return None,
    })
}

/// Какие блоки `/clone` берёт из источника.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloneMask<'a> {
    Replace,
    /// Воздух источника пропускается.
    Masked,
    Filtered(&'a str),
}

/// Можно ли накладывать области и стирать источник.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloneMode {
    /// Пересечение областей — ошибка.
    Normal,
    /// Пересечение разрешено.
    Force,
    /// После копирования источник становится воздухом.
    Move,
}

/// Почему `/clone` не скопировал блоки.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloneError {
    Overlap,
    TooBig { limit: i64, volume: i64 },
    Unloaded,
    Failed,
    OutOfBounds,
    BadDimension,
}

/// Как `/fill` обходится с уже стоящими блоками.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillMode {
    Replace,
    Destroy,
    Hollow,
    Keep,
    Outline,
    /// Постановка как `replace`, но команда падает, если ни один блок не сменился.
    Strict,
}

/// `/summon <entity> [pos] [nbt]`.
pub(super) fn summon(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(name) = input.word() else {
        return usage("/summon <entity> [pos]");
    };
    let name = args::resource_name(name);
    let coords = match (input.word(), input.word(), input.word()) {
        (Some(x), Some(y), Some(z)) => {
            let Some(c) = args::coords3(x, y, z) else {
                return Outcome::Feedback(translate("argument.pos.invalid", Vec::new()));
            };
            args::Basis::from_rotation(0.0, 0.0).resolve(c, call.position(ctx))
        }
        (Some(_), _, _) => return usage("/summon <entity> [x] [y] [z]"),
        _ => call.position(ctx),
    };
    // `minecraft:lightning_bolt` — не моб: она бьёт в точку и отдаёт разряд
    // молниеотводу, если он рядом (`LightningBolt`).
    if name == "minecraft:lightning_bolt" {
        let at = [
            coords[0].floor() as i32,
            coords[1].floor() as i32,
            coords[2].floor() as i32,
        ];
        let struck = ctx.plan.lightning_strike(at);
        return match struck {
            Some(rod) => Outcome::Feedback(translate(
                "commands.summon.success",
                vec![
                    json!({"translate": "entity.minecraft.lightning_bolt"}),
                    Value::from(format!("{} {} {}", rod[0], rod[1], rod[2])),
                ],
            )),
            None => Outcome::Feedback(translate(
                "commands.summon.success",
                vec![
                    json!({"translate": "entity.minecraft.lightning_bolt"}),
                    Value::from(format!("{} {} {}", at[0], at[1], at[2])),
                ],
            )),
        };
    }
    let Some(species) = crate::mobs::Species::from_name(&name) else {
        return Outcome::Feedback(translate(
            "argument.entity.id.invalid",
            vec![Value::String(name)],
        ));
    };
    match ctx.plan.summon_mob(species, coords) {
        Some(id) => Outcome::Feedback(translate(
            "commands.summon.success",
            vec![
                json!({"translate": format!("entity.{}", name.replace(':', "."))}),
                Value::from(id),
            ],
        )),
        None => Outcome::Feedback(translate("commands.summon.failed", Vec::new())),
    }
}

/// `/weather <clear|rain|thunder> [duration]`.
///
/// Вид погоды совпадает с литералом дерева (`/weather rain`), поэтому он
/// приходит в [`Call::path`], а не в аргументах.
pub(super) fn weather(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let kind = match call.path.first().map(String::as_str) {
        Some(kind @ ("clear" | "rain" | "thunder")) => kind,
        _ => match input.word() {
            Some(kind @ ("clear" | "rain" | "thunder")) => kind,
            _ => return usage("/weather <clear|rain|thunder> [duration]"),
        },
    };
    let ticks = match input.word() {
        Some(token) => match args::time_ticks(token).filter(|t| *t > 0) {
            Some(t) => t,
            None => return usage("/weather <clear|rain|thunder> [duration]"),
        },
        None => 6_000,
    };
    let weather = match kind {
        "clear" => super::state::Weather {
            rain: false,
            thunder: false,
            ticks,
        },
        "rain" => super::state::Weather {
            rain: true,
            thunder: false,
            ticks,
        },
        "thunder" => super::state::Weather {
            rain: true,
            thunder: true,
            ticks,
        },
        _ => return usage("/weather <clear|rain|thunder> [duration]"),
    };
    ctx.vars().set_weather(weather);
    ctx.plan.broadcast_weather(weather);
    Outcome::Feedback(translate(
        &format!("commands.weather.set.{}", kind),
        vec![Value::from(ticks / 20)],
    ))
}

/// `/gamerule [rule] [value]`.
///
/// Имя правила — литерал дерева (`/gamerule keep_inventory`), поэтому оно
/// приходит в [`Call::path`], а значение — в аргументах.
pub(super) fn gamerule(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(name) = call
        .path
        .first()
        .map(String::as_str)
        .or_else(|| input.word())
    else {
        let mut rules = ctx.plan.gamerules();
        rules.sort_by(|a, b| a.0.cmp(&b.0));
        let lines: Vec<String> = rules
            .into_iter()
            .map(|(name, value)| format!("{name} = {}", value.render()))
            .collect();
        return Outcome::text(lines.join("\n"));
    };
    let Some(current) = ctx.vars().rule(name) else {
        return Outcome::Feedback(translate(
            "argument.gamerule.invalid",
            vec![Value::String(name.to_owned())],
        ));
    };
    let Some(token) = input.word() else {
        return Outcome::Feedback(translate(
            "commands.gamerule.query",
            vec![json!({"text": name}), json!({"text": current.render()})],
        ));
    };
    let Some(value) = current.parse_like(token) else {
        return Outcome::Feedback(translate(
            "argument.gamerule.invalid",
            vec![json!({"text": name}), json!({"text": token})],
        ));
    };
    ctx.vars().set_rule(name, value.clone());
    Outcome::Feedback(translate(
        "commands.gamerule.set",
        vec![json!({"text": name}), json!({"text": value.render()})],
    ))
}

/// `/clone [from <dimension>] <begin> <end> [to <dimension>] <destination> …`.
pub(super) fn clone(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let source_dim = if call.path.first().map(String::as_str) == Some("from") {
        let Some(dim) = input.word() else {
            return usage("/clone [from <dimension>] <begin> <end> [to <dimension>] <destination>");
        };
        Some(dim.to_owned())
    } else {
        None
    };
    if let Some(dim) = source_dim.as_deref()
        && !ctx.plan.is_current_dimension(dim)
    {
        return Outcome::Feedback(translate(
            "argument.dimension.invalid",
            vec![Value::String(args::resource_name(dim))],
        ));
    }
    let Some(begin) = read_block(&mut input, call, ctx) else {
        return if input.rest().is_empty() {
            usage("/clone [from <dimension>] <begin> <end> [to <dimension>] <destination>")
        } else {
            Outcome::Feedback(translate("argument.pos.invalid", Vec::new()))
        };
    };
    let Some(end) = read_block(&mut input, call, ctx) else {
        return Outcome::Feedback(translate("argument.pos.invalid", Vec::new()));
    };
    let dest_dim = if input.rest().split_whitespace().next() == Some("to") {
        input.word();
        input.word().map(str::to_owned)
    } else {
        None
    };
    if let Some(dim) = dest_dim.as_deref()
        && !ctx.plan.is_current_dimension(dim)
    {
        return Outcome::Feedback(translate(
            "argument.dimension.invalid",
            vec![Value::String(args::resource_name(dim))],
        ));
    }
    let Some(dest) = read_block(&mut input, call, ctx) else {
        return Outcome::Feedback(translate("argument.pos.invalid", Vec::new()));
    };
    let mut strict = false;
    let mut masked = false;
    let mut filter = None;
    let mut mode = CloneMode::Normal;
    let mut word = input.word();
    if word == Some("strict") {
        strict = true;
        word = input.word();
    }
    match word {
        None => {}
        Some("replace") => {}
        Some("masked") => masked = true,
        Some("filtered") => {
            let Some(name) = input.word() else {
                return usage("/clone … filtered <filter> [force|move|normal]");
            };
            filter = Some(name.to_owned());
        }
        Some(_) => {
            return usage(
                "/clone … <destination> [replace|masked|filtered|strict] [force|move|normal]",
            );
        }
    }
    if !read_clone_mode(&mut input, &mut mode) {
        return usage("/clone … [force|move|normal]");
    }
    let mask = if let Some(filter) = filter.as_deref() {
        CloneMask::Filtered(filter)
    } else if masked {
        CloneMask::Masked
    } else {
        CloneMask::Replace
    };
    match ctx.plan.clone_blocks(begin, end, dest, mask, mode, strict) {
        Ok(count) => Outcome::Feedback(translate(
            "commands.clone.success",
            vec![Value::from(count)],
        )),
        Err(CloneError::Overlap) => {
            Outcome::Feedback(translate("commands.clone.overlap", Vec::new()))
        }
        Err(CloneError::TooBig { limit, volume }) => Outcome::Feedback(translate(
            "commands.clone.toobig",
            vec![Value::from(limit), Value::from(volume)],
        )),
        Err(CloneError::Unloaded) => {
            Outcome::Feedback(translate("argument.pos.unloaded", Vec::new()))
        }
        Err(CloneError::OutOfBounds) => {
            Outcome::Feedback(translate("argument.pos.outofbounds", Vec::new()))
        }
        Err(CloneError::Failed | CloneError::BadDimension) => {
            Outcome::Feedback(translate("commands.clone.failed", Vec::new()))
        }
    }
}

fn read_block(input: &mut args::Input<'_>, call: &Call<'_>, ctx: &Context<'_>) -> Option<[i32; 3]> {
    let (x, y, z) = (input.word()?, input.word()?, input.word()?);
    let coords = args::coords3(x, y, z)?;
    let (yaw, pitch) = call
        .source
        .uuid
        .and_then(|uuid| ctx.pose(uuid))
        .map(|(_, yaw, pitch)| (yaw, pitch))
        .unwrap_or((0.0, 0.0));
    Some(args::block_pos(
        coords,
        call.position(ctx),
        args::Basis::from_rotation(yaw, pitch),
    ))
}

fn read_clone_mode(input: &mut args::Input<'_>, mode: &mut CloneMode) -> bool {
    match input.word() {
        None => true,
        Some("force") => {
            *mode = CloneMode::Force;
            true
        }
        Some("move") => {
            *mode = CloneMode::Move;
            true
        }
        Some("normal") => {
            *mode = CloneMode::Normal;
            true
        }
        Some(_) => false,
    }
}

/// `/worldborder`.
pub(super) fn worldborder(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let now = ctx.plan.border_now();
    let route: Vec<&str> = call.path.iter().map(String::as_str).collect();
    match route.as_slice() {
        ["get"] => {
            let size = ctx.vars().border_at(now).size_at(now);
            Outcome::Feedback(translate(
                "commands.worldborder.get",
                vec![size_value(size)],
            ))
        }
        ["set"] => resize_border(ctx, &mut input, now, false),
        ["add"] => resize_border(ctx, &mut input, now, true),
        ["center"] => {
            let (Some(x), Some(z)) = (input.word(), input.word()) else {
                return usage("/worldborder center <pos>");
            };
            let Some((x, z)) = column_xz(x, z, call, ctx) else {
                return Outcome::Feedback(translate("argument.pos2d.incomplete", Vec::new()));
            };
            if !ctx.vars().set_border_center(x, z, now) {
                return Outcome::Feedback(translate(
                    "commands.worldborder.center.failed",
                    Vec::new(),
                ));
            }
            ctx.plan.broadcast_border_center(x, z);
            Outcome::Feedback(translate(
                "commands.worldborder.center.success",
                vec![json!(x), json!(z)],
            ))
        }
        ["damage", "amount"] => border_damage(ctx, &mut input, true),
        ["damage", "buffer"] => border_damage(ctx, &mut input, false),
        ["warning", "distance"] => border_warning(ctx, &mut input, true),
        ["warning", "time"] => border_warning(ctx, &mut input, false),
        _ => usage("/worldborder <set|add|center|damage|warning|get>"),
    }
}

fn resize_border(
    ctx: &Context<'_>,
    input: &mut args::Input<'_>,
    now: i64,
    adding: bool,
) -> Outcome {
    let Some(distance) = input.word().and_then(|token| token.parse::<f64>().ok()) else {
        return usage("/worldborder <set|add> <distance> [<time>]");
    };
    let ticks = match input.word() {
        None => 0,
        Some(token) => match args::time_ticks(token).filter(|ticks| *ticks >= 0) {
            Some(ticks) => ticks,
            None => return usage("/worldborder <set|add> <distance> [<time>]"),
        },
    };
    let before = ctx.vars().border_at(now).size_at(now);
    match ctx.vars().resize_border(distance, ticks, now, adding) {
        super::state::BorderResize::Immediate(size) => {
            ctx.plan.broadcast_border_size(size);
            Outcome::Feedback(translate(
                "commands.worldborder.set.immediate",
                vec![size_value(size)],
            ))
        }
        super::state::BorderResize::Grow { size, ticks } => {
            ctx.plan.broadcast_border_lerp(before, size, ticks);
            Outcome::Feedback(translate(
                "commands.worldborder.set.grow",
                vec![size_value(size), Value::from(ticks / 20)],
            ))
        }
        super::state::BorderResize::Shrink { size, ticks } => {
            ctx.plan.broadcast_border_lerp(before, size, ticks);
            Outcome::Feedback(translate(
                "commands.worldborder.set.shrink",
                vec![size_value(size), Value::from(ticks / 20)],
            ))
        }
        super::state::BorderResize::NoChange => Outcome::Feedback(translate(
            "commands.worldborder.set.failed.nochange",
            Vec::new(),
        )),
        super::state::BorderResize::TooSmall => Outcome::Feedback(translate(
            "commands.worldborder.set.failed.small",
            Vec::new(),
        )),
        super::state::BorderResize::TooBig => {
            Outcome::Feedback(translate("commands.worldborder.set.failed.big", Vec::new()))
        }
        super::state::BorderResize::TooFar => {
            Outcome::Feedback(translate("commands.worldborder.set.failed.far", Vec::new()))
        }
    }
}

fn border_damage(ctx: &Context<'_>, input: &mut args::Input<'_>, amount: bool) -> Outcome {
    let Some(value) = input
        .word()
        .and_then(|token| token.parse::<f32>().ok())
        .filter(|v| *v >= 0.0)
    else {
        return usage("/worldborder damage <amount|buffer> <value>");
    };
    let changed = if amount {
        ctx.vars().set_border_damage(Some(value), None)
    } else {
        ctx.vars().set_border_damage(None, Some(value))
    };
    if !changed {
        let key = if amount {
            "commands.worldborder.damage.amount.failed"
        } else {
            "commands.worldborder.damage.buffer.failed"
        };
        return Outcome::Feedback(translate(key, Vec::new()));
    }
    let key = if amount {
        "commands.worldborder.damage.amount.success"
    } else {
        "commands.worldborder.damage.buffer.success"
    };
    Outcome::Feedback(translate(key, vec![json!(value)]))
}

fn border_warning(ctx: &Context<'_>, input: &mut args::Input<'_>, distance: bool) -> Outcome {
    let value = if distance {
        input
            .word()
            .and_then(|token| token.parse::<i32>().ok())
            .filter(|v| *v >= 0)
    } else {
        input
            .word()
            .and_then(args::time_ticks)
            .filter(|ticks| *ticks >= 0)
            .and_then(|ticks| i32::try_from(ticks).ok())
    };
    let Some(value) = value else {
        return usage("/worldborder warning <distance|time> <value>");
    };
    let changed = if distance {
        ctx.vars().set_border_warning(Some(value), None)
    } else {
        ctx.vars().set_border_warning(None, Some(value))
    };
    if !changed {
        let key = if distance {
            "commands.worldborder.warning.distance.failed"
        } else {
            "commands.worldborder.warning.time.failed"
        };
        return Outcome::Feedback(translate(key, Vec::new()));
    }
    ctx.plan
        .broadcast_border_warning(distance.then_some(value), (!distance).then_some(value));
    let key = if distance {
        "commands.worldborder.warning.distance.success"
    } else {
        "commands.worldborder.warning.time.success"
    };
    Outcome::Feedback(translate(key, vec![Value::from(value)]))
}

fn size_value(size: f64) -> Value {
    if (size - size.round()).abs() < 1e-4 && size.abs() < i64::MAX as f64 {
        Value::from(size.round() as i64)
    } else {
        json!(size)
    }
}

fn column_xz(x: &str, z: &str, call: &Call<'_>, ctx: &Context<'_>) -> Option<(f64, f64)> {
    let coords = [args::Coord::parse(x)?, args::Coord::parse(z)?];
    let base = call.position(ctx);
    let (yaw, pitch) = call
        .source
        .uuid
        .and_then(|uuid| ctx.pose(uuid))
        .map(|(_, yaw, pitch)| (yaw, pitch))
        .unwrap_or((0.0, 0.0));
    let world = args::Basis::from_rotation(yaw, pitch)
        .resolve([coords[0], args::Coord::Abs(base[1]), coords[1]], base);
    Some((world[0], world[2]))
}

fn fillbiome_spec() -> super::dispatch::Spec {
    use super::dispatch::Spec;
    use super::registry::{GAMEMASTERS, block_pos};
    let filter = Spec::arg_leaf(
        "filter",
        GAMEMASTERS,
        chunkwake_protocol::text::Parser::ResourceOrTag("minecraft:worldgen/biome"),
        fillbiome,
    );
    // Биом исполняем: `replace` — продолжение, а не обязательный хвост.
    let biome = Spec::arg_leaf(
        "biome",
        GAMEMASTERS,
        chunkwake_protocol::text::Parser::Resource("minecraft:worldgen/biome"),
        fillbiome,
    )
    .with_children(vec![Spec::group("replace", GAMEMASTERS, vec![filter])]);
    Spec::group(
        "fillbiome",
        GAMEMASTERS,
        vec![Spec::arg(
            "from",
            GAMEMASTERS,
            block_pos(),
            vec![Spec::arg("to", GAMEMASTERS, block_pos(), vec![biome])],
        )],
    )
    .with_handler(fillbiome)
}

/// Узел-аргумент с обработчиком, но без `executable`: строка продолжается
/// дальше (`/setblock <pos> <block>`).
fn arg_group(
    name: &str,
    level: i32,
    parser: chunkwake_protocol::text::Parser,
    handler: super::dispatch::Handler,
) -> super::dispatch::Spec {
    super::dispatch::Spec::arg_group(name, level, parser, handler)
}

fn clone_modes(handler: super::dispatch::Handler) -> Vec<super::dispatch::Spec> {
    use super::dispatch::Spec;
    use super::registry::GAMEMASTERS;
    ["force", "move", "normal"]
        .into_iter()
        .map(|mode| Spec::literal_leaf(mode, GAMEMASTERS, handler))
        .collect()
}

fn clone_masks(handler: super::dispatch::Handler, with_strict: bool) -> Vec<super::dispatch::Spec> {
    use super::dispatch::Spec;
    use super::registry::{GAMEMASTERS, block_predicate};
    let filter = Spec::arg_leaf("filter", GAMEMASTERS, block_predicate(), handler)
        .with_children(clone_modes(handler));
    let mut nodes = vec![
        Spec::group("filtered", GAMEMASTERS, vec![filter]),
        Spec::exec("masked", GAMEMASTERS, handler, clone_modes(handler)),
        Spec::exec("replace", GAMEMASTERS, handler, clone_modes(handler)),
    ];
    if with_strict {
        nodes.push(Spec::exec(
            "strict",
            GAMEMASTERS,
            handler,
            clone_masks(handler, false),
        ));
    }
    nodes
}

fn clone_destination(handler: super::dispatch::Handler) -> super::dispatch::Spec {
    use super::dispatch::Spec;
    use super::registry::{GAMEMASTERS, block_pos};
    Spec::arg_leaf("destination", GAMEMASTERS, block_pos(), handler)
        .with_children(clone_masks(handler, true))
}

fn clone_end(handler: super::dispatch::Handler) -> super::dispatch::Spec {
    use super::dispatch::Spec;
    use super::registry::{GAMEMASTERS, block_pos, dimension};
    Spec::arg(
        "end",
        GAMEMASTERS,
        block_pos(),
        vec![
            clone_destination(handler),
            Spec::group(
                "to",
                GAMEMASTERS,
                vec![Spec::arg(
                    "targetDimension",
                    GAMEMASTERS,
                    dimension(),
                    vec![clone_destination(handler)],
                )],
            ),
        ],
    )
}

fn clone_spec() -> super::dispatch::Spec {
    use super::dispatch::Spec;
    use super::registry::{GAMEMASTERS, block_pos, dimension};
    let begin = Spec::arg("begin", GAMEMASTERS, block_pos(), vec![clone_end(clone)]);
    Spec::group(
        "clone",
        GAMEMASTERS,
        vec![
            begin,
            Spec::group(
                "from",
                GAMEMASTERS,
                vec![Spec::arg(
                    "sourceDimension",
                    GAMEMASTERS,
                    dimension(),
                    vec![Spec::arg(
                        "begin",
                        GAMEMASTERS,
                        block_pos(),
                        vec![clone_end(clone)],
                    )],
                )],
            ),
        ],
    )
    .with_handler(clone)
}

fn worldborder_distance(handler: super::dispatch::Handler) -> super::dispatch::Spec {
    use super::dispatch::Spec;
    use super::registry::{GAMEMASTERS, double, time};
    use super::state::BORDER_MAX_SIZE;
    Spec::arg_leaf(
        "distance",
        GAMEMASTERS,
        double(-BORDER_MAX_SIZE, BORDER_MAX_SIZE),
        handler,
    )
    .with_children(vec![Spec::arg_leaf("time", GAMEMASTERS, time(0), handler)])
}

fn worldborder_spec() -> super::dispatch::Spec {
    use super::dispatch::Spec;
    use super::registry::{GAMEMASTERS, float, int, time};
    Spec::group(
        "worldborder",
        GAMEMASTERS,
        vec![
            Spec::group("add", GAMEMASTERS, vec![worldborder_distance(worldborder)]),
            Spec::group(
                "center",
                GAMEMASTERS,
                vec![Spec::arg_leaf(
                    "pos",
                    GAMEMASTERS,
                    chunkwake_protocol::text::Parser::Vec2,
                    worldborder,
                )],
            ),
            Spec::group(
                "damage",
                GAMEMASTERS,
                vec![
                    Spec::group(
                        "amount",
                        GAMEMASTERS,
                        vec![Spec::arg_leaf(
                            "damagePerBlock",
                            GAMEMASTERS,
                            float(0.0, f32::MAX as f64),
                            worldborder,
                        )],
                    ),
                    Spec::group(
                        "buffer",
                        GAMEMASTERS,
                        vec![Spec::arg_leaf(
                            "distance",
                            GAMEMASTERS,
                            float(0.0, f32::MAX as f64),
                            worldborder,
                        )],
                    ),
                ],
            ),
            Spec::literal_leaf("get", GAMEMASTERS, worldborder),
            Spec::group("set", GAMEMASTERS, vec![worldborder_distance(worldborder)]),
            Spec::group(
                "warning",
                GAMEMASTERS,
                vec![
                    Spec::group(
                        "distance",
                        GAMEMASTERS,
                        vec![Spec::arg_leaf(
                            "distance",
                            GAMEMASTERS,
                            int(0, i32::MAX),
                            worldborder,
                        )],
                    ),
                    Spec::group(
                        "time",
                        GAMEMASTERS,
                        vec![Spec::arg_leaf("time", GAMEMASTERS, time(0), worldborder)],
                    ),
                ],
            ),
        ],
    )
    .with_handler(worldborder)
}

/// Узлы дерева команд мира: форма сверена с ванильным `commands.json` 26.3.
pub(super) fn specs() -> Vec<super::dispatch::Spec> {
    use super::dispatch::Spec;
    use super::registry::{GAMEMASTERS, column_pos};

    let duration = |handler: super::dispatch::Handler| {
        Spec::arg_leaf("duration", GAMEMASTERS, super::registry::time(1), handler)
    };
    vec![
        clone_spec(),
        worldborder_spec(),
        Spec::group(
            "setblock",
            GAMEMASTERS,
            vec![
                arg_group("pos", GAMEMASTERS, super::registry::block_pos(), setblock)
                    .with_children(vec![
                        Spec::arg_leaf(
                            "block",
                            GAMEMASTERS,
                            super::registry::block_state(),
                            setblock,
                        )
                        .with_children(
                            ["destroy", "keep", "replace", "strict"]
                                .into_iter()
                                .map(|mode| Spec::literal_leaf(mode, GAMEMASTERS, setblock))
                                .collect(),
                        ),
                    ]),
            ],
        ),
        Spec::group(
            "fill",
            GAMEMASTERS,
            vec![
                arg_group("from", GAMEMASTERS, super::registry::block_pos(), fill).with_children(
                    vec![
                        arg_group("to", GAMEMASTERS, super::registry::block_pos(), fill)
                            .with_children(vec![fill_block()]),
                    ],
                ),
            ],
        ),
        fillbiome_spec(),
        Spec::group(
            "summon",
            GAMEMASTERS,
            vec![
                Spec::arg_leaf(
                    "entity",
                    GAMEMASTERS,
                    super::registry::entity_type(),
                    summon,
                )
                .with_children(vec![
                    Spec::arg_leaf("pos", GAMEMASTERS, super::registry::vec3(), summon)
                        .with_children(vec![Spec::arg_leaf(
                            "nbt",
                            GAMEMASTERS,
                            super::registry::nbt_compound(),
                            summon,
                        )]),
                ]),
            ],
        ),
        // weather: корень не исполняем (нужен вид погоды), как в ванили.
        Spec::group(
            "weather",
            GAMEMASTERS,
            ["clear", "rain", "thunder"]
                .into_iter()
                .map(|kind| Spec::exec(kind, GAMEMASTERS, weather, vec![duration(weather)]))
                .collect(),
        ),
        Spec::exec("gamerule", GAMEMASTERS, gamerule, gamerule_nodes()),
        Spec::exec(
            "setworldspawn",
            GAMEMASTERS,
            setworldspawn,
            vec![
                Spec::arg_leaf(
                    "pos",
                    GAMEMASTERS,
                    super::registry::block_pos(),
                    setworldspawn,
                )
                .with_children(vec![Spec::arg_leaf(
                    "rotation",
                    GAMEMASTERS,
                    super::registry::rotation(),
                    setworldspawn,
                )]),
            ],
        ),
        Spec::group(
            "forceload",
            GAMEMASTERS,
            vec![
                Spec::group(
                    "add",
                    GAMEMASTERS,
                    vec![
                        Spec::arg_leaf("from", GAMEMASTERS, column_pos(), forceload).with_children(
                            vec![Spec::arg_leaf("to", GAMEMASTERS, column_pos(), forceload)],
                        ),
                    ],
                ),
                Spec::group(
                    "remove",
                    GAMEMASTERS,
                    vec![
                        Spec::literal_leaf("all", GAMEMASTERS, forceload),
                        Spec::arg_leaf("from", GAMEMASTERS, column_pos(), forceload).with_children(
                            vec![Spec::arg_leaf("to", GAMEMASTERS, column_pos(), forceload)],
                        ),
                    ],
                ),
                Spec::exec(
                    "query",
                    GAMEMASTERS,
                    forceload,
                    vec![Spec::arg_leaf("pos", GAMEMASTERS, column_pos(), forceload)],
                ),
            ],
        ),
        locate_spec(),
    ]
}

/// `/locate biome|poi|structure` в форме ванили 26.3.
fn locate_spec() -> super::dispatch::Spec {
    use super::dispatch::Spec;
    use super::registry::GAMEMASTERS;
    use chunkwake_protocol::text::Parser;
    let arg = |name: &str, parser: Parser| Spec::arg_leaf(name, GAMEMASTERS, parser, locate);
    Spec::group(
        "locate",
        GAMEMASTERS,
        vec![
            Spec::group(
                "biome",
                GAMEMASTERS,
                vec![arg(
                    "biome",
                    Parser::ResourceOrTag("minecraft:worldgen/biome"),
                )],
            ),
            Spec::group(
                "poi",
                GAMEMASTERS,
                vec![arg(
                    "poi",
                    Parser::ResourceOrTag("minecraft:point_of_interest_type"),
                )],
            ),
            Spec::group(
                "structure",
                GAMEMASTERS,
                vec![arg(
                    "structure",
                    Parser::ResourceOrTagKey("minecraft:worldgen/structure"),
                )],
            ),
        ],
    )
    .with_handler(locate)
}

/// Хвост `/fill … <block>`: режимы и фильтр `replace`.
fn fill_block() -> super::dispatch::Spec {
    use super::dispatch::Spec;
    use super::registry::{GAMEMASTERS, block_predicate};
    let plain = |mode: &str| Spec::literal_leaf(mode, GAMEMASTERS, fill);
    let filter =
        Spec::arg_leaf("filter", GAMEMASTERS, block_predicate(), fill).with_children(vec![
            plain("destroy"),
            plain("hollow"),
            plain("outline"),
            plain("strict"),
        ]);
    Spec::arg_leaf("block", GAMEMASTERS, super::registry::block_state(), fill).with_children(vec![
        plain("destroy"),
        plain("hollow"),
        plain("keep"),
        plain("outline"),
        Spec::exec("replace", GAMEMASTERS, fill, vec![filter]),
        plain("strict"),
    ])
}

/// Правила объявлены в дереве поимённо, как в ванили: клиент подсказывает их.
fn gamerule_nodes() -> Vec<super::dispatch::Spec> {
    use super::registry::GAMEMASTERS;
    super::state::rule_specs()
        .into_iter()
        .map(|(name, parser)| {
            super::dispatch::Spec::exec(
                &name,
                GAMEMASTERS,
                gamerule,
                vec![super::dispatch::Spec::arg_leaf(
                    "value",
                    GAMEMASTERS,
                    parser,
                    gamerule,
                )],
            )
        })
        .collect()
}

/// `/setworldspawn [pos]`.
fn setworldspawn(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let pos = match (input.word(), input.word(), input.word()) {
        (Some(x), Some(y), Some(z)) => {
            let Some(c) = args::coords3(x, y, z) else {
                return usage("/setworldspawn [x] [y] [z]");
            };
            args::Basis::from_rotation(0.0, 0.0).resolve(c, call.position(ctx))
        }
        _ => call.position(ctx),
    };
    ctx.vars().set_world_spawn(pos);
    ctx.plan.broadcast_world_spawn();
    Outcome::Feedback(translate(
        "commands.setworldspawn.success",
        vec![
            Value::from(pos[0]),
            Value::from(pos[1]),
            Value::from(pos[2]),
        ],
    ))
}

/// `/locate biome` ищет по климату. `structure` — по сиду среди поставленных
/// наборов. `poi` — по блокам уже загруженных чанков.
fn locate(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(kind) = call.path.first().map(String::as_str) else {
        return usage("/locate <biome|poi|structure> <id>");
    };
    let Some(name) = input.word() else {
        return usage("/locate <biome|poi|structure> <id>");
    };
    let name = args::resource_name(name);
    let here = call.position(ctx);
    let origin = [
        here[0].floor() as i32,
        here[1].floor() as i32,
        here[2].floor() as i32,
    ];
    let (key, found) = match kind {
        "biome" => (
            "commands.locate.biome.success",
            ctx.plan.locate_biome(&name, origin),
        ),
        "poi" => (
            "commands.locate.poi.success",
            ctx.plan.locate_poi(&name, origin),
        ),
        "structure" => (
            "commands.locate.structure.success",
            ctx.plan.locate_structure(&name, origin),
        ),
        _ => return usage("/locate <biome|poi|structure> <id>"),
    };
    let Some(found) = found else {
        return Outcome::Feedback(translate(
            "commands.locate.failed",
            vec![json!({"text": name})],
        ));
    };
    let distance = (((found[0] - origin[0]) as f64).hypot((found[2] - origin[2]) as f64)).round();
    Outcome::Feedback(translate(
        key,
        vec![
            json!({"text": name}),
            coordinates(found),
            Value::from(distance as i32),
        ],
    ))
}

/// `/forceload add|remove|query`: подкоманда — литерал дерева, поэтому
/// берётся из [`Call::path`].
fn forceload(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let sub = call
        .path
        .first()
        .map(String::as_str)
        .or_else(|| input.word());
    match sub {
        Some("add") => {
            let count = ctx.plan.force_load_here(call.position(ctx), true);
            Outcome::Feedback(translate(
                "commands.forceload.added",
                vec![Value::from(count)],
            ))
        }
        Some("remove") => {
            let count = ctx.plan.force_load_here(call.position(ctx), false);
            Outcome::Feedback(translate(
                "commands.forceload.removed",
                vec![Value::from(count)],
            ))
        }
        _ => {
            let count = ctx.plan.forced_chunks();
            Outcome::Feedback(translate(
                "commands.forceload.query",
                vec![Value::from(count)],
            ))
        }
    }
}
