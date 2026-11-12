//! `/team`: состав, цвет, префикс и правила столкновений.
//!
//! Хранится рядом со счётом. `teammsg` шлёт текст только участникам команды
//! отправителя.

use super::Context;
use super::dispatch::{Call, Outcome, translate};
use super::entity::usage;
use crate::play::PlayPlan;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

/// Одна команда (`Team`).
#[derive(Clone, Debug)]
pub struct Team {
    pub name: String,
    pub display: Value,
    pub color: String,
    pub prefix: Value,
    pub suffix: Value,
    pub friendly_fire: bool,
    pub see_friendly_invisibles: bool,
    pub nametag: String,
    pub collision: String,
    pub death_message: String,
    pub members: BTreeSet<String>,
}

impl Team {
    fn new(name: &str, display: Value) -> Self {
        Self {
            name: name.to_owned(),
            display,
            color: "white".into(),
            prefix: json!({"text": ""}),
            suffix: json!({"text": ""}),
            friendly_fire: true,
            see_friendly_invisibles: true,
            nametag: "always".into(),
            collision: "always".into(),
            death_message: "always".into(),
            members: BTreeSet::new(),
        }
    }
}

/// Все команды мира, по имени.
#[derive(Default, Debug)]
pub struct Board {
    teams: BTreeMap<String, Team>,
}

impl Board {
    fn of_member(&self, name: &str) -> Option<&Team> {
        self.teams.values().find(|team| team.members.contains(name))
    }

    /// Имя команды участника (`/spreadplayers` с `respectTeams`).
    /// `Avatar.doTeamsAllowDamage`: союзник без friendly fire не получает удар.
    pub(crate) fn can_harm(&self, attacker: &str, target: &str) -> bool {
        let Some(team) = self.of_member(attacker) else {
            return true;
        };
        if !team.members.contains(target) {
            return true;
        }
        team.friendly_fire
    }

    /// `Entity.isAlliedTo`: одна команда (разящий удар союзников не задевает).
    pub(crate) fn allied(&self, a: &str, b: &str) -> bool {
        match (self.of_member(a), self.of_member(b)) {
            (Some(x), Some(y)) => x.name == y.name,
            _ => false,
        }
    }

    pub(crate) fn team_of(&self, member: &str) -> Option<String> {
        self.of_member(member).map(|team| team.name.clone())
    }
}

const COLORS: &[&str] = &[
    "black",
    "dark_blue",
    "dark_green",
    "dark_aqua",
    "dark_red",
    "dark_purple",
    "gold",
    "gray",
    "dark_gray",
    "blue",
    "green",
    "aqua",
    "red",
    "light_purple",
    "yellow",
    "white",
    "reset",
];

const VISIBILITY: &[&str] = &["always", "never", "hideForOtherTeams", "hideForOwnTeam"];
const COLLISION: &[&str] = &["always", "never", "pushOtherTeams", "pushOwnTeam"];

impl PlayPlan {
    /// Игроки онлайн из команды `name`. `None` — отправитель ни в одной команде.
    pub(crate) fn team_recipients(&self, name: &str) -> Option<Vec<Uuid>> {
        let members: Vec<String> = {
            let board = self.teams.lock().unwrap();
            board.of_member(name)?.members.iter().cloned().collect()
        };
        let players = self.hub.players.lock().unwrap();
        Some(
            members
                .iter()
                .filter_map(|member| {
                    players
                        .values()
                        .find(|player| player.entry.name == *member)
                        .map(|player| player.entry.uuid)
                })
                .collect(),
        )
    }

    /// `Player.canHarmPlayer`: союзник без friendly fire не получает удар.
    pub(crate) fn can_harm_player(&self, attacker: &str, target: &str) -> bool {
        self.teams.lock().unwrap().can_harm(attacker, target)
    }

    /// `Entity.isAlliedTo`: общая команда.
    pub(crate) fn allied(&self, a: &str, b: &str) -> bool {
        self.teams.lock().unwrap().allied(a, b)
    }
}

