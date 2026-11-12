//! Единственное объявление команд: имя, уровень прав, узлы, обработчик.
//!
//! Из этой таблицы собираются пакет `commands` для клиента, подсказки
//! `command_suggestions` и права диспетчера — разойтись они не могут.
//!
//! Уровни прав взяты из ванильного дерева 26.3 (`reports/commands.json`):
//! `gamemasters` = 2, `admins` = 3, `owners` = 4. Команды без прав в ванили
//! (`list`, `me`, `msg`, `teammsg`) доступны всем.

use super::dispatch::{Handler, Spec};
use super::{
    admin, advancement, attribute, chat, data, dialog, entity, execute, extra, item_cmd, living,
    loot_cmd, meta, particle, place_cmd, recipe, ret, scoreboard, team as team_cmd, waypoint,
    world,
};
use chunkwake_protocol::text::Parser;
use std::sync::OnceLock;

/// `gamemasters` — уровень 2.
pub const GAMEMASTERS: i32 = 2;
/// `admins` — уровень 3.
pub const ADMINS: i32 = 3;
/// `owners` — уровень 4.
pub const OWNERS: i32 = 4;
/// Доступно всем.
pub const EVERYONE: i32 = 0;

/// Селектор целей (`minecraft:entity`).
pub fn ent(players_only: bool, single: bool) -> Parser {
    Parser::Entity {
        players_only,
        single,
    }
}

/// Одно слово.
pub fn word() -> Parser {
    Parser::String(chunkwake_protocol::text::StringMode::Word)
}

/// Остаток строки.
pub fn greedy() -> Parser {
    Parser::String(chunkwake_protocol::text::StringMode::Greedy)
}

/// Целое в диапазоне.
pub fn int(min: i32, max: i32) -> Parser {
    Parser::Integer(min, max)
}

/// `brigadier:bool`.
pub fn boolean() -> Parser {
    Parser::Bool
}

/// `minecraft:time`: значение в тиках с минимумом.
pub fn time(min: i32) -> Parser {
    Parser::Time(min)
}

/// `minecraft:gamemode`: имя или номер режима.
pub fn gamemode() -> Parser {
    Parser::Gamemode
}

/// `minecraft:game_profile`: имя игрока (в т.ч. офлайн, для `op`/`ban`).
pub fn game_profile() -> Parser {
    Parser::GameProfile
}

/// `minecraft:item_predicate`: предмет или тег.
pub fn item_predicate() -> Parser {
    Parser::ItemPredicate
}

/// `minecraft:damage_type` по реестру.
pub fn damage_type() -> Parser {
    Parser::Resource("minecraft:damage_type")
}

/// `minecraft:block_predicate`: блок или тег (`/fill … replace <filter>`).
pub fn block_predicate() -> Parser {
    Parser::BlockPredicate
}

/// `minecraft:column_pos`: координаты без Y (`/forceload`).
pub fn column_pos() -> Parser {
    Parser::ColumnPos
}

/// `minecraft:vec3`: позиция с плавающей точкой (`/tp`, `/summon`).
pub fn vec3() -> Parser {
    Parser::Vec3
}

/// `minecraft:entity_anchor`: `feet` или `eyes`.
pub fn anchor() -> Parser {
    Parser::Anchor
}

/// `minecraft:team`.
pub fn team() -> Parser {
    Parser::Team
}

/// `minecraft:team_color`.
pub fn team_color() -> Parser {
    Parser::TeamColor
}

/// `minecraft:score_holder` и `minecraft:objective` для `/scoreboard`.
pub fn score_holder(multiple: bool) -> Parser {
    Parser::ScoreHolder { multiple }
}

/// `minecraft:objective`.
pub fn objective() -> Parser {
    Parser::Objective
}

/// `minecraft:objective_criteria`.
pub fn objective_criteria() -> Parser {
    Parser::ObjectiveCriteria
}

/// `minecraft:scoreboard_slot`.
pub fn scoreboard_slot() -> Parser {
    Parser::ScoreboardSlot
}

/// `minecraft:nbt_path`: путь NBT (`Foo.Bar[0]`).
pub fn nbt_path() -> Parser {
    Parser::NbtPath
}

/// `minecraft:int_range`: `1`, `1..5`, `..5`, `1..`.
pub fn int_range() -> Parser {
    Parser::IntRange
}

/// `minecraft:swizzle`: оси (`xyz`, `xz`).
pub fn swizzle() -> Parser {
    Parser::Swizzle
}

/// `minecraft:heightmap`: `world_surface`, `motion_blocking`, …
pub fn heightmap() -> Parser {
    Parser::Heightmap
}

/// `minecraft:dimension`: ключ измерения.
pub fn dimension() -> Parser {
    Parser::Dimension
}

/// `minecraft:message`: свободный текст с подсказкой от сервера.
pub fn message() -> Parser {
    Parser::Message
}

/// `minecraft:nbt_compound_tag`: NBT в фигурных скобках (`/summon … {…}`).
pub fn nbt_compound() -> Parser {
    Parser::NbtCompoundTag
}

/// `minecraft:resource_location`.
pub fn resource_location() -> Parser {
    Parser::ResourceLocation
}

/// Аргумент с обработчиком на листе.
pub fn arg(name: &str, level: i32, parser: Parser, handler: Handler) -> Spec {
    Spec::arg_leaf(name, level, parser, handler)
}

/// `minecraft:item_stack`.
pub fn item_stack() -> Parser {
    Parser::ItemStack
}

/// `minecraft:resource` по реестру мобов.
pub fn mob_effect() -> Parser {
    Parser::Resource("minecraft:mob_effect")
}

