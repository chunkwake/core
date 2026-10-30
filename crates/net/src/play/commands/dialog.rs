//! `/dialog clear` закрывает диалог пакетом `clear_dialog`.
//!
//! `/dialog show` не открывает окно: на сервере нет реестра диалогов,
//! поэтому любой id отвечает «не найден».

use super::Context;
use super::args;
use super::dispatch::{Call, Outcome, translate};
use super::entity::usage;
use super::selector::Target;
use serde_json::{Value, json};

/// `/dialog clear <targets>` и `/dialog show <targets> <dialog>`.
pub(super) fn dialog(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    match call.path.first().map(String::as_str) {
        Some("clear") => clear(ctx, call, &mut input),
        Some("show") => show(ctx, call, &mut input),
        _ => usage("/dialog <clear|show>"),
    }
}

fn clear(ctx: &Context<'_>, call: &Call<'_>, input: &mut args::Input<'_>) -> Outcome {
    let Some(token) = input.word() else {
        return usage("/dialog clear <targets>");
    };
    let players = match players(ctx, call, token) {
        Ok(list) => list,
        Err(outcome) => return outcome,
    };
    let packet = chunkwake_protocol::play::clear_dialog(ctx.plan.ids.clear_dialog);
    for player in &players {
        ctx.plan.send_to(player.uuid, packet.clone());
    }
    if players.len() == 1 {
        Outcome::Feedback(translate(
            "commands.dialog.clear.single",
            vec![json!({"text": players[0].name})],
        ))
    } else {
        Outcome::Feedback(translate(
            "commands.dialog.clear.multiple",
            vec![json!(players.len() as i32)],
        ))
    }
}

fn show(ctx: &Context<'_>, call: &Call<'_>, input: &mut args::Input<'_>) -> Outcome {
    let Some(token) = input.word() else {
        return usage("/dialog show <targets> <dialog>");
    };
    if let Err(outcome) = players(ctx, call, token) {
        return outcome;
    }
    let Some(dialog) = input.balanced() else {
        return usage("/dialog show <targets> <dialog>");
    };
    failed(
        "argument.resource.not_found",
        vec![
            Value::String(dialog.to_owned()),
            Value::String("minecraft:dialog".into()),
        ],
    )
}

fn players(ctx: &Context<'_>, call: &Call<'_>, token: &str) -> Result<Vec<Target>, Outcome> {
    let targets = match ctx.select(call.source, token, false) {
        Ok(list) => list,
        Err(error) => return Err(Outcome::Feedback(error.component())),
    };
    let players: Vec<Target> = targets
        .into_iter()
        .filter(|target| target.game_mode.is_some())
        .collect();
    if players.is_empty() {
        return Err(failed("argument.entity.notfound.player", Vec::new()));
    }
    Ok(players)
}

fn failed(key: &str, with: Vec<Value>) -> Outcome {
    let mut value = translate(key, with);
    if let Some(object) = value.as_object_mut() {
        object.insert("color".into(), json!("red"));
    }
    Outcome::Feedback(value)
}

pub(super) fn specs() -> Vec<super::dispatch::Spec> {
    use super::dispatch::Spec;
    use super::registry::{GAMEMASTERS, ent};
    use chunkwake_protocol::text::Parser;
    vec![
        Spec::group(
            "dialog",
            GAMEMASTERS,
            vec![
                Spec::group(
                    "clear",
                    GAMEMASTERS,
                    vec![Spec::arg_leaf(
                        "targets",
                        GAMEMASTERS,
                        ent(true, false),
                        dialog,
                    )],
                ),
                Spec::group(
                    "show",
                    GAMEMASTERS,
                    vec![Spec::arg(
                        "targets",
                        GAMEMASTERS,
                        ent(true, false),
                        vec![Spec::arg_leaf(
                            "dialog",
                            GAMEMASTERS,
                            Parser::Dialog,
                            dialog,
                        )],
                    )],
                ),
            ],
        )
        .with_handler(dialog),
    ]
}
