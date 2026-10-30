//! `/scoreboard`: цели, счёт игроков и отображение в сайдбаре.
//!
//! Хранилище живёт в мире: `/execute if score`, `team` и плагины читают его
//! напрямую. Пакеты `set_objective`/`set_score`/`set_display_objective`
//! рассылаются как ванильные, иначе счёт на клиенте расходится с сервером.

use super::Context;
use super::dispatch::{Call, Outcome, error_component, translate};
use super::entity::usage;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Критерий цели: ванила знает десятки; нам достаточно типа хранения.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Criterion {
    /// `dummy` — значение ставится командами и плагинами.
    Dummy,
    /// `trigger` — игрок меняет свой счёт через `/trigger`, пока его не закрыли.
    Trigger,
    /// Всё остальное принимаем и храним как `dummy`.
    Other,
}

impl Criterion {
    pub fn parse(token: &str) -> Option<Self> {
        let name = token.strip_prefix("minecraft:").unwrap_or(token);
        Some(match name {
            "dummy" => Criterion::Dummy,
            "trigger" => Criterion::Trigger,
            _ if name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '_' || c == '.') =>
            {
                Criterion::Other
            }
            _ => return None,
        })
    }
}

/// Одна цель (`ScoreboardObjective`).
#[derive(Clone, Debug, PartialEq)]
pub struct Objective {
    pub name: String,
    pub display_name: Value,
    pub criterion: Criterion,
    /// `integer` или `hearts` — как число рисует клиент.
    pub render_type: i32,
    /// Формат числа. `None` — клиент рисует обычное число.
    pub number_format: Option<chunkwake_protocol::play::NumberFormat>,
}

/// Состояние счёта: цели, значения и отображаемые слоты.
#[derive(Default, Debug)]
pub struct Scoreboard {
    /// Цели по имени, в порядке создания.
    pub objectives: BTreeMap<String, Objective>,
    /// `цель → (держатель → значение)`.
    pub scores: BTreeMap<String, BTreeMap<String, i32>>,
    /// Слот отображения (`sidebar`, `list`, `below_name`) → цель.
    pub display: BTreeMap<String, String>,
    /// Формат отдельного счёта. В пакет значения не пишется.
    pub score_formats: BTreeMap<(String, String), chunkwake_protocol::play::NumberFormat>,
    /// Открытые trigger-счета: `(держатель, цель)`. Закрытый счёт `/trigger` не берёт.
    pub primed: BTreeSet<(String, String)>,
}

impl Scoreboard {
    /// Значение счёта; `None` — нет такого держателя или цели.
    pub fn get(&self, holder: &str, objective: &str) -> Option<i32> {
        self.scores
            .get(objective)
            .and_then(|m| m.get(holder).copied())
    }

    /// Ставит значение: `None` — снять счёт.
    pub fn set(&mut self, holder: &str, objective: &str, value: Option<i32>) -> bool {
        if !self.objectives.contains_key(objective) {
            return false;
        }
        let entry = self.scores.entry(objective.to_owned()).or_default();
        match value {
            Some(v) => {
                entry.insert(holder.to_owned(), v);
                true
            }
            None => {
                self.primed
                    .remove(&(holder.to_owned(), objective.to_owned()));
                entry.remove(holder).is_some()
            }
        }
    }

    /// Критерий цели. `None` — цели нет.
    pub fn criterion(&self, name: &str) -> Option<Criterion> {
        self.objectives
            .get(name)
            .map(|objective| objective.criterion)
    }

    /// `/scoreboard players enable`: открывает trigger и ставит 0.
    ///
    /// `Ok(false)` — уже открыт. `Err(false)` — цель не trigger. `Err(true)` — цели нет.
    pub fn enable_trigger(&mut self, holder: &str, objective: &str) -> Result<bool, bool> {
        match self.criterion(objective) {
            None => return Err(true),
            Some(Criterion::Trigger) => {}
            Some(_) => return Err(false),
        }
        let key = (holder.to_owned(), objective.to_owned());
        if !self.primed.insert(key) {
            return Ok(false);
        }
        self.set(holder, objective, Some(0));
        Ok(true)
    }

