//! `/bossbar`, `/enchant`, `/spreadplayers`, `/stopwatch`, `/posteffect`.
//!
//! Полоска и эффекты экрана уходят тем же пакетам, что ждёт клиент.
//! Секундомер считает реальное время. Зачарование пишет чары в предмет
//! основной руки. Разброс ставит сущности на карту высот `motion_blocking`.

use super::Context;
use super::access::ColumnHeight;
use super::args;
use super::dispatch::{Call, Outcome, translate};
use super::entity::usage;
use super::selector::Target;
use crate::live::Control;
use crate::play::PlayPlan;
use chunkwake_protocol::inventory::Stack;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use uuid::Uuid;

/// Полоска босса. Зрители — игроки, которым она сейчас показана.
#[derive(Clone, Debug)]
pub struct BossBar {
    pub uuid: Uuid,
    pub name: Value,
    pub value: i32,
    pub max: i32,
    pub color: i32,
    pub overlay: i32,
    pub visible: bool,
    pub players: BTreeSet<Uuid>,
}

/// Секундомер: момент старта в миллисекундах от эпохи.
#[derive(Clone, Copy, Debug)]
pub struct Watch {
    pub started_ms: i64,
}

impl PlayPlan {
    /// Пакеты полосок и экранных эффектов для игрока, который только вошёл.
    pub(crate) fn joined_packets(&self, uuid: Uuid) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        let bars = self.boss_bars.lock().unwrap();
        for bar in bars.values() {
            if bar.visible && bar.players.contains(&uuid) {
                out.push(self.boss_add_packet(bar));
            }
        }
        drop(bars);
        if let Some(effects) = self.post_effects.lock().unwrap().get(&uuid) {
            out.push(chunkwake_protocol::play::post_effects(
                self.ids.post_effects,
                effects,
            ));
        }
        out.extend(self.waypoint_join_packets());
        out
    }

    fn boss_add_packet(&self, bar: &BossBar) -> Vec<u8> {
        chunkwake_protocol::play::boss_event_add(
            self.ids.boss_event,
            bar.uuid,
            &bar.name,
            bar_progress(bar),
            bar.color,
            bar.overlay,
        )
    }

    fn send_players(&self, players: &BTreeSet<Uuid>, packet: Vec<u8>) {
        for uuid in players {
            self.send_to(*uuid, packet.clone());
        }
    }

    /// Ставит сущность в точку. Игроку уходит телепорт, мобу — `teleport_entity`.
    pub(crate) fn place_at(&self, target: &Target, pos: [f64; 3]) -> bool {
        if target.game_mode.is_some() {
            if let Some(player) = self.hub.players.lock().unwrap().get_mut(&target.uuid) {
                player.pos = pos;
            }
            let _ = self.teleport_to(target.uuid, pos, target.yaw, target.pitch);
            return self.live.lock().unwrap().contains_key(&target.uuid);
        }
        let found = {
            let mut mobs = self.mobs.lock().unwrap();
            let Some(entry) = mobs.get_mut(&target.entity_id) else {
                return false;
            };
            entry.mob.pos = pos;
            true
        };
        if found {
            self.broadcast_packet(chunkwake_protocol::play::teleport_entity(
                self.ids.teleport_entity,
                target.entity_id,
                pos,
                target.yaw,
                target.pitch,
                true,
            ));
        }
        found
    }
}

/// `/bossbar`.
pub(super) fn bossbar(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let route: Vec<&str> = call.path.iter().map(String::as_str).collect();
    match route.as_slice() {
        ["list"] => list_bars(ctx),
        ["add"] => add_bar(ctx, &mut input),
        ["remove"] => remove_bar(ctx, &mut input),
        ["get"] => get_bar(ctx, &mut input),
        ["set"] => set_bar(ctx, call, &mut input),
        _ => usage("/bossbar <add|get|list|remove|set>"),
    }
}

fn add_bar(ctx: &Context<'_>, input: &mut args::Input<'_>) -> Outcome {
    let Some(id) = read_id(input) else {
        return usage("/bossbar add <id> <name>");
    };
    let raw = input.rest().trim();
    if raw.is_empty() {
        return usage("/bossbar add <id> <name>");
    }
    if ctx.plan.boss_bars.lock().unwrap().contains_key(&id) {
        return fail("commands.bossbar.create.failed", vec![json!({"text": id})]);
    }
    let bar = BossBar {
        uuid: bar_uuid(&id),
        name: component_value(raw),
        value: 0,
        max: 100,
        color: 6,
        overlay: 0,
        visible: true,
        players: BTreeSet::new(),
    };
    ctx.plan.boss_bars.lock().unwrap().insert(id.clone(), bar);
    Outcome::Feedback(translate(
        "commands.bossbar.create.success",
        vec![json!({"text": id})],
    ))
}

fn remove_bar(ctx: &Context<'_>, input: &mut args::Input<'_>) -> Outcome {
    let Some(id) = read_id(input) else {
        return usage("/bossbar remove <id>");
    };
    let Some(bar) = ctx.plan.boss_bars.lock().unwrap().remove(&id) else {
        return unknown_bar(&id);
    };
    if bar.visible {
        ctx.plan.send_players(
            &bar.players,
            chunkwake_protocol::play::boss_event_remove(ctx.plan.ids.boss_event, bar.uuid),
        );
    }
    Outcome::Feedback(translate(
        "commands.bossbar.remove.success",
        vec![json!({"text": id})],
    ))
}

fn list_bars(ctx: &Context<'_>) -> Outcome {
    let bars = ctx.plan.boss_bars.lock().unwrap();
    if bars.is_empty() {
        return Outcome::Feedback(translate("commands.bossbar.list.bars.none", Vec::new()));
    }
    let mut names: Vec<&str> = bars.keys().map(String::as_str).collect();
    names.sort_unstable();
    Outcome::Feedback(translate(
        "commands.bossbar.list.bars.some",
        vec![json!({"text": names.join(", ")})],
    ))
}

