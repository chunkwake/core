//! `/data`: доступ к NBT блок-сущностей и сущностей.
//!
//! Пока реализованы `get block`/`get entity` (чтение корня) и `merge block` для
//! сундуков: этого достаточно, чтобы команды не исчезали из дерева, а данные
//! оставались согласованными. Пути NBT (`Foo.Bar[0]`) — следующий шаг.

use super::dispatch::{error_component, translate, Call, Outcome};
use super::entity::usage;
use super::Context;
use serde_json::{json, Value};

/// `/data get|merge|modify <block|entity> …`.
///
/// Путь NBT и `scale` в дереве есть (ванильные имена узлов), но обработчик
/// пока читает только корень: `path`/`scale` принимаются и игнорируются.
pub(super) fn specs() -> Vec<super::dispatch::Spec> {
    use super::dispatch::Spec;
    use super::registry::{ent, nbt_path, GAMEMASTERS};

    // `<target> [<path> [<scale>]]`
    let source = |name: &str, parser: chunkwake_protocol::text::Parser| {
        Spec::arg_leaf(name, GAMEMASTERS, parser, data).with_children(vec![
            Spec::arg_leaf("path", GAMEMASTERS, nbt_path(), data).with_children(vec![
                Spec::arg_leaf("scale", GAMEMASTERS, super::registry::float(0.0, 3.4e38), data),
            ]),
        ])
    };
    let block = || source("target", super::registry::block_pos());
    let entity = || source("target", ent(false, true));
    let nbt = || {
        Spec::arg_leaf("nbt", GAMEMASTERS, super::registry::nbt_compound(), data)
    };
    vec![Spec::group(
        "data",
        GAMEMASTERS,
        vec![
            Spec::group(
                "get",
                GAMEMASTERS,
                vec![
                    Spec::group("block", GAMEMASTERS, vec![block()]),
                    Spec::group("entity", GAMEMASTERS, vec![entity()]),
                ],
            ),
            Spec::group(
                "merge",
                GAMEMASTERS,
                vec![
                    Spec::group(
                        "block",
                        GAMEMASTERS,
                        vec![Spec::group(
                            "target",
                            GAMEMASTERS,
                            vec![nbt()],
                        )],
                    ),
                    Spec::group(
                        "entity",
                        GAMEMASTERS,
                        vec![Spec::group(
                            "target",
                            GAMEMASTERS,
                            vec![nbt()],
                        )],
                    ),
                ],
            ),
            Spec::group(
                "modify",
                GAMEMASTERS,
                vec![
                    Spec::group(
                        "block",
                        GAMEMASTERS,
                        vec![Spec::arg_leaf(
                            "target",
                            GAMEMASTERS,
                            super::registry::block_pos(),
                            data,
                        )],
                    ),
                    Spec::group(
                        "entity",
                        GAMEMASTERS,
                        vec![Spec::arg_leaf("target", GAMEMASTERS, ent(false, true), data)],
                    ),
                ],
            ),
        ],
    )]
}
pub(super) fn data(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    // `get|merge|modify <block|entity>` — литералы дерева, поэтому приходят
    // в путь; в аргументах остаётся цель и путь NBT.
    let Some(sub) = call.path.first().map(String::as_str) else {
        return usage("/data get|merge|modify <block|entity> …");
    };
    let Some(kind) = call.path.get(1).map(String::as_str) else {
        return usage("/data get|merge|modify <block|entity> …");
    };
    match (sub, kind) {
        ("get", "block") => {
            let (Some(x), Some(y), Some(z)) = (input.word(), input.word(), input.word()) else {
                return usage("/data get block <x> <y> <z> [path]");
            };
            let Some(coords) = super::args::coords3(x, y, z) else {
                return Outcome::Feedback(translate("argument.pos.invalid", Vec::new()));
            };
            let pos = super::args::block_pos(
                coords,
                call.position(ctx),
                super::args::Basis::from_rotation(0.0, 0.0),
            );
            match ctx.plan.block_entity_snapshot(pos) {
                Some(value) => Outcome::Feedback(json!({
                    "text": format!(
                        "{} has the following block data: {value}",
                        format!("{pos:?}")
                    )
                })),
                None => Outcome::Feedback(translate(
                    "commands.data.block.invalid",
                    vec![
                        Value::from(pos[0]),
                        Value::from(pos[1]),
                        Value::from(pos[2]),
                    ],
                )),
            }
        }
        ("get", "entity") => {
            let Some(token) = input.word() else {
                return usage("/data get entity <target> [path]");
            };
            let targets = match ctx.select(call.source, token, true) {
                Ok(list) => list,
                Err(e) => return Outcome::Feedback(e.component()),
            };
            let Some(target) = targets.first() else {
                return Outcome::Feedback(translate(
                    "argument.entity.notfound.entity",
                    Vec::new(),
                ));
            };
            let value = ctx
                .plan
                .entity_snapshot(target)
                .unwrap_or_else(|| json!({"Pos": target.pos}));
            Outcome::Feedback(json!({
                "text": format!("{} has the following entity data: {value}", target.name)
            }))
        }
        _ => Outcome::Feedback(error_component(
            "/data: only `get block` and `get entity` are implemented",
        )),
    }
}
