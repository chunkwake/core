//! `/loot give players <targets> loot <table>` и `/loot spawn <pos> loot <table>`.
//!
//! Ролл идёт по настоящим ванильным таблицам (`crate::loot::roll`).
//! Контейнерных форм (`insert`, `replace block`) и `kill`/`mine`/`fish`
//! здесь нет: им нужны слоты контейнера или убийство с источником урона.

use super::Context;
use super::args::{self, Basis};
use super::dispatch::{Call, Outcome, Source, translate};
use super::entity::usage;
use crate::loot::Ctx;
use crate::play::PlayPlan;
use chunkwake_protocol::inventory::Stack;
use serde_json::{Value, json};

impl PlayPlan {
    /// Максимум в стопке для предмета (`item_stack` компонента).
    pub(crate) fn stack_limit(&self, item: i32) -> i32 {
        self.max_stack(item).max(1)
    }
}

/// `/loot …`: разбор пути и хвоста.
pub(super) fn loot(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let path: Vec<&str> = call.path.iter().map(String::as_str).collect();
    let mut input = call.t();
    match path.first().copied() {
        Some("give") => give(ctx, call.source, &path, &mut input),
        Some("spawn") => spawn(ctx, call.source, &mut input),
        Some("insert") => insert(ctx, call.source, &mut input),
        Some("replace") => replace_block(ctx, call.source, &mut input),
        _ => usage("/loot give players <targets> loot <table>"),
    }
}

/// Куда положить лут из `/loot insert` и `/loot replace block`.
///
/// Следующее слово после таблицы — `fish`/`kill`/`loot`/`mine`. Поддержаны
/// только `loot`: у остальных нужен источник урона или удочка.
fn container_source(
    ctx: &Context<'_>,
    source: &Source,
    input: &mut args::Input<'_>,
    usage_text: &str,
) -> Result<Vec<Stack>, Outcome> {
    let Some(word) = input.word() else {
        return Err(usage(usage_text));
    };
    match word {
        "loot" => {}
        "kill" | "fish" | "mine" => {
            return Err(Outcome::Feedback(json!({
                "text": format!(
                    "loot … {word} is not supported yet: this server rolls loot tables, not kills, \
                     fishing or mining; use \"… loot <table>\""
                ),
                "color": "red",
            })));
        }
        other => {
            return Err(Outcome::Feedback(json!({
                "text": format!(
                    "expected the word \"loot\" before the table name, got \"{other}\""
                ),
                "color": "red",
            })));
        }
    }
    let Some(table) = input.word() else {
        return Err(usage(usage_text));
    };
    roll_table(ctx, source, table)
}

/// `/loot insert <pos> loot <table>`: положить лут в контейнер.
fn insert(ctx: &Context<'_>, source: &Source, input: &mut args::Input<'_>) -> Outcome {
    const USAGE: &str = "/loot insert <pos> loot <table>";
    let Some(pos) = read_block_pos(ctx, source, input, USAGE) else {
        return usage(USAGE);
    };
    let rolled = match container_source(ctx, source, input, USAGE) {
        Ok(list) => list,
        Err(outcome) => return outcome,
    };
    if ctx.plan.container_slots(pos).is_none() {
        return failed(
            "commands.loot.replace.block.notAContainer",
            vec![json!(pos[0]), json!(pos[1]), json!(pos[2])],
        );
    }
    if rolled.is_empty() {
        return failed("commands.loot.failed", Vec::new());
    }
    let mut placed = 0;
    for stack in rolled {
        if ctx.plan.container_add(pos, stack) {
            placed += 1;
        }
    }
    if placed == 0 {
        return failed("commands.loot.failed", Vec::new());
    }
    Outcome::Feedback(translate(
        "commands.loot.insert.success",
        vec![json!(placed), json!(pos[0]), json!(pos[1]), json!(pos[2])],
    ))
}