fn get_bar(ctx: &Context<'_>, input: &mut args::Input<'_>) -> Outcome {
    let Some(id) = read_id(input) else {
        return usage("/bossbar get <id> <max|players|value|visible>");
    };
    let Some(field) = input.word() else {
        return usage("/bossbar get <id> <max|players|value|visible>");
    };
    let bars = ctx.plan.boss_bars.lock().unwrap();
    let Some(bar) = bars.get(&id) else {
        return unknown_bar(&id);
    };
    let id_text = json!({"text": id});
    match field {
        "value" => Outcome::Feedback(translate(
            "commands.bossbar.get.value",
            vec![id_text, Value::from(bar.value)],
        )),
        "max" => Outcome::Feedback(translate(
            "commands.bossbar.get.max",
            vec![id_text, Value::from(bar.max)],
        )),
        "visible" => {
            let key = if bar.visible {
                "commands.bossbar.get.visible.visible"
            } else {
                "commands.bossbar.get.visible.hidden"
            };
            Outcome::Feedback(translate(
                key,
                vec![id_text, Value::from(i32::from(bar.visible))],
            ))
        }
        "players" => {
            let names = player_names(ctx, &bar.players);
            if names.is_empty() {
                Outcome::Feedback(translate(
                    "commands.bossbar.get.players.none",
                    vec![id_text, Value::from(0)],
                ))
            } else {
                Outcome::Feedback(translate(
                    "commands.bossbar.get.players.some",
                    vec![
                        id_text,
                        json!({"text": names.join(", ")}),
                        Value::from(names.len() as i32),
                    ],
                ))
            }
        }
        _ => usage("/bossbar get <id> <max|players|value|visible>"),
    }
}

fn set_bar(ctx: &Context<'_>, call: &Call<'_>, input: &mut args::Input<'_>) -> Outcome {
    let Some(id) = read_id(input) else {
        return usage("/bossbar set <id> <color|max|name|players|style|value|visible>");
    };
    let Some(field) = input.word() else {
        return usage("/bossbar set <id> <color|max|name|players|style|value|visible>");
    };
    if !ctx.plan.boss_bars.lock().unwrap().contains_key(&id) {
        return unknown_bar(&id);
    }
    match field {
        "value" => set_value(ctx, &id, input),
        "max" => set_max(ctx, &id, input),
        "color" => set_color(ctx, &id, input),
        "style" => set_style(ctx, &id, input),
        "visible" => set_visible(ctx, &id, input),
        "name" => set_name(ctx, &id, input),
        "players" => set_players(ctx, call, &id, input),
        _ => usage("/bossbar set <id> <color|max|name|players|style|value|visible>"),
    }
}

fn set_value(ctx: &Context<'_>, id: &str, input: &mut args::Input<'_>) -> Outcome {
    let Some(value) = input
        .word()
        .and_then(|token| token.parse::<i32>().ok())
        .filter(|n| *n >= 0)
    else {
        return usage("/bossbar set <id> value <value>");
    };
    let mut bars = ctx.plan.boss_bars.lock().unwrap();
    let Some(bar) = bars.get_mut(id) else {
        return unknown_bar(id);
    };
    let stored = value.min(bar.max);
    if stored == bar.value {
        return fail(
            "commands.bossbar.set.value.unchanged",
            vec![json!({"text": id})],
        );
    }
    bar.value = stored;
    let packet = chunkwake_protocol::play::boss_event_progress(
        ctx.plan.ids.boss_event,
        bar.uuid,
        bar_progress(bar),
    );
    let players = bar.players.clone();
    let visible = bar.visible;
    drop(bars);
    if visible {
        ctx.plan.send_players(&players, packet);
    }
    Outcome::Feedback(translate(
        "commands.bossbar.set.value.success",
        vec![json!({"text": id}), Value::from(stored)],
    ))
}

fn set_max(ctx: &Context<'_>, id: &str, input: &mut args::Input<'_>) -> Outcome {
    let Some(max) = input
        .word()
        .and_then(|token| token.parse::<i32>().ok())
        .filter(|n| *n >= 1)
    else {
        return usage("/bossbar set <id> max <max>");
    };
    let mut bars = ctx.plan.boss_bars.lock().unwrap();
    let Some(bar) = bars.get_mut(id) else {
        return unknown_bar(id);
    };
    if max == bar.max && bar.value <= max {
        return fail(
            "commands.bossbar.set.max.unchanged",
            vec![json!({"text": id})],
        );
    }
    bar.max = max;
    if bar.value > max {
        bar.value = max;
    }
    let packet = chunkwake_protocol::play::boss_event_progress(
        ctx.plan.ids.boss_event,
        bar.uuid,
        bar_progress(bar),
    );
    let players = bar.players.clone();
    let visible = bar.visible;
    drop(bars);
    if visible {
        ctx.plan.send_players(&players, packet);
    }
    Outcome::Feedback(translate(
        "commands.bossbar.set.max.success",
        vec![json!({"text": id}), Value::from(max)],
    ))
}

fn set_color(ctx: &Context<'_>, id: &str, input: &mut args::Input<'_>) -> Outcome {
    let Some(color) = input.word().and_then(bar_color) else {
        return usage("/bossbar set <id> color <color>");
    };
    let mut bars = ctx.plan.boss_bars.lock().unwrap();
    let Some(bar) = bars.get_mut(id) else {
        return unknown_bar(id);
    };
    if bar.color == color {
        return fail(
            "commands.bossbar.set.color.unchanged",
            vec![json!({"text": id})],
        );
    }
    bar.color = color;
    let packet = chunkwake_protocol::play::boss_event_style(
        ctx.plan.ids.boss_event,
        bar.uuid,
        bar.color,
        bar.overlay,
    );
    let players = bar.players.clone();
    let visible = bar.visible;
    drop(bars);
    if visible {
        ctx.plan.send_players(&players, packet);
    }
    Outcome::Feedback(translate(
        "commands.bossbar.set.color.success",
        vec![json!({"text": id})],
    ))
}

