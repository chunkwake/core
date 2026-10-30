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
        Some("insert") | Some("replace") => Outcome::Feedback(json!({
            "text": "loot into a container is not supported yet: containers here keep no slots; \
                     use \"loot give players <targets> loot <table>\" or \"loot spawn <pos> loot <table>\"",
            "color": "red",
        })),
        _ => usage("/loot give players <targets> loot <table>"),
    }
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
    if word != "loot" {
        return Outcome::Feedback(json!({
            "text": format!("expected the word \"loot\" before the table name, got \"{word}\""),
            "color": "red",
        }));
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
    use super::registry::{GAMEMASTERS, ent, loot_table, vec3};
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
    vec![
        Spec::group(
            "loot",
            GAMEMASTERS,
            vec![
                Spec::group("give", GAMEMASTERS, vec![targets]),
                Spec::group("spawn", GAMEMASTERS, vec![spawn]),
            ],
        )
        .with_handler(loot),
    ]
}