pub(super) fn specs() -> Vec<super::dispatch::Spec> {
    use super::dispatch::Spec;
    use super::registry::{GAMEMASTERS, boolean, component_parser, score_holder, word};

    let team_arg = |exec: bool| {
        let parser = super::registry::team();
        if exec {
            Spec::arg_leaf("team", GAMEMASTERS, parser, team)
        } else {
            Spec::arg("team", GAMEMASTERS, parser, Vec::new())
        }
    };
    let vis = |names: &[&str]| {
        names
            .iter()
            .map(|name| Spec::leaf(name, GAMEMASTERS, team))
            .collect()
    };
    let component_option = |group: &str, arg: &str| {
        Spec::group(
            group,
            GAMEMASTERS,
            vec![Spec::arg_leaf(arg, GAMEMASTERS, component_parser(), team)],
        )
    };
    vec![
        Spec::group(
            "team",
            GAMEMASTERS,
            vec![
                Spec::group(
                    "add",
                    GAMEMASTERS,
                    vec![
                        Spec::arg_leaf("team", GAMEMASTERS, word(), team).with_children(vec![
                            Spec::arg_leaf("displayName", GAMEMASTERS, component_parser(), team),
                        ]),
                    ],
                ),
                Spec::group("empty", GAMEMASTERS, vec![team_arg(true)]),
                Spec::group(
                    "join",
                    GAMEMASTERS,
                    vec![team_arg(true).with_children(vec![Spec::arg_leaf(
                        "members",
                        GAMEMASTERS,
                        score_holder(true),
                        team,
                    )])],
                ),
                Spec::group(
                    "leave",
                    GAMEMASTERS,
                    vec![Spec::arg_leaf(
                        "members",
                        GAMEMASTERS,
                        score_holder(true),
                        team,
                    )],
                ),
                Spec::exec("list", GAMEMASTERS, team, vec![team_arg(true)]),
                Spec::group(
                    "modify",
                    GAMEMASTERS,
                    vec![team_arg(false).with_children(vec![
                        Spec::group("collisionRule", GAMEMASTERS, vis(COLLISION)),
                        Spec::group(
                            "color",
                            GAMEMASTERS,
                            vec![
                                Spec::leaf("reset", GAMEMASTERS, team),
                                Spec::arg_leaf(
                                    "value",
                                    GAMEMASTERS,
                                    super::registry::team_color(),
                                    team,
                                ),
                            ],
                        ),
                        Spec::group("deathMessageVisibility", GAMEMASTERS, vis(VISIBILITY)),
                        component_option("displayName", "displayName"),
                        Spec::group(
                            "friendlyFire",
                            GAMEMASTERS,
                            vec![Spec::arg_leaf("allowed", GAMEMASTERS, boolean(), team)],
                        ),
                        Spec::group("nametagVisibility", GAMEMASTERS, vis(VISIBILITY)),
                        component_option("prefix", "prefix"),
                        Spec::group(
                            "seeFriendlyInvisibles",
                            GAMEMASTERS,
                            vec![Spec::arg_leaf("allowed", GAMEMASTERS, boolean(), team)],
                        ),
                        component_option("suffix", "suffix"),
                    ])],
                ),
                Spec::group("remove", GAMEMASTERS, vec![team_arg(true)]),
            ],
        )
        .with_handler(team),
    ]
}

fn team(ctx: &Context<'_>, call: &Call<'_>) -> Outcome {
    let sub = call.path.first().map(String::as_str).unwrap_or("");
    let mut input = call.t();
    match sub {
        "add" => add(ctx, &mut input),
        "remove" => remove(ctx, &mut input),
        "empty" => empty(ctx, &mut input),
        "join" => join(ctx, call, &mut input),
        "leave" => leave(ctx, call, &mut input),
        "list" => list(ctx, &mut input),
        "modify" => modify(ctx, &mut input),
        _ => usage("/team <add|remove|empty|join|leave|list|modify>"),
    }
}

fn add(ctx: &Context<'_>, input: &mut super::args::Input<'_>) -> Outcome {
    let Some(name) = input.word() else {
        return usage("/team add <team> [<displayName>]");
    };
    if !valid_name(name) {
        return Outcome::Feedback(translate(
            "commands.team.add.failed",
            vec![json!({"text": name})],
        ));
    }
    let display = input
        .rest()
        .trim()
        .is_empty()
        .then(|| json!({"text": name}))
        .or_else(|| component(input.greedy()));
    let Some(display) = display else {
        return Outcome::Feedback(translate("argument.component.invalid", Vec::new()));
    };
    let mut board = ctx.plan.teams.lock().unwrap();
    if board.teams.contains_key(name) {
        return Outcome::Feedback(translate(
            "commands.team.add.duplicate",
            vec![json!({"text": name})],
        ));
    }
    board
        .teams
        .insert(name.to_owned(), Team::new(name, display));
    Outcome::Feedback(translate(
        "commands.team.add.success",
        vec![json!({"text": name})],
    ))
}