fn set_style(ctx: &Context<'_>, id: &str, input: &mut args::Input<'_>) -> Outcome {
    let Some(overlay) = input.word().and_then(bar_overlay) else {
        return usage("/bossbar set <id> style <style>");
    };
    let mut bars = ctx.plan.boss_bars.lock().unwrap();
    let Some(bar) = bars.get_mut(id) else {
        return unknown_bar(id);
    };
    if bar.overlay == overlay {
        return fail(
            "commands.bossbar.set.style.unchanged",
            vec![json!({"text": id})],
        );
    }
    bar.overlay = overlay;
    let packet = chunkwake_protocol::play::boss_event_style(
        ctx.plan.ids.boss_event,
        bar.uuid,
        bar.color,
        bar.overlay,
    );
    let players = bar.players.clone();
    let visible = bar.visible;
    drop(bars);
    if visible {
        ctx.plan.send_players(&players, packet);
    }
    Outcome::Feedback(translate(
        "commands.bossbar.set.style.success",
        vec![json!({"text": id})],
    ))
}

fn set_visible(ctx: &Context<'_>, id: &str, input: &mut args::Input<'_>) -> Outcome {
    let Some(visible) = input.word().and_then(parse_bool) else {
        return usage("/bossbar set <id> visible <visible>");
    };
    let mut bars = ctx.plan.boss_bars.lock().unwrap();
    let Some(bar) = bars.get_mut(id) else {
        return unknown_bar(id);
    };
    if bar.visible == visible {
        let key = if visible {
            "commands.bossbar.set.visibility.unchanged.visible"
        } else {
            "commands.bossbar.set.visibility.unchanged.hidden"
        };
        return fail(key, vec![json!({"text": id})]);
    }
    bar.visible = visible;
    let players = bar.players.clone();
    let packet = if visible {
        ctx.plan.boss_add_packet(bar)
    } else {
        chunkwake_protocol::play::boss_event_remove(ctx.plan.ids.boss_event, bar.uuid)
    };
    drop(bars);
    ctx.plan.send_players(&players, packet);
    let key = if visible {
        "commands.bossbar.set.visible.success.visible"
    } else {
        "commands.bossbar.set.visible.success.hidden"
    };
    Outcome::Feedback(translate(key, vec![json!({"text": id})]))
}

fn set_name(ctx: &Context<'_>, id: &str, input: &mut args::Input<'_>) -> Outcome {
    let raw = input.rest().trim();
    if raw.is_empty() {
        return usage("/bossbar set <id> name <name>");
    }
    let name = component_value(raw);
    let mut bars = ctx.plan.boss_bars.lock().unwrap();
    let Some(bar) = bars.get_mut(id) else {
        return unknown_bar(id);
    };
    if bar.name == name {
        return fail(
            "commands.bossbar.set.name.unchanged",
            vec![json!({"text": id})],
        );
    }
    bar.name = name.clone();
    let packet =
        chunkwake_protocol::play::boss_event_name(ctx.plan.ids.boss_event, bar.uuid, &name);
    let players = bar.players.clone();
    let visible = bar.visible;
    drop(bars);
    if visible {
        ctx.plan.send_players(&players, packet);
    }
    Outcome::Feedback(translate(
        "commands.bossbar.set.name.success",
        vec![json!({"text": id})],
    ))
}

fn set_players(
    ctx: &Context<'_>,
    call: &Call<'_>,
    id: &str,
    input: &mut args::Input<'_>,
) -> Outcome {
    let targets = match input.word() {
        None => Vec::new(),
        Some(token) => match ctx.select(call.source, token, false) {
            Ok(list) => list,
            Err(error) => return Outcome::Feedback(error.component()),
        },
    };
    let next: BTreeSet<Uuid> = targets.iter().map(|target| target.uuid).collect();
    let mut bars = ctx.plan.boss_bars.lock().unwrap();
    let Some(bar) = bars.get_mut(id) else {
        return unknown_bar(id);
    };
    if bar.players == next {
        return fail(
            "commands.bossbar.set.players.unchanged",
            vec![json!({"text": id})],
        );
    }
    let removed: BTreeSet<Uuid> = bar.players.difference(&next).copied().collect();
    let added: BTreeSet<Uuid> = next.difference(&bar.players).copied().collect();
    bar.players = next;
    let uuid = bar.uuid;
    let add_packet = ctx.plan.boss_add_packet(bar);
    let visible = bar.visible;
    drop(bars);
    if visible {
        ctx.plan.send_players(
            &removed,
            chunkwake_protocol::play::boss_event_remove(ctx.plan.ids.boss_event, uuid),
        );
        ctx.plan.send_players(&added, add_packet);
    }
    if targets.is_empty() {
        Outcome::Feedback(translate(
            "commands.bossbar.set.players.success.none",
            vec![json!({"text": id})],
        ))
    } else {
        Outcome::Feedback(translate(
            "commands.bossbar.set.players.success.some",
            vec![json!({"text": id}), Value::from(targets.len() as i32)],
        ))
    }
}

