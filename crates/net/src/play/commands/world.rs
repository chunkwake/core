//! Команды мира: `/setblock`, `/fill`, `/clone`, `/summon`, `/weather`,
//! `/gamerule`, `/time` живёт в [`super::meta`], `/forceload`, `/seed` тоже.

use super::args;
use super::dispatch::{translate, Call, Outcome};
use super::entity::usage;
use super::Context;
use serde_json::{json, Value};

/// Предел `/fill` за раз (ванильный `MAX_BLOCK_MODIFICATIONS`, уменьшенный:
/// один тик не должен превращаться в замирание мира).
const FILL_LIMIT: i64 = 32_768;

/// `/setblock <pos> <block> [destroy|keep|replace]`.
pub(super) fn setblock(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let (Some(x), Some(y), Some(z)) = (input.word(), input.word(), input.word()) else {
        return usage("/setblock <x> <y> <z> <block> [destroy|keep|replace]");
    };
    let Some(coords) = args::coords3(x, y, z) else {
        return Outcome::Feedback(translate("argument.pos.invalid", Vec::new()));
    };
    let base = call.position(ctx);
    let pos = args::block_pos(coords, base, args::Basis::from_rotation(0.0, 0.0));
    let Some(spec) = input.word() else {
        return usage("/setblock <x> <y> <z> <block> [destroy|keep|replace]");
    };
    let Some(state) = ctx.plan.parse_block_state(spec) else {
        return Outcome::Feedback(translate(
            "argument.block.id.invalid",
            vec![Value::String(spec.to_owned())],
        ));
    };
    match input.word() {
        None | Some("replace") => {}
        Some("keep") | Some("destroy") => {}
        Some(_) => return usage("/setblock <x> <y> <z> <block> [destroy|keep|replace]"),
    }
    if !ctx.plan.in_world(pos) {
        return Outcome::Feedback(translate("argument.pos.outofbounds", Vec::new()));
    }
    match ctx.plan.place_block(pos, state) {
        Some(true) => Outcome::Feedback(translate(
            "commands.setblock.success",
            vec![
                Value::from(pos[0]),
                Value::from(pos[1]),
                Value::from(pos[2]),
            ],
        )),
        _ => Outcome::Feedback(translate("commands.setblock.failed", Vec::new())),
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
    let mode = match input.word() {
        None | Some("replace") => super::world::FillMode::Replace,
        Some("destroy") => super::world::FillMode::Destroy,
        Some("hollow") => super::world::FillMode::Hollow,
        Some("keep") => super::world::FillMode::Keep,
        Some("outline") => super::world::FillMode::Outline,
        Some(_) => return usage("/fill <from> <to> <block> [replace|destroy|hollow|keep|outline]"),
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
    let changed = ctx.plan.fill_region(from, to, state, mode);
    Outcome::Feedback(translate(
        "commands.fill.success",
        vec![Value::from(changed)],
    ))
}

/// Как `/fill` обходится с уже стоящими блоками.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillMode {
    Replace,
    Destroy,
    Hollow,
    Keep,
    Outline,
}

/// `/summon <entity> [pos] [nbt]`.
pub(super) fn summon(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(name) = input.word() else {
        return usage("/summon <entity> [pos]");
    };
    let name = args::resource_name(name);
    let Some(species) = crate::mobs::Species::from_name(&name) else {
        return Outcome::Feedback(translate(
            "argument.entity.id.invalid",
            vec![Value::String(name)],
        ));
    };    let coords = match (input.word(), input.word(), input.word()) {
        (Some(x), Some(y), Some(z)) => {
            let Some(c) = args::coords3(x, y, z) else {
                return Outcome::Feedback(translate("argument.pos.invalid", Vec::new()));
            };
            args::Basis::from_rotation(0.0, 0.0).resolve(c, call.position(ctx))
        }
        (Some(_), _, _) => return usage("/summon <entity> [x] [y] [z]"),
        _ => call.position(ctx),
    };
    match ctx.plan.summon_mob(species, coords) {
        Some(id) => Outcome::Feedback(translate(
            "commands.summon.success",
            vec![json!({"translate": format!("entity.{}", name.replace(':', "."))}), Value::from(id)],
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
    let Some(name) = call.path.first().map(String::as_str).or_else(|| input.word()) else {
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
            vec![
                json!({"text": name}),
                json!({"text": current.render()}),
            ],
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

/// Узлы дерева команд мира: форма сверена с ванильным `commands.json` 26.3.
pub(super) fn specs() -> Vec<super::dispatch::Spec> {
    use super::dispatch::Spec;
    use super::registry::{column_pos, GAMEMASTERS};

    let duration = |handler: super::dispatch::Handler| {
        Spec::arg_leaf("duration", GAMEMASTERS, super::registry::time(1), handler)
    };
    vec![
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
                arg_group("from", GAMEMASTERS, super::registry::block_pos(), fill)
                    .with_children(vec![
                        arg_group("to", GAMEMASTERS, super::registry::block_pos(), fill)
                            .with_children(vec![fill_block()]),
                    ]),
            ],
        ),
        Spec::group(
            "summon",
            GAMEMASTERS,
            vec![
                Spec::arg_leaf("entity", GAMEMASTERS, super::registry::entity_type(), summon)
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
        Spec::exec(
            "gamerule",
            GAMEMASTERS,
            gamerule,
            gamerule_nodes(),
        ),
        Spec::exec(
            "setworldspawn",
            GAMEMASTERS,
            setworldspawn,
            vec![
                Spec::arg_leaf("pos", GAMEMASTERS, super::registry::block_pos(), setworldspawn)
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
                        Spec::arg_leaf("from", GAMEMASTERS, column_pos(), forceload)
                            .with_children(vec![Spec::arg_leaf(
                                "to",
                                GAMEMASTERS,
                                column_pos(),
                                forceload,
                            )]),
                    ],
                ),
                Spec::group(
                    "remove",
                    GAMEMASTERS,
                    vec![
                        Spec::literal_leaf("all", GAMEMASTERS, forceload),
                        Spec::arg_leaf("from", GAMEMASTERS, column_pos(), forceload)
                            .with_children(vec![Spec::arg_leaf(
                                "to",
                                GAMEMASTERS,
                                column_pos(),
                                forceload,
                            )]),
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
    ]
}

/// Хвост `/fill … <block>`: режимы и фильтр `replace`.
fn fill_block() -> super::dispatch::Spec {
    use super::dispatch::Spec;
    use super::registry::{block_predicate, GAMEMASTERS};
    let plain = |mode: &str| Spec::literal_leaf(mode, GAMEMASTERS, fill);
    let filter = Spec::arg_leaf("filter", GAMEMASTERS, block_predicate(), fill).with_children(
        vec![
            plain("destroy"),
            plain("hollow"),
            plain("outline"),
            plain("strict"),
        ],
    );
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
    Outcome::Feedback(translate(
        "commands.setworldspawn.success",
        vec![
            Value::from(pos[0]),
            Value::from(pos[1]),
            Value::from(pos[2]),
        ],
    ))
}

/// `/forceload add|remove|query`: подкоманда — литерал дерева, поэтому
/// берётся из [`Call::path`].
fn forceload(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let sub = call.path.first().map(String::as_str).or_else(|| input.word());
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
            Outcome::Feedback(translate("commands.forceload.query", vec![Value::from(count)]))
        }
    }
}