fn remove(ctx: &Context<'_>, input: &mut super::args::Input<'_>) -> Outcome {
    let Some(name) = input.word() else {
        return usage("/team remove <team>");
    };
    if ctx.plan.teams.lock().unwrap().teams.remove(name).is_none() {
        return missing(name);
    }
    Outcome::Feedback(translate(
        "commands.team.remove.success",
        vec![json!({"text": name})],
    ))
}

fn empty(ctx: &Context<'_>, input: &mut super::args::Input<'_>) -> Outcome {
    let Some(name) = input.word() else {
        return usage("/team empty <team>");
    };
    let mut board = ctx.plan.teams.lock().unwrap();
    let Some(team) = board.teams.get_mut(name) else {
        return missing(name);
    };
    let count = team.members.len();
    team.members.clear();
    Outcome::Feedback(translate(
        "commands.team.empty.success",
        vec![Value::from(count as i32), json!({"text": name})],
    ))
}

fn join(ctx: &Context<'_>, call: &Call<'_>, input: &mut super::args::Input<'_>) -> Outcome {
    let Some(name) = input.word() else {
        return usage("/team join <team> [<members>]");
    };
    let Ok(members) = member_names(ctx, call, input.rest().trim()) else {
        return Outcome::Feedback(translate("argument.entity.notfound.entity", Vec::new()));
    };
    if members.is_empty() {
        return usage("/team join <team> [<members>]");
    }
    let mut board = ctx.plan.teams.lock().unwrap();
    if !board.teams.contains_key(name) {
        return missing(name);
    }
    for member in &members {
        for team in board.teams.values_mut() {
            team.members.remove(member);
        }
        if let Some(team) = board.teams.get_mut(name) {
            team.members.insert(member.clone());
        }
    }
    let key = if members.len() == 1 {
        "commands.team.join.success.single"
    } else {
        "commands.team.join.success.multiple"
    };
    Outcome::Feedback(translate(
        key,
        vec![json!({"text": members.join(", ")}), json!({"text": name})],
    ))
}

fn leave(ctx: &Context<'_>, call: &Call<'_>, input: &mut super::args::Input<'_>) -> Outcome {
    let Ok(members) = member_names(ctx, call, input.rest().trim()) else {
        return Outcome::Feedback(translate("argument.entity.notfound.entity", Vec::new()));
    };
    if members.is_empty() {
        return usage("/team leave [<members>]");
    }
    let mut board = ctx.plan.teams.lock().unwrap();
    let mut left = Vec::new();
    for member in &members {
        let was = board
            .teams
            .values()
            .any(|team| team.members.contains(member));
        for team in board.teams.values_mut() {
            team.members.remove(member);
        }
        if was {
            left.push(member.clone());
        }
    }
    if left.is_empty() {
        return Outcome::Feedback(translate(
            "commands.team.leave.failed",
            vec![json!({"text": members.join(", ")})],
        ));
    }
    let key = if left.len() == 1 {
        "commands.team.leave.success.single"
    } else {
        "commands.team.leave.success.multiple"
    };
    Outcome::Feedback(translate(key, vec![json!({"text": left.join(", ")})]))
}

fn list(ctx: &Context<'_>, input: &mut super::args::Input<'_>) -> Outcome {
    let board = ctx.plan.teams.lock().unwrap();
    if let Some(name) = input.word() {
        let Some(team) = board.teams.get(name) else {
            return missing(name);
        };
        let names: Vec<&str> = team.members.iter().map(String::as_str).collect();
        return Outcome::Feedback(translate(
            "commands.team.list.members.success",
            vec![
                json!({"text": name}),
                Value::from(names.len() as i32),
                json!({"text": names.join(", ")}),
            ],
        ));
    }
    let names: Vec<&str> = board.teams.keys().map(String::as_str).collect();
    Outcome::Feedback(translate(
        "commands.team.list.teams.success",
        vec![
            Value::from(names.len() as i32),
            json!({"text": names.join(", ")}),
        ],
    ))
}

