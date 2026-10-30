//! `/waypoint`: цвет и стиль уже существующей путевой точки.
//!
//! Точка есть у игрока (как у `WaypointTransmitter` ванили). Моб и предмет
//! её не имеют, и команда точку не создаёт. Клиенту уходит пакет `waypoint`.

use super::Context;
use super::args;
use super::dispatch::{Call, Outcome, Source, translate};
use super::entity::usage;
use super::selector::{Kind, Target};
use crate::play::PlayPlan;
use serde_json::{Value, json};
use uuid::Uuid;

/// Иконка точки. `sent` — клиентам уже ушёл `TRACK`, дальше шлём `UPDATE`.
#[derive(Clone, Debug)]
pub struct WaypointIcon {
    pub color: Option<i32>,
    pub style: String,
    pub sent: bool,
}

impl Default for WaypointIcon {
    fn default() -> Self {
        Self {
            color: None,
            style: DEFAULT_STYLE.into(),
            sent: false,
        }
    }
}

const DEFAULT_STYLE: &str = "minecraft:default";

const TEAM_COLORS: &[(&str, i32)] = &[
    ("black", 0x000000),
    ("dark_blue", 0x0000AA),
    ("dark_green", 0x00AA00),
    ("dark_aqua", 0x00AAAA),
    ("dark_red", 0xAA0000),
    ("dark_purple", 0xAA00AA),
    ("gold", 0xFFAA00),
    ("gray", 0xAAAAAA),
    ("dark_gray", 0x555555),
    ("blue", 0x5555FF),
    ("green", 0x55FF55),
    ("aqua", 0x55FFFF),
    ("red", 0xFF5555),
    ("light_purple", 0xFF55FF),
    ("yellow", 0xFFFF55),
    ("white", 0xFFFFFF),
];

impl PlayPlan {
    /// Точки, которые видит игрок, только что вошедший в мир.
    pub(crate) fn waypoint_join_packets(&self) -> Vec<Vec<u8>> {
        let icons = self.waypoints.lock().unwrap().clone();
        let mut out = Vec::new();
        for (uuid, icon) in icons {
            let Some(pos) = self.player_block(uuid) else {
                continue;
            };
            out.push(waypoint_packet(self.ids.waypoint, 0, uuid, &icon, pos));
        }
        out
    }

    fn player_block(&self, uuid: Uuid) -> Option<[i32; 3]> {
        let players = self.hub.players.lock().unwrap();
        let player = players.get(&uuid)?;
        Some(block_of(player.pos))
    }

    fn paint(&self, target: &Target, edit: impl FnOnce(&mut WaypointIcon)) {
        let (icon, operation) = {
            let mut map = self.waypoints.lock().unwrap();
            let icon = map.entry(target.uuid).or_default();
            edit(icon);
            let operation = if icon.sent { 2 } else { 0 };
            icon.sent = true;
            (icon.clone(), operation)
        };
        self.broadcast_packet(waypoint_packet(
            self.ids.waypoint,
            operation,
            target.uuid,
            &icon,
            block_of(target.pos),
        ));
    }
}

/// `/waypoint list` и `/waypoint modify`.
pub(super) fn waypoint(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    match call.path.first().map(String::as_str) {
        Some("list") => list_waypoints(ctx),
        Some("modify") => {
            let mut input = call.t();
            modify(ctx, call.source, &mut input)
        }
        _ => usage("/waypoint <list|modify>"),
    }
}

fn list_waypoints(ctx: &Context<'_>) -> Outcome {
    let dimension = ctx.plan.login.spawn.dimension.clone();
    let mut players: Vec<String> = ctx
        .targets()
        .into_iter()
        .filter(|target| target.game_mode.is_some())
        .map(|target| target.name)
        .collect();
    if players.is_empty() {
        return Outcome::Feedback(translate(
            "commands.waypoint.list.empty",
            vec![json!(dimension)],
        ));
    }
    players.sort();
    let count = players.len() as i32;
    Outcome::Feedback(translate(
        "commands.waypoint.list.success",
        vec![
            json!(count),
            json!(dimension),
            json!({"text": players.join(", ")}),
        ],
    ))
}

fn modify(ctx: &Context<'_>, source: &Source, input: &mut args::Input<'_>) -> Outcome {
    let Some(who) = input.word() else {
        return usage("/waypoint modify <entity> <color|style>");
    };
    let target = match one_entity(ctx, source, who) {
        Ok(target) => target,
        Err(outcome) => return outcome,
    };
    if target.game_mode.is_none() {
        return failed("argument.waypoint.invalid", Vec::new());
    }
    match input.word() {
        Some("color") => color(ctx, &target, input),
        Some("style") => style(ctx, &target, input),
        _ => usage("/waypoint modify <entity> <color|style>"),
    }
}

fn color(ctx: &Context<'_>, target: &Target, input: &mut args::Input<'_>) -> Outcome {
    match input.word() {
        Some("reset") => {
            ctx.plan.paint(target, |icon| icon.color = None);
            Outcome::Feedback(translate(
                "commands.waypoint.modify.color.reset",
                Vec::new(),
            ))
        }
        Some("hex") => {
            let Some(token) = input.word() else {
                return usage("/waypoint modify <entity> color hex <color>");
            };
            let Some(rgb) = parse_hex(token) else {
                return failed("argument.hexcolor.invalid", vec![json!({"text": token})]);
            };
            ctx.plan.paint(target, |icon| icon.color = Some(rgb));
            Outcome::Feedback(translate(
                "commands.waypoint.modify.color",
                vec![colored(&format!("{rgb:06X}"), rgb)],
            ))
        }
        Some(name) => {
            let Some(rgb) = team_rgb(name) else {
                return failed("argument.enum.invalid", vec![json!({"text": name})]);
            };
            let label = name.to_owned();
            ctx.plan.paint(target, |icon| icon.color = Some(rgb));
            Outcome::Feedback(translate(
                "commands.waypoint.modify.color",
                vec![colored(&label, rgb)],
            ))
        }
        None => usage("/waypoint modify <entity> color <color>"),
    }
}

