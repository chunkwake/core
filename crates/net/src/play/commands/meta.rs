//! Служебные команды: `/help`, `/list`, `/seed`, `/time`, `/tick`, `/tps`,
//! `/perf`, `/top`, `/version`, `/difficulty`, `/stop`, `/setidletimeout`.

use super::Context;
use super::args;
use super::dispatch::{Call, Outcome, translate};
use serde_json::Value;

/// `/help [команда]`: список доступных команд или её использование.
pub(super) fn help(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(name) = input.word() else {
        let tree = super::registry::tree();
        let mut names: Vec<&str> = tree
            .names()
            .filter(|n| tree.level_of(n).is_some_and(|l| call.source.level >= l))
            .collect();
        names.sort_unstable();
        let list = names.join(", ");
        let _ = ctx;
        return Outcome::Feedback(serde_json::json!({
            "text": "",
            "extra": [
                {"text": "Available commands (", "color": "gray"},
                {"text": names.len().to_string(), "color": "white"},
                {"text": "):\n", "color": "gray"},
                {"text": list, "color": "white"},
            ],
        }));
    };
    let tree = super::registry::tree();
    let Some(spec) = tree.spec(name) else {
        return Outcome::Feedback(translate(
            "commands.help.failed",
            vec![Value::String(name.to_owned())],
        ));
    };
    if call.source.level < spec.level {
        return Outcome::Feedback(translate(
            "commands.help.failed",
            vec![Value::String(name.to_owned())],
        ));
    }
    let mut usage = format!("/{name}");
    for child in &spec.children {
        if child.parser.is_some() {
            usage += &format!(" <{}>", child.name);
        } else {
            usage += &format!(" {}", child.name);
        }
    }
    Outcome::Feedback(serde_json::json!({
        "text": format!("{usage}\n"),
        "color": "gray",
        "extra": [{
            "text": match spec.level {
                0 => "Available to everyone",
                1..=2 => "Requires gamemaster permissions",
                3 => "Requires admin permissions",
                _ => "Requires server owner permissions",
            },
            "color": "yellow",
        }],
    }))
}

/// `/list [uuids]`: сколько игроков онлайн и кто они.
pub(super) fn list(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let players = ctx.plan.online_players();
    let uuids = call.path.iter().any(|p| p == "uuids");
    if players.is_empty() {
        return Outcome::text("There are 0 of a max of 0 players online:");
    }
    let mut names: Vec<String> = players
        .iter()
        .map(|p| {
            if uuids {
                format!("{} ({})", p.name, p.uuid)
            } else {
                p.name.clone()
            }
        })
        .collect();
    names.sort();
    let max = ctx.plan.max_players();
    Outcome::text(format!(
        "There are {} of a max of {max} players online: {}",
        names.len(),
        names.join(", ")
    ))
}

/// `/seed`: зерно мира.
pub(super) fn seed(ctx: &Context<'_>, _call: &Call<'_>) -> Outcome {
    Outcome::Feedback(translate(
        "commands.seed.success",
        vec![Value::from(ctx.plan.seed())],
    ))
}

/// `/time set|add|query`.
///
/// Подкоманда — литерал дерева ([`Call::path`]), значение — в аргументах.
pub(super) fn time(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(sub) = call.path.first().map(String::as_str) else {
        return usage("/time <set|add|query> <value>");
    };
    match sub {
        "set" => {
            let Some(token) = input.word() else {
                return usage("/time set <time>");
            };
            let Some(ticks) = args::daytime(token) else {
                return usage("/time set <time>");
            };
            ctx.plan.set_day_time(ticks);
            Outcome::Feedback(translate("commands.time.set", vec![Value::from(ticks)]))
        }
        "add" => {
            let Some(token) = input.word() else {
                return usage("/time add <time>");
            };
            let Some(delta) = args::time_ticks(token) else {
                return usage("/time add <time>");
            };
            let now = ctx.plan.set_day_time_relative(delta);
            Outcome::Feedback(translate("commands.time.set", vec![Value::from(now)]))
        }
        "query" => {
            // Единица — литерал дерева (`/time query daytime`).
            let unit = call
                .path
                .get(1)
                .map(String::as_str)
                .or_else(|| input.word())
                .unwrap_or("daytime");
            let (key, value) = match unit {
                // `time query time` — ванильное имя 26.3; `daytime` оставлено
                // алиасом для старых скриптов лаборатории.
                "time" | "daytime" => ("commands.time.query.daytime", ctx.plan.day_time()),
                "gametime" => ("commands.time.query.gametime", ctx.plan.game_time()),
                "day" => ("commands.time.query.day", ctx.plan.game_time() / 24_000),
                _ => return usage("/time query <gametime|time>"),
            };
            Outcome::Feedback(translate(key, vec![Value::from(value)]))
        }
        _ => usage("/time <set|add|query> <value>"),
    }
}

