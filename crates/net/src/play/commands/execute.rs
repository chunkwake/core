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

use super::Context;
use super::args;
use super::dispatch::{Call, Outcome, Source, error_component, translate};
use super::selector::Target;
use super::snbt;
use chunkwake_world::nbt::Tag;
use serde_json::json;
use uuid::Uuid;

/// Связи `execute on`. Пустые поля — связи нет, команда отвечает «сущность не найдена».
#[derive(Clone, Debug, Default)]
pub struct Links {
    pub vehicle: Option<Uuid>,
    pub passengers: Vec<Uuid>,
    pub owner: Option<Uuid>,
    pub leasher: Option<Uuid>,
    pub attacker: Option<Uuid>,
    pub controller: Option<Uuid>,
    pub origin: Option<Uuid>,
    pub target: Option<Uuid>,
}

/// Контекст выполнения: от чьего имени, где и куда смотрим.
#[derive(Clone, Debug)]
pub struct Exec {
    pub source: Source,
    pub pos: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    /// `anchored eyes`: точка исполнения на 1.62 выше ног.
    pub eyes: bool,
}

impl Exec {
    fn of(call: &Call<'_>, ctx: &Context<'_>) -> Self {
        let (pos, yaw, pitch) = call.source.uuid.and_then(|uuid| ctx.pose(uuid)).unwrap_or((
            call.position(ctx),
            0.0,
            0.0,
        ));
        Self {
            source: call.source.clone(),
            pos,
            yaw,
            pitch,
            eyes: false,
        }
    }

    /// Точка, из которой считаются `~`, `^` и вложенная команда.
    fn view_pos(&self) -> [f64; 3] {
        if self.eyes {
            [self.pos[0], self.pos[1] + 1.62, self.pos[2]]
        } else {
            self.pos
        }
    }
}

/// Предел раскрытия цепочек: защита от `/execute as @e[type=...] as @e …`,
/// которая иначе размножает работу экспоненциально.
const MAX_FORKS: usize = 256;

/// Предел вложенности `/execute run execute run …`. Без него цепочка
/// в 32 КБ команды рекурсивно вызывает диспетчер тысячи раз и переполняет
/// стек процесса (`panic = "abort"`).
const MAX_EXECUTE_DEPTH: usize = 32;

