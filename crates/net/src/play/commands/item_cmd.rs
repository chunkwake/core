//! `/item replace entity <targets> <slots> with <item> [count]`.
//!
//! Слот игрока берётся из зеркала сессии, новая вещь уходит тем же
//! `Control::Slot`, что и `/give`. У моба инвентаря нет, и он отвечает отказом.
//! Остальные ветки ванили (`block`, `from`, `modify`, `fill`, `override`) в дереве
//! не объявлены: у них нет ни источника, ни приёмника.

use super::Context;
use super::dispatch::{Call, Outcome, translate};
use super::entity::usage;
use super::item::parse_item;
use crate::live::Control;
use crate::survival::OFFHAND;
use chunkwake_protocol::inventory::Stack;
use serde_json::{Value, json};

/// Слоты окна игрока для имени из `slot_source`.
///
/// `Err` — понятная причина, почему имя не подходит.
fn window_slots(selected: usize, slot: &str) -> Result<Vec<usize>, String> {
    let hotbar = selected.min(8);
    let one = |index: usize| Ok(vec![index]);
    match slot {
        "weapon" | "weapon.mainhand" => one(36 + hotbar),
        "weapon.offhand" => one(OFFHAND),
        "weapon.*" => Ok(vec![36 + hotbar, OFFHAND]),
        "armor.head" => one(5),
        "armor.chest" => one(6),
        "armor.legs" => one(7),
        "armor.feet" => one(8),
        "armor.*" => Ok(vec![5, 6, 7, 8]),
        "contents" => Err("slot \"contents\" covers the whole inventory and is not supported yet; name one slot such as hotbar.0".into()),
        _ => {
            let Some((name, index)) = slot.split_once('.') else {
                return Err(format!(
                    "unknown slot \"{slot}\"; use weapon, weapon.mainhand, weapon.offhand, armor.head, armor.chest, armor.legs, armor.feet, hotbar.N, inventory.N or container.N"
                ));
            };
            let Ok(index) = index.parse::<usize>() else {
                return Err(format!("unknown slot \"{slot}\": {index} is not a number"));
            };
            match name {
                "hotbar" if index < 9 => one(36 + index),
                "inventory" if index < 27 => one(9 + index),
                "container" if index < 41 => one(container_slot(index)),
                "hotbar" => Err(format!(
                    "slot \"{slot}\" is outside the hotbar: use hotbar.0 .. hotbar.8"
                )),
                "inventory" => Err(format!(
                    "slot \"{slot}\" is outside the main inventory: use inventory.0 .. inventory.26"
                )),
                "container" => Err(format!(
                    "slot \"{slot}\" is outside the player inventory: use container.0 .. container.40"
                )),
                _ => Err(format!(
                    "unknown slot \"{slot}\"; this server stores the player inventory only"
                )),
            }
        }
    }
}

/// Слот `container.N` (`PlayerInventory`): хотбар, основной, броня, вторая рука.
fn container_slot(index: usize) -> usize {
    match index {
        0..=8 => 36 + index,
        9..=35 => index,
        36..=39 => 39 - index,
        40 => OFFHAND,
        _ => 36,
    }
}

/// `/item …`
pub(super) fn item(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let path: Vec<&str> = call.path.iter().map(String::as_str).collect();
    match path.as_slice() {
        ["replace", "entity"] | ["replace", "entity", ..] => replace(ctx, call),
        ["replace", "block", ..] => Outcome::Feedback(json!({
            "text": "item replace block is not supported yet: blocks here keep no container slots",
            "color": "red",
        })),
        _ => usage("/item replace entity <targets> <slots> with <item> [count]"),
    }
}

