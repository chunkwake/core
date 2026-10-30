//! Команды общения: `/say`, `/me`, `/msg`, `/teammsg`, `/tellraw`, `/title`,
//! `/playsound`, `/stopsound`.
//!
//! Всё уходит теми же пакетами, что и обычный чат, поэтому мод-клиент видит
//! команды как ванильные сообщения: `system_chat`, `sound`, `title`.

use super::Context;
use super::args;
use super::dispatch::{Call, Outcome, text_component, translate};
use super::entity::usage;
use serde_json::{Value, json};

/// `/say <message>`: сообщение от имени сервера или игрока.
pub(super) fn say(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let text = args::message(call.rest);
    if text.is_empty() {
        return usage("/say <message>");
    }
    let name = call.source.name.clone();
    let line = format!("[{name}] {text}");
    let packet = chunkwake_protocol::play::system_chat(ctx.plan.chat_packet_id(), &line, false);
    ctx.plan.broadcast_packet(packet);
    tracing::info!("{line}");
    Outcome::Feedback(text_component(&line))
}

/// `/me <action>`.
pub(super) fn me(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let text = args::message(call.rest);
    if text.is_empty() {
        return usage("/me <action>");
    }
    let line = format!("* {} {text}", call.source.name);
    let packet = chunkwake_protocol::play::system_chat(ctx.plan.chat_packet_id(), &line, false);
    ctx.plan.broadcast_packet(packet);
    tracing::info!("{line}");
    Outcome::Silent
}

/// `/msg`, `/tell`, `/w <targets> <message>`.
pub(super) fn msg(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(token) = input.word() else {
        return usage("/msg <targets> <message>");
    };
    let targets = match ctx.select(call.source, token, false) {
        Ok(list) => list,
        Err(e) => return Outcome::Feedback(e.component()),
    };
    let text = args::message(input.rest());
    if text.is_empty() {
        return usage("/msg <targets> <message>");
    }
    let from = call.source.name.clone();
    // Отправителю — «-> имя», получателям — «имя -> мне».
    let outgoing = translate(
        "commands.message.display.outgoing",
        vec![
            json!({"text": targets.iter().map(|t| t.name.clone()).collect::<Vec<_>>().join(", ")}),
            json!({"text": text, "color": "gray", "italic": true}),
        ],
    );
    let incoming = translate(
        "commands.message.display.incoming",
        vec![
            json!({"text": from}),
            json!({"text": text, "color": "gray", "italic": true}),
        ],
    );
    for target in &targets {
        ctx.plan.send_json(target.uuid, &incoming.to_string());
    }
    tracing::info!("{} -> {}: {text}", call.source.name, targets.len());
    Outcome::Feedback(outgoing)
}

/// `/teammsg` и `/tm`.
pub(super) fn teammsg(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let text = args::message(call.rest);
    if text.is_empty() {
        return usage("/teammsg <message>");
    }
    let Some(recipients) = ctx.plan.team_recipients(&call.source.name) else {
        return Outcome::Feedback(translate("commands.teammsg.failed", Vec::new()));
    };
    let line = format!("[{}] {text}", call.source.name);
    let component = serde_json::json!({"text": line}).to_string();
    for uuid in recipients {
        ctx.plan.send_json(uuid, &component);
    }
    Outcome::Silent
}

/// `/tellraw <targets> <component>`.
pub(super) fn tellraw(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(token) = input.word() else {
        return usage("/tellraw <targets> <json>");
    };
    let targets = match ctx.select(call.source, token, false) {
        Ok(list) => list,
        Err(e) => return Outcome::Feedback(e.component()),
    };
    let raw = input.rest();
    let Ok(component) = serde_json::from_str::<Value>(raw) else {
        return Outcome::Feedback(translate(
            "argument.component.invalid",
            vec![Value::String(raw.to_owned())],
        ));
    };
    let json = component.to_string();
    for target in &targets {
        ctx.plan.send_json(target.uuid, &json);
    }
    Outcome::Silent
}