thread_local! {
    static EXECUTE_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

struct ExecuteDepth;

impl ExecuteDepth {
    fn enter() -> Option<Self> {
        EXECUTE_DEPTH.with(|depth| {
            let current = depth.get();
            (current < MAX_EXECUTE_DEPTH).then(|| {
                depth.set(current + 1);
                Self
            })
        })
    }
}

impl Drop for ExecuteDepth {
    fn drop(&mut self) {
        EXECUTE_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

/// Узлы дерева `/execute`.
///
/// Ванильное дерево разбирает `execute` по подкомандам, и каждая ветка
/// возвращает разбор в корень `execute` (`redirect`). Обработчик при этом
/// получает строку целиком и разбирает цепочку сам — дерево нужно клиенту,
/// чтобы подсвечивать `as`/`at`/`run` и подсказывать аргументы.
pub(super) fn specs() -> Vec<super::dispatch::Spec> {
    use super::dispatch::Spec;
    use super::registry::{GAMEMASTERS, anchor, block_predicate, ent, rotation, vec3};
    const EXECUTE: &str = "execute";

    // `run` — литерал без детей: дальше идёт произвольная команда.
    let run = Spec::literal_leaf("run", GAMEMASTERS, execute);
    let entities = || ent(false, false);
    let branch = |name: &str, parser, children: Vec<Spec>| {
        Spec::arg_redirect(name, GAMEMASTERS, parser, EXECUTE, children)
    };
    let literal_branch = |name: &str, children: Vec<Spec>| Spec::group(name, GAMEMASTERS, children);
    let nbt_store = |name: &str, parser| {
        let types = ["byte", "short", "int", "long", "float", "double"]
            .into_iter()
            .map(|kind| {
                Spec::group(
                    kind,
                    GAMEMASTERS,
                    vec![Spec::arg_redirect(
                        "scale",
                        GAMEMASTERS,
                        super::registry::double(-1.0e300, 1.0e300),
                        EXECUTE,
                        Vec::new(),
                    )],
                )
            })
            .collect();
        literal_branch(
            name,
            vec![Spec::arg(
                "target",
                GAMEMASTERS,
                parser,
                vec![Spec::arg(
                    "path",
                    GAMEMASTERS,
                    super::registry::nbt_path(),
                    types,
                )],
            )],
        )
    };
    let store_targets = || {
        vec![
            literal_branch(
                "score",
                vec![Spec::arg_redirect(
                    "name",
                    GAMEMASTERS,
                    super::registry::score_holder(false),
                    EXECUTE,
                    vec![branch(
                        "objective",
                        super::registry::objective(),
                        Vec::new(),
                    )],
                )],
            ),
            nbt_store("block", super::registry::block_pos()),
            nbt_store("entity", entities()),
        ]
    };
    let on_relations = [
        "attacker",
        "controller",
        "leasher",
        "origin",
        "owner",
        "passengers",
        "target",
        "vehicle",
    ]
    .into_iter()
    .map(|name| Spec::group(name, GAMEMASTERS, Vec::new()).redirect_to(EXECUTE))
    .collect();
    let condition = || {
        vec![
            literal_branch("entity", vec![branch("entities", entities(), Vec::new())]),
            literal_branch(
                "block",
                vec![Spec::arg_redirect(
                    "pos",
                    GAMEMASTERS,
                    super::registry::block_pos(),
                    EXECUTE,
                    vec![branch("block", block_predicate(), Vec::new())],
                )],
            ),
            literal_branch(
                "score",
                vec![Spec::arg_redirect(
                    "target",
                    GAMEMASTERS,
                    super::registry::score_holder(true),
                    EXECUTE,
                    vec![Spec::arg_redirect(
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
                                vec![branch("range", super::registry::int_range(), Vec::new())],
                            )))
                            .collect(),
                    )],
                )],
            ),
            literal_branch(
                "biome",
                vec![Spec::arg_redirect(
                    "pos",
                    GAMEMASTERS,
                    super::registry::block_pos(),
                    EXECUTE,
                    vec![branch(
                        "biome",
                        chunkwake_protocol::text::Parser::ResourceOrTag("minecraft:worldgen/biome"),
                        Vec::new(),
                    )],
                )],
            ),
            literal_branch(
                "blocks",
                vec![Spec::arg(
                    "start",
                    GAMEMASTERS,
                    super::registry::block_pos(),
                    vec![Spec::arg(
                        "end",
                        GAMEMASTERS,
                        super::registry::block_pos(),
                        vec![Spec::arg(
                            "destination",
                            GAMEMASTERS,
                            super::registry::block_pos(),
                            vec![
                                Spec::leaf("all", GAMEMASTERS, execute).redirect_to(EXECUTE),
                                Spec::leaf("masked", GAMEMASTERS, execute).redirect_to(EXECUTE),
                            ],
                        )],
                    )],
                )],
            ),
            literal_branch(
                "data",
                vec![
                    literal_branch(
                        "block",
                        vec![Spec::arg(
                            "source",
                            GAMEMASTERS,
                            super::registry::block_pos(),
                            vec![branch("path", super::registry::nbt_path(), Vec::new())],
                        )],
                    ),
                    literal_branch(
                        "entity",
                        vec![Spec::arg(
                            "source",
                            GAMEMASTERS,
                            ent(false, true),
                            vec![branch("path", super::registry::nbt_path(), Vec::new())],
                        )],
                    ),
                    literal_branch(
                        "storage",
                        vec![Spec::arg(
                            "source",
                            GAMEMASTERS,
                            super::registry::resource_location(),
                            vec![branch("path", super::registry::nbt_path(), Vec::new())],
                        )],
                    ),
                ],
            ),
            literal_branch(
                "loaded",
                vec![branch("pos", super::registry::block_pos(), Vec::new())],
            ),
            literal_branch(
                "items",
                vec![
                    literal_branch(
                        "block",
                        vec![Spec::arg(
                            "source",
                            GAMEMASTERS,
                            super::registry::block_pos(),
                            vec![Spec::arg(
                                "slots",
                                GAMEMASTERS,
                                chunkwake_protocol::text::Parser::Raw("minecraft:slot_source"),
                                vec![branch(
                                    "item_predicate",
                                    super::registry::item_predicate(),
                                    Vec::new(),
                                )],
                            )],
                        )],
                    ),
                    literal_branch(
                        "entity",
                        vec![Spec::arg(
                            "source",
                            GAMEMASTERS,
                            entities(),
                            vec![Spec::arg(
                                "slots",
                                GAMEMASTERS,
                                chunkwake_protocol::text::Parser::Raw("minecraft:slot_source"),
                                vec![branch(
                                    "item_predicate",
                                    super::registry::item_predicate(),
                                    Vec::new(),
                                )],
                            )],
                        )],
                    ),
                ],
            ),
            // `slots`: ровно один слот и его стопка, в отличие от `items`.
            literal_branch(
                "slots",
                vec![
                    literal_branch(
                        "block",
                        vec![Spec::arg(
                            "source",
                            GAMEMASTERS,
                            super::registry::block_pos(),
                            vec![Spec::arg(
                                "slot",
                                GAMEMASTERS,
                                chunkwake_protocol::text::Parser::ItemSlot,
                                vec![branch(
                                    "item",
                                    super::registry::item_predicate(),
                                    Vec::new(),
                                )],
                            )],
                        )],
                    ),
                    literal_branch(
                        "entity",
                        vec![Spec::arg(
                            "source",
                            GAMEMASTERS,
                            entities(),
                            vec![Spec::arg(
                                "slot",
                                GAMEMASTERS,
                                chunkwake_protocol::text::Parser::ItemSlot,
                                vec![branch(
                                    "item",
                                    super::registry::item_predicate(),
                                    Vec::new(),
                                )],
                            )],
                        )],
                    ),
                ],
            ),
        ]
    };
    let children = vec![
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
        literal_branch(
            "in",
            vec![branch(
                "dimension",
                super::registry::dimension(),
                Vec::new(),
            )],
        ),
        literal_branch(
            "positioned",
            vec![
                literal_branch("as", vec![branch("targets", entities(), Vec::new())]),
                literal_branch(
                    "over",
                    vec![branch(
                        "heightmap",
                        super::registry::heightmap(),
                        Vec::new(),
                    )],
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
        literal_branch(
            "align",
            vec![branch("axes", super::registry::swizzle(), Vec::new())],
        ),
        literal_branch("anchored", vec![branch("anchor", anchor(), Vec::new())]),
        literal_branch("on", on_relations),
        literal_branch(
            "store",
            vec![
                literal_branch("result", store_targets()),
                literal_branch("success", store_targets()),
            ],
        ),
        literal_branch("summon", vec![branch("entity", entities(), Vec::new())]),
        run,
    ];
    vec![Spec::exec("execute", GAMEMASTERS, execute, children)]
}

/// `/execute … run <command>`.
///
/// Дерево разбирает цепочку по литералам, но `rest` уже содержит строку
/// после имени команды целиком (литералы пути входят и в `path`, и в
/// `rest`), поэтому цепочка берётся только из `rest` — иначе `positioned`
/// попал бы в строку дважды.
/// `BlockPredicate` для `execute if block`: id или `#тег`, плюс только
/// перечисленные свойства (`chest[facing=north]`); остальные не важны.
fn block_matches(ctx: &Context<'_>, state: u32, spec: &str) -> bool {
    let plan = ctx.plan;
    let (head, properties) = match spec.split_once('[') {
        Some((head, tail)) => match tail.strip_suffix(']') {
            Some(tail) => (head.trim(), Some(tail)),
            None => return false,
        },
        None => (spec.trim(), None),
    };
    if let Some(tag) = head.strip_prefix('#') {
        let tag = args::resource_name(tag);
        return plan
            .survival
            .block_tags
            .get(&tag)
            .is_some_and(|set| set.contains(plan.state_name(state)));
    }
    let Some(parsed) = plan.parse_block_state(head) else {
        return false;
    };
    if plan.state_name(parsed) != plan.state_name(state) {
        return false;
    }
    properties
        .into_iter()
        .flat_map(|p| p.split(','))
        .all(|pair| {
            let Some((key, value)) = pair.split_once('=') else {
                return false;
            };
            plan.survival.placement.prop(state, key.trim()) == Some(value.trim())
        })
}

pub(super) fn execute(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let Some(_depth) = ExecuteDepth::enter() else {
        return Outcome::Feedback(error_component("/execute: maximum nesting depth exceeded"));
    };
    let exec = Exec::of(call, ctx);
    let mut budget = MAX_FORKS;
    // `execute in <измерение>` переключает активное измерение; после цепочки
    // его надо вернуть, иначе следующие команды уйдут не туда.
    let before = ctx.plan.dimension_override();
    let outcome = run_chain(ctx, &exec, call.rest.trim(), &mut budget);
    ctx.plan.restore_dimension_override(before);
    outcome
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
        }
        let (word, tail) = split(&rest);
        match word {
            "run" => {
                let line = tail.trim();
                if line.is_empty() {
                    return Outcome::Feedback(error_component("/execute … run <command>"));
                }
                // Вложенная команда видит позицию и поворот контекста: для
                // `~ ~ ~` нужна точка цепочки, а не место исполнителя.
                return match ctx.plan.run_command_at(
                    &exec.source,
                    line,
                    exec.view_pos(),
                    exec.yaw,
                    exec.pitch,
                ) {
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
                    return Outcome::Feedback(error_component("/execute as|at <targets> …"));
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
                if head == "over" {
                    let (map, next) = split(tail2);
                    match ctx.plan.height_over(exec.pos[0], exec.pos[2], map) {
                        super::access::ColumnHeight::Y(y) => {
                            exec.pos[1] = y;
                            exec.eyes = false;
                            rest = next.to_owned();
                            continue;
                        }
                        super::access::ColumnHeight::Unknown => {
                            return Outcome::Feedback(error_component(
                                "/execute positioned over <heightmap> …",
                            ));
                        }
                        super::access::ColumnHeight::Missing => {
                            return Outcome::Feedback(translate(
                                "argument.pos.unloaded",
                                Vec::new(),
                            ));
                        }
                    }
                }
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
                    return Outcome::Feedback(error_component("/execute positioned <x> <y> <z> …"));
                };
                exec.pos =
                    args::Basis::from_rotation(exec.yaw, exec.pitch).resolve(c, exec.view_pos());
                exec.eyes = false;
                rest = next.to_owned();
                continue;
            }
            "align" => {
                let (token, next) = split(tail);
                let Some(axes) = swizzle_axes(token) else {
                    return Outcome::Feedback(error_component("/execute align <axes> …"));
                };
                if axes[0] {
                    exec.pos[0] = exec.pos[0].floor();
                }
                if axes[1] {
                    exec.pos[1] = exec.pos[1].floor();
                }
                if axes[2] {
                    exec.pos[2] = exec.pos[2].floor();
                }
                rest = next.to_owned();
                continue;
            }
            "anchored" => {
                let (anchor, next) = split(tail);
                match anchor {
                    "eyes" => exec.eyes = true,
                    "feet" => exec.eyes = false,
                    _ => {
                        return Outcome::Feedback(error_component(
                            "/execute anchored <eyes|feet> …",
                        ));
                    }
                }
                rest = next.to_owned();
                continue;
            }
            "on" => {
                let (relation, next) = split(tail);
                return follow_relation(ctx, &exec, relation, next, budget);
            }
            "store" => {
                return store_value(ctx, &exec, tail, budget);
            }
            "summon" => {
                let mut budget_now = *budget;
                let outcome = summon_then(ctx, &exec, tail, &mut budget_now);
                *budget = budget_now;
                return outcome;
            }
            "rotated" => {
                let (yaw, tail2) = split(tail);
                let (pitch, next) = split(tail2);
                let Some((yaw, pitch)) = args::rotation(yaw, pitch, (exec.yaw, exec.pitch)) else {
                    return Outcome::Feedback(error_component("/execute rotated <yaw> <pitch> …"));
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
                        return Outcome::Feedback(error_component("/execute facing <x> <y> <z> …"));
                    };
                    (
                        args::Basis::from_rotation(exec.yaw, exec.pitch).resolve(c, exec.pos),
                        rest3,
                    )
                };
                let (yaw, pitch) = look_at(exec.view_pos(), point);
                (exec.yaw, exec.pitch) = (yaw, pitch);
                rest = next.to_owned();
                continue;
            }
            "in" => {
                // `execute in <измерение>`: дальше всё считается в нём. У нас
                // измерений несколько, поэтому переключаем активное.
                let (dim, next) = split(tail);
                if let Some(dimension) = crate::play::Dimension::from_key(dim) {
                    ctx.plan.set_active_dimension(dimension);
                }
                rest = next.to_owned();
                continue;
            }
            "if" | "unless" => {
                let want = word == "if";
                let (kind, tail2) = split(tail);
                let checked = match kind {
                    "entity" => {
                        let (token, next) = split(tail2);
                        let ok = !token.is_empty() && ctx.select(&exec.source, token, true).is_ok();
                        Ok((ok, next))
                    }
                    "block" => {
                        let (x, tail3) = split(tail2);
                        let (y, tail4) = split(tail3);
                        let (z, tail5) = split(tail4);
                        let (block, next) = split(tail5);
                        let ok = args::coords3(x, y, z).is_some_and(|c| {
                            let pos = args::Basis::from_rotation(exec.yaw, exec.pitch)
                                .resolve(c, exec.pos);
                            let pos = [
                                pos[0].floor() as i32,
                                pos[1].floor() as i32,
                                pos[2].floor() as i32,
                            ];
                            // Ваниль принимает block predicate: имя и
                            // свойства (`stone`, `chest[facing=north]`).
                            // `BlockPredicate`: id/тег и только перечисленные
                            // свойства; остальные не важны.
                            ctx.plan
                                .block(pos)
                                .is_some_and(|state| block_matches(ctx, state, block))
                        });
                        Ok((ok, next))
                    }
                    "score" => Ok(score_compare(ctx, tail2)),
                    "biome" => biome_test(ctx, &exec, tail2),
                    "blocks" => blocks_test(ctx, &exec, tail2),
                    "data" => data_test(ctx, &exec, tail2),
                    "loaded" => loaded_test(ctx, &exec, tail2),
                    "items" => items_test(ctx, &exec, tail2),
                    "slots" => slots_test(ctx, &exec, tail2),
                    _ => {
                        return Outcome::Feedback(error_component(
                            "/execute if|unless <entity|block|score|biome|blocks|data|loaded|items|slots> …",
                        ));
                    }
                };
                let (ok, next) = match checked {
                    Ok(pair) => pair,
                    Err(outcome) => return outcome,
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

/// `execute summon <entity> …`: ставит сущность и выполняет хвост от её имени.
fn summon_then(ctx: &Context<'_>, exec: &Exec, tail: &str, budget: &mut usize) -> Outcome {
    let (token, next) = split(tail);
    let name = super::args::resource_name(token);
    let Some(species) = crate::mobs::Species::from_name(&name) else {
        return Outcome::Feedback(translate(
            "argument.entity.id.invalid",
            vec![serde_json::json!(name)],
        ));
    };
    let Some(entity_id) = ctx.plan.summon_mob(species, exec.pos) else {
        return Outcome::Feedback(translate("commands.summon.failed", Vec::new()));
    };
    // Моб уже в снимке: его UUID выводится из `entity_id`.
    let Some(target) = ctx.plan.command_target(entity_uuid(entity_id)) else {
        return Outcome::Feedback(translate("commands.summon.failed", Vec::new()));
    };
    let mut branch = exec.clone();
    branch.source = Source::player(target.uuid, target.name.clone(), exec.source.level);
    branch.pos = target.pos;
    (branch.yaw, branch.pitch) = (target.yaw, target.pitch);
    branch.eyes = false;
    run_chain(ctx, &branch, next, budget)
}

/// UUID моба выводится из `entity_id` (та же схема, что у целей селектора).
fn entity_uuid(entity_id: i32) -> Uuid {
    Uuid::from_u128(u128::from(entity_id as u32).rotate_left(32))
}

/// `execute on <relation>`: пассажир и транспорт, если связь уже записана.
fn follow_relation(
    ctx: &Context<'_>,
    exec: &Exec,
    relation: &str,
    next: &str,
    budget: &mut usize,
) -> Outcome {
    let Some(uuid) = exec.source.uuid else {
        return Outcome::Feedback(translate("argument.entity.notfound.entity", Vec::new()));
    };
    let ids = {
        let links = ctx.plan.links.lock().unwrap();
        links
            .get(&uuid)
            .map(|links| related(links, relation))
            .unwrap_or_default()
    };
    let found: Vec<Target> = ids
        .iter()
        .filter_map(|id| ctx.plan.command_target(*id))
        .collect();
    if found.is_empty() {
        return Outcome::Feedback(translate("argument.entity.notfound.entity", Vec::new()));
    }
    let mut outcomes = Vec::new();
    for target in &found {
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        let mut branch = exec.clone();
        branch.source = Source::player(target.uuid, target.name.clone(), exec.source.level);
        branch.pos = target.pos;
        (branch.yaw, branch.pitch) = (target.yaw, target.pitch);
        outcomes.push(run_chain(ctx, &branch, next, budget));
    }
    combine(outcomes)
}

fn related(links: &Links, kind: &str) -> Vec<Uuid> {
    match kind {
        "vehicle" => links.vehicle.into_iter().collect(),
        "passengers" => links.passengers.clone(),
        "owner" => links.owner.into_iter().collect(),
        "leasher" => links.leasher.into_iter().collect(),
        "attacker" => links.attacker.into_iter().collect(),
        "controller" => links.controller.into_iter().collect(),
        "origin" => links.origin.into_iter().collect(),
        "target" => links.target.into_iter().collect(),
        _ => Vec::new(),
    }
}

/// `store result|success` в счёт или в NBT блока и сущности.
fn store_value(ctx: &Context<'_>, exec: &Exec, tail: &str, budget: &mut usize) -> Outcome {
    let (mode, tail) = split(tail);
    let (kind, tail) = split(tail);
    if !matches!(mode, "result" | "success") {
        return Outcome::Feedback(error_component(
            "/execute store <result|success> <score|block|entity> …",
        ));
    }
    match kind {
        "score" => store_score(ctx, exec, mode, tail, budget),
        "block" => store_block(ctx, exec, mode, tail, budget),
        "entity" => store_entity(ctx, exec, mode, tail, budget),
        _ => Outcome::Feedback(error_component(
            "/execute store <result|success> <score|block|entity> …",
        )),
    }
}

/// `store result|success score <name> <objective>` — только в уже существующую цель.
fn store_score(
    ctx: &Context<'_>,
    exec: &Exec,
    mode: &str,
    tail: &str,
    budget: &mut usize,
) -> Outcome {
    let (holder, tail) = split(tail);
    let (objective, next) = split(tail);
    if holder.is_empty() || objective.is_empty() {
        return Outcome::Feedback(error_component(
            "/execute store <result|success> score <name> <objective> …",
        ));
    }
    if !ctx.plan.has_objective(objective) {
        return Outcome::Feedback(translate(
            "arguments.objective.notFound",
            vec![json!({"text": objective})],
        ));
    }
    let outcome = run_chain(ctx, exec, next, budget);
    let returned = command_result(&outcome);
    let stored = if mode == "success" {
        i32::from(returned != 0)
    } else {
        returned
    };
    ctx.plan.set_score(holder, objective, Some(stored));
    outcome
}

fn store_block(
    ctx: &Context<'_>,
    exec: &Exec,
    mode: &str,
    tail: &str,
    budget: &mut usize,
) -> Outcome {
    let (x, tail) = split(tail);
    let (y, tail) = split(tail);
    let (z, tail) = split(tail);
    let (path, tail) = split(tail);
    let (kind, tail) = split(tail);
    let (scale, next) = split(tail);
    let Some(coords) = args::coords3(x, y, z) else {
        return Outcome::Feedback(error_component(
            "/execute store <result|success> block <pos> <path> <type> <scale> …",
        ));
    };
    let pos = args::Basis::from_rotation(exec.yaw, exec.pitch).resolve(coords, exec.view_pos());
    let pos = [
        pos[0].floor() as i32,
        pos[1].floor() as i32,
        pos[2].floor() as i32,
    ];
    if ctx.plan.block_nbt(pos).is_none() {
        return Outcome::Feedback(translate(
            "commands.data.block.invalid",
            vec![json!(pos[0]), json!(pos[1]), json!(pos[2])],
        ));
    }
    let outcome = run_chain(ctx, exec, next, budget);
    let _ = write_stored(ctx, mode, path, kind, scale, &outcome, |tag| {
        let mut root = Tag::Compound(ctx.plan.block_nbt(pos)?);
        let steps = snbt::parse_path(path)?;
        snbt::set_path(&mut root, &steps, tag).ok()?;
        let Tag::Compound(map) = root else {
            return None;
        };
        ctx.plan.store_block_nbt(pos, map).then_some(())
    });
    outcome
}

fn store_entity(
    ctx: &Context<'_>,
    exec: &Exec,
    mode: &str,
    tail: &str,
    budget: &mut usize,
) -> Outcome {
    let (token, tail) = split(tail);
    let (path, tail) = split(tail);
    let (kind, tail) = split(tail);
    let (scale, next) = split(tail);
    let Ok(targets) = ctx.select(&exec.source, token, true) else {
        return Outcome::Feedback(translate("argument.entity.notfound.entity", Vec::new()));
    };
    if targets.is_empty() {
        return Outcome::Feedback(translate("argument.entity.notfound.entity", Vec::new()));
    }
    let outcome = run_chain(ctx, exec, next, budget);
    for target in &targets {
        let _ = write_stored(ctx, mode, path, kind, scale, &outcome, |tag| {
            let mut root = Tag::Compound(ctx.plan.entity_nbt(target));
            let steps = snbt::parse_path(path)?;
            snbt::set_path(&mut root, &steps, tag).ok()?;
            let Tag::Compound(map) = root else {
                return None;
            };
            ctx.plan.store_entity_nbt(target, map).then_some(())
        });
    }
    outcome
}

fn write_stored(
    ctx: &Context<'_>,
    mode: &str,
    path: &str,
    kind: &str,
    scale: &str,
    outcome: &Outcome,
    write: impl FnOnce(Tag) -> Option<()>,
) -> Option<()> {
    let _ = (ctx, path);
    let returned = command_result(outcome);
    let number = if mode == "success" {
        f64::from(returned != 0)
    } else {
        f64::from(returned)
    };
    let scale = scale.parse::<f64>().unwrap_or(1.0);
    let tag = snbt::numeric_tag(kind, number * scale)?;
    write(tag)
}

/// Число, которое ванильный `store result` записал бы в счёт.
fn command_result(outcome: &Outcome) -> i32 {
    let value = match outcome {
        Outcome::Feedback(value) | Outcome::Broadcast(value) => value,
        Outcome::Silent => return 0,
    };
    if value.get("color").and_then(|c| c.as_str()) == Some("red") {
        return 0;
    }
    let key = value
        .get("translate")
        .and_then(|k| k.as_str())
        .unwrap_or("");
    if key.contains("failed")
        || key.contains("unknown")
        || key.contains("not_found")
        || key.contains(".invalid")
    {
        return 0;
    }
    if key == "commands.stopwatch.query"
        || key.starts_with("commands.bossbar.get.")
        || (key.starts_with("commands.attribute.") && key.ends_with(".get.success"))
    {
        return last_stored_number(value);
    }
    if key.starts_with("commands.time.query")
        || matches!(
            key,
            "commands.fill.success"
                | "commands.clone.success"
                | "commands.worldborder.get"
                | "commands.random.roll"
                | "commands.random.sample.success"
                | "commands.trigger.simple.success"
                | "commands.trigger.add.success"
                | "commands.trigger.set.success"
                | "commands.spreadplayers.success.entities"
                | "commands.spreadplayers.success.teams"
                | "commands.fillbiome.success.count"
        )
    {
        return value
            .get("with")
            .and_then(|with| with.as_array())
            .and_then(|items| items.first())
            .and_then(|item| {
                item.as_i64()
                    .or_else(|| item.as_str().and_then(|s| s.parse().ok()))
            })
            .unwrap_or(0) as i32;
    }
    1
}

/// Последнее число в `with`: id в сообщении стоит раньше результата.
fn last_stored_number(value: &serde_json::Value) -> i32 {
    value
        .get("with")
        .and_then(|with| with.as_array())
        .and_then(|items| {
            items.iter().rev().find_map(|item| {
                item.as_i64()
                    .or_else(|| item.as_f64().map(|n| n.floor() as i64))
                    .or_else(|| item.as_str().and_then(|s| s.parse().ok()))
            })
        })
        .unwrap_or(0) as i32
}

/// Оси `x`/`y`/`z` без повторов (`minecraft:swizzle`).
fn swizzle_axes(token: &str) -> Option<[bool; 3]> {
    if token.is_empty() || token.len() > 3 {
        return None;
    }
    let mut axes = [false; 3];
    for ch in token.chars() {
        let index = match ch {
            'x' => 0,
            'y' => 1,
            'z' => 2,
            _ => return None,
        };
        if axes[index] {
            return None;
        }
        axes[index] = true;
    }
    Some(axes)
}

/// `if score <target> <objective> <op> <source> <objective>` и `matches <range>`.
///
/// Нет значения у держателя — условие ложно, ноль не подставляется.
fn score_compare<'a>(ctx: &Context<'_>, text: &'a str) -> (bool, &'a str) {
    let (left, rest) = split(text);
    let (left_obj, rest) = split(rest);
    let (op, rest) = split(rest);
    if left.is_empty() || op.is_empty() {
        return (false, rest);
    }
    let Some(left_score) = ctx.plan.score(left, left_obj) else {
        let rest = match op {
            "matches" => split(rest).1,
            "<" | "<=" | "=" | "==" | ">=" | ">" => split(split(rest).1).1,
            _ => rest,
        };
        return (false, rest);
    };
    match op {
        "matches" => {
            let (range, rest) = split(rest);
            (int_range_holds(range, left_score), rest)
        }
        "<" | "<=" | "=" | "==" | ">=" | ">" => {
            let (right, rest) = split(rest);
            let (right_obj, rest) = split(rest);
            let Some(right_score) = ctx.plan.score(right, right_obj) else {
                return (false, rest);
            };
            let ok = match op {
                "<" => left_score < right_score,
                "<=" => left_score <= right_score,
                "=" | "==" => left_score == right_score,
                ">=" => left_score >= right_score,
                _ => left_score > right_score,
            };
            (ok, rest)
        }
        _ => (false, rest),
    }
}

/// `1`, `1..5`, `..5`, `1..`.
fn int_range_holds(token: &str, value: i32) -> bool {
    let Some((lo, hi)) = token.split_once("..") else {
        return token.parse::<i32>().is_ok_and(|n| n == value);
    };
    let lo = if lo.is_empty() {
        None
    } else {
        let Ok(n) = lo.parse::<i32>() else {
            return false;
        };
        Some(n)
    };
    let hi = if hi.is_empty() {
        None
    } else {
        let Ok(n) = hi.parse::<i32>() else {
            return false;
        };
        Some(n)
    };
    lo.is_none_or(|n| value >= n) && hi.is_none_or(|n| value <= n)
}

fn take_block<'a>(exec: &Exec, text: &'a str) -> Option<([i32; 3], &'a str)> {
    let (x, text) = split(text);
    let (y, text) = split(text);
    let (z, text) = split(text);
    let coords = args::coords3(x, y, z)?;
    let pos = args::Basis::from_rotation(exec.yaw, exec.pitch).resolve(coords, exec.pos);
    Some((
        [
            pos[0].floor() as i32,
            pos[1].floor() as i32,
            pos[2].floor() as i32,
        ],
        text,
    ))
}

fn biome_test<'a>(
    ctx: &Context<'_>,
    exec: &Exec,
    text: &'a str,
) -> Result<(bool, &'a str), Outcome> {
    let Some((pos, text)) = take_block(exec, text) else {
        return Err(Outcome::Feedback(error_component(
            "/execute if biome <pos> <biome> …",
        )));
    };
    let (name, text) = split(text);
    if name.is_empty() {
        return Err(Outcome::Feedback(error_component(
            "/execute if biome <pos> <biome> …",
        )));
    }
    let Some(found) = ctx.plan.biome_name_at(pos) else {
        return Ok((false, text));
    };
    let ok = if let Some(tag) = name.strip_prefix('#') {
        let tag = args::resource_name(tag);
        ctx.plan
            .survival
            .biome_tags
            .get(&tag)
            .is_some_and(|set| set.contains(&found))
    } else {
        found == args::resource_name(name)
    };
    Ok((ok, text))
}

fn blocks_test<'a>(
    ctx: &Context<'_>,
    exec: &Exec,
    text: &'a str,
) -> Result<(bool, &'a str), Outcome> {
    let Some((start, text)) = take_block(exec, text) else {
        return Err(Outcome::Feedback(error_component(
            "/execute if blocks <start> <end> <destination> <all|masked> …",
        )));
    };
    let Some((end, text)) = take_block(exec, text) else {
        return Err(Outcome::Feedback(error_component(
            "/execute if blocks <start> <end> <destination> <all|masked> …",
        )));
    };
    let Some((dest, text)) = take_block(exec, text) else {
        return Err(Outcome::Feedback(error_component(
            "/execute if blocks <start> <end> <destination> <all|masked> …",
        )));
    };
    let (mode, text) = split(text);
    if !matches!(mode, "all" | "masked") {
        return Err(Outcome::Feedback(error_component(
            "/execute if blocks <start> <end> <destination> <all|masked> …",
        )));
    }
    let volume = (end[0] - start[0]).unsigned_abs() as u64 + 1;
    let volume = volume.saturating_mul((end[1] - start[1]).unsigned_abs() as u64 + 1);
    let volume = volume.saturating_mul((end[2] - start[2]).unsigned_abs() as u64 + 1);
    if volume > 32_768 {
        return Err(Outcome::Feedback(translate(
            "commands.clone.toobig",
            vec![json!(32768), json!(volume)],
        )));
    }
    let (x1, x2) = (start[0].min(end[0]), start[0].max(end[0]));
    let (y1, y2) = (start[1].min(end[1]), start[1].max(end[1]));
    let (z1, z2) = (start[2].min(end[2]), start[2].max(end[2]));
    let masked = mode == "masked";
    for y in y1..=y2 {
        for z in z1..=z2 {
            for x in x1..=x2 {
                let from = [x, y, z];
                let source = ctx.plan.block_state_at(from);
                if masked && source.is_none_or(|state| is_air(ctx, state)) {
                    continue;
                }
                let at = [
                    dest[0] + (x - start[0]),
                    dest[1] + (y - start[1]),
                    dest[2] + (z - start[2]),
                ];
                if source != ctx.plan.block_state_at(at) {
                    return Ok((false, text));
                }
            }
        }
    }
    Ok((true, text))
}

fn is_air(ctx: &Context<'_>, state: u32) -> bool {
    let name = ctx.plan.state_name(state);
    let bare = name.rsplit(':').next().unwrap_or(name);
    matches!(bare, "air" | "cave_air" | "void_air")
}

fn data_test<'a>(
    ctx: &Context<'_>,
    exec: &Exec,
    text: &'a str,
) -> Result<(bool, &'a str), Outcome> {
    let (kind, text) = split(text);
    match kind {
        "block" => {
            let Some((pos, text)) = take_block(exec, text) else {
                return Err(Outcome::Feedback(error_component(
                    "/execute if data block <pos> <path> …",
                )));
            };
            let (path, text) = split(text);
            let ok = snbt::parse_path(path).is_some_and(|steps| {
                ctx.plan
                    .block_nbt(pos)
                    .is_some_and(|root| snbt::get_path(&Tag::Compound(root), &steps).is_some())
            });
            Ok((ok, text))
        }
        "entity" => {
            let (token, text) = split(text);
            let (path, text) = split(text);
            let Ok(list) = ctx.select(&exec.source, token, true) else {
                return Ok((false, text));
            };
            if list.len() != 1 {
                return Ok((false, text));
            }
            let ok = snbt::parse_path(path).is_some_and(|steps| {
                let root = ctx.plan.entity_nbt(&list[0]);
                snbt::get_path(&Tag::Compound(root), &steps).is_some()
            });
            Ok((ok, text))
        }
        // Командного хранилища нет: узел есть, условие ложно.
        "storage" => {
            let (_, text) = split(text);
            let (_, text) = split(text);
            Ok((false, text))
        }
        _ => Err(Outcome::Feedback(error_component(
            "/execute if data <block|entity|storage> …",
        ))),
    }
}

