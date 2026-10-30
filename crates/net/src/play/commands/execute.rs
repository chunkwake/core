//! `/execute`: цепочка контекстов и запуск вложенной команды.
//!
//! Ванила строит контекст (`as`, `at`, `positioned`, `rotated`, `facing`, `in`)
//! и проверяет условия (`if`, `unless`), а затем выполняет хвост через `run`.
//! Здесь тот же порядок: контекст — это источник плюс позиция и поворот,
//! поэтому `/execute as @a at @s run say hi` говорит от каждого игрока из его
//! точки.
//!
//! Строка разбирается во владеемой форме: `as`/`at` раскрывают несколько
//! целей, и каждая ветвь должна получить свой хвост без заимствований.

use super::args;
use super::dispatch::{error_component, translate, Call, Outcome, Source};
use super::selector::Target;
use super::Context;
use serde_json::json;

/// Контекст выполнения: от чьего имени, где и куда смотрим.
#[derive(Clone, Debug)]
pub struct Exec {
    pub source: Source,
    pub pos: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
}

impl Exec {
    fn of(call: &Call<'_>, ctx: &Context<'_>) -> Self {
        let (pos, yaw, pitch) = call
            .source
            .uuid
            .and_then(|uuid| ctx.pose(uuid))
            .unwrap_or((call.position(ctx), 0.0, 0.0));
        Self {
            source: call.source.clone(),
            pos,
            yaw,
            pitch,
        }
    }
}

/// Предел раскрытия цепочек: защита от `/execute as @e[type=...] as @e …`,
/// которая иначе размножает работу экспоненциально.
const MAX_FORKS: usize = 256;

/// Узлы дерева `/execute`.
///
/// Ванильное дерево разбирает `execute` по подкомандам, и каждая ветка
/// возвращает разбор в корень `execute` (`redirect`). Обработчик при этом
/// получает строку целиком и разбирает цепочку сам — дерево нужно клиенту,
/// чтобы подсвечивать `as`/`at`/`run` и подсказывать аргументы.
pub(super) fn specs() -> Vec<super::dispatch::Spec> {
    use super::dispatch::Spec;
    use super::registry::{
        anchor, block_predicate, ent, float, int, rotation, vec3, GAMEMASTERS,
    };
    const EXECUTE: &str = "execute";

    // `run` — литерал без детей: дальше идёт произвольная команда.
    let run = Spec::literal_leaf("run", GAMEMASTERS, execute);
    let entities = || ent(false, false);
    let branch = |name: &str, parser, children: Vec<Spec>| {
        Spec::arg_redirect(name, GAMEMASTERS, parser, EXECUTE, children)
    };
    let literal_branch = |name: &str, children: Vec<Spec>| {
        Spec::group(name, GAMEMASTERS, children)
    };
    let condition = || {
        vec![
            literal_branch(
                "entity",
                vec![branch("entities", entities(), Vec::new())],
            ),
            literal_branch(
                "block",
                vec![
                    Spec::arg_redirect(
                        "pos",
                        GAMEMASTERS,
                        super::registry::block_pos(),
                        EXECUTE,
                        vec![branch("block", block_predicate(), Vec::new())],
                    ),
                ],
            ),
            literal_branch(
                "score",
                vec![
                    Spec::arg_redirect(
                        "target",
                        GAMEMASTERS,
                        super::registry::score_holder(true),
                        EXECUTE,
                        vec![
                            Spec::arg_redirect(
                                "targetObjective",
                                GAMEMASTERS,
                                super::registry::objective(),
                                EXECUTE,
                                ["<", "<=", "=", ">=", ">"]
                                    .into_iter()
                                    .map(|op| {
                                        Spec::group(
                                            op,
                                            GAMEMASTERS,
                                            vec![Spec::arg_redirect(
                                                "source",
                                                GAMEMASTERS,
                                                super::registry::score_holder(true),
                                                EXECUTE,
                                                vec![branch(
                                                    "sourceObjective",
                                                    super::registry::objective(),
                                                    Vec::new(),
                                                )],
                                            )],
                                        )
                                    })
                                    .chain(std::iter::once(Spec::group(
                                        "matches",
                                        GAMEMASTERS,
                                        vec![branch(
                                            "range",
                                            super::registry::int_range(),
                                            Vec::new(),
                                        )],
                                    )))
                                    .collect(),
                            ),
                        ],
                    ),
                ],
            ),
        ]
    };
    let mut children = vec![
        literal_branch("as", vec![branch("targets", entities(), Vec::new())]),
        literal_branch("at", vec![branch("targets", entities(), Vec::new())]),
        literal_branch(
            "facing",
            vec![
                literal_branch(
                    "entity",
                    vec![Spec::arg_redirect(
                        "targets",
                        GAMEMASTERS,
                        entities(),
                        EXECUTE,
                        vec![branch("anchor", anchor(), Vec::new())],
                    )],
                ),
                branch("pos", vec3(), Vec::new()),
            ],
        ),
        literal_branch("if", condition()),
        literal_branch("unless", condition()),
        literal_branch("in", vec![branch("dimension", super::registry::dimension(), Vec::new())]),
        literal_branch(
            "positioned",
            vec![
                literal_branch("as", vec![branch("targets", entities(), Vec::new())]),
                literal_branch(
                    "over",
                    vec![branch("heightmap", super::registry::heightmap(), Vec::new())],
                ),
                branch("pos", vec3(), Vec::new()),
            ],
        ),
        literal_branch(
            "rotated",
            vec![
                literal_branch("as", vec![branch("targets", entities(), Vec::new())]),
                branch("rot", rotation(), Vec::new()),
            ],
        ),
        run,
    ];
    // `align`, `anchored`, `on`, `store` обработчик пока не разбирает, поэтому
    // клиенту они не показываются: обещать неработающий узел хуже, чем скрыть.
    children.retain(|child| {
        !matches!(child.name.as_str(), "align" | "anchored" | "on" | "store")
    });
    let _ = (float, int);
    vec![Spec::exec("execute", GAMEMASTERS, execute, children)]
}

