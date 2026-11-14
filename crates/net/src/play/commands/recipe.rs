//! `/recipe give|take <targets> <recipe|*>`: выдача и отзыв рецептов.

use super::Context;
use super::args;
use super::dispatch::{Call, Outcome, Spec, translate};
use super::registry::{GAMEMASTERS, ent};
use serde_json::{Value, json};

fn recipe(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let action = call.path.first().map(String::as_str).unwrap_or("");
    let give = match action {
        "give" => true,
        "take" => false,
        _ => return usage(),
    };
    let mut input = call.t();
    let Some(token) = input.word() else {
        return usage();
    };
    let targets = match ctx.select(call.source, token, true) {
        Ok(list) => list,
        Err(error) => return Outcome::Feedback(error.component()),
    };
    let Some(what) = input.word() else {
        return usage();
    };
    let ids = if what == "*" {
        ctx.plan.recipe_book_ids()
    } else {
        let name = args::resource_name(what);
        let name = name.trim_start_matches("minecraft:").to_owned();
        if ctx.plan.recipe_book_entry(&name).is_none() {
            return Outcome::Feedback(translate(
                "argument.resource.invalid",
                vec![Value::String(name)],
            ));
        }
        vec![name]
    };
    let mut total = 0;
    for target in &targets {
        total += if give {
            ctx.plan.recipe_give(target.uuid, &ids)
        } else {
            ctx.plan.recipe_take(target.uuid, &ids)
        };
    }
    let key = match (give, targets.len() == 1) {
        (true, true) => "commands.recipe.give.success.single",
        (true, false) => "commands.recipe.give.success.multiple",
        (false, true) => "commands.recipe.take.success.single",
        (false, false) => "commands.recipe.take.success.multiple",
    };
    let mut with = vec![json!(total)];
    match targets.first() {
        Some(t) if targets.len() == 1 => with.push(json!({ "text": t.name })),
        _ => with.push(json!(targets.len())),
    }
    if total == 0 {
        let failed = if give {
            "commands.recipe.give.failed"
        } else {
            "commands.recipe.take.failed"
        };
        return Outcome::Feedback(translate(failed, Vec::new()));
    }
    Outcome::Feedback(json!({ "translate": key, "with": with }))
}

fn usage() -> Outcome {
    Outcome::Feedback(translate(
        "commands.recipe.usage",
        vec![Value::String(
            "/recipe <give|take> <targets> <recipe|*>".into(),
        )],
    ))
}

pub(super) fn specs() -> Vec<Spec> {
    let targets = || {
        Spec::arg(
            "targets",
            GAMEMASTERS,
            ent(true, false),
            vec![
                Spec::literal_leaf("*", GAMEMASTERS, recipe),
                Spec::arg_leaf(
                    "recipe",
                    GAMEMASTERS,
                    chunkwake_protocol::text::Parser::ResourceKey("minecraft:recipe"),
                    recipe,
                ),
            ],
        )
        .with_handler(recipe)
    };
    vec![Spec::group(
        "recipe",
        GAMEMASTERS,
        vec![
            Spec::group("give", GAMEMASTERS, vec![targets()]),
            Spec::group("take", GAMEMASTERS, vec![targets()]),
        ],
    )]
}