/// `/stopwatch`.
pub(super) fn stopwatch(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let route: Vec<&str> = call.path.iter().map(String::as_str).collect();
    let Some(id) = read_id(&mut input) else {
        return usage("/stopwatch <create|query|remove|restart> <id>");
    };
    match route.as_slice() {
        ["create"] => {
            let mut watches = ctx.plan.stopwatches.lock().unwrap();
            if watches.contains_key(&id) {
                return fail(
                    "commands.stopwatch.already_exists",
                    vec![json!({"text": id})],
                );
            }
            watches.insert(
                id.clone(),
                Watch {
                    started_ms: now_ms(),
                },
            );
            Outcome::Feedback(translate(
                "commands.stopwatch.create.success",
                vec![json!({"text": id})],
            ))
        }
        ["restart"] => {
            let mut watches = ctx.plan.stopwatches.lock().unwrap();
            let Some(watch) = watches.get_mut(&id) else {
                return missing_watch(&id);
            };
            watch.started_ms = now_ms();
            Outcome::Feedback(translate(
                "commands.stopwatch.restart.success",
                vec![json!({"text": id})],
            ))
        }
        ["remove"] => {
            if ctx.plan.stopwatches.lock().unwrap().remove(&id).is_none() {
                return missing_watch(&id);
            }
            Outcome::Feedback(translate(
                "commands.stopwatch.remove.success",
                vec![json!({"text": id})],
            ))
        }
        ["query"] => {
            let scale = match input.word() {
                None => 1.0,
                Some(token) => match token.parse::<f64>() {
                    Ok(scale) => scale,
                    Err(_) => return usage("/stopwatch query <id> [scale]"),
                },
            };
            let Some(watch) = ctx.plan.stopwatches.lock().unwrap().get(&id).copied() else {
                return missing_watch(&id);
            };
            let seconds = (now_ms() - watch.started_ms).max(0) as f64 / 1000.0;
            let scaled = seconds * scale;
            let result = scaled.floor().clamp(i32::MIN as f64, i32::MAX as f64) as i32;
            Outcome::Feedback(translate(
                "commands.stopwatch.query",
                vec![json!({"text": id}), json!(scaled), Value::from(result)],
            ))
        }
        _ => usage("/stopwatch <create|query|remove|restart> <id>"),
    }
}

/// `/posteffect`.
pub(super) fn posteffect(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let route: Vec<&str> = call.path.iter().map(String::as_str).collect();
    match route.as_slice() {
        ["add"] => post_add(ctx, call, &mut input),
        ["remove"] => post_remove(ctx, call, &mut input),
        ["clear"] => post_clear(ctx, call, &mut input),
        ["list"] => post_list(ctx, call, &mut input),
        _ => usage("/posteffect <add|clear|list|remove>"),
    }
}

fn post_add(ctx: &Context<'_>, call: &Call<'_>, input: &mut args::Input<'_>) -> Outcome {
    let Some(token) = input.word() else {
        return usage("/posteffect add <targets> <effect>");
    };
    let Some(effect) = input.word().and_then(location) else {
        return usage("/posteffect add <targets> <effect>");
    };
    let targets = match players_of(ctx, call, token) {
        Ok(list) => list,
        Err(outcome) => return outcome,
    };
    let mut changed = Vec::new();
    for target in &targets {
        let mut map = ctx.plan.post_effects.lock().unwrap();
        let list = map.entry(target.uuid).or_default();
        if list.iter().any(|held| held == &effect) {
            continue;
        }
        list.push(effect.clone());
        let packet = chunkwake_protocol::play::post_effects(ctx.plan.ids.post_effects, list);
        drop(map);
        ctx.plan.send_to(target.uuid, packet);
        changed.push(target);
    }
    count_result(
        &changed,
        "commands.posteffect.add.success.single",
        "commands.posteffect.add.success.multiple",
        "commands.posteffect.add.failed",
        &effect,
    )
}

fn post_remove(ctx: &Context<'_>, call: &Call<'_>, input: &mut args::Input<'_>) -> Outcome {
    let Some(token) = input.word() else {
        return usage("/posteffect remove <targets> <effect>");
    };
    let Some(effect) = input.word().and_then(location) else {
        return usage("/posteffect remove <targets> <effect>");
    };
    let targets = match players_of(ctx, call, token) {
        Ok(list) => list,
        Err(outcome) => return outcome,
    };
    let mut changed = Vec::new();
    for target in &targets {
        let mut map = ctx.plan.post_effects.lock().unwrap();
        let Some(list) = map.get_mut(&target.uuid) else {
            continue;
        };
        let before = list.len();
        list.retain(|held| held != &effect);
        if list.len() == before {
            continue;
        }
        let packet = chunkwake_protocol::play::post_effects(ctx.plan.ids.post_effects, list);
        drop(map);
        ctx.plan.send_to(target.uuid, packet);
        changed.push(target);
    }
    count_result(
        &changed,
        "commands.posteffect.remove.success.single",
        "commands.posteffect.remove.success.multiple",
        "commands.posteffect.remove.failed",
        &effect,
    )
}

fn post_clear(ctx: &Context<'_>, call: &Call<'_>, input: &mut args::Input<'_>) -> Outcome {
    let Some(token) = input.word() else {
        return usage("/posteffect clear <targets>");
    };
    let targets = match players_of(ctx, call, token) {
        Ok(list) => list,
        Err(outcome) => return outcome,
    };
    let mut changed = Vec::new();
    for target in &targets {
        let mut map = ctx.plan.post_effects.lock().unwrap();
        let Some(list) = map.get_mut(&target.uuid) else {
            continue;
        };
        if list.is_empty() {
            continue;
        }
        list.clear();
        let packet = chunkwake_protocol::play::post_effects(ctx.plan.ids.post_effects, list);
        drop(map);
        ctx.plan.send_to(target.uuid, packet);
        changed.push(target);
    }
    count_result(
        &changed,
        "commands.posteffect.clear.success.single",
        "commands.posteffect.clear.success.multiple",
        "commands.posteffect.clear.failed",
        "",
    )
}

fn post_list(ctx: &Context<'_>, call: &Call<'_>, input: &mut args::Input<'_>) -> Outcome {
    let Some(token) = input.word() else {
        return usage("/posteffect list <target>");
    };
    let targets = match players_of(ctx, call, token) {
        Ok(list) => list,
        Err(outcome) => return outcome,
    };
    if targets.len() != 1 {
        return Outcome::Feedback(translate("argument.entity.toomany", Vec::new()));
    }
    let target = &targets[0];
    let effects = ctx
        .plan
        .post_effects
        .lock()
        .unwrap()
        .get(&target.uuid)
        .cloned()
        .unwrap_or_default();
    if effects.is_empty() {
        return Outcome::Feedback(translate(
            "commands.posteffect.list.empty",
            vec![name_of(target)],
        ));
    }
    Outcome::Feedback(translate(
        "commands.posteffect.list.success",
        vec![name_of(target), json!({"text": effects.join(", ")})],
    ))
}