/// `/execute … run <command>`.
///
/// Дерево разбирает цепочку по литералам, но `rest` уже содержит строку
/// после имени команды целиком (литералы пути входят и в `path`, и в
/// `rest`), поэтому цепочка берётся только из `rest` — иначе `positioned`
/// попал бы в строку дважды.
pub(super) fn execute(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let exec = Exec::of(call, ctx);
    let mut budget = MAX_FORKS;
    run_chain(ctx, &exec, call.rest.trim(), &mut budget)
}

/// Выполняет цепочку подкоманд до `run`.
fn run_chain(ctx: &Context<'_>, exec: &Exec, rest: &str, budget: &mut usize) -> Outcome {
    let mut exec = exec.clone();
    let mut rest = rest.trim().to_owned();
    loop {
        if rest.is_empty() {
            return Outcome::Feedback(super::dispatch::error_component(
                "/execute <as|at|positioned|rotated|facing|in|if|unless|run> …",
            ));
        }        let (word, tail) = split(&rest);
        match word {
            "run" => {                let line = tail.trim();
                if line.is_empty() {
                    return Outcome::Feedback(error_component("/execute … run <command>"));
                }
                // Вложенная команда видит позицию и поворот контекста: для
                // `~ ~ ~` нужна точка цепочки, а не место исполнителя.
                return match ctx
                    .plan
                    .run_command_at(&exec.source, line, exec.pos, exec.yaw, exec.pitch)
                {
                    Some(outcome) => outcome,
                    None => Outcome::Feedback(translate(
                        "command.unknown.command",
                        vec![json!({"text": line})],
                    )),
                };
            }
            "as" | "at" => {
                let (token, next) = split(tail);
                if token.is_empty() {
                    return Outcome::Feedback(error_component(
                        "/execute as|at <targets> …",
                    ));
                }
                let targets = match ctx.select(&exec.source, token, true) {
                    Ok(list) => list,
                    Err(e) => return Outcome::Feedback(e.component()),
                };
                if targets.is_empty() {
                    return Outcome::Silent;
                }
                let mut outcomes = Vec::new();
                for target in targets {
                    if *budget == 0 {
                        break;
                    }
                    *budget -= 1;
                    let mut branch = exec.clone();
                    if word == "as" {
                        branch.source =
                            Source::player(target.uuid, target.name.clone(), exec.source.level);
                        (branch.yaw, branch.pitch) = (target.yaw, target.pitch);
                    }
                    branch.pos = target.pos;
                    outcomes.push(run_chain(ctx, &branch, next, budget));
                }
                return combine(outcomes);
            }
            "positioned" => {
                let (head, tail2) = split(tail);
                if head == "as" {
                    let (token, next) = split(tail2);
                    let Some(target) = ctx
                        .select(&exec.source, token, true)
                        .ok()
                        .and_then(|list| list.first().cloned())
                    else {
                        return Outcome::Feedback(translate(
                            "argument.entity.notfound.entity",
                            Vec::new(),
                        ));
                    };
                    exec.pos = target.pos;
                    rest = next.to_owned();
                    continue;
                }
                let (y, tail3) = split(tail2);
                let (z, next) = split(tail3);
                let Some(c) = args::coords3(head, y, z) else {
                    return Outcome::Feedback(error_component(
                        "/execute positioned <x> <y> <z> …",
                    ));
                };
                exec.pos = args::Basis::from_rotation(exec.yaw, exec.pitch).resolve(c, exec.pos);
                rest = next.to_owned();
                continue;
            }
            "rotated" => {
                let (yaw, tail2) = split(tail);
                let (pitch, next) = split(tail2);
                let Some((yaw, pitch)) = args::rotation(yaw, pitch, (exec.yaw, exec.pitch)) else {
                    return Outcome::Feedback(error_component(
                        "/execute rotated <yaw> <pitch> …",
                    ));
                };
                (exec.yaw, exec.pitch) = (yaw, pitch);
                rest = next.to_owned();
                continue;
            }
            "facing" => {
                let (head, tail2) = split(tail);
                let (point, next) = if head == "entity" {
                    let (token, rest2) = split(tail2);
                    let Some(target) = ctx
                        .select(&exec.source, token, true)
                        .ok()
                        .and_then(|list| list.first().cloned())
                    else {
                        return Outcome::Feedback(translate(
                            "argument.entity.notfound.entity",
                            Vec::new(),
                        ));
                    };
                    // `facing entity <targets> <feet|eyes>`
                    let (_, rest3) = split(rest2);
                    (target.pos, rest3)
                } else {
                    let (y, tail3) = split(tail2);
                    let (z, rest3) = split(tail3);
                    let Some(c) = args::coords3(head, y, z) else {
                        return Outcome::Feedback(error_component(
                            "/execute facing <x> <y> <z> …",
                        ));
                    };
                    (
                        args::Basis::from_rotation(exec.yaw, exec.pitch).resolve(c, exec.pos),
                        rest3,
                    )
                };
                let (yaw, pitch) = look_at(exec.pos, point);
                (exec.yaw, exec.pitch) = (yaw, pitch);
                rest = next.to_owned();
                continue;
            }
            "in" => {
                // Мир один: `in minecraft:overworld` принимаем и продолжаем.
                let (_, next) = split(tail);
                rest = next.to_owned();
                continue;
            }
            "if" | "unless" => {
                let want = word == "if";
                let (kind, tail2) = split(tail);
                let (ok, next) = match kind {
                    "entity" => {
                        let (token, next) = split(tail2);
                        let ok = !token.is_empty()
                            && ctx.select(&exec.source, token, true).is_ok();
                        (ok, next)
                    }
                    "block" => {
                        let (x, tail3) = split(tail2);
                        let (y, tail4) = split(tail3);
                        let (z, tail5) = split(tail4);
                        let (block, next) = split(tail5);
                        let ok = args::coords3(x, y, z).is_some_and(|c| {
                            let pos =
                                args::Basis::from_rotation(exec.yaw, exec.pitch).resolve(c, exec.pos);
                            let pos = [
                                pos[0].floor() as i32,
                                pos[1].floor() as i32,
                                pos[2].floor() as i32,
                            ];
                            ctx.plan
                                .block_name_at(pos)
                                .is_some_and(|name| name == args::resource_name(block))
                        });
                        (ok, next)
                    }
                    "score" => {
                        let (ok, next) = score_compare(ctx, tail2);
                        (ok, next)
                    }
                    _ => {
                        return Outcome::Feedback(error_component(
                            "/execute if|unless <entity|block|score> …",
                        ));
                    }
                };
                // Условие не выполнено — хвост не запускаем.
                if ok != want {
                    return Outcome::Silent;
                }
                rest = next.to_owned();
                continue;
            }
            other => {
                return Outcome::Feedback(error_component(&format!(
                    "/execute: unknown subcommand {other}"
                )));
            }
        }
    }
}