fn loaded_test<'a>(
    ctx: &Context<'_>,
    exec: &Exec,
    text: &'a str,
) -> Result<(bool, &'a str), Outcome> {
    let Some((pos, text)) = take_block(exec, text) else {
        return Err(Outcome::Feedback(error_component(
            "/execute if loaded <pos> …",
        )));
    };
    Ok((ctx.plan.chunk_resident(pos[0], pos[2]), text))
}

fn items_test<'a>(
    ctx: &Context<'_>,
    exec: &Exec,
    text: &'a str,
) -> Result<(bool, &'a str), Outcome> {
    let (kind, text) = split(text);
    let (groups, text) = match kind {
        "block" => {
            let Some((pos, text)) = take_block(exec, text) else {
                return Err(Outcome::Feedback(error_component(
                    "/execute if items block <pos> <slot> <item> …",
                )));
            };
            let (slot, text) = split(text);
            let Some(which) = container_slots(slot) else {
                return Err(Outcome::Feedback(error_component(
                    "/execute if items block <pos> <slot> <item> …",
                )));
            };
            let all = ctx.plan.container_items(pos);
            let stacks = match which {
                None => all,
                Some(index) => all.get(index).copied().into_iter().collect(),
            };
            (vec![stacks], text)
        }
        "entity" => {
            let (token, text) = split(text);
            let (slot, text) = split(text);
            let list = ctx.select(&exec.source, token, false).unwrap_or_default();
            if list.is_empty() {
                return Ok((false, split(text).1));
            }
            let Some(windows) = entity_windows(ctx, &list, slot) else {
                return Err(Outcome::Feedback(error_component(
                    "/execute if items entity <targets> <slot> <item> …",
                )));
            };
            let groups = windows
                .into_iter()
                .map(|(uuid, windows)| ctx.plan.player_windows(uuid, &windows))
                .collect();
            (groups, text)
        }
        _ => {
            return Err(Outcome::Feedback(error_component(
                "/execute if items <block|entity> …",
            )));
        }
    };
    let (item, text) = split(text);
    if item.is_empty() {
        return Err(Outcome::Feedback(error_component(
            "/execute if items <block|entity> <target> <slot> <item> …",
        )));
    }
    let ok = groups.iter().all(|stacks| {
        stacks
            .iter()
            .flatten()
            .filter(|stack| item_matches(ctx, stack, item))
            .map(|stack| stack.count)
            .sum::<i32>()
            >= 1
    });
    Ok((ok, text))
}

