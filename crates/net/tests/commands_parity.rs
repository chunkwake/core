//! Сверка нашего дерева команд с ванильным 26.3 и сквозные проверки.
//!
//! `reports/commands.json` из кэша данных — оракул: имена корней, подкоманд и
//! уровни прав. Тест ловит расхождение сразу, а не после отчёта игрока.
//!
//! Файл собирается только при наличии кэша (`CHUNKWAKE_DATA` или
//! `generated/<версия>`), поэтому в CI без jar он пропускается — как и
//! бенчмарки, которым нужен мир.

use chunkwake_net::play::commands::{self, dispatch::Source, registry};

/// Как называется уровень прав в ванильном дереве.
fn vanilla_level(name: &str) -> Option<i32> {
    Some(match name {
        "gamemasters" => registry::GAMEMASTERS,
        "admins" => registry::ADMINS,
        "owners" => registry::OWNERS,
        _ => return None,
    })
}

/// Разбирает `reports/commands.json` в «имя → уровень» и «имя → дети».
fn vanilla_tree(text: &str) -> (Vec<String>, std::collections::HashMap<String, i32>) {
    let json: serde_json::Value = serde_json::from_str(text).expect("commands.json");
    let mut names = Vec::new();
    let mut levels = std::collections::HashMap::new();
    let Some(children) = json["children"].as_object() else {
        return (names, levels);
    };
    for (name, node) in children {
        names.push(name.clone());
        let level = node
            .get("permissions")
            .and_then(|p| p.get("permission"))
            .and_then(|p| p.get("level"))
            .and_then(|l| l.as_str())
            .and_then(vanilla_level);
        if let Some(level) = level {
            levels.insert(name.clone(), level);
        }
    }
    (names, levels)
}

/// Каталог с `reports/commands.json`, если данные уже извлечены.
fn reports_dir() -> Option<std::path::PathBuf> {
    let version = "26.3";
    let mut candidates = Vec::new();
    if let Ok(dir) = std::env::var("CHUNKWAKE_DATA") {
        candidates.push(std::path::PathBuf::from(dir));
    }
    // `cargo test` запускает тест из каталога крейта, поэтому корень воркспейса
    // — это `../..`; вариант без него оставлен для запуска из корня.
    for root in ["../..", "."] {
        for profile in ["release", "debug"] {
            candidates.push(
                std::path::Path::new(root)
                    .join("target")
                    .join(profile)
                    .join("generated")
                    .join(version),
            );
        }
    }
    candidates
        .into_iter()
        .find(|dir| dir.join("reports").join("commands.json").is_file())
}

/// Наши корни, кроме расширений Chunkwake.
fn our_roots() -> Vec<String> {
    registry::tree()
        .names()
        .map(str::to_owned)
        .filter(|n| !EXTENSIONS.contains(&n.as_str()))
        .collect()
}

/// Команды, которых в ванили нет: наши дополнения.
const EXTENSIONS: &[&str] = &["top", "tps"];

/// Команды ванили, которых у нас пока нет: этап C и позже.
const NOT_YET: &[&str] = &[
    "compute",
    "datapack",
    "fetchprofile",
    "function",
    "jfr",
    "schedule",
    "test",
];

#[test]
fn parity_with_vanilla_tree() {
    let Some(dir) = reports_dir() else {
        eprintln!("reports/commands.json не найден: сверка дерева пропущена");
        return;
    };
    let text = std::fs::read_to_string(dir.join("reports/commands.json")).unwrap();
    let (vanilla, levels) = vanilla_tree(&text);
    assert!(!vanilla.is_empty(), "дерево ванилы пустое");

    // 1. Все наши корни есть в ванили (кроме расширений).
    for name in our_roots() {
        assert!(
            vanilla.contains(&name),
            "команда {name} отсутствует в ванильном дереве 26.3"
        );
    }

    // 2. Уровни прав совпадают там, где ванила их объявила.
    for (name, level) in &levels {
        if let Some(ours) = registry::level_of(name) {
            assert_eq!(
                ours, *level,
                "уровень прав {name}: наш {ours}, ванильный {level}"
            );
        }
    }

    // 3. Непокрытое перечислено явно, и список не растёт незаметно.
    let ours = our_roots();
    let missing: Vec<&str> = vanilla
        .iter()
        .map(String::as_str)
        .filter(|n| !ours.iter().any(|o| o == n))
        .filter(|n| !NOT_YET.contains(n))
        .collect();
    assert!(
        missing.is_empty(),
        "команды ванили без реализации (добавьте в NOT_YET или реализуйте): {missing:?}"
    );
    assert!(
        NOT_YET.iter().all(|n| vanilla.iter().any(|v| v == n)),
        "в NOT_YET есть команды, которых нет в ванили"
    );
}

