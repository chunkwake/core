//! Команды общения: `/say`, `/me`, `/msg`, `/teammsg`, `/tellraw`, `/title`,
//! `/playsound`, `/stopsound`.
//!
//! Всё уходит теми же пакетами, что и обычный чат, поэтому мод-клиент видит
//! команды как ванильные сообщения: `system_chat`, `sound`, `title`.

use super::args;
use super::dispatch::{text_component, translate, Call, Outcome};
use super::entity::usage;
use super::Context;
use serde_json::{json, Value};

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
    let line = format!("[{}] {text}", call.source.name);
    let packet = chunkwake_protocol::play::system_chat(ctx.plan.chat_packet_id(), &line, false);
    ctx.plan.broadcast_packet(packet);
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
                ctx.plan.send_title_animation(target.uuid, fade_in, stay, fade_out);
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

/// `/playsound <sound> <source> <targets> [pos] [volume] [pitch]`.
///
/// Источник — литерал дерева (`master`, `player`, …), поэтому он приходит в
/// [`Call::path`], а позиция читается из аргументов.
pub(super) fn playsound(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(sound) = input.word() else {
        return usage("/playsound <sound> <source> <targets> [volume] [pitch]");
    };
    let sound = args::resource_name(sound);
    // `pos` необязателен: координаты отличаем по форме токена.
    let mut next = input.word();
    if next.is_some_and(args::is_coord) {
        // Три координаты: позиция перед целью не поддерживается нашим
        // проигрыванием (звук идёт от цели), но строка должна разбираться.
        for _ in 0..3 {
            let _ = input.word();
        }
        next = input.word();
    }
    let Some(token) = next else {
        return usage("/playsound <sound> <source> <targets> [volume] [pitch]");
    };
    let targets = match ctx.select(call.source, token, false) {
        Ok(list) => list,
        Err(e) => return Outcome::Feedback(e.component()),
    };
    // Позиция перед объёмом допустима и в ванильной ветке с целью.
    if input.word().is_some_and(args::is_coord) {
        for _ in 0..3 {
            let _ = input.word();
        }
    }
    let volume = input
        .word()
        .map_or(1.0, |t| args::float(t, 0.0, 1000.0).unwrap_or(1.0));
    let pitch = input
        .word()
        .map_or(1.0, |t| args::float(t, 0.0, 2.0).unwrap_or(1.0));
    let mut played = 0;
    for target in &targets {
        if ctx
            .plan
            .play_named_sound(&sound, target.uuid, target.pos, volume as f32, pitch as f32)
        {
            played += 1;
        }
    }
    if played == 0 {
        return Outcome::Feedback(translate(
            "argument.sound.unknown",
            vec![Value::String(sound)],
        ));
    }
    Outcome::Feedback(translate(
        "commands.playsound.success",
        vec![Value::String(sound), name_of(&targets)],
    ))
}

/// `/stopsound <targets> [sound]`.
pub(super) fn stopsound(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(token) = input.word() else {
        return usage("/stopsound <targets> [sound]");
    };
    let targets = match ctx.select(call.source, token, false) {
        Ok(list) => list,
        Err(e) => return Outcome::Feedback(e.component()),
    };
    let sound = input.word().map(args::resource_name);
    for target in &targets {
        ctx.plan.stop_sound(target.uuid, sound.as_deref());
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

/// Узлы дерева команд общения: форма сверена с ванильным `commands.json` 26.3.
pub(super) fn specs() -> Vec<super::dispatch::Spec> {
    use super::dispatch::Spec;
    use super::registry::{ent, GAMEMASTERS, EVERYONE};

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
                args_group("targets", EVERYONE, ent(true, false), msg).with_children(vec![
                    Spec::arg_leaf("message", EVERYONE, message(), msg),
                ]),
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
                                .with_children(vec![Spec::arg_leaf(
                                    "stay",
                                    GAMEMASTERS,
                                    super::registry::time(0),
                                    title,
                                )
                                .with_children(vec![Spec::arg_leaf(
                                    "fadeOut",
                                    GAMEMASTERS,
                                    super::registry::time(0),
                                    title,
                                )])]),
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
    use super::registry::{ent, float, vec3, GAMEMASTERS};
    let mut sources = Vec::new();
    for source in [
        "ambient", "block", "hostile", "master", "music", "neutral", "player", "record", "ui",
        "voice", "weather",
    ] {
        let pos = Spec::arg_leaf("pos", GAMEMASTERS, vec3(), playsound).with_children(vec![
            Spec::arg_leaf("volume", GAMEMASTERS, float(0.0, 3.4e38), playsound).with_children(vec![
                Spec::arg_leaf("pitch", GAMEMASTERS, float(0.0, 2.0), playsound).with_children(vec![
                    Spec::arg_leaf("minVolume", GAMEMASTERS, float(0.0, 1.0), playsound),
                ]),
            ]),
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
    use super::registry::{ent, GAMEMASTERS};
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
            args_group("targets", GAMEMASTERS, ent(true, false), stopsound)
                .with_children(sources),
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