/// `execute if slots <block|entity> … <slot> <item>`.
///
/// Отличие от `if items`: сравнивается ровно указанный слот и его стопка,
/// включая компоненты. Несколько целей — каждая должна совпасть.
fn slots_test<'a>(
    ctx: &Context<'_>,
    exec: &Exec,
    text: &'a str,
) -> Result<(bool, &'a str), Outcome> {
    let (kind, text) = split(text);
    let (stacks, text) = match kind {
        "block" => {
            let Some((pos, text)) = take_block(exec, text) else {
                return Err(Outcome::Feedback(error_component(
                    "/execute if slots block <pos> <slot> <item>",
                )));
            };
            let (slot, text) = split(text);
            let Some(which) = container_slots(slot) else {
                return Err(Outcome::Feedback(error_component(
                    "/execute if slots block <pos> <slot> <item>",
                )));
            };
            let all = ctx.plan.container_items(pos);
            let chosen = match which {
                None => all,
                Some(index) => all.get(index).copied().into_iter().collect(),
            };
            (vec![chosen], text)
        }
        "entity" => {
            let (token, text) = split(text);
            let (slot, text) = split(text);
            let list = ctx.select(&exec.source, token, false).unwrap_or_default();
            if list.is_empty() {
                return Ok((false, split(text).1));
            }
            let Some(windows) = entity_windows(ctx, &list, slot) else {
                return Err(Outcome::Feedback(error_component(
                    "/execute if slots entity <targets> <slot> <item>",
                )));
            };
            let groups = windows
                .into_iter()
                .map(|(uuid, windows)| ctx.plan.player_windows(uuid, &windows))
                .collect();
            (groups, text)
        }
        _ => {
            return Err(Outcome::Feedback(error_component(
                "/execute if slots <block|entity> …",
            )));
        }
    };
    let (item, text) = split(text);
    if item.is_empty() {
        return Err(Outcome::Feedback(error_component(
            "/execute if slots <block|entity> <target> <slot> <item>",
        )));
    }
    // В отличие от `items`, пустой слот не совпадает: сравнивается одна стопка.
    let ok = stacks.iter().all(|group| {
        group
            .iter()
            .flatten()
            .any(|stack| item_matches(ctx, stack, item))
    });
    Ok((ok, text))
}