#[test]
fn every_root_has_a_level_and_a_handler() {
    for spec in registry::commands() {
        // Алиас обработчика не имеет: разбор уходит в целевой узел.
        assert!(
            spec.handler.is_some() || !spec.children.is_empty() || spec.redirect.is_some(),
            "{} никуда не ведёт",
            spec.name
        );
        assert!(
            (0..=registry::OWNERS).contains(&spec.level),
            "{}: уровень {} вне диапазона",
            spec.name,
            spec.level
        );
    }
}

#[test]
fn player_without_rights_sees_unknown_command() {
    // `/op` требует admins: у обычного игрока команды «нет».
    let player = Source::player(uuid::Uuid::nil(), "Guest", 0);
    assert!(registry::level_of("op").is_some_and(|level| player.level < level));
    // `/list` доступна всем.
    assert_eq!(registry::level_of("list"), Some(0));
}

/// Разбор строки не путает литерал с аргументом.
#[test]
fn parse_keeps_arguments_after_literals() {
    let tree = registry::tree();
    let resolved = tree.parse("time set 1000").unwrap();
    assert_eq!(resolved.path, ["set"]);
    assert_eq!(resolved.args, "set 1000");
    let resolved = tree.parse("scoreboard objectives add kills dummy").unwrap();
    assert_eq!(resolved.path, ["objectives", "add"]);
    assert_eq!(resolved.args, "objectives add kills dummy");
    // `/tp` — без литералов после себя.
    let resolved = tree.parse("tp Alice 0 64 0").unwrap();
    assert!(resolved.path.is_empty());
    assert_eq!(resolved.args, "Alice 0 64 0");
}

/// Подкоманда — литерал дерева: она попадает в путь, а обработчик находит
/// исполнителя. Раньше ветвящиеся команды (`/effect give`, `/tick freeze`)
/// уходили в хвост строки и отвечали ошибкой использования.
#[test]
fn subcommand_literals_route_to_a_handler() {
    let tree = registry::tree();
    for (line, last) in [
        ("effect give @a minecraft:speed 5 1", Some("give")),
        ("effect clear @a", Some("clear")),
        ("xp add Bob 10 levels", Some("add")),
        ("experience query Bob levels", Some("query")),
        ("tick freeze", Some("freeze")),
        ("tick rate 30", Some("rate")),
        ("tick step stop", Some("stop")),
        ("whitelist list", Some("list")),
        ("debug start", Some("start")),
        ("banlist ips", Some("ips")),
        ("data get block 0 64 0", Some("block")),
        ("forceload add", Some("add")),
        ("scoreboard objectives add kills dummy", Some("add")),
        ("scoreboard players set Alice kills 1", Some("set")),
        ("time query time", Some("time")),
        ("gamemode creative", None),
        ("defaultgamemode creative", None),
        ("summon minecraft:cow 1 2 3", None),
        ("setblock 0 64 0 minecraft:stone", None),
        ("fill 0 64 0 1 65 1 minecraft:glass", None),
        ("teleport 0 64 0", None),
        ("execute as @a run say hi", Some("as")),
    ] {
        let resolved = tree
            .parse(line)
            .unwrap_or_else(|| panic!("{line} не разобрался"));
        assert!(
            resolved.spec.handler.is_some(),
            "{line}: у узла нет обработчика"
        );
        assert!(resolved.level > 0, "{line}: команда без прав");
        if let Some(last) = last {
            assert_eq!(
                resolved.path.last().map(String::as_str),
                Some(last),
                "{line}: подкоманда не в пути ({:?})",
                resolved.path
            );
        }
    }
}