/// `/enchant`.
pub(super) fn enchant(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(token) = input.word() else {
        return usage("/enchant <targets> <enchantment> [level]");
    };
    let Some(enchantment) = input.word() else {
        return usage("/enchant <targets> <enchantment> [level]");
    };
    let level = match input.word() {
        None => 1,
        Some(token) => match token.parse::<i32>().ok().filter(|n| *n >= 0) {
            Some(level) => level,
            None => return usage("/enchant <targets> <enchantment> [level]"),
        },
    };
    let name = match location(enchantment) {
        Some(name) => name,
        None => {
            return fail(
                "argument.resource.invalid",
                vec![Value::String(enchantment.to_owned())],
            );
        }
    };
    let Some(def) = ctx
        .plan
        .survival
        .enchanting
        .defs
        .iter()
        .find(|def| def.name == name)
    else {
        return fail("argument.resource.invalid", vec![Value::String(name)]);
    };
    if level > def.max_level {
        return fail(
            "commands.enchant.failed.level",
            vec![Value::from(def.max_level)],
        );
    }
    let enchant_id = def.id;
    let exclusive: Vec<String> = def.exclusive.iter().cloned().collect();
    let targets = match ctx.select(call.source, token, true) {
        Ok(list) => list,
        Err(error) => return Outcome::Feedback(error.component()),
    };
    let mut applied = Vec::new();
    let mut single_error = None;
    for target in &targets {
        match enchant_one(ctx, target, enchant_id, level, &exclusive) {
            Ok(()) => applied.push(target),
            Err(key) if targets.len() == 1 => single_error = Some(key),
            Err(_) => {}
        }
    }
    if applied.is_empty() {
        let key = single_error.unwrap_or("commands.enchant.failed");
        let with = if targets.len() == 1 {
            vec![name_of(&targets[0])]
        } else {
            Vec::new()
        };
        return fail(key, with);
    }
    if applied.len() == 1 {
        Outcome::Feedback(translate(
            "commands.enchant.success.single",
            vec![
                json!({"text": name}),
                Value::from(level),
                name_of(applied[0]),
            ],
        ))
    } else {
        Outcome::Feedback(translate(
            "commands.enchant.success.multiple",
            vec![Value::from(applied.len() as i32)],
        ))
    }
}

fn enchant_one(
    ctx: &Context<'_>,
    target: &Target,
    enchant_id: i32,
    level: i32,
    exclusive: &[String],
) -> Result<(), &'static str> {
    if target.game_mode.is_none() {
        return Err("commands.enchant.failed.entity");
    }
    let (window, stack, item_name) = {
        let live = ctx.plan.live.lock().unwrap();
        let Some(player) = live.get(&target.uuid) else {
            return Err("commands.enchant.failed.entity");
        };
        let window = 36 + usize::from(player.selected).min(8);
        let Some(stack) = player.slots.get(window).copied().flatten() else {
            return Err("commands.enchant.failed.itemless");
        };
        let item_name = ctx.plan.item_name(stack.item).to_owned();
        (window, stack, item_name)
    };
    let supported = ctx
        .plan
        .survival
        .enchanting
        .defs
        .iter()
        .find(|def| def.id == enchant_id)
        .is_some_and(|def| def.supported.contains(&item_name));
    if !supported {
        return Err("commands.enchant.failed.incompatible");
    }
    let mut extra = (*chunkwake_protocol::components::get(stack.extra)).clone();
    let book = item_name == "minecraft:book" || item_name == "minecraft:enchanted_book";
    let list = if book {
        &mut extra.stored
    } else {
        &mut extra.enchantments
    };
    if level > 0 && list_conflicts(ctx, list, enchant_id, exclusive) {
        return Err("commands.enchant.failed.incompatible");
    }
    if level == 0 {
        list.retain(|(id, _)| *id != enchant_id);
    } else if let Some(slot) = list.iter_mut().find(|(id, _)| *id == enchant_id) {
        slot.1 = level;
    } else {
        list.push((enchant_id, level));
    }
    let next = Stack {
        extra: chunkwake_protocol::components::intern(extra),
        ..stack
    };
    let _ = ctx
        .plan
        .push_control(target.uuid, Control::Slot(window, Some(next)));
    Ok(())
}

fn list_conflicts(
    ctx: &Context<'_>,
    list: &[(i32, i32)],
    enchant_id: i32,
    exclusive: &[String],
) -> bool {
    for &(id, _) in list {
        if id == enchant_id {
            continue;
        }
        let Some(other) = ctx
            .plan
            .survival
            .enchanting
            .defs
            .iter()
            .find(|def| def.id == id)
        else {
            continue;
        };
        if exclusive.iter().any(|name| name == &other.name)
            || other.exclusive.iter().any(|name| {
                ctx.plan
                    .survival
                    .enchanting
                    .defs
                    .iter()
                    .find(|def| def.id == enchant_id)
                    .is_some_and(|def| name == &def.name)
            })
        {
            return true;
        }
    }
    false
}

