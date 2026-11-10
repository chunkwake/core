//! Администрирование: `/op`, `/deop`, `/kick`, `/whitelist`, `/ban`,
//! `/ban-ip`, `/banlist`, `/pardon`, `/pardon-ip`, `/save-all`, `/save-off`,
//! `/save-on`, `/publish`, `/unpublish`, `/transfer`, `/reload`.

use super::Context;
use super::args;
use super::dispatch::{Call, Outcome, translate};
use super::entity::usage;
use super::state::AccessList;
use serde_json::{Value, json};

/// `/op <targets>`.
pub(super) fn op(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    set_op(ctx, call, true)
}

/// `/deop <targets>`.
pub(super) fn deop(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    set_op(ctx, call, false)
}

fn set_op(ctx: &Context<'_>, call: &Call<'_>, grant: bool) -> Outcome {
    let mut input = call.t();
    let Some(token) = input.word() else {
        return usage(if grant {
            "/op <targets>"
        } else {
            "/deop <targets>"
        });
    };
    let targets = match ctx.plan.command_targets_named(token) {
        Some(list) if !list.is_empty() => list,
        _ => return Outcome::Feedback(translate("argument.player.unknown", Vec::new())),
    };
    // Сообщение и обновление прав у клиента — в `op_command`: там же запись
    // `ops.json` и рассылка нового дерева команд.
    let mut changed = Vec::new();
    for target in &targets {
        let msg = ctx.plan.op_command(&target.name, grant);
        if msg.get("translate").and_then(|v| v.as_str())
            == Some(if grant {
                "commands.op.success"
            } else {
                "commands.deop.success"
            })
        {
            changed.push(target.name.clone());
        }
    }
    if changed.is_empty() {
        let key = if grant {
            "commands.op.failed"
        } else {
            "commands.deop.failed"
        };
        return Outcome::Feedback(translate(key, Vec::new()));
    }
    let key = if grant {
        "commands.op.success"
    } else {
        "commands.deop.success"
    };
    Outcome::Feedback(translate(key, vec![json!({"text": changed.join(", ")})]))
}

/// `/kick <targets> [reason]`.
pub(super) fn kick(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(token) = input.word() else {
        return usage("/kick <targets> [reason]");
    };
    let targets = match ctx.select(call.source, token, false) {
        Ok(list) => list,
        Err(e) => return Outcome::Feedback(e.component()),
    };
    // `KickCommand`: без причины — `multiplayer.disconnect.kicked`.
    let text = args::message(input.rest());
    let reason = if text.is_empty() {
        json!({"translate": "multiplayer.disconnect.kicked"})
    } else {
        json!({"text": text})
    };
    for target in &targets {
        ctx.plan.kick(target.uuid, &reason.to_string(), "KICKED");
    }
    Outcome::Feedback(translate(
        "commands.kick.success",
        vec![
            json!({"text": targets
                .iter()
                .map(|t| t.name.clone())
                .collect::<Vec<_>>()
                .join(", ")}),
            reason,
        ],
    ))
}

/// `/whitelist <add|remove|list|reload|on|off>`: подкоманда — литерал дерева,
/// поэтому берётся из [`Call::path`].
pub(super) fn whitelist(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(sub) = call.path.first().map(String::as_str) else {
        return usage("/whitelist <add|remove|list|reload|on|off>");
    };
    match sub {
        "add" | "remove" => {
            let Some(name) = input.word() else {
                return usage("/whitelist <add|remove> <player>");
            };
            let added = sub == "add";
            let changed = ctx
                .vars()
                .update_access(AccessList::Whitelist, name, Some(added));
            let key = match (added, changed) {
                (true, true) => "commands.whitelist.add.success",
                (false, true) => "commands.whitelist.remove.success",
                (true, false) => "commands.whitelist.add.failed",
                (false, false) => "commands.whitelist.remove.failed",
            };
            Outcome::Feedback(translate(key, vec![json!({"text": name})]))
        }
        "list" => {
            let access = ctx.vars().access();
            if access.whitelist.is_empty() {
                return Outcome::Feedback(translate("commands.whitelist.list", Vec::new()));
            }
            Outcome::Feedback(translate(
                "commands.whitelist.list",
                vec![
                    json!({"text": access.whitelist.join(", ")}),
                    Value::from(access.whitelist.len()),
                ],
            ))
        }
        "reload" => {
            ctx.plan.reload_whitelist();
            Outcome::Feedback(translate("commands.whitelist.reloaded", Vec::new()))
        }
        "on" | "off" => {
            ctx.plan.set_whitelist_enabled(sub == "on");
            Outcome::Feedback(translate(
                &format!("commands.whitelist.{}", sub),
                Vec::new(),
            ))
        }
        _ => usage("/whitelist <add|remove|list|reload|on|off>"),
    }
}