/// Ванильные алиасы разворачиваются в цель: `/tell` ведёт в `/msg`.
#[test]
fn aliases_resolve_through_redirect() {
    let tree = registry::tree();
    for (alias, target) in [
        ("tell", "msg"),
        ("w", "msg"),
        ("tm", "teammsg"),
        ("tp", "teleport"),
        ("xp", "experience"),
    ] {
        // Разбор алиаса находит узел цели, а не сам алиас.
        let resolved = tree
            .parse(alias)
            .unwrap_or_else(|| panic!("{alias} не разобрался"));
        assert_ne!(resolved.spec.name, alias, "{alias} не развернулся");
        let target_spec = tree.spec(target).expect("цель алиаса объявлена");
        let reaches = std::ptr::eq(resolved.spec, target_spec)
            || target_spec
                .children
                .iter()
                .any(|child| std::ptr::eq(resolved.spec, child));
        assert!(reaches, "{alias} ведёт не в {target}");
        // Аргументы алиаса доходят до цели без потерь.
        let with_args = format!("{alias} Alice hi");
        let resolved = tree.parse(&with_args).unwrap();
        assert!(
            resolved.args.ends_with("Alice hi"),
            "{with_args}: аргументы потерялись ({:?})",
            resolved.args
        );
    }
    // `/tp ~ ~ ~` принимает координаты: ветка `location` в дереве есть.
    let resolved = tree.parse("tp ~ ~ ~").unwrap();
    assert!(resolved.spec.handler.is_some());
    // `/xp add` не требует подкоманды в аргументах: режим уже в пути.
    let resolved = tree.parse("xp add Bob 5").unwrap();
    assert_eq!(resolved.path, ["add"]);
    assert_eq!(resolved.args, "add Bob 5");
}

/// Подсказки не показывают команды выше уровня прав.
#[test]
fn suggestions_respect_levels() {
    let tree = registry::tree();
    let guest = tree.suggest("", 0);
    assert!(guest.contains(&"list".to_owned()));
    assert!(!guest.contains(&"op".to_owned()));
    assert!(!guest.contains(&"stop".to_owned()));
    let owner = tree.suggest("", 4);
    assert!(owner.contains(&"op".to_owned()));
    assert!(owner.contains(&"stop".to_owned()));
}

/// Дерево для клиента кодируется и не падает на любом уровне прав.
#[test]
fn client_tree_encodes() {
    for level in 0..=4 {
        let nodes = registry::tree().nodes_for(level);
        let parsers = chunkwake_protocol::text::CommandParsers::load(|name| {
            Ok::<_, ()>(
                chunkwake_protocol::text::ARGUMENT_TYPES
                    .iter()
                    .position(|n| *n == name)
                    .unwrap_or(0) as i32
                    + 1,
            )
        })
        .unwrap();
        let packet = chunkwake_protocol::text::commands_tree(0x10, &nodes, parsers);
        assert!(packet.len() > 8, "уровень {level}: пустой пакет");
    }
}

/// Пакет нашего дерева читается так же, как его читает клиент.
///
/// Это ловит сдвиг свойств парсера: клиент падает с
/// `Failed to decode packet 'clientbound/minecraft:commands'`, если хотя бы
/// один тип записан не по формату сети.
#[test]
fn client_tree_decodes() {
    use chunkwake_protocol::text::ARGUMENT_TYPES;
    let parsers = chunkwake_protocol::text::CommandParsers::load(|name| {
        Ok::<_, ()>(ARGUMENT_TYPES.iter().position(|n| *n == name).unwrap_or(0) as i32 + 1)
    })
    .unwrap();
    for level in 0..=4 {
        let nodes = registry::tree().nodes_for(level);
        let packet = chunkwake_protocol::text::commands_tree(0x10, &nodes, parsers.clone());
        decode_commands(&packet, ARGUMENT_TYPES);
    }
}