    /// `/trigger`: забирает открытый счёт и ставит новое значение.
    ///
    /// `Err(true)` — цели нет, `Err(false)` — это не trigger или он закрыт.
    pub fn pull_trigger(
        &mut self,
        holder: &str,
        objective: &str,
        value: i32,
        add: bool,
    ) -> Result<i32, bool> {
        match self.criterion(objective) {
            None => return Err(true),
            Some(Criterion::Trigger) => {}
            Some(_) => return Err(false),
        }
        let key = (holder.to_owned(), objective.to_owned());
        if !self.primed.remove(&key) {
            return Err(false);
        }
        let current = self.get(holder, objective).unwrap_or(0);
        let next = if add {
            current.saturating_add(value)
        } else {
            value
        };
        self.set(holder, objective, Some(next));
        Ok(next)
    }

    /// Добавляет цель; `false` — имя занято.
    pub fn add_objective(&mut self, objective: Objective) -> bool {
        if self.objectives.contains_key(&objective.name) {
            return false;
        }
        self.scores.entry(objective.name.clone()).or_default();
        self.objectives.insert(objective.name.clone(), objective);
        true
    }

    /// Меняет подпись или способ рисования; `false` — цели нет.
    pub fn modify(&mut self, name: &str, display: Option<Value>, render: Option<i32>) -> bool {
        let Some(objective) = self.objectives.get_mut(name) else {
            return false;
        };
        if let Some(display) = display {
            objective.display_name = display;
        }
        if let Some(render) = render {
            objective.render_type = render;
        }
        true
    }

    /// Ставит или сбрасывает формат числа цели.
    pub fn set_format(
        &mut self,
        name: &str,
        format: Option<chunkwake_protocol::play::NumberFormat>,
    ) -> bool {
        let Some(objective) = self.objectives.get_mut(name) else {
            return false;
        };
        objective.number_format = format;
        true
    }

    /// Формат одного держателя. Цели нет — `false`.
    pub fn set_score_format(
        &mut self,
        holder: &str,
        objective: &str,
        format: Option<chunkwake_protocol::play::NumberFormat>,
    ) -> bool {
        if !self.objectives.contains_key(objective) {
            return false;
        }
        let key = (holder.to_owned(), objective.to_owned());
        match format {
            Some(format) => {
                self.score_formats.insert(key, format);
            }
            None => {
                self.score_formats.remove(&key);
            }
        }
        true
    }

    /// Убирает цель вместе со значениями; `false` — такой не было.
    pub fn remove_objective(&mut self, name: &str) -> bool {
        self.scores.remove(name);
        self.display.retain(|_, value| value != name);
        self.score_formats
            .retain(|(_, objective), _| objective != name);
        self.primed.retain(|(_, objective)| objective != name);
        self.objectives.remove(name).is_some()
    }

    /// Список значений цели по убыванию (как сайдбар).
    pub fn sorted(&self, objective: &str) -> Vec<(String, i32)> {
        let mut out: Vec<(String, i32)> = self
            .scores
            .get(objective)
            .map(|m| m.iter().map(|(k, v)| (k.clone(), *v)).collect())
            .unwrap_or_default();
        out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        out
    }
}