/// `/spreadplayers`.
pub(super) fn spreadplayers(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let (Some(x), Some(z)) = (input.word(), input.word()) else {
        return usage(
            "/spreadplayers <center> <spreadDistance> <maxRange> <respectTeams> <targets>",
        );
    };
    let Some((center_x, center_z)) = column_xz(x, z, call, ctx) else {
        return fail("argument.pos2d.incomplete", Vec::new());
    };
    let Some(spread) = input
        .word()
        .and_then(|token| token.parse::<f32>().ok())
        .filter(|n| *n >= 0.0)
    else {
        return usage("/spreadplayers <center> <spreadDistance> <maxRange> …");
    };
    let Some(range) = input
        .word()
        .and_then(|token| token.parse::<f32>().ok())
        .filter(|n| *n >= 1.0)
    else {
        return usage("/spreadplayers <center> <spreadDistance> <maxRange> …");
    };
    let under = if input.rest().split_whitespace().next() == Some("under") {
        input.word();
        let Some(height) = input.word().and_then(|token| token.parse::<i32>().ok()) else {
            return usage("/spreadplayers … under <maxHeight> <respectTeams> <targets>");
        };
        Some(height)
    } else {
        None
    };
    if let Some(height) = under
        && height < ctx.plan.min_y
    {
        return fail("commands.spreadplayers.failed.invalid.height", Vec::new());
    }
    let Some(respect) = input.word().and_then(parse_bool) else {
        return usage("/spreadplayers … <respectTeams> <targets>");
    };
    let Some(token) = input.word() else {
        return usage("/spreadplayers … <targets>");
    };
    let targets = match ctx.select(call.source, token, true) {
        Ok(list) => list,
        Err(error) => return Outcome::Feedback(error.component()),
    };
    let groups = group_targets(ctx, &targets, respect);
    let mut seed =
        (ctx.plan.game_time() as u64) ^ (center_x.to_bits()) ^ center_z.to_bits().rotate_left(17);
    let spots = scatter(
        center_x,
        center_z,
        f64::from(spread),
        f64::from(range),
        groups.len(),
        &mut seed,
    );
    let fail_key = if respect {
        "commands.spreadplayers.failed.teams"
    } else {
        "commands.spreadplayers.failed.entities"
    };
    let mut standing = Vec::new();
    for (group, spot) in groups.iter().zip(spots.iter()) {
        if group.iter().any(|target| !can_place(ctx, target)) {
            return fail(fail_key, Vec::new());
        }
        let Some(y) = feet_y(ctx.plan, spot[0], spot[1], under) else {
            return fail(fail_key, Vec::new());
        };
        standing.push((group, [spot[0], y, spot[1]]));
    }
    let mut placed = Vec::new();
    for (group, pos) in standing {
        for target in group {
            if !ctx.plan.place_at(target, pos) {
                return fail(fail_key, Vec::new());
            }
        }
        placed.push([pos[0], pos[2]]);
    }
    let key = if respect {
        "commands.spreadplayers.success.teams"
    } else {
        "commands.spreadplayers.success.entities"
    };
    Outcome::Feedback(translate(
        key,
        vec![
            Value::from(targets.len() as i32),
            json!(center_x),
            json!(center_z),
            json!(average_gap(&placed)),
        ],
    ))
}

fn can_place(ctx: &Context<'_>, target: &Target) -> bool {
    target.game_mode.is_some()
        || ctx
            .plan
            .mobs
            .lock()
            .unwrap()
            .contains_key(&target.entity_id)
}

fn feet_y(plan: &PlayPlan, x: f64, z: f64, under: Option<i32>) -> Option<f64> {
    let surface = match plan.height_over(x, z, "motion_blocking") {
        ColumnHeight::Y(y) => y,
        ColumnHeight::Missing | ColumnHeight::Unknown => return None,
    };
    match under {
        None => Some(surface),
        Some(limit) if surface <= f64::from(limit) => Some(surface),
        Some(_) => None,
    }
}

fn group_targets(ctx: &Context<'_>, targets: &[Target], respect: bool) -> Vec<Vec<Target>> {
    if !respect {
        return targets.iter().cloned().map(|target| vec![target]).collect();
    }
    let mut teams: Vec<(String, Vec<Target>)> = Vec::new();
    let mut alone = Vec::new();
    for target in targets {
        let team = ctx.plan.teams.lock().unwrap().team_of(&target.name);
        match team {
            Some(name) => {
                if let Some((_, group)) = teams.iter_mut().find(|(team, _)| team == &name) {
                    group.push(target.clone());
                } else {
                    teams.push((name, vec![target.clone()]));
                }
            }
            None => alone.push(vec![target.clone()]),
        }
    }
    teams
        .into_iter()
        .map(|(_, group)| group)
        .chain(alone)
        .collect()
}

fn scatter(
    cx: f64,
    cz: f64,
    spread: f64,
    range: f64,
    count: usize,
    seed: &mut u64,
) -> Vec<[f64; 2]> {
    let mut points = Vec::with_capacity(count);
    for _ in 0..count {
        let x = cx + (unit(seed) * 2.0 - 1.0) * range;
        let z = cz + (unit(seed) * 2.0 - 1.0) * range;
        points.push([x, z]);
    }
    for _ in 0..64 {
        let mut moved = false;
        for i in 0..points.len() {
            for j in (i + 1)..points.len() {
                let dx = points[i][0] - points[j][0];
                let dz = points[i][1] - points[j][1];
                let dist = (dx * dx + dz * dz).sqrt();
                if dist >= spread {
                    continue;
                }
                let push = (spread - dist) / 2.0 + 1e-3;
                let (nx, nz) = if dist < 1e-4 {
                    (1.0, 0.0)
                } else {
                    (dx / dist, dz / dist)
                };
                points[i][0] += nx * push;
                points[i][1] += nz * push;
                points[j][0] -= nx * push;
                points[j][1] -= nz * push;
                moved = true;
            }
        }
        for point in &mut points {
            point[0] = point[0].clamp(cx - range, cx + range);
            point[1] = point[1].clamp(cz - range, cz + range);
        }
        if !moved {
            break;
        }
    }
    points
}

fn average_gap(points: &[[f64; 2]]) -> f64 {
    if points.len() < 2 {
        return 0.0;
    }
    let mut sum = 0.0;
    for (i, point) in points.iter().enumerate() {
        let mut best = f64::MAX;
        for (j, other) in points.iter().enumerate() {
            if i == j {
                continue;
            }
            let dx = point[0] - other[0];
            let dz = point[1] - other[1];
            best = best.min((dx * dx + dz * dz).sqrt());
        }
        sum += best;
    }
    sum / points.len() as f64
}

fn unit(seed: &mut u64) -> f64 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    ((*seed >> 33) as f64) / (u32::MAX as f64)
}