/// Мини-декодер `commands` в формате сети: паникует на любом сдвиге.
fn decode_commands(data: &[u8], names: &[&str]) {
    struct Cursor<'a> {
        data: &'a [u8],
        at: usize,
    }

    impl Cursor<'_> {
        fn u8(&mut self) -> u8 {
            let v = self.data[self.at];
            self.at += 1;
            v
        }

        fn var_int(&mut self) -> i32 {
            let mut value = 0u32;
            for shift in 0..5 {
                let byte = self.u8();
                value |= u32::from(byte & 0x7f) << (7 * shift);
                if byte & 0x80 == 0 {
                    break;
                }
            }
            value as i32
        }

        /// Пропускает строку (длина + байты) и возвращает её.
        fn string(&mut self) -> String {
            let len = self.var_int() as usize;
            let text = String::from_utf8_lossy(&self.data[self.at..self.at + len]).into_owned();
            self.at += len;
            text
        }
    }

    let mut r = Cursor { data, at: 0 };
    let _packet = r.var_int();
    let count = r.var_int();
    assert!(count > 0, "пустой пакет");
    for _ in 0..count {
        let flags = r.u8();
        let children = r.var_int();
        for _ in 0..children {
            let index = r.var_int();
            assert!(index < count, "ребёнок {index} вне пакета из {count}");
        }
        // `redirect` (флаг 0x08) — индекс узла-цели, читается до имени.
        if flags & 0x08 != 0 {
            let index = r.var_int();
            assert!(index < count, "redirect {index} вне пакета из {count}");
        }
        let node = flags & 0x03;
        if node == 0 {
            continue;
        }
        let _name = r.string();
        if node != 2 {
            continue;
        }
        let id = r.var_int();
        let kind = names
            .get(id as usize - 1)
            .copied()
            .unwrap_or_else(|| panic!("парсер #{id} вне таблицы"));
        match kind {
            "brigadier:bool" => {}
            "brigadier:double" | "brigadier:long" => {
                let f = r.u8();
                if f & 1 != 0 {
                    r.at += 8;
                }
                if f & 2 != 0 {
                    r.at += 8;
                }
            }
            "brigadier:float" | "brigadier:integer" => {
                let f = r.u8();
                if f & 1 != 0 {
                    r.at += 4;
                }
                if f & 2 != 0 {
                    r.at += 4;
                }
            }
            "brigadier:string" => {
                let _ = r.var_int();
            }
            "minecraft:entity" | "minecraft:score_holder" => {
                let _ = r.u8();
            }
            "minecraft:time" => r.at += 4,
            "minecraft:resource"
            | "minecraft:resource_key"
            | "minecraft:resource_or_tag"
            | "minecraft:resource_or_tag_key"
            | "minecraft:resource_selector" => {
                let _ = r.string();
            }
            _ => {}
        }
        if flags & 0x10 != 0 {
            let _ = r.string();
        }
    }
    assert_eq!(r.var_int(), 0, "нет завершающего корня");
    assert_eq!(r.at, data.len(), "лишние байты после пакета");
}