/// `/title <targets> <title|subtitle|actionbar|times|clear|reset> …`.
pub(super) fn title(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(token) = input.word() else {
        return usage("/title <targets> <title|subtitle|actionbar|times|clear|reset> ...");
    };
    let targets = match ctx.select(call.source, token, false) {
        Ok(list) => list,
        Err(e) => return Outcome::Feedback(e.component()),
    };
    let Some(part) = input.word() else {
        return usage("/title <targets> <title|subtitle|actionbar|times|clear|reset> ...");
    };
    match part {
        "times" => {
            let (Some(fade_in), Some(stay), Some(fade_out)) =
                (input.word(), input.word(), input.word())
            else {
                return usage("/title <targets> times <fadeIn> <stay> <fadeOut>");
            };
            let (Some(fade_in), Some(stay), Some(fade_out)) = (
                fade_in.parse::<i32>().ok(),
                stay.parse::<i32>().ok(),
                fade_out.parse::<i32>().ok(),
            ) else {
                return usage("/title <targets> times <fadeIn> <stay> <fadeOut>");
            };
            for target in &targets {
                ctx.plan
                    .send_title_animation(target.uuid, fade_in, stay, fade_out);
            }
            Outcome::Feedback(translate(
                "commands.title.times.success",
                vec![name_of(&targets)],
            ))
        }
        "clear" | "reset" => {
            for target in &targets {
                ctx.plan.send_clear_titles(target.uuid, part == "reset");
            }
            Outcome::Feedback(translate(
                &format!("commands.title.{part}.success"),
                vec![name_of(&targets)],
            ))
        }
        "title" | "subtitle" | "actionbar" => {
            let raw = args::message(input.rest());
            let Some(component) = component_of(&raw) else {
                return Outcome::Feedback(translate(
                    "argument.component.invalid",
                    vec![Value::String(raw)],
                ));
            };
            for target in &targets {
                ctx.plan.send_title(target.uuid, part, component.clone());
            }
            Outcome::Feedback(translate(
                &format!("commands.title.{part}.success"),
                vec![name_of(&targets)],
            ))
        }
        _ => usage("/title <targets> <title|subtitle|actionbar|times|clear|reset> ..."),
    }
}

/// Компонент из JSON или простого текста.
fn component_of(raw: &str) -> Option<Value> {
    if raw.trim_start().starts_with('{') || raw.trim_start().starts_with('[') {
        return serde_json::from_str(raw).ok();
    }
    (!raw.is_empty()).then(|| text_component(raw))
}

/// Имя цели или «N targets».
fn name_of(targets: &[super::selector::Target]) -> Value {
    match targets {
        [one] => json!({"text": one.name}),
        _ => json!({"text": format!("{} targets", targets.len())}),
    }
}

fn is_sound_source(token: &str) -> bool {
    matches!(
        token,
        "ambient"
            | "block"
            | "hostile"
            | "master"
            | "music"
            | "neutral"
            | "player"
            | "record"
            | "ui"
            | "voice"
            | "weather"
    )
}

/// `/playsound <sound> <source> <targets> [pos] [volume] [pitch]`.
///
/// Источник — литерал дерева (`master`, `player`, …), поэтому он приходит в
/// [`Call::path`], а позиция читается из аргументов.
pub(super) fn playsound(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(sound) = input.word() else {
        return usage("/playsound <sound> <source> <targets> [pos] [volume] [pitch]");
    };
    let sound = args::resource_name(sound);
    // Источник — литерал дерева, но он стоит после аргумента звука, поэтому
    // в `path` не попадает и остаётся в строке.
    let source = input
        .rest()
        .split_whitespace()
        .next()
        .filter(|token| is_sound_source(token))
        .and_then(|_| input.word())
        .and_then(sound_source_id)
        .unwrap_or(0);
    let Some(token) = input.word() else {
        return usage("/playsound <sound> <source> <targets> [pos] [volume] [pitch]");
    };
    let targets = match ctx.select(call.source, token, false) {
        Ok(list) => list,
        Err(e) => return Outcome::Feedback(e.component()),
    };
    let (origin, yaw, pitch_look) = if let Some(pose) = ctx.execution_origin() {
        pose
    } else {
        call.source
            .uuid
            .and_then(|uuid| ctx.pose(uuid))
            .unwrap_or((call.position(ctx), 0.0, 0.0))
    };
    let at = if input
        .rest()
        .split_whitespace()
        .next()
        .is_some_and(args::is_coord)
    {
        let (x, y, z) = (input.word(), input.word(), input.word());
        match (x, y, z) {
            (Some(x), Some(y), Some(z)) => match args::coords3(x, y, z) {
                Some(coords) => {
                    Some(args::Basis::from_rotation(yaw, pitch_look).resolve(coords, origin))
                }
                None => return Outcome::Feedback(translate("argument.pos.invalid", Vec::new())),
            },
            _ => return usage("/playsound <sound> <source> <targets> [pos] [volume] [pitch]"),
        }
    } else {
        None
    };
    let volume = input
        .word()
        .map_or(1.0, |t| args::float(t, 0.0, 1000.0).unwrap_or(1.0));
    let pitch = input
        .word()
        .map_or(1.0, |t| args::float(t, 0.0, 2.0).unwrap_or(1.0));
    let min_volume = input
        .word()
        .map_or(0.0, |t| args::float(t, 0.0, 1.0).unwrap_or(0.0));
    if !ctx.plan.survival.sounds.contains_key(&sound) {
        return Outcome::Feedback(translate(
            "argument.sound.unknown",
            vec![Value::String(sound)],
        ));
    }
    let mut played = 0;
    for target in &targets {
        let (pos, heard) = match at {
            Some(origin) if distance(target.pos, origin) > volume * 16.0 => {
                if min_volume <= 0.0 {
                    continue;
                }
                (target.pos, min_volume)
            }
            Some(origin) => (origin, volume),
            None => (target.pos, volume),
        };
        if ctx.plan.play_named_sound_from(
            &sound,
            target.uuid,
            pos,
            heard as f32,
            pitch as f32,
            source,
        ) {
            played += 1;
        }
    }
    if played == 0 {
        return Outcome::Feedback(translate(
            "commands.playsound.failed",
            vec![name_of(&targets)],
        ));
    }
    Outcome::Feedback(translate(
        "commands.playsound.success",
        vec![Value::String(sound), name_of(&targets)],
    ))
}