/// `/loot replace block <pos> <slot> [count] loot <table>`.
fn replace_block(ctx: &Context<'_>, source: &Source, input: &mut args::Input<'_>) -> Outcome {
    const USAGE: &str = "/loot replace block <pos> <slot> [count] loot <table>";
    let Some(pos) = read_block_pos(ctx, source, input, USAGE) else {
        return usage(USAGE);
    };
    let Some(slot_token) = input.word() else {
        return usage(USAGE);
    };
    let Ok(slot) = slot_token.parse::<usize>() else {
        return Outcome::Feedback(json!({
            "text": format!("slot must be a number from 0, got \"{slot_token}\""),
            "color": "red",
        }));
    };
    let count = match input.peek_word() {
        Some(word) if word.parse::<usize>().is_ok() => {
            input.word();
            word.parse::<usize>().ok()
        }
        _ => None,
    };
    let rolled = match container_source(ctx, source, input, USAGE) {
        Ok(list) => list,
        Err(outcome) => return outcome,
    };
    let mut rolled = rolled;
    let Some(mut slots) = ctx.plan.container_slots(pos) else {
        return failed(
            "commands.loot.replace.block.notAContainer",
            vec![json!(pos[0]), json!(pos[1]), json!(pos[2])],
        );
    };
    if slot >= slots.len() {
        return failed("commands.loot.replace.block.noSuchSlot", vec![json!(slot)]);
    }
    if rolled.is_empty() {
        return failed("commands.loot.failed", Vec::new());
    }
    let limit = count.unwrap_or(1);
    let mut written = 0;
    for stack in rolled.drain(..).take(limit) {
        let index = slot + written;
        if index >= slots.len() {
            break;
        }
        slots[index] = Some(stack);
        written += 1;
    }
    if written == 0 {
        return failed("commands.loot.failed", Vec::new());
    }
    ctx.plan.set_container_slots(pos, &slots);
    Outcome::Feedback(translate(
        "commands.loot.replace.success",
        vec![json!(written), json!(pos[0]), json!(pos[1]), json!(pos[2])],
    ))
}

/// Позиция блока из трёх слов (`~` считается от исполнителя).
fn read_block_pos(
    ctx: &Context<'_>,
    source: &Source,
    input: &mut args::Input<'_>,
    _usage: &str,
) -> Option<[i32; 3]> {
    let (x, y, z) = (input.word()?, input.word()?, input.word()?);
    let coords = args::coords3(x, y, z)?;
    Some(args::block_pos(
        coords,
        call_base(ctx, source),
        basis_of(ctx, source),
    ))
}

fn give(ctx: &Context<'_>, source: &Source, path: &[&str], input: &mut args::Input<'_>) -> Outcome {
    // Путь — только `give`: `players` это аргумент, хвост начинается с целей.
    let _ = path;
    let Some(targets_token) = input.word() else {
        return usage("/loot give players <targets> loot <table>");
    };
    let targets = match ctx.select(source, targets_token, false) {
        Ok(list) => list,
        Err(error) => return Outcome::Feedback(error.component()),
    };
    let targets: Vec<_> = targets
        .into_iter()
        .filter(|target| target.game_mode.is_some())
        .collect();
    if targets.is_empty() {
        return failed("argument.entity.notfound.player", Vec::new());
    }
    let Some(word) = input.word() else {
        return usage("/loot give players <targets> loot <table>");
    };
    match word {
        "loot" => {}
        "kill" | "fish" | "mine" => {
            return Outcome::Feedback(json!({
                "text": format!(
                    "loot give … {word} is not supported yet: this server rolls loot tables only, \
                     use \"loot give players <targets> loot <table>\""
                ),
                "color": "red",
            }));
        }
        other => {
            return Outcome::Feedback(json!({
                "text": format!(
                    "expected the word \"loot\" before the table name, got \"{other}\""
                ),
                "color": "red",
            }));
        }
    }
    let Some(table) = input.word() else {
        return usage("/loot give players <targets> loot <table>");
    };
    let rolled = match roll_table(ctx, source, table) {
        Ok(list) => list,
        Err(outcome) => return outcome,
    };
    if rolled.is_empty() {
        return failed("commands.loot.failed", Vec::new());
    }
    let mut items = 0i32;
    for target in &targets {
        for stack in &rolled {
            if ctx.plan.give_item(
                target.uuid,
                stack.item,
                stack.count,
                stack.extra,
                stack.damage,
            ) {
                items += 1;
            }
        }
    }
    if targets.len() == 1 {
        Outcome::Feedback(translate(
            "commands.loot.give.single",
            vec![json!(items), json!({"text": targets[0].name})],
        ))
    } else {
        Outcome::Feedback(translate(
            "commands.loot.give.multiple",
            vec![json!(items), json!(targets.len() as i32)],
        ))
    }
}

fn spawn(ctx: &Context<'_>, source: &Source, input: &mut args::Input<'_>) -> Outcome {
    let Some(x) = input.word() else {
        return usage("/loot spawn <pos> loot <table>");
    };
    let Some(y) = input.word() else {
        return usage("/loot spawn <pos> loot <table>");
    };
    let Some(z) = input.word() else {
        return usage("/loot spawn <pos> loot <table>");
    };
    let Some(coords) = args::coords3(x, y, z) else {
        return Outcome::Feedback(json!({
            "text": format!("cannot read the position \"{x} {y} {z}\": use numbers or ~"),
            "color": "red",
        }));
    };
    let at = basis_of(ctx, source).resolve(coords, call_base(ctx, source));
    let Some(word) = input.word() else {
        return usage("/loot spawn <pos> loot <table>");
    };
    if word != "loot" {
        return Outcome::Feedback(json!({
            "text": format!("expected \"loot\" after the position, got \"{word}\""),
            "color": "red",
        }));
    }
    let Some(table) = input.word() else {
        return usage("/loot spawn <pos> loot <table>");
    };
    let rolled = match roll_table(ctx, source, table) {
        Ok(list) => list,
        Err(outcome) => return outcome,
    };
    if rolled.is_empty() {
        return failed("commands.loot.failed", Vec::new());
    }
    for stack in rolled {
        ctx.plan.drop_in_world(at, stack);
    }
    Outcome::Feedback(translate(
        "commands.loot.spawn",
        vec![
            json!({"text": args::resource_name(table)}),
            json!({"text": format!(
                "{} {} {}",
                at[0].floor() as i32,
                at[1].floor() as i32,
                at[2].floor() as i32
            )}),
        ],
    ))
}