/// `/scoreboard objectives …`: подкоманда приходит из [`Call::path`].
pub(super) fn objectives(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(sub) = call
        .path
        .get(1)
        .map(String::as_str)
        .or_else(|| input.word())
    else {
        return usage("/scoreboard objectives <add|remove|list|setdisplay>");
    };
    match sub {
        "add" => {
            let (Some(name), Some(criterion)) = (input.word(), input.word()) else {
                return usage("/scoreboard objectives add <objective> <criterion> [displayName]");
            };
            let Some(criterion) = Criterion::parse(criterion) else {
                return Outcome::Feedback(translate(
                    "commands.scoreboard.objectives.criterion.invalid",
                    vec![json!({"text": criterion})],
                ));
            };
            let display = match input.word() {
                Some(raw) => serde_json::from_str(raw).unwrap_or_else(|_| json!({"text": raw})),
                None => json!({"text": name}),
            };
            let objective = Objective {
                name: name.to_owned(),
                display_name: display,
                criterion,
                render_type: 0,
                number_format: None,
            };
            if ctx.plan.add_objective(objective) {
                Outcome::Feedback(translate(
                    "commands.scoreboard.objectives.add.success",
                    vec![json!({"text": name})],
                ))
            } else {
                Outcome::Feedback(translate(
                    "commands.scoreboard.objectives.add.alreadyExists",
                    vec![json!({"text": name})],
                ))
            }
        }
        "remove" => {
            let Some(name) = input.word() else {
                return usage("/scoreboard objectives remove <objective>");
            };
            if ctx.plan.remove_objective(name) {
                Outcome::Feedback(translate(
                    "commands.scoreboard.objectives.remove.success",
                    vec![json!({"text": name})],
                ))
            } else {
                Outcome::Feedback(translate(
                    "commands.scoreboard.objectives.remove.none",
                    vec![json!({"text": name})],
                ))
            }
        }
        "list" => {
            let names = ctx.plan.objective_names();
            Outcome::Feedback(translate(
                "commands.scoreboard.objectives.list.success",
                vec![json!({"text": names.join(", ")}), Value::from(names.len())],
            ))
        }
        "setdisplay" => {
            let (Some(slot), Some(name)) = (input.word(), input.word()) else {
                return usage("/scoreboard objectives setdisplay <slot> [objective]");
            };
            match ctx.plan.set_objective_display(slot, name) {
                true => Outcome::Feedback(translate(
                    "commands.scoreboard.objectives.setdisplay.successful",
                    vec![json!({"text": name}), json!({"text": slot})],
                )),
                false => Outcome::Feedback(translate(
                    "commands.scoreboard.objectives.setdisplay.invalidObjective",
                    vec![json!({"text": name})],
                )),
            }
        }
        "modify" => {
            let Some(name) = input.word() else {
                return usage(
                    "/scoreboard objectives modify <objective> <displayname|rendertype> …",
                );
            };
            let Some(field) = input.word() else {
                return usage(
                    "/scoreboard objectives modify <objective> <displayname|rendertype> …",
                );
            };
            let (display, render) = match field {
                "displayname" => {
                    let raw = input.rest().trim();
                    if raw.is_empty() {
                        return usage(
                            "/scoreboard objectives modify <objective> displayname <name>",
                        );
                    }
                    let display =
                        serde_json::from_str(raw).unwrap_or_else(|_| json!({"text": raw}));
                    (Some(display), None)
                }
                "rendertype" => {
                    let Some(kind) = input.word() else {
                        return usage(
                            "/scoreboard objectives modify <objective> rendertype <integer|hearts>",
                        );
                    };
                    let render = match kind {
                        "integer" => 0,
                        "hearts" => 1,
                        _ => {
                            return usage(
                                "/scoreboard objectives modify <objective> rendertype <integer|hearts>",
                            );
                        }
                    };
                    (None, Some(render))
                }
                "numberformat" => {
                    let format = match read_number_format(&mut input) {
                        Ok(format) => format,
                        Err(outcome) => return outcome,
                    };
                    if ctx.plan.set_objective_format(name, format) {
                        return Outcome::Feedback(translate(
                            "commands.scoreboard.objectives.modify.success",
                            vec![json!({"text": name})],
                        ));
                    }
                    return Outcome::Feedback(translate(
                        "arguments.objective.notFound",
                        vec![json!({"text": name})],
                    ));
                }
                "displayautoupdate" => {
                    return Outcome::Feedback(error_component(
                        "/scoreboard objectives modify supports displayname, rendertype and numberformat",
                    ));
                }
                _ => {
                    return usage(
                        "/scoreboard objectives modify <objective> <displayname|rendertype> …",
                    );
                }
            };
            if ctx.plan.modify_objective(name, display, render) {
                Outcome::Feedback(translate(
                    "commands.scoreboard.objectives.modify.success",
                    vec![json!({"text": name})],
                ))
            } else {
                Outcome::Feedback(translate(
                    "arguments.objective.notFound",
                    vec![json!({"text": name})],
                ))
            }
        }
        _ => usage("/scoreboard objectives <add|remove|list|setdisplay>"),
    }
}