/// `/ban`, `/ban-ip`.
pub(super) fn ban(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    ban_common(ctx, call, AccessList::BannedPlayers)
}

/// `/ban-ip`.
pub(super) fn ban_ip(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    ban_common(ctx, call, AccessList::BannedIps)
}

fn ban_common(ctx: &Context<'_>, call: &Call<'_>, list: AccessList) -> Outcome {
    let mut input = call.t();
    let Some(name) = input.word() else {
        return usage("/ban <target> [reason]");
    };
    let reason = args::message(input.rest());
    let changed = ctx.vars().update_access(list, name, Some(true));
    if !changed {
        let key = if list == AccessList::BannedIps {
            "commands.banip.failed"
        } else {
            "commands.ban.failed"
        };
        return Outcome::Feedback(translate(key, Vec::new()));
    }
    // Кикаем забаненного игрока, как ванила: `multiplayer.disconnect.banned`
    // (`ip_banned` для `/ban-ip`), причина бана в сообщение не идёт.
    let (key, cause) = if list == AccessList::BannedIps {
        ("multiplayer.disconnect.ip_banned", "IP_BANNED")
    } else {
        ("multiplayer.disconnect.banned", "BANNED")
    };
    let message = json!({"translate": key}).to_string();
    let targets = ctx.plan.command_targets_named(name).unwrap_or_default();
    for target in &targets {
        ctx.plan.kick(target.uuid, &message, cause);
    }
    let key = if list == AccessList::BannedIps {
        "commands.banip.success"
    } else {
        "commands.ban.success"
    };
    Outcome::Feedback(translate(
        key,
        vec![json!({"text": name}), json!({"text": reason})],
    ))
}

/// `/banlist [players|ips]`: подкоманда — литерал дерева, поэтому берётся из
/// [`Call::path`].
pub(super) fn banlist(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let ips = call.path.first().map(String::as_str) == Some("ips") || input.word() == Some("ips");
    let access = ctx.vars().access();
    let list = if ips {
        access.banned_ips
    } else {
        access.banned_players
    };
    let key = if ips {
        "commands.banlist.ips"
    } else {
        "commands.banlist.players"
    };
    Outcome::Feedback(translate(
        key,
        vec![json!({"text": list.join(", ")}), Value::from(list.len())],
    ))
}

/// `/pardon`, `/pardon-ip`.
pub(super) fn pardon(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    pardon_common(ctx, call, AccessList::BannedPlayers)
}

/// `/pardon-ip`.
pub(super) fn pardon_ip(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    pardon_common(ctx, call, AccessList::BannedIps)
}

fn pardon_common(ctx: &Context<'_>, call: &Call<'_>, list: AccessList) -> Outcome {
    let mut input = call.t();
    let Some(name) = input.word() else {
        return usage("/pardon <name>");
    };
    let changed = ctx.vars().update_access(list, name, Some(false));
    let key = match (list, changed) {
        (AccessList::BannedIps, true) => "commands.unbanip.success",
        (AccessList::BannedIps, false) => "commands.unbanip.failed",
        (_, true) => "commands.unban.success",
        (_, false) => "commands.unban.failed",
    };
    Outcome::Feedback(translate(key, vec![json!({"text": name})]))
}

/// `/save-all [flush]`.
pub(super) fn save_all(ctx: &Context<'_>, _call: &Call<'_>) -> Outcome {
    ctx.plan.request_save();
    Outcome::Feedback(translate("commands.save.success", Vec::new()))
}

/// `/save-off`.
pub(super) fn save_off(ctx: &Context<'_>, _call: &Call<'_>) -> Outcome {
    ctx.vars().set_saving(false);
    Outcome::Feedback(translate("commands.save.disabled", Vec::new()))
}

/// `/save-on`.
pub(super) fn save_on(ctx: &Context<'_>, _call: &Call<'_>) -> Outcome {
    ctx.vars().set_saving(true);
    Outcome::Feedback(translate("commands.save.enabled", Vec::new()))
}

/// `/publish [port]`.
pub(super) fn publish(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let port = input
        .word()
        .and_then(|t| args::int(t, 1, 65_535))
        .unwrap_or(25565);
    ctx.vars().set_published(Some(port as u16));
    Outcome::Feedback(translate(
        "commands.publish.started",
        vec![Value::from(port)],
    ))
}

/// `/unpublish`.
pub(super) fn unpublish(ctx: &Context<'_>, _call: &Call<'_>) -> Outcome {
    ctx.vars().set_published(None);
    Outcome::Feedback(translate("commands.publish.stopped", Vec::new()))
}

