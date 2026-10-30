//! `/data`: чтение и запись NBT блок-сущностей и сущностей.
//!
//! Путь `Foo.Bar[0]`, `merge` корня и `modify set|merge|append|prepend|insert`.

use super::Context;
use super::dispatch::{Call, Outcome, translate};
use super::entity::usage;
use super::snbt::{self, EditError, Modify};
use chunkwake_world::nbt::{Compound, Tag};
use serde_json::{Value, json};

/// `/data get|merge|modify <block|entity> …`.
pub(super) fn specs() -> Vec<super::dispatch::Spec> {
    use super::dispatch::Spec;
    use super::registry::{GAMEMASTERS, ent, nbt_path};

    let source = |name: &str, parser: chunkwake_protocol::text::Parser| {
        Spec::arg_leaf(name, GAMEMASTERS, parser, data).with_children(vec![
            Spec::arg_leaf("path", GAMEMASTERS, nbt_path(), data).with_children(vec![
                Spec::arg_leaf(
                    "scale",
                    GAMEMASTERS,
                    super::registry::double(-1.0e300, 1.0e300),
                    data,
                ),
            ]),
        ])
    };
    let block_pos = super::registry::block_pos();
    let entity = ent(false, true);
    let nbt = || Spec::arg_leaf("nbt", GAMEMASTERS, super::registry::nbt_compound(), data);
    let sources = || {
        vec![
            Spec::group(
                "from",
                GAMEMASTERS,
                vec![
                    Spec::group(
                        "block",
                        GAMEMASTERS,
                        vec![Spec::arg(
                            "source",
                            GAMEMASTERS,
                            block_pos.clone(),
                            vec![Spec::arg_leaf("sourcePath", GAMEMASTERS, nbt_path(), data)],
                        )],
                    ),
                    Spec::group(
                        "entity",
                        GAMEMASTERS,
                        vec![Spec::arg(
                            "source",
                            GAMEMASTERS,
                            entity.clone(),
                            vec![Spec::arg_leaf("sourcePath", GAMEMASTERS, nbt_path(), data)],
                        )],
                    ),
                ],
            ),
            Spec::group(
                "value",
                GAMEMASTERS,
                vec![Spec::arg_leaf(
                    "value",
                    GAMEMASTERS,
                    chunkwake_protocol::text::Parser::NbtTag,
                    data,
                )],
            ),
        ]
    };
    let modify_tail = || {
        let mut modes = vec!["set", "merge", "append", "prepend"]
            .into_iter()
            .map(|name| Spec::group(name, GAMEMASTERS, sources()))
            .collect::<Vec<_>>();
        modes.push(Spec::group(
            "insert",
            GAMEMASTERS,
            vec![Spec::arg(
                "index",
                GAMEMASTERS,
                super::registry::int(i32::MIN, i32::MAX),
                sources(),
            )],
        ));
        modes
    };
    let modify_target = |name: &str, parser: chunkwake_protocol::text::Parser| {
        Spec::arg(
            name,
            GAMEMASTERS,
            parser,
            vec![Spec::arg(
                "targetPath",
                GAMEMASTERS,
                nbt_path(),
                modify_tail(),
            )],
        )
    };
    vec![
        Spec::group(
            "data",
            GAMEMASTERS,
            vec![
                Spec::group(
                    "get",
                    GAMEMASTERS,
                    vec![
                        Spec::group(
                            "block",
                            GAMEMASTERS,
                            vec![source("target", block_pos.clone())],
                        ),
                        Spec::group(
                            "entity",
                            GAMEMASTERS,
                            vec![source("target", entity.clone())],
                        ),
                    ],
                ),
                Spec::group(
                    "merge",
                    GAMEMASTERS,
                    vec![
                        Spec::group(
                            "block",
                            GAMEMASTERS,
                            vec![Spec::arg(
                                "target",
                                GAMEMASTERS,
                                block_pos.clone(),
                                vec![nbt()],
                            )],
                        ),
                        Spec::group(
                            "entity",
                            GAMEMASTERS,
                            vec![Spec::arg(
                                "target",
                                GAMEMASTERS,
                                entity.clone(),
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
                            vec![modify_target("target", block_pos.clone())],
                        ),
                        Spec::group(
                            "entity",
                            GAMEMASTERS,
                            vec![modify_target("target", entity.clone())],
                        ),
                    ],
                ),
            ],
        )
        .with_handler(data),
    ]
}

pub(super) fn data(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(sub) = call.path.first().map(String::as_str) else {
        return usage("/data get|merge|modify <block|entity> …");
    };
    let Some(kind) = call.path.get(1).map(String::as_str) else {
        return usage("/data get|merge|modify <block|entity> …");
    };
    match (sub, kind) {
        ("get", "block") => get_block(ctx, call, &mut input),
        ("get", "entity") => get_entity(ctx, call, &mut input),
        ("merge", "block") => merge_block(ctx, call, &mut input),
        ("merge", "entity") => merge_entity(ctx, call, &mut input),
        ("modify", "block") => modify_block(ctx, call, &mut input),
        ("modify", "entity") => modify_entity(ctx, call, &mut input),
        _ => usage("/data get|merge|modify <block|entity> …"),
    }
}

fn get_block(ctx: &Context<'_>, call: &Call<'_>, input: &mut super::args::Input<'_>) -> Outcome {
    let Some(pos) = block_here(call, ctx, input) else {
        return usage("/data get block <pos> [<path> [<scale>]]");
    };
    let Some(root) = ctx.plan.block_nbt(pos) else {
        return block_invalid(pos);
    };
    report_get("block", &block_label(pos), &Tag::Compound(root), input)
}

fn get_entity(ctx: &Context<'_>, call: &Call<'_>, input: &mut super::args::Input<'_>) -> Outcome {
    let Some(target) = one_entity(ctx, call, input) else {
        return Outcome::Feedback(translate("argument.entity.notfound.entity", Vec::new()));
    };
    let root = ctx.plan.entity_nbt(&target);
    report_get("entity", &target.name, &Tag::Compound(root), input)
}

fn report_get(kind: &str, label: &str, root: &Tag, input: &mut super::args::Input<'_>) -> Outcome {
    let path = input.balanced().unwrap_or("");
    if path.is_empty() {
        return Outcome::Feedback(translate(
            &format!("commands.data.{kind}.query"),
            vec![
                json!({"text": label}),
                json!({"text": snbt::format_tag(root)}),
            ],
        ));
    }
    let Some(steps) = snbt::parse_path(path) else {
        return bad_path(path);
    };
    let Some(found) = snbt::get_path(root, &steps) else {
        return missing_path(path);
    };
    let scale = input
        .word()
        .and_then(|token| token.parse::<f64>().ok())
        .unwrap_or(1.0);
    let shown = if let Some(number) = snbt::number(&found) {
        json!({"text": format_scaled(number * scale)})
    } else if (scale - 1.0).abs() > f64::EPSILON {
        return missing_path(path);
    } else {
        json!({"text": snbt::format_tag(&found)})
    };
    Outcome::Feedback(translate(
        &format!("commands.data.{kind}.get"),
        vec![
            json!({"text": path}),
            json!({"text": label}),
            json!({"text": format_scaled(scale)}),
            shown,
        ],
    ))
}

fn merge_block(ctx: &Context<'_>, call: &Call<'_>, input: &mut super::args::Input<'_>) -> Outcome {
    let Some(pos) = block_here(call, ctx, input) else {
        return usage("/data merge block <pos> <nbt>");
    };
    let Some(patch) = compound_rest(input) else {
        return malformed();
    };
    let Some(mut root) = ctx.plan.block_nbt(pos) else {
        return block_invalid(pos);
    };
    let before = root.clone();
    snbt::merge_compounds(&mut root, &patch);
    if root == before {
        return Outcome::Feedback(translate("commands.data.merge.failed", Vec::new()));
    }
    if !ctx.plan.store_block_nbt(pos, root) {
        return block_invalid(pos);
    }
    modified_block(pos)
}

fn merge_entity(ctx: &Context<'_>, call: &Call<'_>, input: &mut super::args::Input<'_>) -> Outcome {
    let Some(target) = one_entity(ctx, call, input) else {
        return Outcome::Feedback(translate("argument.entity.notfound.entity", Vec::new()));
    };
    let Some(patch) = compound_rest(input) else {
        return malformed();
    };
    let mut root = Tag::Compound(ctx.plan.entity_nbt(&target));
    let before = root.clone();
    if let Tag::Compound(map) = &mut root {
        snbt::merge_compounds(map, &patch);
    }
    if root == before {
        return Outcome::Feedback(translate("commands.data.merge.failed", Vec::new()));
    }
    ctx.plan.store_entity_nbt(&target, take_compound(root));
    modified_entity(&target.name)
}

fn modify_block(ctx: &Context<'_>, call: &Call<'_>, input: &mut super::args::Input<'_>) -> Outcome {
    let Some(pos) = block_here(call, ctx, input) else {
        return usage("/data modify block <pos> <path> <mode> …");
    };
    let Some(root) = ctx.plan.block_nbt(pos) else {
        return block_invalid(pos);
    };
    let mut root = Tag::Compound(root);
    if let Err(error) = apply_modify(ctx, call, input, &mut root) {
        return error;
    }
    if !ctx.plan.store_block_nbt(pos, take_compound(root)) {
        return block_invalid(pos);
    }
    modified_block(pos)
}

fn modify_entity(
    ctx: &Context<'_>,
    call: &Call<'_>,
    input: &mut super::args::Input<'_>,
) -> Outcome {
    let Some(target) = one_entity(ctx, call, input) else {
        return Outcome::Feedback(translate("argument.entity.notfound.entity", Vec::new()));
    };
    let mut root = Tag::Compound(ctx.plan.entity_nbt(&target));
    if let Err(error) = apply_modify(ctx, call, input, &mut root) {
        return error;
    }
    ctx.plan.store_entity_nbt(&target, take_compound(root));
    modified_entity(&target.name)
}

fn apply_modify(
    ctx: &Context<'_>,
    call: &Call<'_>,
    input: &mut super::args::Input<'_>,
    root: &mut Tag,
) -> Result<(), Outcome> {
    let Some(path) = input.balanced() else {
        return Err(usage("/data modify <target> <path> <mode> …"));
    };
    let Some(steps) = snbt::parse_path(path) else {
        return Err(bad_path(path));
    };
    let Some(mode_word) = input.word() else {
        return Err(usage("/data modify <target> <path> <mode> …"));
    };
    let mode = if mode_word == "insert" {
        let Some(index) = input.word().and_then(|token| token.parse::<i32>().ok()) else {
            return Err(usage("/data modify <target> <path> insert <index> …"));
        };
        Modify::Insert(index)
    } else {
        match mode_word {
            "set" => Modify::Set,
            "merge" => Modify::Merge,
            "append" => Modify::Append,
            "prepend" => Modify::Prepend,
            _ => {
                return Err(usage(
                    "/data modify <target> <path> <set|merge|append|prepend|insert> …",
                ));
            }
        }
    };
    let Some(value) = source_value(ctx, call, input) else {
        return Err(malformed());
    };
    match snbt::modify(root, &steps, mode, value) {
        Ok(()) => Ok(()),
        Err(EditError::Type) => Err(Outcome::Feedback(translate(
            "commands.data.modify.invalid",
            vec![json!({"text": path})],
        ))),
        Err(EditError::Missing) | Err(EditError::Path) => Err(missing_path(path)),
    }
}

fn source_value(
    ctx: &Context<'_>,
    call: &Call<'_>,
    input: &mut super::args::Input<'_>,
) -> Option<Tag> {
    match input.word()? {
        "value" => snbt::parse_value(input.rest()),
        "from" => match input.word()? {
            "block" => {
                let pos = block_here(call, ctx, input)?;
                let path = input.balanced().unwrap_or("");
                let steps = snbt::parse_path(path)?;
                let root = Tag::Compound(ctx.plan.block_nbt(pos)?);
                snbt::get_path(&root, &steps)
            }
            "entity" => {
                let target = one_entity(ctx, call, input)?;
                let path = input.balanced().unwrap_or("");
                let steps = snbt::parse_path(path)?;
                let root = Tag::Compound(ctx.plan.entity_nbt(&target));
                snbt::get_path(&root, &steps)
            }
            _ => None,
        },
        _ => None,
    }
}

fn block_here(
    call: &Call<'_>,
    ctx: &Context<'_>,
    input: &mut super::args::Input<'_>,
) -> Option<[i32; 3]> {
    let (x, y, z) = (input.word()?, input.word()?, input.word()?);
    let coords = super::args::coords3(x, y, z)?;
    let pos = super::args::block_pos(
        coords,
        call.position(ctx),
        super::args::Basis::from_rotation(0.0, 0.0),
    );
    Some(pos)
}

fn one_entity(
    ctx: &Context<'_>,
    call: &Call<'_>,
    input: &mut super::args::Input<'_>,
) -> Option<super::selector::Target> {
    let token = input.word()?;
    let list = ctx.select(call.source, token, true).ok()?;
    (list.len() == 1).then(|| list.into_iter().next()).flatten()
}

fn compound_rest(input: &mut super::args::Input<'_>) -> Option<Compound> {
    let Tag::Compound(map) = snbt::parse_value(input.rest())? else {
        return None;
    };
    Some(map)
}

fn take_compound(tag: Tag) -> Compound {
    match tag {
        Tag::Compound(map) => map,
        _ => Compound::new(),
    }
}

fn block_invalid(pos: [i32; 3]) -> Outcome {
    Outcome::Feedback(translate(
        "commands.data.block.invalid",
        vec![
            Value::from(pos[0]),
            Value::from(pos[1]),
            Value::from(pos[2]),
        ],
    ))
}

fn modified_block(pos: [i32; 3]) -> Outcome {
    Outcome::Feedback(translate(
        "commands.data.block.modified",
        vec![json!({"text": block_label(pos)})],
    ))
}

fn modified_entity(name: &str) -> Outcome {
    Outcome::Feedback(translate(
        "commands.data.entity.modified",
        vec![json!({"text": name})],
    ))
}

fn missing_path(path: &str) -> Outcome {
    Outcome::Feedback(translate(
        "commands.data.get.invalid",
        vec![json!({"text": path})],
    ))
}

fn bad_path(path: &str) -> Outcome {
    Outcome::Feedback(translate(
        "argument.nbt.path.invalid",
        vec![json!({"text": path})],
    ))
}

fn malformed() -> Outcome {
    Outcome::Feedback(translate("argument.nbt.malformed", Vec::new()))
}

fn block_label(pos: [i32; 3]) -> String {
    format!("{}, {}, {}", pos[0], pos[1], pos[2])
}

fn format_scaled(value: f64) -> String {
    if value.fract() == 0.0 && value.is_finite() {
        format!("{value:.1}")
    } else {
        value.to_string()
    }
}