/// `None` — все слоты контейнера, `Some` — один `container.N`.
fn container_slots(token: &str) -> Option<Option<usize>> {
    let (group, index) = token.split_once('.').unwrap_or((token, ""));
    match group.strip_prefix("minecraft:").unwrap_or(group) {
        "contents" => Some(None),
        "container" if index == "*" => Some(None),
        "container" => index.parse::<usize>().ok().map(Some),
        _ => None,
    }
}

/// Окна инвентаря каждой цели. `None` — слот не разобран.
fn entity_windows(
    ctx: &Context<'_>,
    targets: &[Target],
    token: &str,
) -> Option<Vec<(Uuid, Vec<usize>)>> {
    let mut out = Vec::new();
    for target in targets {
        target.game_mode?;
        let windows = player_windows(ctx, target.uuid, token)?;
        out.push((target.uuid, windows));
    }
    Some(out)
}

fn player_windows(ctx: &Context<'_>, uuid: Uuid, token: &str) -> Option<Vec<usize>> {
    let (group, index) = token.split_once('.').unwrap_or((token, ""));
    let group = group.strip_prefix("minecraft:").unwrap_or(group);
    let one = |base: usize, count: usize| -> Option<Vec<usize>> {
        if index == "*" {
            return Some((0..count).map(|i| base + i).collect());
        }
        let n = index.parse::<usize>().ok()?;
        (n < count).then(|| vec![base + n])
    };
    match group {
        "contents" => Some(
            (9..36)
                .chain(36..45)
                .chain(5..9)
                .chain(std::iter::once(45))
                .collect(),
        ),
        "hotbar" => one(36, 9),
        "inventory" => one(9, 27),
        "armor" => match index {
            "*" => Some((5..9).collect()),
            "head" => Some(vec![5]),
            "chest" => Some(vec![6]),
            "legs" => Some(vec![7]),
            "feet" => Some(vec![8]),
            _ => None,
        },
        "weapon" => match index {
            "*" => Some(vec![ctx.plan.held_window(uuid).unwrap_or(36), 45]),
            "mainhand" => Some(vec![ctx.plan.held_window(uuid).unwrap_or(36)]),
            "offhand" => Some(vec![45]),
            _ => None,
        },
        _ => None,
    }
}

