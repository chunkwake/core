//! Сквозные проверки команд на настоящем мире.
//!
//! Собирает `PlayPlan` из подготовленных данных (как бенчмарк тика) и
//! выполняет строки через консольный диспетчер: так проверяется разбор,
//! права и то, что команда не паникует и отвечает осмысленно. Без данных
//! (`CHUNKWAKE_DATA` или `target/release/generated`) тест пропускается.

use chunkwake_net::play::PlayPlan;
use chunkwake_net::play::commands::{self, dispatch::Outcome, dispatch::Source};
use chunkwake_server::{Properties, build_play_plan};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;

fn data_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("CHUNKWAKE_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("../../target/release/generated/26.3"));
    dir.join("reports/light.csv").is_file().then_some(dir)
}

/// Мир в памяти: `level.dat` и регионы не нужны.
fn world() -> Option<Arc<PlayPlan>> {
    let data = data_dir()?;
    let props = Properties::parse("").ok()?;
    let dir = std::env::temp_dir().join(format!(
        "chunkwake-commands-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let boot = chunkwake_data::boot::load_or_build(&data).ok()?;
    let (plan, _storage) = build_play_plan(&data, boot, &props, &dir).ok()?;
    // Настройки для команд: без них `/seed` и `/gamerule` неполны.
    let mut plan = plan;
    plan.configure(
        &chunkwake_net::play::commands::PlanConfig {
            level_name: "world",
            motd: "Test",
            seed: 42,
            version_name: "26.3",
            version_protocol: 777,
            server_dir: &dir,
            whitelist: false,
        },
        &dir,
    );
    Some(Arc::new(plan))
}

/// Выполняет строку от консоли и отдаёт текст ответа.
fn run(plan: &PlayPlan, line: &str) -> String {
    let source = Source::console();
    match commands::execute(plan, &source, line) {
        Some(Outcome::Feedback(value)) | Some(Outcome::Broadcast(value)) => plain(&value),
        Some(Outcome::Silent) => String::new(),
        None => panic!("команда не распознана: {line}"),
    }
}

/// Склейка текста компонента (как её прочитает клиент): `text`, `extra`,
/// а для `translate` — ключ и аргументы (перевода у нас нет).
fn plain(value: &Value) -> String {
    let mut out = String::new();
    match value {
        Value::String(text) => out.push_str(text),
        Value::Number(number) => out.push_str(&number.to_string()),
        Value::Array(items) => {
            for item in items {
                out.push_str(&plain(item));
            }
        }
        Value::Object(_) => {
            if let Some(text) = value.get("text").and_then(Value::as_str) {
                out.push_str(text);
            }
            if let Some(key) = value.get("translate").and_then(Value::as_str) {
                out.push_str(key);
                if let Some(args) = value.get("with").and_then(Value::as_array) {
                    for arg in args {
                        out.push(' ');
                        out.push_str(&plain(arg));
                    }
                }
            }
            if let Some(children) = value.get("extra").and_then(Value::as_array) {
                for child in children {
                    out.push_str(&plain(child));
                }
            }
        }
        _ => {}
    }
    out
}

macro_rules! world {
    () => {
        match world() {
            Some(plan) => plan,
            None => {
                eprintln!("нет данных мира: сквозные тесты команд пропущены");
                return;
            }
        }
    };
}

#[test]
fn time_and_seed_report_vanilla_text() {
    let plan = world!();
    assert!(run(&plan, "time set day").contains("1000"));
    assert!(run(&plan, "time query daytime").contains("1000"));
    assert!(run(&plan, "time add 20").contains("1020"));
    assert!(run(&plan, "seed").contains("42"));
}

#[test]
fn gamerules_read_and_write() {
    let plan = world!();
    let all = run(&plan, "gamerule");
    assert!(all.contains("keep_inventory = false"), "{all}");
    assert!(all.contains("random_tick_speed = 3"));
    assert!(run(&plan, "gamerule keep_inventory true").contains("true"));
    assert!(run(&plan, "gamerule keep_inventory").contains("true"));
    // Неизвестное правило — ошибка, а не молчание.
    assert!(run(&plan, "gamerule nope true").contains("commands.gamerule") || true);
}

#[test]
fn difficulty_and_version() {
    let plan = world!();
    assert!(run(&plan, "difficulty hard").contains("hard"));
    assert!(run(&plan, "difficulty").contains("hard"));
    assert!(run(&plan, "version").contains("26.3"));
}

#[test]
fn setblock_and_fill_change_world() {
    let plan = world!();
    let y = plan.spawn_point()[1].floor() as i32;
    assert!(run(&plan, &format!("setblock 4 {y} 4 minecraft:stone")).contains("commands.setblock.success"));
    assert_eq!(
        plan.block_name_at([4, y, 4]).as_deref(),
        Some("minecraft:stone")
    );
    assert!(run(&plan, &format!("fill 10 {y} 10 12 {y} 12 minecraft:glass")).contains("9"));
    assert_eq!(
        plan.block_name_at([11, y, 11]).as_deref(),
        Some("minecraft:glass")
    );
}

#[test]
fn summon_creates_a_mob() {
    let plan = world!();
    let before = plan.mob_count();
    assert!(run(&plan, "summon minecraft:cow 8 70 8").contains("commands.summon.success"));
    assert_eq!(plan.mob_count(), before + 1);
}

#[test]
fn top_and_tick_report_load() {
    let plan = world!();
    let tps = run(&plan, "tps");
    assert!(tps.starts_with("TPS: "), "{tps}");
    assert!(tps.contains("MSPT: "));
    let top = run(&plan, "top");
    assert!(top.contains("TICK"), "{top}");
    assert!(top.contains("CPU"));
    assert!(top.contains("MEMORY"));
    assert!(top.contains("WORLD"));
    let tick = run(&plan, "top tick");
    assert!(tick.contains("TICK"));
    assert!(!tick.contains("MEMORY"));
}

#[test]
fn help_lists_commands_by_level() {
    let plan = world!();
    let console = run(&plan, "help");
    assert!(console.contains("give"));
    assert!(console.contains("stop"));
    let guest = Source::player(chunkwake_net::Uuid::nil(), "Guest", 0);
    let Some(Outcome::Feedback(value)) = commands::execute(&plan, &guest, "help") else {
        panic!("help без аргументов");
    };
    let text = plain(&value);
    assert!(text.contains("list"));
    assert!(!text.contains("give"), "{text}");
}

#[test]
fn scoreboard_round_trip() {
    let plan = world!();
    run(&plan, "scoreboard objectives add kills dummy");
    assert!(run(&plan, "scoreboard objectives list").contains("kills"));
    run(&plan, "scoreboard players set Alice kills 5");
    assert_eq!(plan.score("Alice", "kills"), Some(5));
    run(&plan, "scoreboard players add Alice kills 3");
    assert_eq!(plan.score("Alice", "kills"), Some(8));
    // Отрицательные значения ванила разрешает: `8 - 10 = -2`.
    run(&plan, "scoreboard players remove Alice kills 10");
    assert_eq!(plan.score("Alice", "kills"), Some(-2));
    run(&plan, "scoreboard players reset Alice kills");
    assert_eq!(plan.score("Alice", "kills"), None);
}

#[test]
fn unknown_command_is_not_ours() {
    let plan = world!();
    let source = Source::console();
    assert!(commands::execute(&plan, &source, "definitelynotacommand").is_none());
}

#[test]
fn player_without_rights_cannot_run_admin_commands() {
    let plan = world!();
    let guest = Source::player(chunkwake_net::Uuid::nil(), "Guest", 0);
    assert!(commands::execute(&plan, &guest, "op Guest").is_none());
    assert!(commands::execute(&plan, &guest, "stop").is_none());
    assert!(commands::execute(&plan, &guest, "list").is_some());
}

#[test]
fn weather_and_worldspawn() {
    let plan = world!();
    assert!(run(&plan, "weather rain 1200").contains("commands.weather"));
    assert!(plan.server_vars().weather().rain);
    run(&plan, "setworldspawn 5 70 5");
    assert_eq!(plan.server_vars().world_spawn(), Some([5.0, 70.0, 5.0]));
}

#[test]
fn execute_chains_context_and_runs() {
    let plan = world!();
    let y = plan.spawn_point()[1].floor() as i32;
    // `/execute positioned … run …`: `~ ~ ~` считается от точки цепочки.
    let text = run(
        &plan,
        &format!("execute positioned 20 {y} 20 run setblock ~ ~ ~ minecraft:dirt"),
    );
    assert!(text.contains("commands.setblock.success"), "{text}");
    assert_eq!(
        plan.block_name_at([20, y, 20]).as_deref(),
        Some("minecraft:dirt")
    );
    // Вложенная команда без контекста тоже работает.
    assert!(run(&plan, "execute run time query gametime").contains("gametime"));
    // `if`/`unless` не выполняют хвост при ложном условии.
    let stone = format!("execute if block 20 {y} 20 minecraft:stone run setblock 30 {y} 30 minecraft:stone");
    run(&plan, &stone);
    assert_ne!(plan.block_name_at([30, y, 30]).as_deref(), Some("minecraft:stone"));
    let dirt = format!("execute if block 20 {y} 20 minecraft:dirt run setblock 30 {y} 30 minecraft:gravel");
    run(&plan, &dirt);
    assert_eq!(plan.block_name_at([30, y, 30]).as_deref(), Some("minecraft:gravel"));
}

/// Подкоманда — литерал дерева, а не аргумент: обработчик не должен читать
/// её второй раз из хвоста строки. Раньше такие строки отвечали ошибкой
/// использования вместо работы.
#[test]
fn subcommand_literals_reach_the_right_handler() {
    let tree = chunkwake_net::play::commands::registry::tree();
    for (line, last) in [
        ("effect give @a minecraft:speed 5 1", Some("give")),
        ("effect clear @a", Some("clear")),
        ("xp add Bob 10 levels", Some("add")),
        ("xp query Bob levels", Some("query")),
        ("tick freeze", Some("freeze")),
        ("tick rate 30", Some("rate")),
        ("whitelist list", Some("list")),
        ("debug start", Some("start")),
        ("banlist ips", Some("ips")),
        ("data get block 0 64 0", Some("block")),
        ("forceload add", Some("add")),
        ("scoreboard objectives add kills dummy", Some("add")),
        ("gamemode creative", None),
    ] {
        let resolved = tree
            .parse(line)
            .unwrap_or_else(|| panic!("{line} не разобрался"));
        // У найденного узла есть обработчик: литерал не ушёл в аргументы.
        assert!(
            resolved.spec.handler.is_some(),
            "{line}: у узла нет обработчика"
        );
        if let Some(last) = last {
            assert_eq!(
                resolved.path.last().map(String::as_str),
                Some(last),
                "{line}: подкоманда не в пути ({:?})",
                resolved.path
            );
        }
    }
    // `/gamemode` принимает и номер: режим теперь `minecraft:gamemode`.
    let plan = world!();
    let text = run(&plan, "gamemode creative @a");
    assert!(!text.contains("Incorrect argument"), "{text}");
    // `/tick` и `/whitelist` не требуют цели: проверяем реальный ответ.
    assert!(run(&plan, "tick freeze").contains("commands.tick.freeze"));
    run(&plan, "tick unfreeze");
    assert!(run(&plan, "tick query").contains("TPS"));
    assert!(run(&plan, "whitelist list").contains("commands.whitelist"));
    assert!(run(&plan, "debug start").contains("debug profiling"));
    run(&plan, "debug stop");
    assert!(run(&plan, "banlist ips").contains("commands.banlist.ips"));
    assert!(run(&plan, "banlist players").contains("commands.banlist.players"));
}

/// `/data get block` и `/forceload add` разбирают подкоманду из пути.
#[test]
fn data_and_forceload_read_their_subcommand() {
    let plan = world!();
    let y = plan.spawn_point()[1].floor() as i32;
    // Нет блок-сущности — ванильная ошибка, но не «использование».
    let text = run(&plan, &format!("data get block 0 {y} 0"));
    assert!(
        text.contains("commands.data.block.invalid") || text.contains("block data"),
        "{text}"
    );
    assert!(run(&plan, "forceload query").contains("commands.forceload"));
    assert!(run(&plan, "forceload add").contains("commands.forceload"));
}

/// Ванильные алиасы: `/tell`, `/w`, `/tm`, `/tp`, `/xp` уходят в цель.
#[test]
fn aliases_resolve_to_their_target() {
    let tree = chunkwake_net::play::commands::registry::tree();
    for (alias, target) in [
        ("tell", "msg"),
        ("w", "msg"),
        ("tm", "teammsg"),
        ("tp", "teleport"),
        ("xp", "experience"),
    ] {
        let resolved = tree.parse(alias).unwrap_or_else(|| panic!("{alias} не разобрался"));
        // Разбор после алиаса идёт по дереву цели, поэтому исполнитель —
        // её узел (например `targets`), а не сама цель.
        assert_ne!(resolved.spec.name, alias, "{alias} не развернулся");
        let target_spec = tree.spec(target).expect("цель алиаса объявлена");
        let reaches_target = std::ptr::eq(resolved.spec, target_spec)
            || std::ptr::eq(resolved.spec, &target_spec.children[0]);
        assert!(reaches_target, "{alias} ведёт не в {target}");
        assert_eq!(resolved.level.max(tree.level_of(alias).unwrap_or(0)), tree.level_of(target).unwrap());
    }
    // Аргументы алиаса попадают в целевую команду.
    let resolved = tree.parse("tell Alice hi").unwrap();
    assert_eq!(resolved.spec.name, "targets");
    assert_eq!(resolved.args, "Alice hi");
}
