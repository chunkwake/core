//! Ванильные команды: дерево, разбор, права и обработчики.
//!
//! Команда объявляется один раз в [`registry`]: имя, уровень прав, узлы и
//! обработчик. Из этого же объявления собираются пакет `commands` для
//! клиента и подсказки `command_suggestions`, поэтому дерево, права и
//! выполнение не могут разойтись.
//!
//! Обработчики живут в подмодулях как `impl PlayPlan`: подмодуль `play`
//! видит приватные поля мира, а команды — не отдельная подсистема, а его
//! публичный фасад.

pub mod access;
pub mod admin;
pub mod args;
pub mod chat;
pub mod data;
pub mod dispatch;
pub mod entity;
pub mod execute;
pub mod meta;
pub mod net;
pub mod perf;
pub mod registry;
pub mod scoreboard;
pub mod selector;
pub mod state;
pub mod world;

use crate::play::{PlayPlan, SurvivalData};
use dispatch::{Call, Outcome, Source};
use selector::{Selector, Target};
use state::ServerVars;
use uuid::Uuid;

/// Доступ к миру для обработчиков команд.
pub struct Context<'a> {
    pub plan: &'a PlayPlan,
    /// Позиция выполнения: `/execute positioned … run` меняет её для вложенной
    /// команды, поэтому `~ ~ ~` считается от неё, а не от игрока.
    pub origin: Option<([f64; 3], f32, f32)>,
}

impl<'a> Context<'a> {
    pub fn new(plan: &'a PlayPlan) -> Self {
        Self {
            plan,
            origin: None,
        }
    }

    /// Тот же мир, но с точкой выполнения цепочки `/execute`.
    pub fn at(plan: &'a PlayPlan, pos: [f64; 3], yaw: f32, pitch: f32) -> Self {
        Self {
            plan,
            origin: Some((pos, yaw, pitch)),
        }
    }
}

/// Настройки сервера, которые нужны командам.
///
/// Отдельная структура, а не `Properties` из `chunkwake-server`: `net` не
/// зависит от бинарника, а полей немного.
pub struct PlanConfig<'a> {
    pub level_name: &'a str,
    pub motd: &'a str,
    pub seed: i64,
    pub version_name: &'a str,
    pub version_protocol: i32,
    /// Каталог сервера: там `ops.json` для `/reload`.
    pub server_dir: &'a std::path::Path,
    /// `white-list` из `server.properties`.
    pub whitelist: bool,
}

impl<'a> Context<'a> {
    /// Точка появления мира (`setworldspawn` важнее настроек).
    pub fn spawn(&self) -> [f64; 3] {
        self.plan.vars.world_spawn().unwrap_or(self.plan.spawn)
    }

    /// Позиция и поворот игрока по UUID.
    pub fn pose(&self, uuid: Uuid) -> Option<([f64; 3], f32, f32)> {
        self.plan.player_at(uuid)
    }

    /// Точка выполнения: цепочка `/execute` важнее позиции игрока.
    pub fn execution_origin(&self) -> Option<([f64; 3], f32, f32)> {
        self.origin
    }

    /// Все цели мира для селекторов: игроки, мобы, выпавшие предметы.
    pub fn targets(&self) -> Vec<Target> {
        self.plan.command_targets()
    }

    /// Цели по селектору: `@s`/`@p`/`@r`/`@a`/`@e` и имя.
    pub fn select(
        &self,
        source: &Source,
        token: &str,
        allow_entities: bool,
    ) -> Result<Vec<Target>, args::ArgError> {
        let selector = Selector::parse(token)?;
        if selector.entities && !allow_entities && selector.kind != selector::Kind::AllEntities {
            return Err(args::ArgError::new("argument.entity.invalid"));
        }
        let targets = self.targets();
        let me = source.uuid.and_then(|uuid| {
            targets
                .iter()
                .find(|t| t.uuid == uuid)
                .cloned()
                .or_else(|| {
                    // Игрок ещё не попал в снимок: берём его позицию из сессии.
                    self.pose(uuid).map(|(pos, yaw, pitch)| Target {
                        uuid,
                        name: source.name.clone(),
                        entity_id: -1,
                        pos,
                        yaw,
                        pitch,
                        game_mode: Some(0),
                        tags: Vec::new(),
                    })
                })
        });
        let out = selector.apply(me.as_ref(), &targets, |t| t.tags.clone())?;
        if out.is_empty() {
            return Err(args::ArgError::new("argument.entity.notfound.player"));
        }
        Ok(out)
    }

    /// Правила сервера.
    pub fn vars(&self) -> &ServerVars {
        &self.plan.vars
    }

    /// Данные выживания: реестры предметов, блоков, эффектов.
    pub fn survival(&self) -> &SurvivalData {
        &self.plan.survival
    }
}

/// Выполнение строки без `/`.
///
/// `None` — команда не наша: сессия отвечает «Unknown command» и отдаёт
/// строку плагинам.
pub fn execute(plan: &PlayPlan, source: &Source, line: &str) -> Option<Outcome> {
    execute_with(plan, source, line, None)
}

/// То же, но с точкой выполнения цепочки `/execute`.
pub fn execute_with(
    plan: &PlayPlan,
    source: &Source,
    line: &str,
    origin: Option<([f64; 3], f32, f32)>,
) -> Option<Outcome> {
    let tree = registry::tree();
    let resolved = tree.parse(line)?;
    if source.level < resolved.level {
        // Ванила прячет такие команды и отвечает как на неизвестную.
        return None;
    }
    let Some(handler) = resolved.spec.handler else {
        return Some(Outcome::error(
            "Incorrect argument for command at position 0: <--[HERE]",
        ));
    };
    let ctx = match origin {
        Some((pos, yaw, pitch)) => Context::at(plan, pos, yaw, pitch),
        None => Context::new(plan),
    };
    let call = Call {
        source,
        path: &resolved.path,
        rest: &resolved.args,
    };
    Some(handler(&ctx, &call))
}

/// Пакет `commands` для уровня прав: встроенные и команды плагинов.
pub fn commands_packet(plan: &PlayPlan, level: i32) -> Vec<u8> {
    let mut roots = registry::tree().nodes_for(level);
    roots.extend(plan.plugin_commands().iter().map(|n| {
        chunkwake_protocol::text::Node::literal(
            n,
            true,
            vec![chunkwake_protocol::text::Node::argument(
                "args",
                true,
                chunkwake_protocol::text::Parser::String(
                    chunkwake_protocol::text::StringMode::Greedy,
                ),
                vec![],
            )],
        )
    }));
    chunkwake_protocol::text::commands_tree(plan.commands_id(), &roots, plan.parsers().clone())
}

/// Подсказки встроенных команд и команд плагинов.
pub fn suggestions(plan: &PlayPlan, line: &str, level: i32) -> Vec<String> {
    let line = line.trim_start_matches('/');
    let mut out = registry::tree().suggest(line, level);
    // Имена плагинов — только на верхнем уровне.
    if !line.contains(char::is_whitespace) {
        out.extend(
            plan.plugin_commands()
                .iter()
                .filter(|n| n.starts_with(line))
                .cloned(),
        );
        out.sort();
        out.dedup();
    }
    out
}
