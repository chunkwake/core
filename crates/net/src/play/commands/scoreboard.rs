//! `/scoreboard`: цели, счёт игроков и отображение в сайдбаре.
//!
//! Хранилище живёт в мире: `/execute if score`, `team` и плагины читают его
//! напрямую. Пакеты `set_objective`/`set_score`/`set_display_objective`
//! рассылаются как ванильные, иначе счёт на клиенте расходится с сервером.

use super::dispatch::{error_component, translate, Call, Outcome};
use super::entity::usage;
use super::Context;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// Критерий цели: ванила знает десятки; нам достаточно типа хранения.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Criterion {
    /// `dummy` — значение ставится командами и плагинами.
    Dummy,
    /// Всё остальное принимаем и храним как `dummy`.
    Other,
}

impl Criterion {
    pub fn parse(token: &str) -> Option<Self> {
        let name = token.strip_prefix("minecraft:").unwrap_or(token);
        Some(match name {
            "dummy" => Criterion::Dummy,
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
            None => entry.remove(holder).is_some(),
        }
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

    /// Убирает цель вместе со значениями; `false` — такой не было.
    pub fn remove_objective(&mut self, name: &str) -> bool {
        self.scores.remove(name);
        self.display.retain(|_, value| value != name);
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
    let Some(sub) = call.path.get(1).map(String::as_str).or_else(|| input.word()) else {
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
        "modify" => Outcome::Feedback(error_component(
            "/scoreboard objectives modify is not implemented",
        )),
        _ => usage("/scoreboard objectives <add|remove|list|setdisplay>"),
    }
}

/// `/scoreboard players …`.
pub(super) fn players(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let mut input = call.t();
    let Some(sub) = call.path.get(1).map(String::as_str).or_else(|| input.word()) else {
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
        _ => return usage("/scoreboard players <set|add|remove|reset|list>"),
    }
    if changed == 0 {
        return Outcome::Feedback(translate(
            "commands.scoreboard.players.set.failed",
            vec![json!({"text": objective})],
        ));
    }
    Outcome::Feedback(translate(
        "commands.scoreboard.players.set.success",
        vec![json!({"text": holders.join(", ")}), json!({"text": objective})],
    ))
}

/// `/scoreboard <objectives|players> …`: форма узлов сверена с ванильным
/// `commands.json` 26.3.
pub(super) fn specs() -> Vec<super::dispatch::Spec> {
    use super::dispatch::Spec;
    use super::registry::{
        int, objective, objective_criteria, score_holder, scoreboard_slot, word, GAMEMASTERS,
    };

    // `<targets> <objective> [score]` — общий хвост `/scoreboard players`.
    let scored = |value: Option<chunkwake_protocol::text::Parser>| {
        let objective = Spec::arg_leaf("objective", GAMEMASTERS, objective(), players);
        match value {
            Some(parser) => objective.with_children(vec![Spec::arg_leaf(
                "score",
                GAMEMASTERS,
                parser,
                players,
            )]),
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
                                    .with_children(vec![Spec::arg_leaf(
                                        "displayName",
                                        GAMEMASTERS,
                                        super::registry::component_parser(),
                                        objectives,
                                    )]),
                                ]),
                        ],
                    ),
                    Spec::exec(
                        "list",
                        GAMEMASTERS,
                        objectives,
                        Vec::new(),
                    ),
                    Spec::group(
                        "remove",
                        GAMEMASTERS,
                        vec![Spec::arg_leaf("objective", GAMEMASTERS, objective(), objectives)],
                    ),
                    Spec::group(
                        "setdisplay",
                        GAMEMASTERS,
                        vec![
                            Spec::arg_leaf("slot", GAMEMASTERS, scoreboard_slot(), objectives)
                                .with_children(vec![
                                    Spec::arg_leaf("objective", GAMEMASTERS, objective(), objectives),
                                ]),
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
        });
        board
    }

    #[test]
    fn set_get_and_sorted() {
        let mut board = board();
        assert!(board.set("Alice", "kills", Some(3)));
        assert!(board.set("Bob", "kills", Some(7)));
        assert_eq!(board.get("Alice", "kills"), Some(3));
        assert_eq!(board.sorted("kills"), vec![("Bob".into(), 7), ("Alice".into(), 3)]);
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
            Criterion::parse("minecraft:health"),
            Some(Criterion::Other)
        );
        assert_eq!(Criterion::parse("Not Valid!"), None);
    }
}