fn usage(text: &str) -> Outcome {
    Outcome::Feedback(super::dispatch::error_component(text))
}

/// `/tick query|rate|freeze|unfreeze|step|sprint`: подкоманда — литерал
/// дерева, поэтому берётся из [`Call::path`].
pub(super) fn tick(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let sub = call.path.first().map(String::as_str);
    let sub = match sub {
        Some(sub) => sub,
        None => {
            // `/tick` без подкоманды — ванильный `query`.
            return Outcome::Feedback(serde_json::json!({
                "text": ctx.plan.tick_meter_line(),
            }));
        }
    };
    match sub {
        "query" => Outcome::Feedback(serde_json::json!({"text": ctx.plan.tick_meter_line()})),
        "rate" => {
            let Some(token) = input.word() else {
                return usage("/tick rate <rate>");
            };
            let Some(rate) = params_rate(token) else {
                return Outcome::Feedback(super::dispatch::error_component(
                    "Rate must be a number between 1 and 10000",
                ));
            };
            ctx.plan.set_tick_rate(rate);
            Outcome::Feedback(translate(
                "commands.tick.rate.success",
                vec![Value::from(rate)],
            ))
        }
        "freeze" => {
            ctx.plan.set_tick_frozen(true);
            Outcome::Feedback(translate("commands.tick.freeze.success", Vec::new()))
        }
        "unfreeze" => {
            ctx.plan.set_tick_frozen(false);
            Outcome::Feedback(translate(
                "commands.tick.rate.success",
                vec![Value::from(20)],
            ))
        }
        "step" => {
            // `step stop` — литерал дерева; `step <time>` — аргумент.
            if call.path.get(1).map(String::as_str) == Some("stop") {
                let Some(ran) = ctx.plan.stop_sprint() else {
                    return Outcome::text("No tick steps are pending");
                };
                return Outcome::Feedback(translate(
                    "commands.tick.step.stop.success",
                    vec![Value::from(ran)],
                ));
            }
            let Some(token) = input.word() else {
                return usage("/tick step <time|stop>");
            };
            let Some(ticks) = args::time_ticks(token).filter(|t| *t > 0) else {
                return usage("/tick step <time|stop>");
            };
            ctx.plan.request_steps(ticks as u64);
            Outcome::Feedback(translate(
                "commands.tick.step.success",
                vec![Value::from(ticks)],
            ))
        }
        "sprint" => {
            // `sprint stop` — литерал дерева.
            if call.path.get(1).map(String::as_str) == Some("stop") {
                let Some(ran) = ctx.plan.stop_sprint() else {
                    return Outcome::text("The server is not sprinting");
                };
                return Outcome::Feedback(translate(
                    "commands.tick.sprint.stop.success",
                    vec![Value::from(ran)],
                ));
            }
            let Some(token) = input.word() else {
                return usage("/tick sprint <time|stop>");
            };
            let Some(ticks) = args::time_ticks(token).filter(|t| *t > 0) else {
                return usage("/tick sprint <time|stop>");
            };
            ctx.plan.start_sprint(ticks as u64);
            Outcome::Feedback(translate(
                "commands.tick.sprint.start",
                vec![Value::from(ticks)],
            ))
        }
        _ => usage("/tick <query|rate|freeze|unfreeze|step|sprint> <value>"),
    }
}

fn params_rate(token: &str) -> Option<i32> {
    token
        .parse::<f64>()
        .ok()
        .filter(|r| (1.0..=10_000.0).contains(r))
        .map(|r| r.round() as i32)
}

/// `/tps` — короткий алиас `tick query` в прежнем формате для лаборатории.
pub(super) fn tps(ctx: &Context<'_>, _call: &Call<'_>) -> Outcome {
    Outcome::text(ctx.plan.tick_meter_line())
}

