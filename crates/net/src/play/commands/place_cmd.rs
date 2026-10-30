//! `/place template <template> <pos> [rotation] [mirror] [integrity] [seed]`.
//!
//! Постройка берётся из `data/minecraft/structure/**/*.nbt` того же кэша,
//! что использует генератор. Блоки ставятся как `/setblock replace`;
//! блочные сущности шаблона (сундук, табличка) не переносятся, и команда
//! говорит об этом в ответе.

use super::Context;
use super::args::{self, Basis};
use super::dispatch::{Call, Outcome, translate};
use super::entity::usage;
use super::world::failed;
use chunkwake_worldgen::templates::{self, Mirror, Rotation};
use serde_json::json;

/// `/place template …`
pub(super) fn place(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let path: Vec<&str> = call.path.iter().map(String::as_str).collect();
    match path.first().copied() {
        Some("template") => template(ctx, call),
        Some("feature") => Outcome::Feedback(json!({
            "text": "place feature is not supported: the world generator keeps features for chunk generation only",
            "color": "red",
        })),
        Some("jigsaw") => Outcome::Feedback(json!({
            "text": "place jigsaw is not supported: template pools are not loaded here",
            "color": "red",
        })),
        Some("structure") => Outcome::Feedback(json!({
            "text": "place structure is not supported yet: use \"place template <name> <pos>\" with a structure template id",
            "color": "red",
        })),
        _ => usage("/place template <template> <pos>"),
    }
}

fn template(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(id_token) = input.word() else {
        return usage("/place template <template> <pos> [rotation] [mirror]");
    };
    let (Some(x), Some(y), Some(z)) = (input.word(), input.word(), input.word()) else {
        return usage("/place template <template> <pos> [rotation] [mirror]");
    };
    let Some(coords) = args::coords3(x, y, z) else {
        return Outcome::Feedback(translate("argument.pos.invalid", Vec::new()));
    };
    let (_, yaw, pitch) = pose(ctx, call);
    let basis = Basis::from_rotation(yaw, pitch);
    let origin = args::block_pos(coords, call.position(ctx), basis);
    let rotation = match input.word() {
        Some(token) => match Rotation::parse(token) {
            Some(rotation) => rotation,
            None => {
                return Outcome::Feedback(json!({
                    "text": format!(
                        "unknown rotation \"{token}\": use none, clockwise_90, clockwise_180 or counterclockwise_90"
                    ),
                    "color": "red",
                }));
            }
        },
        None => Rotation::None,
    };
    let mirror = match input.word() {
        Some(token) => match Mirror::parse(token) {
            Some(mirror) => mirror,
            None => {
                return Outcome::Feedback(json!({
                    "text": format!(
                        "unknown mirror \"{token}\": use none, left_right or front_back"
                    ),
                    "color": "red",
                }));
            }
        },
        None => Mirror::None,
    };
    // `integrity` и `seed` в ванили задают процессоры; здесь их нет.
    if let Some(token) = input.word() {
        let seed = input.word().unwrap_or("-");
        return Outcome::Feedback(json!({
            "text": format!(
                "integrity {token} (seed {seed}) is not supported: this server places templates without processors, \
                 so every block is kept"
            ),
            "color": "red",
        }));
    }
    let name = args::resource_name(id_token);
    let palette = templates::Palette::new(
        &ctx.plan.survival.data_dir,
        &ctx.plan.survival.block_states,
        &|n| ctx.plan.survival.placement.default_state(n),
    );
    let blocks = match templates::template_blocks(
        &ctx.plan.survival.data_dir,
        &palette,
        &name,
        origin,
        rotation,
        mirror,
        false,
    ) {
        Ok(blocks) => blocks,
        Err(error) => {
            return Outcome::Feedback(json!({
                "text": format!("no template \"{name}\": {error}"),
                "color": "red",
            }));
        }
    };
    if blocks.is_empty() {
        return failed("commands.place.template.failed", Vec::new());
    }
    let mut placed = 0i32;
    let mut with_nbt = 0i32;
    for block in &blocks {
        if !ctx.plan.in_world(block.pos) {
            continue;
        }
        if ctx.plan.place_block(block.pos, block.state).is_some() {
            placed += 1;
        }
        if block.has_nbt {
            with_nbt += 1;
        }
    }
    let mut message = translate(
        "commands.place.template.success",
        vec![
            json!({"text": name}),
            json!(origin[0]),
            json!(origin[1]),
            json!(origin[2]),
        ],
    );
    if with_nbt > 0 {
        message = json!({
            "text": "",
            "extra": [
                message,
                {"text": format!(" ({placed} blocks; {with_nbt} block entities were not filled)")},
            ],
        });
    }
    Outcome::Feedback(message)
}

fn pose(ctx: &Context<'_>, call: &Call<'_>) -> ([f64; 3], f32, f32) {
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

pub(super) fn specs() -> Vec<super::dispatch::Spec> {
    use super::dispatch::Spec;
    use super::registry::{
        GAMEMASTERS, block_pos, resource_location, template_mirror, template_rotation,
    };
    let integrity = Spec::arg_leaf(
        "seed",
        GAMEMASTERS,
        chunkwake_protocol::text::Parser::Integer(i32::MIN, i32::MAX),
        place,
    );
    let template = Spec::arg_leaf("template", GAMEMASTERS, resource_location(), place)
        .with_children(vec![
            Spec::arg_leaf("pos", GAMEMASTERS, block_pos(), place).with_children(vec![
                Spec::arg_leaf("rotation", GAMEMASTERS, template_rotation(), place).with_children(
                    vec![
                        Spec::arg_leaf("mirror", GAMEMASTERS, template_mirror(), place)
                            .with_children(vec![
                                Spec::arg_leaf(
                                    "integrity",
                                    GAMEMASTERS,
                                    chunkwake_protocol::text::Parser::Float(0.0, 1.0),
                                    place,
                                )
                                .with_children(vec![integrity]),
                            ]),
                    ],
                ),
            ]),
        ]);
    vec![
        Spec::group(
            "place",
            GAMEMASTERS,
            vec![
                Spec::group("feature", GAMEMASTERS, Vec::new()),
                Spec::group("jigsaw", GAMEMASTERS, Vec::new()),
                Spec::group("structure", GAMEMASTERS, Vec::new()),
                Spec::group("template", GAMEMASTERS, vec![template]),
            ],
        )
        .with_handler(place),
    ]
}