fn replace(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(targets_token) = input.word() else {
        return usage("/item replace entity <targets> <slots> with <item> [count]");
    };
    let targets = match ctx.select(call.source, targets_token, true) {
        Ok(list) => list,
        Err(error) => return Outcome::Feedback(error.component()),
    };
    let Some(slot_token) = input.word() else {
        return usage("/item replace entity <targets> <slots> with <item> [count]");
    };
    let Some(mode) = input.word() else {
        return usage("/item replace entity <targets> <slots> with <item> [count]");
    };
    if mode != "with" {
        return Outcome::Feedback(json!({
            "text": format!(
                "item replace entity ... {mode} is not supported yet; this server supports \"with <item> [count]\""
            ),
            "color": "red",
        }));
    }
    let Some(stack_token) = input.balanced() else {
        return usage("/item replace entity <targets> <slots> with <item> [count]");
    };
    let Some(parsed) = parse_item(stack_token, &ctx.plan.survival.item_ids) else {
        return failed(
            "argument.item.id.invalid",
            vec![json!({"text": stack_token})],
        );
    };
    let count = match input.word() {
        Some(token) => match token.parse::<i32>() {
            Ok(count) if (1..=99).contains(&count) => count,
            _ => {
                return failed(
                    "argument.integer.range",
                    vec![json!(1), json!(99), json!({"text": token})],
                );
            }
        },
        None => 1,
    };
    let stack = Stack {
        item: parsed.id,
        count,
        damage: parsed.damage.unwrap_or(0),
        extra: parsed.extra,
    };
    let shown = item_name(ctx, parsed.id);
    let mut changed_targets = 0i32;
    let mut changed_slots = 0i32;
    let mut last_error = None;
    for target in &targets {
        if target.game_mode.is_none() {
            // У моба нет инвентаря игрока: слотов, которые просит команда, у него нет.
            last_error = Some(failed(
                "commands.item.target.no_such_slot",
                vec![json!({"text": slot_token})],
            ));
            continue;
        }
        let selected = ctx
            .plan
            .live
            .lock()
            .unwrap()
            .get(&target.uuid)
            .map_or(0, |live| live.selected);
        let slots = match window_slots(selected, slot_token) {
            Ok(slots) => slots,
            Err(message) => return Outcome::Feedback(json!({"text": message, "color": "red"})),
        };
        for window in &slots {
            ctx.plan
                .push_control(target.uuid, Control::Slot(*window, Some(stack)));
            changed_slots += 1;
        }
        changed_targets += 1;
    }
    if changed_targets == 0 {
        return last_error.unwrap_or_else(|| {
            failed(
                "commands.item.target.failed.known_item",
                vec![shown.clone()],
            )
        });
    }
    if targets.len() == 1 {
        Outcome::Feedback(translate(
            "commands.item.entity.replace.success.single.known_item",
            vec![
                json!(changed_slots),
                json!({"text": targets[0].name}),
                shown,
            ],
        ))
    } else {
        Outcome::Feedback(translate(
            "commands.item.entity.replace.success.multiple.known_item",
            vec![json!(changed_targets), shown],
        ))
    }
}

fn item_name(ctx: &Context<'_>, id: i32) -> Value {
    let name = ctx.plan.item_name_of(id);
    let path = name.strip_prefix("minecraft:").unwrap_or(name);
    json!({"translate": format!("item.minecraft.{path}")})
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
    use super::registry::{GAMEMASTERS, ent, int, item_stack};
    vec![
        Spec::group(
            "item",
            GAMEMASTERS,
            vec![Spec::group(
                "replace",
                GAMEMASTERS,
                vec![
                    Spec::group("block", GAMEMASTERS, Vec::new()),
                    Spec::group(
                        "entity",
                        GAMEMASTERS,
                        vec![Spec::arg(
                            "target",
                            GAMEMASTERS,
                            ent(false, false),
                            vec![Spec::arg(
                                "slots",
                                GAMEMASTERS,
                                chunkwake_protocol::text::Parser::Raw("minecraft:slot_source"),
                                vec![Spec::group(
                                    "with",
                                    GAMEMASTERS,
                                    vec![
                                        Spec::arg_leaf("item", GAMEMASTERS, item_stack(), item)
                                            .with_children(vec![Spec::arg_leaf(
                                                "count",
                                                GAMEMASTERS,
                                                int(1, 99),
                                                item,
                                            )]),
                                    ],
                                )],
                            )],
                        )],
                    ),
                ],
            )],
        )
        .with_handler(item),
    ]
}