/// Один ролл таблицы. `Err` — понятный отказ вместо пустого результата.
fn roll_table(ctx: &Context<'_>, source: &Source, table: &str) -> Result<Vec<Stack>, Outcome> {
    let name = args::resource_name(table);
    if ctx.plan.survival.loot.table(&name).is_none() {
        return Err(failed(
            "argument.resource.not_found",
            vec![
                Value::String(name),
                Value::String("minecraft:loot_table".into()),
            ],
        ));
    }
    let prop = |_: &str| None;
    let level = |_: &str| 0;
    let smelt = |_: &str| None;
    let loot_ctx = Ctx {
        prop: &prop,
        block: &name,
        tool: None,
        level: &level,
        entity: None,
        killed_by_player: true,
        smelt: &smelt,
    };
    let mut rng = rand::thread_rng();
    let out = crate::loot::roll(&ctx.plan.survival.loot, &loot_ctx, &mut rng);
    let _ = source;
    Ok(out
        .into_iter()
        .filter_map(|(item, count)| {
            let id = ctx.plan.item_id_named(&item)?;
            Some(Stack::new(id, count.min(ctx.plan.stack_limit(id))))
        })
        .collect())
}

fn basis_of(ctx: &Context<'_>, source: &Source) -> Basis {
    let (_, yaw, pitch) = pose(ctx, source);
    Basis::from_rotation(yaw, pitch)
}

fn call_base(ctx: &Context<'_>, source: &Source) -> [f64; 3] {
    pose(ctx, source).0
}

fn pose(ctx: &Context<'_>, source: &Source) -> ([f64; 3], f32, f32) {
    if let Some(origin) = ctx.execution_origin() {
        return origin;
    }
    if let Some(uuid) = source.uuid
        && let Some(pose) = ctx.pose(uuid)
    {
        return pose;
    }
    (ctx.spawn(), 0.0, 0.0)
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
    use super::registry::{GAMEMASTERS, block_pos, ent, int, loot_table, vec3};
    let targets = Spec::arg(
        "players",
        GAMEMASTERS,
        ent(true, false),
        vec![Spec::group(
            "loot",
            GAMEMASTERS,
            vec![Spec::arg_leaf(
                "loot_table",
                GAMEMASTERS,
                loot_table(),
                loot,
            )],
        )],
    );
    let spawn = Spec::arg(
        "targetPos",
        GAMEMASTERS,
        vec3(),
        vec![Spec::group(
            "loot",
            GAMEMASTERS,
            vec![Spec::arg_leaf(
                "loot_table",
                GAMEMASTERS,
                loot_table(),
                loot,
            )],
        )],
    );
    let loot_branch = |children: Vec<Spec>| {
        Spec::group(
            "loot",
            GAMEMASTERS,
            vec![Spec::arg_leaf(
                "loot_table",
                GAMEMASTERS,
                loot_table(),
                loot,
            )],
        )
        .with_children(children)
    };
    let insert = Spec::arg(
        "targetPos",
        GAMEMASTERS,
        block_pos(),
        vec![loot_branch(Vec::new())],
    );
    let slot = Spec::arg(
        "slot",
        GAMEMASTERS,
        chunkwake_protocol::text::Parser::ItemSlot,
        vec![
            Spec::arg(
                "count",
                GAMEMASTERS,
                int(1, 99),
                vec![loot_branch(Vec::new())],
            ),
            loot_branch(Vec::new()),
        ],
    );
    let replace_block = Spec::arg("targetPos", GAMEMASTERS, block_pos(), vec![slot]);
    vec![
        Spec::group(
            "loot",
            GAMEMASTERS,
            vec![
                Spec::group("give", GAMEMASTERS, vec![targets]),
                Spec::group("insert", GAMEMASTERS, vec![insert]),
                Spec::group(
                    "replace",
                    GAMEMASTERS,
                    vec![Spec::group("block", GAMEMASTERS, vec![replace_block])],
                ),
                Spec::group("spawn", GAMEMASTERS, vec![spawn]),
            ],
        )
        .with_handler(loot),
    ]
}