/// Число с плавающей точкой (`brigadier:float`).
pub fn float(min: f64, max: f64) -> Parser {
    Parser::Float(min as f32, max as f32)
}

/// Число двойной точности (`brigadier:double`).
pub fn double(min: f64, max: f64) -> Parser {
    Parser::Double(min, max)
}

/// `minecraft:component`.
pub fn component_parser() -> Parser {
    Parser::Component
}

/// `minecraft:resource` по реестру звуков.
pub fn sound() -> Parser {
    Parser::Resource("minecraft:sound_event")
}

/// `minecraft:block_state`.
pub fn block_state() -> Parser {
    Parser::BlockState
}

/// `minecraft:block_pos`.
pub fn block_pos() -> Parser {
    Parser::BlockPos
}

/// `minecraft:loot_table`: ключ таблицы лута (`/loot`).
pub fn loot_table() -> Parser {
    Parser::LootTable
}

/// `minecraft:template_mirror`.
pub fn template_mirror() -> Parser {
    Parser::TemplateMirror
}

/// `minecraft:template_rotation`.
pub fn template_rotation() -> Parser {
    Parser::TemplateRotation
}

/// `minecraft:entity_type` через реестр сущностей.
pub fn entity_type() -> Parser {
    Parser::Resource("minecraft:entity_type")
}

/// `minecraft:rotation`.
pub fn rotation() -> Parser {
    Parser::Rotation
}

/// Все команды верхнего уровня.
pub fn commands() -> Vec<Spec> {
    let mut out = Vec::new();
    out.extend(entity::specs());
    out.extend(chat::specs());
    out.extend(world::specs());
    out.extend(meta::specs());
    out.extend(admin::specs());
    out.extend(scoreboard::specs());
    out.extend(team_cmd::specs());
    out.extend(data::specs());
    out.extend(execute::specs());
    out.extend(living::specs());
    out.extend(extra::specs());
    out.extend(attribute::specs());
    out.extend(waypoint::specs());
    out.extend(dialog::specs());
    out.extend(particle::specs());
    out.extend(ret::specs());
    out.extend(item_cmd::specs());
    out.extend(loot_cmd::specs());
    out.extend(place_cmd::specs());
    out.extend(advancement::specs());
    out.extend(recipe::specs());
    out
}
/// Таблица строится один раз: разбор идёт на каждую команду игрока.
pub fn tree() -> &'static super::dispatch::Tree {
    static TREE: OnceLock<super::dispatch::Tree> = OnceLock::new();
    TREE.get_or_init(|| super::dispatch::Tree::new(commands()))
}

/// Уровень прав команды по имени; `None` — не наша команда.
pub fn level_of(name: &str) -> Option<i32> {
    tree().level_of(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_duplicate_roots() {
        let mut names: Vec<String> = commands().iter().map(|s| s.name.clone()).collect();
        names.sort();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "дубли имён: {names:?}");
    }

    /// Уровни прав сверены с ванильным деревом 26.3.
    #[test]
    fn levels_match_vanilla() {
        for (name, level) in [
            ("op", ADMINS),
            ("deop", ADMINS),
            ("kick", ADMINS),
            ("whitelist", ADMINS),
            ("ban", ADMINS),
            ("tick", ADMINS),
            ("debug", ADMINS),
            ("setidletimeout", ADMINS),
            ("stop", OWNERS),
            ("save-all", OWNERS),
            ("perf", OWNERS),
            ("publish", OWNERS),
            ("unpublish", OWNERS),
            ("transfer", ADMINS),
            ("give", GAMEMASTERS),
            ("tp", GAMEMASTERS),
            ("setblock", GAMEMASTERS),
            ("gamerule", GAMEMASTERS),
            ("list", EVERYONE),
            ("me", EVERYONE),
            ("msg", EVERYONE),
            ("teammsg", EVERYONE),
            ("tps", EVERYONE),
        ] {
            assert_eq!(level_of(name), Some(level), "{name}");
        }
    }

    #[test]
    fn every_command_leads_somewhere() {
        for spec in commands() {
            assert!(
                spec.handler.is_some() || !spec.children.is_empty() || spec.redirect.is_some(),
                "{} никуда не ведёт",
                spec.name
            );
            // Алиас обязан вести в существующую команду, иначе клиент получит
            // `redirect` в никуда и разбор строки кончится ничем.
            if let Some(target) = &spec.redirect {
                assert!(
                    tree().spec(target).is_some(),
                    "{} перенаправляет в несуществующую {target}",
                    spec.name
                );
            }
        }
    }

    #[test]
    fn tree_is_cached() {
        assert!(std::ptr::eq(tree(), tree()));
    }

    /// Ни один узел не объявлен дважды внутри своей команды.
    #[test]
    fn children_are_unique() {
        fn walk(spec: &Spec, path: &str) {
            let mut seen = std::collections::HashSet::new();
            for child in &spec.children {
                assert!(
                    seen.insert(child.name.clone()),
                    "дубль {} в {}",
                    child.name,
                    path
                );
                walk(child, &format!("{path}/{}", child.name));
            }
        }
        for spec in commands() {
            walk(&spec, &spec.name);
        }
    }

    /// Дерево для клиента собирается для всех уровней прав.
    #[test]
    fn client_tree_builds_at_every_level() {
        for level in 0..=4 {
            let nodes = tree().nodes_for(level);
            assert!(!nodes.is_empty());
            for node in &nodes {
                assert!(matches!(
                    node,
                    chunkwake_protocol::text::Node::Literal { .. }
                ));
            }
        }
        assert!(tree().nodes_for(0).len() < tree().nodes_for(4).len());
    }
}