fn item_matches(
    ctx: &Context<'_>,
    stack: &chunkwake_protocol::inventory::Stack,
    token: &str,
) -> bool {
    use super::item::{parse_item, tag_ids};
    if let Some(tag) = token.strip_prefix('#') {
        let tag = format!("#{tag}");
        return tag_ids(
            &ctx.plan.survival.item_tags,
            &ctx.plan.survival.item_ids,
            &tag,
        )
        .is_some_and(|ids| ids.contains(&stack.item));
    }
    let Some(parsed) = parse_item(token, &ctx.plan.survival.item_ids) else {
        return false;
    };
    stack.item == parsed.id
        && parsed.damage.is_none_or(|value| value == stack.damage)
        && crate::survival::components_satisfy(stack.extra, parsed.extra)
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
    fn execute_depth_is_capped() {
        let mut guards = Vec::new();
        for _ in 0..MAX_EXECUTE_DEPTH {
            guards.push(ExecuteDepth::enter().expect("depth below limit"));
        }
        assert!(ExecuteDepth::enter().is_none());
        drop(guards);
        assert!(
            ExecuteDepth::enter().is_some(),
            "после выхода место снова есть"
        );
    }

    #[test]
    fn split_words() {
        assert_eq!(split("as @a run say hi"), ("as", "@a run say hi"));
        assert_eq!(split("run"), ("run", ""));
        assert_eq!(split("   spaced  "), ("spaced", ""));
    }
}