/// `if score <target> <objective> <op> <source> <objective>`.
fn score_compare<'a>(ctx: &Context<'_>, text: &'a str) -> (bool, &'a str) {
    let mut rest = text;
    let mut words = [""; 6];
    for slot in &mut words {
        let (head, tail) = split(rest);
        *slot = head;
        rest = tail;
    }
    let [left, left_obj, op, right, right_obj, _] = words;
    if left.is_empty() || op.is_empty() {
        return (false, rest);
    }
    let left = ctx
        .plan
        .score(left, left_obj)
        .unwrap_or(0);
    let right = ctx.plan.score(right, right_obj).unwrap_or(0);
    let ok = match op {
        "<" => left < right,
        "<=" => left <= right,
        "=" | "==" => left == right,
        ">=" => left >= right,
        ">" => left > right,
        _ => false,
    };
    (ok, rest)
}

/// Склеивает результаты ветвей: первое сообщение, иначе тишина.
fn combine(outcomes: Vec<Outcome>) -> Outcome {
    let mut feedback = None;
    for outcome in outcomes {
        if let Outcome::Feedback(msg) = outcome
            && feedback.is_none()
        {
            feedback = Some(msg);
        }
    }
    match feedback {
        Some(msg) => Outcome::Feedback(msg),
        None => Outcome::Silent,
    }
}