/// `/scoreboard players …`.
pub(super) fn players(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    if call.path.get(1).map(String::as_str) == Some("display") {
        return display_score(ctx, call);
    }
    let mut input = call.t();
    let Some(sub) = call
        .path
        .get(1)
        .map(String::as_str)
        .or_else(|| input.word())
    else {
        return usage("/scoreboard players <set|add|remove|reset|list>");
    };
    let (Some(holder), Some(objective)) = (input.word(), input.word()) else {
        return usage("/scoreboard players <set|add|remove|reset|list> <targets> <objective>");
    };
    let holders = ctx.plan.score_holders(holder);
    let mut changed = 0;
    match sub {
        "set" | "add" | "remove" => {
            let Some(value) = input.word().and_then(|t| t.parse::<i32>().ok()) else {
                return usage("/scoreboard players set <targets> <objective> <score>");
            };
            for holder in &holders {
                let current = ctx.plan.score(holder, objective).unwrap_or(0);
                let next = match sub {
                    "set" => value,
                    "add" => current.saturating_add(value),
                    _ => current.saturating_sub(value),
                };
                if ctx.plan.set_score(holder, objective, Some(next)) {
                    changed += 1;
                }
            }
        }
        "reset" => {
            for holder in &holders {
                if ctx.plan.set_score(holder, objective, None) {
                    changed += 1;
                }
            }
        }
        "enable" => return enable_scores(ctx, &holders, objective),
        "list" => {
            for holder in &holders {
                let scores = ctx.plan.score_list(holder);
                let text: Vec<String> = scores
                    .into_iter()
                    .map(|(objective, value)| format!("{objective}: {value}"))
                    .collect();
                if !text.is_empty() {
                    return Outcome::text(format!("{holder} has {} [{text:?}]", text.len()));
                }
            }
            return Outcome::text(format!("{holder} has no scores"));
        }
        _ => return usage("/scoreboard players <set|add|remove|reset|enable|list>"),
    }
    if changed == 0 {
        return Outcome::Feedback(translate(
            "commands.scoreboard.players.set.failed",
            vec![json!({"text": objective})],
        ));
    }
    Outcome::Feedback(translate(
        "commands.scoreboard.players.set.success",
        vec![
            json!({"text": holders.join(", ")}),
            json!({"text": objective}),
        ],
    ))
}

/// `/scoreboard players enable <targets> <objective>`: открывает trigger.
fn enable_scores(ctx: &Context<'_>, holders: &[String], objective: &str) -> Outcome {
    let mut changed = 0;
    let mut missing = false;
    let mut invalid = false;
    for holder in holders {
        match ctx.plan.enable_trigger(holder, objective) {
            Ok(true) => changed += 1,
            Ok(false) => {}
            Err(true) => missing = true,
            Err(false) => invalid = true,
        }
    }
    if missing {
        return Outcome::Feedback(translate(
            "arguments.objective.notFound",
            vec![json!({"text": objective})],
        ));
    }
    if invalid {
        return Outcome::Feedback(translate(
            "commands.scoreboard.players.enable.invalid",
            Vec::new(),
        ));
    }
    if changed == 0 {
        return Outcome::Feedback(translate(
            "commands.scoreboard.players.enable.failed",
            Vec::new(),
        ));
    }
    let key = if changed == 1 {
        "commands.scoreboard.players.enable.success.single"
    } else {
        "commands.scoreboard.players.enable.success.multiple"
    };
    Outcome::Feedback(translate(
        key,
        vec![
            json!({"text": holders.join(", ")}),
            json!({"text": objective}),
            Value::from(changed),
        ],
    ))
}