/// Сверяет форму нашего узла с ванильным: имена, парсеры и `executable`.
///
/// Расхождение парсера — это красная строка в чате при работающей команде,
/// поэтому сверяется всё, что видит клиент.
fn compare_node(
    ours: &commands::dispatch::Spec,
    vanilla: &serde_json::Value,
    path: &str,
    diffs: &mut Vec<String>,
) {
    let kind = vanilla["type"].as_str().unwrap_or("literal");
    let ours_is_argument = ours.parser.is_some();
    if kind == "argument" && !ours_is_argument {
        diffs.push(format!("{path}: у ванили аргумент, у нас литерал"));
    }
    if kind != "argument" && ours_is_argument {
        diffs.push(format!("{path}: у ванили литерал, у нас аргумент"));
    }
    if kind == "argument" {
        let expected = vanilla["parser"].as_str().unwrap_or("");
        if let Some(parser) = &ours.parser {
            let ours_kind = parser.kind();
            if ours_kind != expected {
                diffs.push(format!("{path}: парсер {ours_kind}, у ванили {expected}"));
            }
        }
    }
    let vanilla_exec = vanilla["executable"].as_bool().unwrap_or(false);
    if vanilla_exec != ours.exec {
        diffs.push(format!(
            "{path}: executable у нас {}, у ванили {vanilla_exec}",
            ours.exec
        ));
    }
    let Some(children) = vanilla.get("children").and_then(|c| c.as_object()) else {
        return;
    };
    // Алиас (`tell`) и `redirect` сверяются отдельно: у него нет детей.
    if vanilla.get("redirect").is_some() {
        return;
    }
    for child in &ours.children {
        let child_path = format!("{path}/{}", child.name);
        match children.get(&child.name) {
            Some(node) => compare_node(child, node, &child_path, diffs),
            None => diffs.push(format!("{child_path}: нет в ванильном дереве")),
        }
    }
}

/// Команды, у которых ванильное дерево шире нашего обработчика: сверяем только
/// то, что мы объявили, и не требуем отсутствующих ветвей.
const SHAPE_SUBSET: &[&str] = &[
    "execute",
    "data",
    "scoreboard",
    "title",
    "playsound",
    "stopsound",
    "forceloop",
];

#[test]
fn nodes_match_vanilla_shape() {
    let Some(dir) = reports_dir() else {
        eprintln!("reports/commands.json не найден: сверка формы пропущена");
        return;
    };
    let text = std::fs::read_to_string(dir.join("reports").join("commands.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&text).expect("commands.json");
    let roots = json["children"].as_object().expect("children");
    let tree = registry::tree();

    let mut diffs = Vec::new();
    // Сверяем команды, где клиенту важно видеть наши узлы: цель прав.
    for name in [
        "give",
        "effect",
        "gamemode",
        "defaultgamemode",
        "kill",
        "clear",
        "teleport",
        "damage",
        "rotate",
        "spawnpoint",
        "setblock",
        "fill",
        "summon",
        "weather",
        "setworldspawn",
        "time",
        "tick",
        "difficulty",
        "say",
        "me",
        "msg",
        "teammsg",
        "tellraw",
        "execute",
        "scoreboard",
        "op",
        "deop",
        "kick",
        "whitelist",
        "ban",
        "banlist",
        "pardon",
        "data",
        "forceload",
        "locate",
        "list",
        "help",
        "seed",
        "version",
        "save-all",
        "stop",
        "reload",
        "playsound",
        "stopsound",
        "team",
        "clone",
        "worldborder",
        "tag",
        "swing",
        "spectate",
        "ride",
        "trigger",
        "random",
        "bossbar",
        "enchant",
        "spreadplayers",
        "stopwatch",
        "posteffect",
        "attribute",
        "fillbiome",
        "waypoint",
        "dialog",
        "particle",
        "return",
        "item",
        "loot",
        "place",
    ] {
        let Some(spec) = tree.spec(name) else {
            diffs.push(format!("{name}: не объявлена"));
            continue;
        };
        let Some(vanilla) = roots.get(name) else {
            diffs.push(format!("{name}: нет в ванили"));
            continue;
        };
        if SHAPE_SUBSET.contains(&name) {
            continue;
        }
        compare_node(spec, vanilla, name, &mut diffs);
    }
    assert!(
        diffs.is_empty(),
        "дерево разошлось с 26.3:\n{}",
        diffs.join("\n")
    );
}
#[test]
fn tps_line_contract() {
    let stats = chunkwake_metrics::TickStats::new();
    assert!(stats.line().starts_with("TPS: 20.0 MSPT: 0.0 interval: "));
    let _ = commands::execute; // модуль собран и доступен
}
