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
pub mod attribute;
pub mod chat;
pub mod data;
pub mod dialog;
pub mod dispatch;
pub mod entity;
pub mod execute;
pub mod extra;
pub mod item;
pub mod item_cmd;
pub mod living;
pub mod loot_cmd;
pub mod meta;
pub mod net;
pub mod particle;
pub mod perf;
pub mod place_cmd;
pub mod registry;
pub mod ret;
pub mod scoreboard;
pub mod selector;
pub mod snbt;
pub mod state;
pub mod team;
pub mod waypoint;
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
        Self { plan, origin: None }
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
    /// Стартовое `pvp`, если в `gamerules.json` ключа ещё нет.
    pub pvp: bool,
    /// `allow-flight`: выживание и приключение тоже могут летать.
    pub allow_flight: bool,
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

    /// Первая цель по селектору или `None`.
    ///
    /// Для `limit=1` этого достаточно, а полный снимок мира (игроки, мобы,
    /// предметы) собирать незачем.
    pub fn select_first(
        &self,
        source: &Source,
        token: &str,
        allow_entities: bool,
    ) -> Option<Target> {
        let selector = Selector::parse(token).ok()?;
        if !selector.wants_one() {
            return None;
        }
        if selector.entities && !allow_entities && selector.kind != selector::Kind::AllEntities {
            return None;
        }
        if selector.kind() == selector::Kind::Self_ {
            return self.plan.command_target(source.uuid?);
        }
        // Игроки идут первыми в снимке, поэтому `@p` находит ближайшего,
        // но `sort=nearest` среди мобов без полного снимка не считается.
        if selector.kind() == selector::Kind::Nearest && selector.sort == selector::Sort::Nearest {
            let targets = self.targets();
            let me = source
                .uuid
                .and_then(|uuid| targets.iter().find(|t| t.uuid == uuid).cloned());
            return selector
                .apply(me.as_ref(), &targets, |t| t.tags.clone())
                .ok()
                .and_then(|list| list.into_iter().next());
        }
        let target = self.command_target_by_name(&selector.name)?;
        selector
            .apply(Some(&target), std::slice::from_ref(&target), |t| {
                t.tags.clone()
            })
            .ok()
            .and_then(|list| list.into_iter().next())
    }

    /// Цель по имени без полного снимка мира.
    fn command_target_by_name(&self, name: &str) -> Option<Target> {
        if name.is_empty() {
            return None;
        }
        let uuid = self.plan.player_uuid_by_name(name)?;
        self.plan.command_target(uuid)
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
    if let Some((registry, prefix)) = registry::tree().pending_resource(line, level)
        && registry == "minecraft:worldgen/structure"
    {
        out.extend(
            plan.structure_names
                .iter()
                .filter(|name| structure_match(name, prefix))
                .cloned(),
        );
        out.sort();
        out.dedup();
    }
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

/// `village` совпадает с `minecraft:village_plains`, `#vil` — с `#minecraft:village`.
fn structure_match(name: &str, prefix: &str) -> bool {
    if prefix.is_empty() {
        return true;
    }
    let prefix = prefix.to_ascii_lowercase();
    let name_l = name.to_ascii_lowercase();
    if name_l.starts_with(&prefix) {
        return true;
    }
    let tag = name_l.starts_with('#');
    if tag != prefix.starts_with('#') {
        return false;
    }
    let bare = name_l.trim_start_matches('#');
    let path = bare.split_once(':').map(|(_, p)| p).unwrap_or(bare);
    let typed = prefix.trim_start_matches('#');
    let typed = typed.split_once(':').map(|(_, p)| p).unwrap_or(typed);
    if typed.is_empty() {
        return tag;
    }
    path.starts_with(typed)
}