fn column_xz(x: &str, z: &str, call: &Call<'_>, ctx: &Context<'_>) -> Option<(f64, f64)> {
    let coords = [args::Coord::parse(x)?, args::Coord::parse(z)?];
    let base = call.position(ctx);
    let (yaw, pitch) = call
        .source
        .uuid
        .and_then(|uuid| ctx.pose(uuid))
        .map(|(_, yaw, pitch)| (yaw, pitch))
        .unwrap_or((0.0, 0.0));
    let world = args::Basis::from_rotation(yaw, pitch)
        .resolve([coords[0], args::Coord::Abs(base[1]), coords[1]], base);
    Some((world[0], world[2]))
}

fn players_of(ctx: &Context<'_>, call: &Call<'_>, token: &str) -> Result<Vec<Target>, Outcome> {
    match ctx.select(call.source, token, false) {
        Ok(list) if list.is_empty() => Err(Outcome::Feedback(translate(
            "argument.entity.notfound.player",
            Vec::new(),
        ))),
        Ok(list) => Ok(list),
        Err(error) => Err(Outcome::Feedback(error.component())),
    }
}

fn count_result(
    changed: &[&Target],
    single: &str,
    many: &str,
    failed: &str,
    extra: &str,
) -> Outcome {
    match changed {
        [] => fail(failed, Vec::new()),
        [one] => Outcome::Feedback(translate(
            single,
            vec![name_of(one), json!({"text": extra})],
        )),
        many_targets => Outcome::Feedback(translate(
            many,
            vec![
                Value::from(many_targets.len() as i32),
                json!({"text": extra}),
            ],
        )),
    }
}

fn player_names(ctx: &Context<'_>, players: &BTreeSet<Uuid>) -> Vec<String> {
    players
        .iter()
        .map(|uuid| {
            ctx.plan
                .command_target(*uuid)
                .map(|target| target.name)
                .unwrap_or_else(|| uuid.to_string())
        })
        .collect()
}

fn read_id(input: &mut args::Input<'_>) -> Option<String> {
    input.word().and_then(location)
}

fn location(token: &str) -> Option<String> {
    let name = args::resource_name(token);
    let (namespace, path) = name.split_once(':')?;
    let ok = |text: &str| {
        !text.is_empty()
            && text.chars().all(|ch| {
                ch.is_ascii_lowercase()
                    || ch.is_ascii_digit()
                    || matches!(ch, '_' | '.' | '/' | '-')
            })
    };
    (ok(namespace) && ok(path)).then_some(name)
}

fn component_value(raw: &str) -> Value {
    let raw = raw.trim();
    if raw.starts_with('{') || raw.starts_with('[') || raw.starts_with('"') {
        if let Ok(value) = serde_json::from_str(raw) {
            return value;
        }
    }
    json!({"text": raw})
}