/// `/perf` и `/top`: неинтерактивный снимок нагрузки.
pub(super) fn top(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let section = input.word();
    let snapshot = ctx.plan.load_snapshot();
    Outcome::Feedback(crate::play::commands::perf::render(&snapshot, section))
}

/// `/version`.
pub(super) fn version(ctx: &Context<'_>, _call: &Call<'_>) -> Outcome {
    let (name, protocol) = ctx.plan.version();
    Outcome::text(format!(
        "This server is running Chunkwake on Minecraft {name} (protocol {protocol})"
    ))
}

/// `/difficulty <level>`: уровень приходит литералом дерева.
pub(super) fn difficulty(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let level = match call.path.first().map(String::as_str) {
        Some(token) => match difficulty_of(token) {
            Some(level) => level,
            None => return usage("/difficulty <peaceful|easy|normal|hard>"),
        },
        None => {
            let Some(token) = input.word() else {
                let current = match ctx.plan.difficulty() {
                    crate::survival::Difficulty::Peaceful => "peaceful",
                    crate::survival::Difficulty::Easy => "easy",
                    crate::survival::Difficulty::Normal => "normal",
                    crate::survival::Difficulty::Hard => "hard",
                };
                return Outcome::Feedback(translate(
                    "commands.difficulty.query",
                    vec![
                        translate("options.difficulty", Vec::new()),
                        Value::String(current.into()),
                    ],
                ));
            };
            match difficulty_of(token) {
                Some(level) => level,
                None => return usage("/difficulty <peaceful|easy|normal|hard|0-3>"),
            }
        }
    };
    ctx.plan.set_difficulty(level);
    // Запрос показывает состояние после установки: `/difficulty` без значения.
    let current = match ctx.plan.difficulty() {
        crate::survival::Difficulty::Peaceful => "peaceful",
        crate::survival::Difficulty::Easy => "easy",
        crate::survival::Difficulty::Normal => "normal",
        crate::survival::Difficulty::Hard => "hard",
    };
    Outcome::Feedback(translate(
        "commands.difficulty.success",
        vec![
            translate(
                &format!("options.difficulty.{}", name_of(level)),
                Vec::new(),
            ),
            Value::String(current.into()),
        ],
    ))
}

fn difficulty_of(token: &str) -> Option<u8> {
    Some(match token {
        "peaceful" | "p" | "0" => 0,
        "easy" | "e" | "1" => 1,
        "normal" | "n" | "2" => 2,
        "hard" | "h" | "3" => 3,
        _ => return None,
    })
}

fn name_of(level: u8) -> &'static str {
    match level {
        0 => "peaceful",
        1 => "easy",
        2 => "normal",
        _ => "hard",
    }
}

/// `/setidletimeout <minutes>`.
pub(super) fn idle_timeout(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(token) = input.word() else {
        return usage("/setidletimeout <minutes>");
    };
    let Some(minutes) = token.parse::<i64>().ok().filter(|m| *m >= 0) else {
        return usage("/setidletimeout <minutes>");
    };
    ctx.vars().set_idle_timeout(minutes);
    Outcome::Feedback(translate(
        "commands.setidletimeout.success",
        vec![Value::from(minutes)],
    ))
}

/// `/stop`: сохранить мир и выйти.
pub(super) fn stop(ctx: &Context<'_>, _call: &Call<'_>) -> Outcome {
    ctx.plan.request_shutdown();
    Outcome::text("Stopping the server")
}

/// `/debug start|stop` — только сообщение, профилировщик отдельный этап.
pub(super) fn debug(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    match call.path.first().map(String::as_str) {
        Some("start") => {
            ctx.plan.start_debug_profile();
            Outcome::text("Started debug profiling")
        }
        Some("stop") => {
            let Some(path) = ctx.plan.stop_debug_profile() else {
                return Outcome::text("Debug profiling is not running");
            };
            Outcome::text(format!(
                "Stopped debug profiling; profile at {}",
                path.display()
            ))
        }
        _ => usage("/debug start|stop"),
    }
}