/// `/stopsound <targets> [<source>] [<sound>]`.
pub(super) fn stopsound(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(token) = input.word() else {
        return usage("/stopsound <targets> [<source>] [<sound>]");
    };
    let targets = match ctx.select(call.source, token, false) {
        Ok(list) => list,
        Err(e) => return Outcome::Feedback(e.component()),
    };
    let (source, sound) = match input.word() {
        None => (None, None),
        Some("*") => (None, input.word().map(args::resource_name)),
        Some(token) if is_sound_source(token) => (
            sound_source_id(token),
            input.word().map(args::resource_name),
        ),
        Some(token) => (None, Some(args::resource_name(token))),
    };
    for target in &targets {
        ctx.plan.stop_sound(target.uuid, sound.as_deref(), source);
    }
    match sound {
        Some(sound) => Outcome::Feedback(translate(
            "commands.stopsound.success.soundSource",
            vec![Value::String(sound), name_of(&targets)],
        )),
        None => Outcome::Feedback(translate(
            "commands.stopsound.success.all",
            vec![name_of(&targets)],
        )),
    }
}

fn sound_source_id(token: &str) -> Option<i32> {
    Some(match token {
        "master" => 0,
        "music" => 1,
        "record" => 2,
        "weather" => 3,
        "block" => 4,
        "hostile" => 5,
        "neutral" => 6,
        "player" => 7,
        "ambient" => 8,
        "voice" => 9,
        "ui" => 10,
        _ => return None,
    })
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// Узлы дерева команд общения: форма сверена с ванильным `commands.json` 26.3.
pub(super) fn specs() -> Vec<super::dispatch::Spec> {
    use super::dispatch::Spec;
    use super::registry::{EVERYONE, GAMEMASTERS, ent};

    // `minecraft:message`, а не greedy-строка: клиент понимает, что тут
    // свободный текст, и не подсвечивает его как ошибку.
    let message = || super::registry::message();
    let say = Spec::group(
        "say",
        GAMEMASTERS,
        vec![Spec::arg_leaf("message", GAMEMASTERS, message(), say)],
    );
    let me = Spec::group(
        "me",
        EVERYONE,
        vec![Spec::arg_leaf("action", EVERYONE, message(), me)],
    );
    let msg = |name: &str| {
        Spec::group(
            name,
            EVERYONE,
            vec![
                args_group("targets", EVERYONE, ent(true, false), msg)
                    .with_children(vec![Spec::arg_leaf("message", EVERYONE, message(), msg)]),
            ],
        )
    };
    let teammsg = |name: &str| {
        Spec::group(
            name,
            EVERYONE,
            vec![Spec::arg_leaf("message", EVERYONE, message(), teammsg)],
        )
    };
    vec![
        say,
        me,
        msg("msg"),
        Spec::alias("tell", EVERYONE, "msg"),
        Spec::alias("w", EVERYONE, "msg"),
        teammsg("teammsg"),
        Spec::alias("tm", EVERYONE, "teammsg"),
        Spec::group(
            "tellraw",
            GAMEMASTERS,
            vec![
                args_group("targets", GAMEMASTERS, ent(true, false), tellraw).with_children(vec![
                    Spec::arg_leaf(
                        "message",
                        GAMEMASTERS,
                        super::registry::component_parser(),
                        tellraw,
                    ),
                ]),
            ],
        ),
        Spec::group(
            "title",
            GAMEMASTERS,
            vec![
                args_group("targets", GAMEMASTERS, ent(true, false), title).with_children(vec![
                    Spec::group(
                        "actionbar",
                        GAMEMASTERS,
                        vec![Spec::arg_leaf(
                            "title",
                            GAMEMASTERS,
                            super::registry::component_parser(),
                            title,
                        )],
                    ),
                    Spec::literal_leaf("clear", GAMEMASTERS, title),
                    Spec::literal_leaf("reset", GAMEMASTERS, title),
                    Spec::group(
                        "subtitle",
                        GAMEMASTERS,
                        vec![Spec::arg_leaf(
                            "title",
                            GAMEMASTERS,
                            super::registry::component_parser(),
                            title,
                        )],
                    ),
                    Spec::group(
                        "times",
                        GAMEMASTERS,
                        vec![
                            Spec::arg_leaf("fadeIn", GAMEMASTERS, super::registry::time(0), title)
                                .with_children(vec![
                                    Spec::arg_leaf(
                                        "stay",
                                        GAMEMASTERS,
                                        super::registry::time(0),
                                        title,
                                    )
                                    .with_children(vec![
                                        Spec::arg_leaf(
                                            "fadeOut",
                                            GAMEMASTERS,
                                            super::registry::time(0),
                                            title,
                                        ),
                                    ]),
                                ]),
                        ],
                    ),
                    Spec::group(
                        "title",
                        GAMEMASTERS,
                        vec![Spec::arg_leaf(
                            "title",
                            GAMEMASTERS,
                            super::registry::component_parser(),
                            title,
                        )],
                    ),
                ]),
            ],
        ),
        playsound_spec(),
        stopsound_spec(),
    ]
}

/// `/playsound <sound> <source> <targets> [pos] [volume] [pitch] [minVolume]`:
/// источник — литерал дерева (`master`, `player`, …), как в ванили.
fn playsound_spec() -> super::dispatch::Spec {
    use super::dispatch::Spec;
    use super::registry::{GAMEMASTERS, ent, float, vec3};
    let mut sources = Vec::new();
    for source in [
        "ambient", "block", "hostile", "master", "music", "neutral", "player", "record", "ui",
        "voice", "weather",
    ] {
        let pos = Spec::arg_leaf("pos", GAMEMASTERS, vec3(), playsound).with_children(vec![
            Spec::arg_leaf("volume", GAMEMASTERS, float(0.0, 3.4e38), playsound).with_children(
                vec![
                    Spec::arg_leaf("pitch", GAMEMASTERS, float(0.0, 2.0), playsound).with_children(
                        vec![Spec::arg_leaf(
                            "minVolume",
                            GAMEMASTERS,
                            float(0.0, 1.0),
                            playsound,
                        )],
                    ),
                ],
            ),
        ]);
        let targets = args_group("targets", GAMEMASTERS, ent(true, false), playsound)
            .with_children(vec![pos]);
        sources.push(Spec::exec(source, GAMEMASTERS, playsound, vec![targets]));
    }
    Spec::group(
        "playsound",
        GAMEMASTERS,
        vec![
            Spec::arg_leaf("sound", GAMEMASTERS, super::registry::sound(), playsound)
                .with_children(sources),
        ],
    )
}

/// `/stopsound <targets> <source|*>` — источник тоже литерал.
fn stopsound_spec() -> super::dispatch::Spec {
    use super::dispatch::Spec;
    use super::registry::{GAMEMASTERS, ent};
    let sound = || {
        Spec::arg_leaf(
            "sound",
            GAMEMASTERS,
            super::registry::resource_location(),
            stopsound,
        )
    };
    let mut sources = vec![Spec::exec("*", GAMEMASTERS, stopsound, vec![sound()])];
    for source in [
        "ambient", "block", "hostile", "master", "music", "neutral", "player", "record", "ui",
        "voice", "weather",
    ] {
        sources.push(Spec::exec(source, GAMEMASTERS, stopsound, vec![sound()]));
    }
    Spec::group(
        "stopsound",
        GAMEMASTERS,
        vec![
            args_group("targets", GAMEMASTERS, ent(true, false), stopsound).with_children(sources),
        ],
    )
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
