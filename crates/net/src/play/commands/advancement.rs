//! `/advancement grant|revoke <targets> ...` — выдача и отзыв достижений.

use super::Context;
use super::args;
use super::dispatch::{Call, Outcome, Spec, translate};
use super::registry::{GAMEMASTERS, ent, greedy};
use serde_json::{Value, json};

/// Мини-дерево для `only/from/through/until` (обработчик один).
fn branch(mode: &str) -> Spec {
    let resource = || chunkwake_protocol::text::Parser::ResourceKey("minecraft:advancement");
    match mode {
        "only" => Spec::group(
            "only",
            GAMEMASTERS,
            vec![
                Spec::arg(
                    "advancement",
                    GAMEMASTERS,
                    resource(),
                    vec![Spec::arg_leaf(
                        "criterion",
                        GAMEMASTERS,
                        greedy(),
                        advancement,
                    )],
                )
                .with_handler(advancement),
            ],
        ),
        other => Spec::group(
            other,
            GAMEMASTERS,
            vec![Spec::arg_leaf(
                "advancement",
                GAMEMASTERS,
                resource(),
                advancement,
            )],
        ),
    }
}

pub(super) fn advancement(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let action = call.path.first().map(String::as_str).unwrap_or("");
    let grant = action == "grant";
    if !grant && action != "revoke" {
        return usage();
    }
    let mut input = call.t();
    let Some(token) = input.word() else {
        return usage();
    };
    let targets = match ctx.select(call.source, token, true) {
        Ok(list) => list,
        Err(error) => return Outcome::Feedback(error.component()),
    };
    let Some(mode) = input.word() else {
        return usage();
    };
    let (ids, criterion) = match mode {
        "everything" => (ctx.plan.advancement_all(), None),
        "only" | "from" | "through" | "until" => {
            let Some(raw) = input.word() else {
                return usage();
            };
            let name = args::resource_name(raw);
            let name = name.trim_start_matches("minecraft:").to_owned();
            if ctx.plan.advancement_index(&name).is_none() {
                return Outcome::Feedback(translate(
                    "argument.resource.invalid",
                    vec![Value::String(name)],
                ));
            }
            let ids = match mode {
                "only" => vec![name.clone()],
                "from" => ctx.plan.advancement_from(&name),
                "through" => ctx.plan.advancement_through(&name, true),
                _ => ctx.plan.advancement_through(&name, false),
            };
            let criterion = if mode == "only" {
                input.word().map(str::to_owned)
            } else {
                None
            };
            (ids, criterion)
        }
        _ => return usage(),
    };
    if ids.is_empty() {
        return Outcome::Feedback(translate(
            "commands.advancement.grant.only.failed",
            Vec::new(),
        ));
    }
    for target in &targets {
        for id in &ids {
            if grant {
                ctx.plan
                    .advancement_grant(target.uuid, id, criterion.as_deref());
            } else {
                ctx.plan
                    .advancement_revoke(target.uuid, id, criterion.as_deref());
            }
        }
    }
    let key = if grant {
        "commands.advancement.grant.everything.success.single"
    } else {
        "commands.advancement.revoke.everything.success.single"
    };
    let who = targets.first().map(|t| t.name.clone()).unwrap_or_default();
    Outcome::Feedback(json!({
        "translate": key,
        "with": [{"text": who}, {"text": format!("{} advancement(s)", ids.len())}],
    }))
}

fn usage() -> Outcome {
    Outcome::Feedback(translate(
        "commands.advancement.usage",
        vec![Value::String(
            "/advancement <grant|revoke> <targets> <everything|only|from|through|until> ...".into(),
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
                Spec::leaf("everything", GAMEMASTERS, advancement),
                branch("only"),
                branch("from"),
                branch("through"),
                branch("until"),
            ],
        )
        .with_handler(advancement)
    };
    vec![Spec::group(
        "advancement",
        GAMEMASTERS,
        vec![
            Spec::group("grant", GAMEMASTERS, vec![targets()]),
            Spec::group("revoke", GAMEMASTERS, vec![targets()]),
        ],
    )]
}