/// Узлы дерева служебных команд.
pub(super) fn specs() -> Vec<super::dispatch::Spec> {
    use super::dispatch::Spec;
    use super::registry::{ADMINS, EVERYONE, GAMEMASTERS, OWNERS, arg, int, word};

    vec![
        Spec::exec(
            "help",
            EVERYONE,
            help,
            vec![arg("command", EVERYONE, super::registry::greedy(), help)],
        ),
        Spec::exec(
            "list",
            EVERYONE,
            list,
            vec![Spec::literal_leaf("uuids", EVERYONE, list)],
        ),
        Spec::leaf("seed", GAMEMASTERS, seed),
        Spec::leaf("tps", EVERYONE, tps),
        Spec::exec(
            "top",
            GAMEMASTERS,
            top,
            vec![arg("section", GAMEMASTERS, word(), top)],
        ),
        // `/time` — группа без `executable` (ванила не считает её законченной),
        // но с обработчиком: `query` ветвится на литералы, и глубокий узел
        // выполняет корень, разбирая хвост сам.
        Spec::group(
            "time",
            GAMEMASTERS,
            vec![
                Spec::group(
                    "set",
                    GAMEMASTERS,
                    vec![Spec::arg_leaf(
                        "time",
                        GAMEMASTERS,
                        super::registry::time(0),
                        time,
                    )],
                ),
                Spec::group(
                    "add",
                    GAMEMASTERS,
                    vec![Spec::arg_leaf(
                        "time",
                        GAMEMASTERS,
                        super::registry::time(i32::MIN),
                        time,
                    )],
                ),
                Spec::group(
                    "query",
                    GAMEMASTERS,
                    vec![
                        Spec::literal_leaf("gametime", GAMEMASTERS, time),
                        Spec::literal_leaf("time", GAMEMASTERS, time),
                    ],
                ),
            ],
        )
        .with_handler(time),
        Spec::exec(
            "difficulty",
            GAMEMASTERS,
            difficulty,
            ["peaceful", "easy", "normal", "hard"]
                .into_iter()
                .map(|name| Spec::literal_leaf(name, GAMEMASTERS, difficulty))
                .collect(),
        ),
        Spec::group(
            "tick",
            ADMINS,
            vec![
                Spec::literal_leaf("query", ADMINS, tick),
                Spec::group(
                    "rate",
                    ADMINS,
                    vec![Spec::arg_leaf(
                        "rate",
                        ADMINS,
                        super::registry::float(1.0, 10_000.0),
                        tick,
                    )],
                ),
                Spec::literal_leaf("freeze", ADMINS, tick),
                Spec::literal_leaf("unfreeze", ADMINS, tick),
                Spec::exec(
                    "step",
                    ADMINS,
                    tick,
                    vec![
                        Spec::arg_leaf("time", ADMINS, super::registry::time(1), tick),
                        Spec::literal_leaf("stop", ADMINS, tick),
                    ],
                ),
                Spec::group(
                    "sprint",
                    ADMINS,
                    vec![
                        Spec::literal_leaf("stop", ADMINS, tick),
                        Spec::arg_leaf("time", ADMINS, super::registry::time(1), tick),
                    ],
                ),
            ],
        ),
        Spec::group(
            "perf",
            OWNERS,
            vec![
                Spec::literal_leaf("start", OWNERS, perf_start),
                Spec::literal_leaf("stop", OWNERS, perf_stop),
            ],
        ),
        Spec::leaf("version", GAMEMASTERS, version),
        Spec::group(
            "setidletimeout",
            ADMINS,
            vec![arg("minutes", ADMINS, int(0, 1_000_000), idle_timeout)],
        ),
        Spec::leaf("stop", OWNERS, stop),
        Spec::group(
            "debug",
            ADMINS,
            vec![
                Spec::literal_leaf("start", ADMINS, debug),
                Spec::literal_leaf("stop", ADMINS, debug),
            ],
        ),
    ]
}

/// `/perf start`: снимок нагрузки и запуск профиля тика.
fn perf_start(ctx: &Context<'_>, _call: &Call<'_>) -> Outcome {
    let started = ctx.plan.start_debug_profile();
    let snapshot = crate::play::commands::perf::render_snapshot(ctx.plan);
    let note = if started {
        "Debug profiling started"
    } else {
        "Debug profiling is already running"
    };
    Outcome::Feedback(serde_json::json!({
        "text": format!("{note}\n"),
        "extra": [snapshot],
    }))
}

/// `/perf stop`: остановить профиль и отдать путь к файлу.
fn perf_stop(ctx: &Context<'_>, _call: &Call<'_>) -> Outcome {
    match ctx.plan.stop_debug_profile() {
        Some(path) => Outcome::text(format!("Stopped profiling; profile at {}", path.display())),
        None => Outcome::text("Debug profiling is not running"),
    }
}