/// `/transfer <host> [<port>] [<players>]`.
pub(super) fn transfer(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(host) = input.word() else {
        return usage("/transfer <host> [<port>] [<players>]");
    };
    let port = input
        .word()
        .and_then(|t| args::int(t, 1, 65_535))
        .unwrap_or(25565);
    let token = input.word().unwrap_or("@a");
    let targets = match ctx.select(call.source, token, false) {
        Ok(list) => list,
        Err(e) => return Outcome::Feedback(e.component()),
    };
    for target in &targets {
        ctx.plan.transfer(target.uuid, host, port as u16);
    }
    Outcome::Feedback(translate(
        "commands.transfer.success",
        vec![
            json!({"text": host}),
            Value::from(port),
            Value::from(targets.len()),
        ],
    ))
}

/// `/reload`: перечитать `ops.json`, белый список и правила с диска.
pub(super) fn reload(ctx: &Context<'_>, _call: &Call<'_>) -> Outcome {
    ctx.plan.reload_configuration();
    Outcome::Feedback(translate("commands.reload.success", Vec::new()))
}

/// Узлы дерева административных команд: форма сверена с ванильным
/// `commands.json` 26.3.
pub(super) fn specs() -> Vec<super::dispatch::Spec> {
    use super::dispatch::Spec;
    use super::registry::{ADMINS, OWNERS, ent, game_profile, message, word};

    // `/op`, `/deop`, `/pardon`: цель — `minecraft:game_profile` (имя игрока,
    // в том числе офлайн).
    let profile = |name: &str, handler: super::dispatch::Handler| {
        Spec::group(
            name,
            ADMINS,
            vec![Spec::arg_leaf("targets", ADMINS, game_profile(), handler)],
        )
    };
    vec![
        profile("op", op),
        profile("deop", deop),
        Spec::group(
            "kick",
            ADMINS,
            vec![
                Spec::arg_leaf("targets", ADMINS, ent(true, false), kick)
                    .with_children(vec![Spec::arg_leaf("reason", ADMINS, message(), kick)]),
            ],
        ),
        Spec::group(
            "whitelist",
            ADMINS,
            vec![
                Spec::group(
                    "add",
                    ADMINS,
                    vec![Spec::arg_leaf("targets", ADMINS, game_profile(), whitelist)],
                ),
                Spec::literal_leaf("list", ADMINS, whitelist),
                Spec::literal_leaf("off", ADMINS, whitelist),
                Spec::literal_leaf("on", ADMINS, whitelist),
                Spec::literal_leaf("reload", ADMINS, whitelist),
                Spec::group(
                    "remove",
                    ADMINS,
                    vec![Spec::arg_leaf("targets", ADMINS, game_profile(), whitelist)],
                ),
            ],
        ),
        Spec::group(
            "ban",
            ADMINS,
            vec![
                Spec::arg_leaf("targets", ADMINS, game_profile(), ban)
                    .with_children(vec![Spec::arg_leaf("reason", ADMINS, message(), ban)]),
            ],
        ),
        Spec::group(
            "ban-ip",
            ADMINS,
            vec![
                args_group("target", ADMINS, word(), ban_ip).with_children(vec![Spec::arg_leaf(
                    "reason",
                    ADMINS,
                    message(),
                    ban_ip,
                )]),
            ],
        ),
        Spec::exec(
            "banlist",
            ADMINS,
            banlist,
            vec![
                Spec::literal_leaf("ips", ADMINS, banlist),
                Spec::literal_leaf("players", ADMINS, banlist),
            ],
        ),
        profile("pardon", pardon),
        Spec::group(
            "pardon-ip",
            ADMINS,
            vec![Spec::arg_leaf("target", ADMINS, word(), pardon_ip)],
        ),
        Spec::exec(
            "save-all",
            OWNERS,
            save_all,
            vec![Spec::literal_leaf("flush", OWNERS, save_all)],
        ),
        Spec::leaf("save-off", OWNERS, save_off),
        Spec::leaf("save-on", OWNERS, save_on),
        Spec::exec(
            "publish",
            OWNERS,
            publish,
            vec![Spec::arg_leaf(
                "port",
                OWNERS,
                super::registry::int(1, 65_535),
                publish,
            )],
        ),
        Spec::leaf("unpublish", OWNERS, unpublish),
        Spec::group(
            "transfer",
            ADMINS,
            vec![
                args_group("hostname", ADMINS, word(), transfer).with_children(vec![
                    args_group("port", ADMINS, super::registry::int(1, 65_535), transfer)
                        .with_children(vec![Spec::arg_leaf(
                            "players",
                            ADMINS,
                            ent(true, false),
                            transfer,
                        )]),
                ]),
            ],
        ),
        Spec::leaf("reload", super::registry::GAMEMASTERS, reload),
    ]
}

/// Узел-аргумент с обработчиком, чьи дети подключаются отдельно.
fn args_group(
    name: &str,
    level: i32,
    parser: chunkwake_protocol::text::Parser,
    handler: super::dispatch::Handler,
) -> super::dispatch::Spec {
    super::dispatch::Spec::arg(name, level, parser, Vec::new()).with_handler(handler)
}