fn parse_bool(token: &str) -> Option<bool> {
    match token {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn bar_color(name: &str) -> Option<i32> {
    Some(match name {
        "pink" => 0,
        "blue" => 1,
        "red" => 2,
        "green" => 3,
        "yellow" => 4,
        "purple" => 5,
        "white" => 6,
        _ => return None,
    })
}

fn bar_overlay(name: &str) -> Option<i32> {
    Some(match name {
        "progress" => 0,
        "notched_6" => 1,
        "notched_10" => 2,
        "notched_12" => 3,
        "notched_20" => 4,
        _ => return None,
    })
}

fn bar_progress(bar: &BossBar) -> f32 {
    if bar.max <= 0 {
        0.0
    } else {
        (bar.value as f32 / bar.max as f32).clamp(0.0, 1.0)
    }
}

fn bar_uuid(id: &str) -> Uuid {
    let mut hash = 0xcbf29ce484222325u128;
    for byte in id.as_bytes() {
        hash ^= u128::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    Uuid::from_u128(hash)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn unknown_bar(id: &str) -> Outcome {
    fail("commands.bossbar.unknown", vec![json!({"text": id})])
}

fn missing_watch(id: &str) -> Outcome {
    fail(
        "commands.stopwatch.does_not_exist",
        vec![json!({"text": id})],
    )
}

fn fail(key: &str, with: Vec<Value>) -> Outcome {
    let mut value = translate(key, with);
    if let Some(object) = value.as_object_mut() {
        object.insert("color".into(), Value::String("red".into()));
    }
    Outcome::Feedback(value)
}

fn name_of(target: &Target) -> Value {
    json!({"text": target.name})
}

/// Узлы, сверенные с `commands.json` 26.3.
pub(super) fn specs() -> Vec<super::dispatch::Spec> {
    vec![
        bossbar_spec(),
        stopwatch_spec(),
        posteffect_spec(),
        enchant_spec(),
        spread_spec(),
    ]
}

fn bossbar_spec() -> super::dispatch::Spec {
    use super::dispatch::Spec;
    use super::registry::{GAMEMASTERS, boolean, component_parser, ent, int, resource_location};
    let name = Spec::arg_leaf("name", GAMEMASTERS, component_parser(), bossbar);
    let id_get = Spec::arg(
        "id",
        GAMEMASTERS,
        resource_location(),
        ["max", "players", "value", "visible"]
            .into_iter()
            .map(|field| Spec::literal_leaf(field, GAMEMASTERS, bossbar))
            .collect(),
    );
    let colors = ["blue", "green", "pink", "purple", "red", "white", "yellow"]
        .into_iter()
        .map(|color| Spec::literal_leaf(color, GAMEMASTERS, bossbar))
        .collect();
    let styles = [
        "notched_10",
        "notched_12",
        "notched_20",
        "notched_6",
        "progress",
    ]
    .into_iter()
    .map(|style| Spec::literal_leaf(style, GAMEMASTERS, bossbar))
    .collect();
    Spec::group(
        "bossbar",
        GAMEMASTERS,
        vec![
            Spec::group(
                "add",
                GAMEMASTERS,
                vec![Spec::arg(
                    "id",
                    GAMEMASTERS,
                    resource_location(),
                    vec![name],
                )],
            ),
            Spec::group("get", GAMEMASTERS, vec![id_get]),
            Spec::literal_leaf("list", GAMEMASTERS, bossbar),
            Spec::group(
                "remove",
                GAMEMASTERS,
                vec![Spec::arg_leaf(
                    "id",
                    GAMEMASTERS,
                    resource_location(),
                    bossbar,
                )],
            ),
            Spec::group(
                "set",
                GAMEMASTERS,
                vec![Spec::arg(
                    "id",
                    GAMEMASTERS,
                    resource_location(),
                    vec![
                        Spec::group("color", GAMEMASTERS, colors),
                        Spec::group(
                            "max",
                            GAMEMASTERS,
                            vec![Spec::arg_leaf(
                                "max",
                                GAMEMASTERS,
                                int(1, i32::MAX),
                                bossbar,
                            )],
                        ),
                        Spec::group(
                            "name",
                            GAMEMASTERS,
                            vec![Spec::arg_leaf(
                                "name",
                                GAMEMASTERS,
                                component_parser(),
                                bossbar,
                            )],
                        ),
                        Spec::exec(
                            "players",
                            GAMEMASTERS,
                            bossbar,
                            vec![Spec::arg_leaf(
                                "targets",
                                GAMEMASTERS,
                                ent(true, false),
                                bossbar,
                            )],
                        ),
                        Spec::group("style", GAMEMASTERS, styles),
                        Spec::group(
                            "value",
                            GAMEMASTERS,
                            vec![Spec::arg_leaf(
                                "value",
                                GAMEMASTERS,
                                int(0, i32::MAX),
                                bossbar,
                            )],
                        ),
                        Spec::group(
                            "visible",
                            GAMEMASTERS,
                            vec![Spec::arg_leaf("visible", GAMEMASTERS, boolean(), bossbar)],
                        ),
                    ],
                )],
            ),
        ],
    )
    .with_handler(bossbar)
}

fn stopwatch_spec() -> super::dispatch::Spec {
    use super::dispatch::Spec;
    use super::registry::{GAMEMASTERS, double, resource_location};
    let ident = || Spec::arg_leaf("id", GAMEMASTERS, resource_location(), stopwatch);
    Spec::group(
        "stopwatch",
        GAMEMASTERS,
        vec![
            Spec::group("create", GAMEMASTERS, vec![ident()]),
            Spec::group(
                "query",
                GAMEMASTERS,
                vec![
                    Spec::arg_leaf("id", GAMEMASTERS, resource_location(), stopwatch)
                        .with_children(vec![Spec::arg_leaf(
                            "scale",
                            GAMEMASTERS,
                            double(f64::MIN, f64::MAX),
                            stopwatch,
                        )]),
                ],
            ),
            Spec::group("remove", GAMEMASTERS, vec![ident()]),
            Spec::group("restart", GAMEMASTERS, vec![ident()]),
        ],
    )
    .with_handler(stopwatch)
}

fn posteffect_spec() -> super::dispatch::Spec {
    use super::dispatch::Spec;
    use super::registry::{GAMEMASTERS, ent, resource_location};
    let effect = || Spec::arg_leaf("posteffect", GAMEMASTERS, resource_location(), posteffect);
    Spec::group(
        "posteffect",
        GAMEMASTERS,
        vec![
            Spec::group(
                "add",
                GAMEMASTERS,
                vec![Spec::arg(
                    "targets",
                    GAMEMASTERS,
                    ent(true, false),
                    vec![effect()],
                )],
            ),
            Spec::group(
                "clear",
                GAMEMASTERS,
                vec![Spec::arg_leaf(
                    "targets",
                    GAMEMASTERS,
                    ent(true, false),
                    posteffect,
                )],
            ),
            Spec::group(
                "list",
                GAMEMASTERS,
                vec![Spec::arg_leaf(
                    "target",
                    GAMEMASTERS,
                    ent(true, true),
                    posteffect,
                )],
            ),
            Spec::group(
                "remove",
                GAMEMASTERS,
                vec![Spec::arg(
                    "targets",
                    GAMEMASTERS,
                    ent(true, false),
                    vec![effect()],
                )],
            ),
        ],
    )
    .with_handler(posteffect)
}

fn enchant_spec() -> super::dispatch::Spec {
    use super::dispatch::Spec;
    use super::registry::{GAMEMASTERS, ent, int};
    Spec::group(
        "enchant",
        GAMEMASTERS,
        vec![Spec::arg(
            "targets",
            GAMEMASTERS,
            ent(false, false),
            vec![
                Spec::arg_leaf(
                    "enchantment",
                    GAMEMASTERS,
                    chunkwake_protocol::text::Parser::Resource("minecraft:enchantment"),
                    enchant,
                )
                .with_children(vec![Spec::arg_leaf(
                    "level",
                    GAMEMASTERS,
                    int(0, i32::MAX),
                    enchant,
                )]),
            ],
        )],
    )
    .with_handler(enchant)
}

fn spread_spec() -> super::dispatch::Spec {
    use super::dispatch::Spec;
    use super::registry::{GAMEMASTERS, boolean, ent, float, int};
    let teams = |handler| {
        Spec::arg(
            "respectTeams",
            GAMEMASTERS,
            boolean(),
            vec![Spec::arg_leaf(
                "targets",
                GAMEMASTERS,
                ent(false, false),
                handler,
            )],
        )
    };
    Spec::group(
        "spreadplayers",
        GAMEMASTERS,
        vec![Spec::arg(
            "center",
            GAMEMASTERS,
            chunkwake_protocol::text::Parser::Vec2,
            vec![Spec::arg(
                "spreadDistance",
                GAMEMASTERS,
                float(0.0, f32::MAX as f64),
                vec![Spec::arg(
                    "maxRange",
                    GAMEMASTERS,
                    float(1.0, f32::MAX as f64),
                    vec![
                        teams(spreadplayers),
                        Spec::group(
                            "under",
                            GAMEMASTERS,
                            vec![Spec::arg(
                                "maxHeight",
                                GAMEMASTERS,
                                int(i32::MIN, i32::MAX),
                                vec![teams(spreadplayers)],
                            )],
                        ),
                    ],
                )],
            )],
        )],
    )
    .with_handler(spreadplayers)
}