fn modify(ctx: &Context<'_>, input: &mut super::args::Input<'_>) -> Outcome {
    let Some(name) = input.word() else {
        return usage("/team modify <team> <option> <value>");
    };
    let Some(option) = input.word() else {
        return usage("/team modify <team> <option> <value>");
    };
    let value = input.greedy();
    let mut board = ctx.plan.teams.lock().unwrap();
    let Some(team) = board.teams.get_mut(name) else {
        return missing(name);
    };
    let label = json!({"text": name});
    match option {
        "color" => {
            let color = value.trim();
            if !COLORS.contains(&color) {
                return usage("/team modify <team> color <color>");
            }
            team.color = color.to_owned();
            Outcome::Feedback(translate(
                "commands.team.option.color.success",
                vec![label, json!({"text": color})],
            ))
        }
        "displayName" => {
            let Some(text) = component(value) else {
                return Outcome::Feedback(translate("argument.component.invalid", Vec::new()));
            };
            team.display = text;
            Outcome::Feedback(translate(
                "commands.team.option.displayName.success",
                vec![label],
            ))
        }
        "prefix" | "suffix" => {
            let Some(text) = component(value) else {
                return Outcome::Feedback(translate("argument.component.invalid", Vec::new()));
            };
            if option == "prefix" {
                team.prefix = text;
            } else {
                team.suffix = text;
            }
            Outcome::Feedback(translate(
                &format!("commands.team.option.{option}.success"),
                vec![label],
            ))
        }
        "friendlyFire" | "seeFriendlyInvisibles" => {
            let Some(flag) = parse_bool(value.trim()) else {
                return usage("/team modify <team> <option> <true|false>");
            };
            if option == "friendlyFire" {
                team.friendly_fire = flag;
            } else {
                team.see_friendly_invisibles = flag;
            }
            let key = if flag { "enabled" } else { "disabled" };
            Outcome::Feedback(translate(
                &format!("commands.team.option.{option}.{key}"),
                vec![label],
            ))
        }
        "nametagVisibility" | "deathMessageVisibility" => {
            let value = value.trim();
            if !VISIBILITY.contains(&value) {
                return usage(
                    "/team modify <team> <visibility> <always|never|hideForOtherTeams|hideForOwnTeam>",
                );
            }
            if option == "nametagVisibility" {
                team.nametag = value.to_owned();
            } else {
                team.death_message = value.to_owned();
            }
            Outcome::Feedback(translate(
                &format!("commands.team.option.{option}.success"),
                vec![label, json!({"text": value})],
            ))
        }
        "collisionRule" => {
            let value = value.trim();
            if !COLLISION.contains(&value) {
                return usage(
                    "/team modify <team> collisionRule <always|never|pushOtherTeams|pushOwnTeam>",
                );
            }
            team.collision = value.to_owned();
            Outcome::Feedback(translate(
                "commands.team.option.collisionRule.success",
                vec![label, json!({"text": value})],
            ))
        }
        _ => usage("/team modify <team> <option> <value>"),
    }
}

/// Селектор, список имён или сам отправитель, если аргумента нет.
fn member_names(ctx: &Context<'_>, call: &Call<'_>, token: &str) -> Result<Vec<String>, ()> {
    if token.is_empty() {
        return if call.source.uuid.is_some() {
            Ok(vec![call.source.name.clone()])
        } else {
            Ok(Vec::new())
        };
    }
    if token.starts_with('@') {
        return ctx
            .select(call.source, token, false)
            .map(|list| list.into_iter().map(|target| target.name).collect())
            .map_err(|_| ());
    }
    Ok(token.split_whitespace().map(str::to_owned).collect())
}

fn missing(name: &str) -> Outcome {
    Outcome::Feedback(translate("team.notFound", vec![json!({"text": name})]))
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

fn parse_bool(token: &str) -> Option<bool> {
    match token {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn component(raw: &str) -> Option<Value> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if raw.starts_with('{') || raw.starts_with('[') || raw.starts_with('"') {
        return serde_json::from_str(raw).ok();
    }
    Some(json!({"text": raw}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board(team: &str, friendly: bool, members: &[&str]) -> Board {
        let mut board = Board::default();
        let mut t = Team::new(team, json!({"text": team}));
        t.friendly_fire = friendly;
        t.members = members.iter().map(|m| (*m).to_owned()).collect();
        board.teams.insert(team.to_owned(), t);
        board
    }

    #[test]
    fn friendly_fire_blocks_only_teammates() {
        let board = board("red", false, &["alice", "bob"]);
        assert!(!board.can_harm("alice", "bob"));
        assert!(board.can_harm("alice", "carol"));
        assert!(board.can_harm("carol", "dave"));
        assert!(board.allied("alice", "bob"));
        assert!(!board.allied("alice", "carol"));
    }

    #[test]
    fn friendly_fire_allows_teammate_damage() {
        let board = board("red", true, &["alice", "bob"]);
        assert!(board.can_harm("alice", "bob"));
        assert!(board.allied("alice", "bob"));
    }
}