/// `/scoreboard players display numberformat <targets> <objective> […]`.
fn display_score(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    if call.path.get(2).map(String::as_str) != Some("numberformat") {
        return usage("/scoreboard players display numberformat <targets> <objective>");
    }
    let mut input = call.t();
    let (Some(holder), Some(objective)) = (input.word(), input.word()) else {
        return usage("/scoreboard players display numberformat <targets> <objective>");
    };
    let format = match read_number_format(&mut input) {
        Ok(format) => format,
        Err(outcome) => return outcome,
    };
    let holders = ctx.plan.score_holders(holder);
    let mut changed = 0;
    for holder in &holders {
        if ctx.plan.set_score_format(holder, objective, format.clone()) {
            changed += 1;
        }
    }
    if changed == 0 {
        return Outcome::Feedback(translate(
            "arguments.objective.notFound",
            vec![json!({"text": objective})],
        ));
    }
    Outcome::Feedback(translate(
        "commands.scoreboard.players.display.numberFormat.success",
        vec![
            json!({"text": holders.join(", ")}),
            json!({"text": objective}),
        ],
    ))
}

/// Голый `numberformat` сбрасывает формат. Дальше — `blank`, `fixed`, `styled`.
fn read_number_format(
    input: &mut super::args::Input<'_>,
) -> Result<Option<chunkwake_protocol::play::NumberFormat>, Outcome> {
    Ok(match input.word() {
        None => None,
        Some("blank") => Some(chunkwake_protocol::play::NumberFormat::Blank),
        Some("fixed") => {
            let raw = input.rest().trim();
            if raw.is_empty() {
                return Err(usage("/scoreboard … numberformat fixed <component>"));
            }
            Some(chunkwake_protocol::play::NumberFormat::Fixed(
                component_value(raw),
            ))
        }
        Some("styled") => {
            let raw = input.rest().trim();
            let Ok(style) = serde_json::from_str::<Value>(raw) else {
                return Err(Outcome::Feedback(translate(
                    "argument.component.invalid",
                    vec![Value::String(raw.to_owned())],
                )));
            };
            Some(chunkwake_protocol::play::NumberFormat::Styled(style))
        }
        Some(_) => {
            return Err(usage(
                "/scoreboard … numberformat [blank|fixed <component>|styled <style>]",
            ));
        }
    })
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

/// `/scoreboard <objectives|players> …`: форма узлов сверена с ванильным
/// `commands.json` 26.3.
pub(super) fn specs() -> Vec<super::dispatch::Spec> {
    use super::dispatch::Spec;
    use super::registry::{
        GAMEMASTERS, int, objective, objective_criteria, score_holder, scoreboard_slot, word,
    };

    fn format_choices(handler: super::dispatch::Handler) -> Vec<Spec> {
        vec![
            Spec::literal_leaf("blank", GAMEMASTERS, handler),
            Spec::group(
                "fixed",
                GAMEMASTERS,
                vec![Spec::arg_leaf(
                    "contents",
                    GAMEMASTERS,
                    super::registry::component_parser(),
                    handler,
                )],
            ),
            Spec::group(
                "styled",
                GAMEMASTERS,
                vec![Spec::arg_leaf(
                    "style",
                    GAMEMASTERS,
                    chunkwake_protocol::text::Parser::Style,
                    handler,
                )],
            ),
        ]
    }
    fn number_format(handler: super::dispatch::Handler) -> Spec {
        Spec::exec(
            "numberformat",
            GAMEMASTERS,
            handler,
            format_choices(handler),
        )
    }

    // `<targets> <objective> [score]` — общий хвост `/scoreboard players`.
    let scored = |value: Option<chunkwake_protocol::text::Parser>| {
        let objective = Spec::arg_leaf("objective", GAMEMASTERS, objective(), players);
        match value {
            Some(parser) => {
                objective.with_children(vec![Spec::arg_leaf("score", GAMEMASTERS, parser, players)])
            }
            None => objective,
        }
    };
    let players_branch = |name: &str, value: Option<chunkwake_protocol::text::Parser>| {
        Spec::exec(
            name,
            GAMEMASTERS,
            players,
            vec![
                Spec::arg_leaf("targets", GAMEMASTERS, score_holder(true), players)
                    .with_children(vec![scored(value)]),
            ],
        )
    };
    vec![Spec::exec(
        "scoreboard",
        GAMEMASTERS,
        dispatch,
        vec![
            Spec::group(
                "objectives",
                GAMEMASTERS,
                vec![
                    Spec::group(
                        "add",
                        GAMEMASTERS,
                        vec![
                            Spec::arg_leaf("objective", GAMEMASTERS, word(), objectives)
                                .with_children(vec![
                                    Spec::arg_leaf(
                                        "criteria",
                                        GAMEMASTERS,
                                        objective_criteria(),
                                        objectives,
                                    )
                                    .with_children(vec![
                                        Spec::arg_leaf(
                                            "displayName",
                                            GAMEMASTERS,
                                            super::registry::component_parser(),
                                            objectives,
                                        ),
                                    ]),
                                ]),
                        ],
                    ),
                    Spec::exec("list", GAMEMASTERS, objectives, Vec::new()),
                    Spec::group(
                        "remove",
                        GAMEMASTERS,
                        vec![Spec::arg_leaf(
                            "objective",
                            GAMEMASTERS,
                            objective(),
                            objectives,
                        )],
                    ),
                    Spec::group(
                        "modify",
                        GAMEMASTERS,
                        vec![
                            super::dispatch::Spec::arg_group(
                                "objective",
                                GAMEMASTERS,
                                objective(),
                                objectives,
                            )
                            .with_children(vec![
                                Spec::group(
                                    "displayname",
                                    GAMEMASTERS,
                                    vec![Spec::arg_leaf(
                                        "displayName",
                                        GAMEMASTERS,
                                        super::registry::component_parser(),
                                        objectives,
                                    )],
                                ),
                                Spec::group(
                                    "rendertype",
                                    GAMEMASTERS,
                                    vec![
                                        Spec::literal_leaf("integer", GAMEMASTERS, objectives),
                                        Spec::literal_leaf("hearts", GAMEMASTERS, objectives),
                                    ],
                                ),
                                number_format(objectives),
                            ]),
                        ],
                    ),
                    Spec::group(
                        "setdisplay",
                        GAMEMASTERS,
                        vec![
                            Spec::arg_leaf("slot", GAMEMASTERS, scoreboard_slot(), objectives)
                                .with_children(vec![Spec::arg_leaf(
                                    "objective",
                                    GAMEMASTERS,
                                    objective(),
                                    objectives,
                                )]),
                        ],
                    ),
                ],
            ),
            Spec::group(
                "players",
                GAMEMASTERS,
                vec![
                    players_branch("add", Some(int(i32::MIN, i32::MAX))),
                    players_branch("remove", Some(int(i32::MIN, i32::MAX))),
                    players_branch("set", Some(int(i32::MIN, i32::MAX))),
                    players_branch("reset", None),
                    Spec::group(
                        "enable",
                        GAMEMASTERS,
                        vec![Spec::arg(
                            "targets",
                            GAMEMASTERS,
                            score_holder(true),
                            vec![Spec::arg_leaf(
                                "objective",
                                GAMEMASTERS,
                                objective(),
                                players,
                            )],
                        )],
                    ),
                    // `list` берёт держателя без цели.
                    Spec::exec(
                        "list",
                        GAMEMASTERS,
                        players,
                        vec![Spec::arg_leaf(
                            "targets",
                            GAMEMASTERS,
                            score_holder(true),
                            players,
                        )],
                    ),
                    Spec::group(
                        "display",
                        GAMEMASTERS,
                        vec![Spec::exec(
                            "numberformat",
                            GAMEMASTERS,
                            players,
                            vec![Spec::arg(
                                "targets",
                                GAMEMASTERS,
                                score_holder(true),
                                vec![
                                    Spec::arg_leaf("objective", GAMEMASTERS, objective(), players)
                                        .with_children(format_choices(players)),
                                ],
                            )],
                        )],
                    ),
                ],
            ),
        ],
    )]
}

/// `/scoreboard <objectives|players> …`.
pub(super) fn dispatch(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    match call.path.first().map(String::as_str) {
        Some("objectives") => objectives(ctx, call),
        Some("players") => players(ctx, call),
        _ => usage("/scoreboard <objectives|players> …"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board() -> Scoreboard {
        let mut board = Scoreboard::default();
        board.add_objective(Objective {
            name: "kills".into(),
            display_name: json!({"text": "Kills"}),
            criterion: Criterion::Dummy,
            render_type: 0,
            number_format: None,
        });
        board
    }

    #[test]
    fn set_get_and_sorted() {
        let mut board = board();
        assert!(board.set("Alice", "kills", Some(3)));
        assert!(board.set("Bob", "kills", Some(7)));
        assert_eq!(board.get("Alice", "kills"), Some(3));
        assert_eq!(
            board.sorted("kills"),
            vec![("Bob".into(), 7), ("Alice".into(), 3)]
        );
        assert!(board.set("Alice", "kills", None));
        assert_eq!(board.get("Alice", "kills"), None);
    }

    #[test]
    fn unknown_objective_is_rejected() {
        let mut board = board();
        assert!(!board.set("Alice", "nope", Some(1)));
        assert!(!board.add_objective(Objective {
            name: "kills".into(),
            display_name: json!({"text": "Kills"}),
            criterion: Criterion::Dummy,
            render_type: 0,
            number_format: None,
        }));
    }

    #[test]
    fn removing_clears_display_and_scores() {
        let mut board = board();
        board.set("Alice", "kills", Some(1));
        board.display.insert("sidebar".into(), "kills".into());
        assert!(board.remove_objective("kills"));
        assert!(board.display.is_empty());
        assert!(board.scores.get("kills").is_none());
        assert!(!board.remove_objective("kills"));
    }

    #[test]
    fn criteria_parse() {
        assert_eq!(Criterion::parse("dummy"), Some(Criterion::Dummy));
        assert_eq!(
            Criterion::parse("minecraft:trigger"),
            Some(Criterion::Trigger)
        );
        assert_eq!(Criterion::parse("minecraft:health"), Some(Criterion::Other));
        assert_eq!(Criterion::parse("Not Valid!"), None);
    }

    #[test]
    fn trigger_stays_closed_until_enabled() {
        let mut board = Scoreboard::default();
        board.add_objective(Objective {
            name: "trig".into(),
            display_name: json!({"text": "trig"}),
            criterion: Criterion::Trigger,
            render_type: 0,
            number_format: None,
        });
        assert_eq!(board.pull_trigger("Ada", "trig", 1, true), Err(false));
        assert_eq!(board.enable_trigger("Ada", "trig"), Ok(true));
        assert_eq!(board.enable_trigger("Ada", "trig"), Ok(false));
        assert_eq!(board.pull_trigger("Ada", "trig", 1, true), Ok(1));
        assert_eq!(board.get("Ada", "trig"), Some(1));
        assert_eq!(board.pull_trigger("Ada", "trig", 1, true), Err(false));
        assert_eq!(board.enable_trigger("Ada", "kills"), Err(true));
    }
}