/// Делит строку на первое слово и остаток.
fn split(text: &str) -> (&str, &str) {
    let text = text.trim_start();
    match text.split_once(char::is_whitespace) {
        Some((head, tail)) => (head, tail.trim_start()),
        None => (text, ""),
    }
}

/// Углы взгляда из точки на цель.
fn look_at(from: [f64; 3], to: [f64; 3]) -> (f32, f32) {
    let (dx, dy, dz) = (to[0] - from[0], to[1] - from[1], to[2] - from[2]);
    let horizontal = (dx * dx + dz * dz).sqrt();
    let yaw = (-dx).atan2(dz).to_degrees() as f32;
    let pitch = (-dy).atan2(horizontal).to_degrees() as f32;
    (yaw, pitch)
}

/// Имена целей одной строкой (для сообщений).
#[allow(dead_code)]
fn names(targets: &[Target]) -> String {
    targets
        .iter()
        .map(|t| t.name.clone())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn look_at_points_towards_target() {
        let (yaw, pitch) = look_at([0.0, 0.0, 0.0], [0.0, 0.0, 10.0]);
        assert!(yaw.abs() < 1e-3, "на юг — yaw 0: {yaw}");
        assert!(pitch.abs() < 1e-3, "на одном уровне — pitch 0: {pitch}");
        let (yaw, _) = look_at([0.0, 0.0, 0.0], [-10.0, 0.0, 0.0]);
        assert!((yaw - 90.0).abs() < 1e-3, "на запад — yaw 90: {yaw}");
    }

    #[test]
    fn split_words() {
        assert_eq!(split("as @a run say hi"), ("as", "@a run say hi"));
        assert_eq!(split("run"), ("run", ""));
        assert_eq!(split("   spaced  "), ("spaced", ""));
    }
}