fn style(ctx: &Context<'_>, target: &Target, input: &mut args::Input<'_>) -> Outcome {
    match input.word() {
        Some("reset") => {
            ctx.plan
                .paint(target, |icon| icon.style = DEFAULT_STYLE.into());
            Outcome::Feedback(translate("commands.waypoint.modify.style", Vec::new()))
        }
        Some("set") => {
            let Some(token) = input.word() else {
                return usage("/waypoint modify <entity> style set <style>");
            };
            let Some(name) = location(token) else {
                return failed("argument.id.invalid", vec![json!({"text": token})]);
            };
            ctx.plan.paint(target, |icon| icon.style = name);
            Outcome::Feedback(translate("commands.waypoint.modify.style", Vec::new()))
        }
        _ => usage("/waypoint modify <entity> style <set|reset>"),
    }
}

fn one_entity(ctx: &Context<'_>, source: &Source, who: &str) -> Result<Target, Outcome> {
    let Ok(selector) = super::selector::Selector::parse(who) else {
        return Err(failed("argument.entity.invalid", Vec::new()));
    };
    if !selector.wants_one() && selector.kind != Kind::Self_ {
        return Err(failed("argument.entity.toomany", Vec::new()));
    }
    // `limit=1` гарантирует одну цель: полный снимок мира не нужен,
    // перебор останавливается на первой подходящей.
    if let Some(found) = ctx.select_first(source, who, true) {
        return Ok(found);
    }
    let targets = match ctx.select(source, who, true) {
        Ok(list) => list,
        Err(error) => return Err(Outcome::Feedback(error.component())),
    };
    if targets.len() != 1 {
        return Err(failed("argument.entity.toomany", Vec::new()));
    }
    Ok(targets.into_iter().next().unwrap())
}

fn waypoint_packet(
    id: i32,
    operation: i32,
    uuid: Uuid,
    icon: &WaypointIcon,
    pos: [i32; 3],
) -> Vec<u8> {
    chunkwake_protocol::play::waypoint_position(id, operation, uuid, &icon.style, icon.color, pos)
}

fn block_of(pos: [f64; 3]) -> [i32; 3] {
    [
        pos[0].floor() as i32,
        pos[1].floor() as i32,
        pos[2].floor() as i32,
    ]
}

fn team_rgb(name: &str) -> Option<i32> {
    TEAM_COLORS
        .iter()
        .find(|(color, _)| *color == name)
        .map(|(_, rgb)| *rgb)
}

fn parse_hex(token: &str) -> Option<i32> {
    let hex = token.strip_prefix('#')?;
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    i32::from_str_radix(hex, 16).ok()
}

fn colored(label: &str, rgb: i32) -> Value {
    json!({
        "text": label,
        "color": format!("#{rgb:06X}"),
    })
}

fn location(token: &str) -> Option<String> {
    let name = args::resource_name(token);
    let (namespace, path) = name.split_once(':')?;
    let ok = |part: &str| {
        !part.is_empty()
            && part.bytes().all(|b| {
                b.is_ascii_lowercase()
                    || b.is_ascii_digit()
                    || matches!(b, b'_' | b'.' | b'/' | b'-')
            })
    };
    (ok(namespace) && ok(path)).then_some(name)
}

fn failed(key: &str, with: Vec<Value>) -> Outcome {
    let mut value = translate(key, with);
    if let Some(obj) = value.as_object_mut() {
        obj.insert("color".into(), json!("red"));
    }
    Outcome::Feedback(value)
}

pub(super) fn specs() -> Vec<super::dispatch::Spec> {
    use super::dispatch::Spec;
    use super::registry::{GAMEMASTERS, ent, resource_location, team_color};
    use chunkwake_protocol::text::Parser;
    vec![
        Spec::group(
            "waypoint",
            GAMEMASTERS,
            vec![
                Spec::literal_leaf("list", GAMEMASTERS, waypoint),
                Spec::group(
                    "modify",
                    GAMEMASTERS,
                    vec![Spec::arg(
                        "waypoint",
                        GAMEMASTERS,
                        ent(false, true),
                        vec![
                            Spec::group(
                                "color",
                                GAMEMASTERS,
                                vec![
                                    Spec::arg_leaf("color", GAMEMASTERS, team_color(), waypoint),
                                    Spec::group(
                                        "hex",
                                        GAMEMASTERS,
                                        vec![Spec::arg_leaf(
                                            "color",
                                            GAMEMASTERS,
                                            Parser::HexColor,
                                            waypoint,
                                        )],
                                    ),
                                    Spec::literal_leaf("reset", GAMEMASTERS, waypoint),
                                ],
                            ),
                            Spec::group(
                                "style",
                                GAMEMASTERS,
                                vec![
                                    Spec::literal_leaf("reset", GAMEMASTERS, waypoint),
                                    Spec::group(
                                        "set",
                                        GAMEMASTERS,
                                        vec![Spec::arg_leaf(
                                            "style",
                                            GAMEMASTERS,
                                            resource_location(),
                                            waypoint,
                                        )],
                                    ),
                                ],
                            ),
                        ],
                    )],
                ),
            ],
        )
        .with_handler(waypoint),
    ]
}
