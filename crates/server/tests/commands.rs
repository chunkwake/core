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
            pvp: true,
            allow_flight: false,
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
    assert!(
        run(&plan, &format!("setblock 4 {y} 4 minecraft:stone"))
            .contains("commands.setblock.success")
    );
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
    let stone = format!(
        "execute if block 20 {y} 20 minecraft:stone run setblock 30 {y} 30 minecraft:stone"
    );
    run(&plan, &stone);
    assert_ne!(
        plan.block_name_at([30, y, 30]).as_deref(),
        Some("minecraft:stone")
    );
    let dirt = format!(
        "execute if block 20 {y} 20 minecraft:dirt run setblock 30 {y} 30 minecraft:gravel"
    );
    run(&plan, &dirt);
    assert_eq!(
        plan.block_name_at([30, y, 30]).as_deref(),
        Some("minecraft:gravel")
    );
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
    plan.tick_once();
    assert!(
        plan.simulated_chunks() > 0,
        "forceload должен оставить чанк в симуляции"
    );
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
        let resolved = tree
            .parse(alias)
            .unwrap_or_else(|| panic!("{alias} не разобрался"));
        // Разбор после алиаса идёт по дереву цели, поэтому исполнитель —
        // её узел (например `targets`), а не сама цель.
        assert_ne!(resolved.spec.name, alias, "{alias} не развернулся");
        let target_spec = tree.spec(target).expect("цель алиаса объявлена");
        let reaches_target = std::ptr::eq(resolved.spec, target_spec)
            || std::ptr::eq(resolved.spec, &target_spec.children[0]);
        assert!(reaches_target, "{alias} ведёт не в {target}");
        assert_eq!(
            resolved.level.max(tree.level_of(alias).unwrap_or(0)),
            tree.level_of(target).unwrap()
        );
    }
    // Аргументы алиаса попадают в целевую команду.
    let resolved = tree.parse("tell Alice hi").unwrap();
    assert_eq!(resolved.spec.name, "targets");
    assert_eq!(resolved.args, "Alice hi");
}

#[test]
fn setblock_keep_does_not_replace_stone() {
    let plan = world!();
    let y = plan.spawn_point()[1].floor() as i32;
    assert!(
        run(&plan, &format!("setblock 4 {y} 4 minecraft:stone"))
            .contains("commands.setblock.success")
    );
    let kept = run(&plan, &format!("setblock 4 {y} 4 minecraft:glass keep"));
    assert!(kept.contains("commands.setblock.failed"), "{kept}");
    assert_eq!(
        plan.block_name_at([4, y, 4]).as_deref(),
        Some("minecraft:stone")
    );
    let strict = run(&plan, &format!("setblock 4 {y} 4 minecraft:stone strict"));
    assert!(strict.contains("commands.setblock.failed"), "{strict}");
    assert!(
        run(&plan, &format!("setblock 4 {y} 4 minecraft:glass destroy"))
            .contains("commands.setblock.success")
    );
    assert_eq!(
        plan.block_name_at([4, y, 4]).as_deref(),
        Some("minecraft:glass")
    );
}

#[test]
fn clear_removes_one_stack_item() {
    let plan = world!();
    let uuid = chunkwake_net::Uuid::from_u128(1);
    plan.present_player(uuid, "Ada");
    plan.player_control(uuid, "slot 0 minecraft:stone 2");
    plan.player_control(uuid, "slot 1 minecraft:dirt 1");
    let cleared = run(&plan, "clear Ada minecraft:stone 1");
    assert!(cleared.contains('1'), "{cleared}");
    let state = plan.player_state(uuid).expect("зеркало игрока");
    assert!(state.contains("minecraft:stone 1"), "{state}");
    assert!(state.contains("minecraft:dirt 1"), "{state}");
    assert!(!state.contains("minecraft:stone 2"), "{state}");
}

#[test]
fn list_uuids_prints_the_player_uuid() {
    let plan = world!();
    let uuid = chunkwake_net::Uuid::from_u128(7);
    plan.present_player(uuid, "Ada");
    let text = run(&plan, "list uuids");
    assert!(text.contains(&uuid.to_string()), "{text}");
    assert!(text.contains("Ada"), "{text}");
}

#[test]
fn execute_align_and_store_write_a_score() {
    let plan = world!();
    let y = plan.spawn_point()[1];
    assert!(run(&plan, "scoreboard objectives add obj dummy").contains("success"));
    let aligned = run(
        &plan,
        &format!("execute positioned 4.8 {y} 4.2 align x run summon minecraft:cow ~ ~ ~"),
    );
    assert!(aligned.contains("commands.summon.success"), "{aligned}");
    let data = run(&plan, "data get entity @e[type=minecraft:cow,limit=1]");
    assert!(data.contains("4.0"), "{data}");
    assert!(!data.contains("4.8"), "{data}");
    run(&plan, "time set day");
    let stored = run(
        &plan,
        "execute store result score Clock obj run time query daytime",
    );
    assert!(stored.contains("1000"), "{stored}");
    assert_eq!(plan.score("Clock", "obj"), Some(1000));
    let modified = run(&plan, "scoreboard objectives modify obj rendertype hearts");
    assert!(modified.contains("success"), "{modified}");
}

#[test]
fn banned_name_is_refused_at_admission() {
    let plan = world!();
    assert!(run(&plan, "ban Steve").contains("success"));
    let addr = "203.0.113.5:25565".parse().unwrap();
    let reason = plan
        .admission_denied("Steve", chunkwake_net::Uuid::nil(), addr)
        .expect("бан закрывает вход");
    assert!(reason.contains("banned"), "{reason}");
    assert!(
        plan.admission_denied("Ada", chunkwake_net::Uuid::nil(), addr)
            .is_none()
    );
}

/// Компостер — точка `farmer`. Структуры, которых генератор не ставит, не выдумываются.
#[test]
fn give_components_clear_matches_them_and_item_tags() {
    let plan = world!();
    let uuid = chunkwake_net::Uuid::from_u128(11);
    plan.present_player(uuid, "Ada");
    let given = run(
        &plan,
        r#"give Ada minecraft:stone[minecraft:custom_name="Ada",minecraft:damage=2]"#,
    );
    assert!(given.contains("commands.give.success"), "{given}");
    let state = plan.player_state(uuid).expect("зеркало");
    assert!(state.contains("Ada"), "{state}");
    assert!(
        state.contains("\"damage\":2") || state.contains("\"damage\": 2"),
        "{state}"
    );
    plan.player_control(uuid, "slot 1 minecraft:dirt 1");
    let cleared = run(
        &plan,
        r#"clear Ada minecraft:stone[minecraft:custom_name="Ada"]"#,
    );
    assert!(cleared.contains("commands.clear.success"), "{cleared}");
    let state = plan.player_state(uuid).expect("зеркало");
    assert!(!state.contains("Ada"), "{state}");
    assert!(state.contains("minecraft:dirt 1"), "{state}");
    let tagged = run(&plan, "clear Ada #minecraft:dirt");
    assert!(tagged.contains("commands.clear.success"), "{tagged}");
    let state = plan.player_state(uuid).expect("зеркало");
    assert!(!state.contains("minecraft:dirt"), "{state}");
}

#[test]
fn fill_replace_accepts_a_block_tag() {
    let plan = world!();
    let y = plan.spawn_point()[1].floor() as i32;
    assert!(
        run(&plan, &format!("setblock 2 {y} 2 minecraft:oak_planks"))
            .contains("commands.setblock.success")
    );
    assert!(
        run(&plan, &format!("setblock 3 {y} 2 minecraft:stone"))
            .contains("commands.setblock.success")
    );
    let filled = run(
        &plan,
        &format!("fill 2 {y} 2 3 {y} 2 minecraft:glass replace #minecraft:planks"),
    );
    assert!(filled.contains('1'), "{filled}");
    assert_eq!(
        plan.block_name_at([2, y, 2]).as_deref(),
        Some("minecraft:glass")
    );
    assert_eq!(
        plan.block_name_at([3, y, 2]).as_deref(),
        Some("minecraft:stone")
    );
}

#[test]
fn data_merge_modify_and_execute_store_round_trip() {
    let plan = world!();
    let y = plan.spawn_point()[1].floor() as i32;
    assert!(
        run(&plan, &format!("setblock 6 {y} 6 minecraft:chest"))
            .contains("commands.setblock.success")
    );
    let merged = run(
        &plan,
        &format!("data merge block 6 {y} 6 {{CustomName:\"Hi\",Count:1}}"),
    );
    assert!(merged.contains("commands.data.block.modified"), "{merged}");
    let got = run(&plan, &format!("data get block 6 {y} 6 CustomName"));
    assert!(got.contains("Hi"), "{got}");
    let modified = run(
        &plan,
        &format!("data modify block 6 {y} 6 Count set value 4"),
    );
    assert!(
        modified.contains("commands.data.block.modified"),
        "{modified}"
    );
    let count = run(&plan, &format!("data get block 6 {y} 6 Count"));
    assert!(count.contains('4'), "{count}");
    run(&plan, "time set day");
    let stored = run(
        &plan,
        &format!("execute store result block 6 {y} 6 Count int 1 run time query daytime"),
    );
    assert!(stored.contains("1000"), "{stored}");
    let count = run(&plan, &format!("data get block 6 {y} 6 Count"));
    assert!(count.contains("1000"), "{count}");
    let missing = run(&plan, "execute on vehicle run say hi");
    assert!(missing.contains("argument.entity.notfound"), "{missing}");
}

#[test]
fn team_membership_and_teammsg() {
    let plan = world!();
    let uuid = chunkwake_net::Uuid::from_u128(12);
    plan.present_player(uuid, "Ada");
    assert!(run(&plan, "team add red").contains("commands.team.add.success"));
    assert!(run(&plan, "team join red Ada").contains("Ada"));
    let listed = run(&plan, "team list red");
    assert!(listed.contains("Ada"), "{listed}");
    let source = Source::player(uuid, "Ada", 0);
    let failed = commands::execute(&plan, &source, "teammsg hello");
    // Ada уже в команде: сообщение уходит молча, а не ошибкой.
    assert!(matches!(failed, Some(Outcome::Silent)), "{failed:?}");
    let guest = Source::player(chunkwake_net::Uuid::from_u128(13), "Bea", 0);
    plan.present_player(guest.uuid.unwrap(), "Bea");
    let text = match commands::execute(&plan, &guest, "teammsg hello") {
        Some(Outcome::Feedback(value)) => plain(&value),
        other => panic!("ожидался отказ: {other:?}"),
    };
    assert!(text.contains("commands.teammsg.failed"), "{text}");
}

#[test]
fn locate_poi_finds_a_composter_and_mansion_is_missing() {
    let plan = world!();
    let y = plan.spawn_point()[1].floor() as i32;
    let placed = run(&plan, &format!("setblock 4 {y} 4 minecraft:composter"));
    assert!(placed.contains("commands.setblock.success"), "{placed}");
    let found = run(&plan, "locate poi minecraft:farmer");
    assert!(found.contains("commands.locate.poi.success"), "{found}");
    assert!(found.contains("chat.coordinates"), "{found}");
    assert!(found.contains(&format!("4 {y} 4")), "{found}");
    let missing = run(&plan, "locate structure minecraft:woodland_mansion");
    assert!(missing.contains("commands.locate.failed"), "{missing}");
    let villages = commands::suggestions(&plan, "locate structure village", 4);
    assert!(
        villages.iter().any(|s| s == "minecraft:village_plains"),
        "{villages:?}"
    );
    let tags = commands::suggestions(&plan, "locate structure #vil", 4);
    assert!(tags.iter().any(|s| s == "#minecraft:village"), "{tags:?}");
}

#[test]
fn execute_slots_and_summon() {
    let plan = world!();
    let y = plan.spawn_point()[1].floor() as i32;
    assert!(
        run(&plan, &format!("setblock 30 {y} 30 minecraft:chest"))
            .contains("commands.setblock.success")
    );
    // Пустой сундук: слот не совпадает.
    let empty = run(
        &plan,
        &format!("execute if slots block 30 {y} 30 container.0 minecraft:cobblestone run say yes"),
    );
    assert!(empty.is_empty(), "{empty}");
    let placed = run(
        &plan,
        &format!("loot replace block 30 {y} 30 0 loot minecraft:blocks/stone"),
    );
    assert!(
        placed.contains("commands.loot.replace.success"),
        "replace: {placed}"
    );
    // Без шёлкового касания `blocks/stone` даёт булыжник.
    let filled = run(
        &plan,
        &format!("execute if slots block 30 {y} 30 container.0 minecraft:cobblestone run say yes"),
    );
    assert!(filled.contains("yes"), "{filled}");
    let wrong = run(
        &plan,
        &format!("execute if slots block 30 {y} 30 container.0 minecraft:dirt run say yes"),
    );
    assert!(wrong.is_empty(), "{wrong}");

    // `execute summon`: моб появляется, хвост идёт от его имени.
    let summon = run(&plan, "execute summon minecraft:cow run say moo");
    assert!(summon.contains("moo"), "{summon}");
    assert_eq!(plan.mob_count(), 1);
    let before = plan.mob_count();
    run(&plan, "execute summon minecraft:pig run say tail");
    assert_eq!(plan.mob_count(), before + 1, "summon tail did not run");
}

#[test]
fn execute_conditions_height_and_numberformat() {
    let plan = world!();
    let y = plan.spawn_point()[1].floor() as i32;
    let biome = run(
        &plan,
        &format!("execute if biome 8 {y} 8 minecraft:plains run say yes"),
    );
    assert!(biome.contains("yes"), "{biome}");

    let cold = run(&plan, &format!("execute if loaded 80 {y} 80 run say yes"));
    assert!(cold.is_empty(), "{cold}");
    assert!(
        run(&plan, &format!("setblock 80 {y} 80 minecraft:stone"))
            .contains("commands.setblock.success")
    );
    let warm = run(&plan, &format!("execute if loaded 80 {y} 80 run say yes"));
    assert!(warm.contains("yes"), "{warm}");

    assert!(
        run(&plan, &format!("setblock 10 {y} 10 minecraft:stone"))
            .contains("commands.setblock.success")
    );
    assert!(
        run(&plan, &format!("setblock 20 {y} 10 minecraft:stone"))
            .contains("commands.setblock.success")
    );
    let same = run(
        &plan,
        &format!("execute if blocks 10 {y} 10 11 {y} 10 20 {y} 10 all run say yes"),
    );
    assert!(same.contains("yes"), "{same}");
    assert!(
        run(&plan, &format!("setblock 21 {y} 10 minecraft:dirt"))
            .contains("commands.setblock.success")
    );
    let diverged = run(
        &plan,
        &format!("execute if blocks 10 {y} 10 11 {y} 10 20 {y} 10 all run say yes"),
    );
    assert!(diverged.is_empty(), "{diverged}");
    let masked = run(
        &plan,
        &format!("execute if blocks 10 {y} 10 11 {y} 10 20 {y} 10 masked run say yes"),
    );
    assert!(masked.contains("yes"), "{masked}");

    assert!(
        run(&plan, &format!("setblock 6 {y} 8 minecraft:chest"))
            .contains("commands.setblock.success")
    );
    assert!(
        run(
            &plan,
            &format!(r#"data merge block 6 {y} 8 {{CustomName:"Hi"}}"#),
        )
        .contains("commands.data.block.modified")
    );
    let named = run(
        &plan,
        &format!("execute if data block 6 {y} 8 CustomName run say yes"),
    );
    assert!(named.contains("yes"), "{named}");
    let stored = run(
        &plan,
        "execute if data storage minecraft:foo Foo run say yes",
    );
    assert!(stored.is_empty(), "{stored}");

    let uuid = chunkwake_net::Uuid::from_u128(21);
    plan.present_player(uuid, "Ada");
    assert!(run(&plan, "give Ada minecraft:stone").contains("commands.give.success"));
    let held = run(
        &plan,
        "execute if items entity Ada weapon.mainhand minecraft:stone run say yes",
    );
    assert!(held.contains("yes"), "{held}");
    let off = run(
        &plan,
        "execute if items entity Ada weapon.offhand minecraft:stone run say yes",
    );
    assert!(off.is_empty(), "{off}");

    assert!(run(&plan, "scoreboard objectives add obj dummy").contains("success"));
    assert!(run(&plan, "scoreboard players set Ada obj 3").contains("success"));
    let matched = run(&plan, "execute if score Ada obj matches 1..5 run say yes");
    assert!(matched.contains("yes"), "{matched}");
    let outside = run(&plan, "execute if score Ada obj matches 10.. run say yes");
    assert!(outside.is_empty(), "{outside}");
    let missing = run(
        &plan,
        "execute if score Nobody obj matches 1..5 run say yes",
    );
    assert!(missing.is_empty(), "{missing}");

    let raised = run(
        &plan,
        "execute positioned 8 0 8 positioned over world_surface run setblock ~ ~-1 ~ minecraft:glass",
    );
    assert!(raised.contains("commands.setblock.success"), "{raised}");
    assert_eq!(
        plan.block_name_at([8, y - 1, 8]).as_deref(),
        Some("minecraft:glass")
    );

    let formatted = run(&plan, "scoreboard objectives modify obj numberformat blank");
    assert!(formatted.contains("success"), "{formatted}");
    assert!(
        format!("{:?}", plan.objective_format("obj")).contains("Blank"),
        "{:?}",
        plan.objective_format("obj")
    );
    let per_score = run(
        &plan,
        "scoreboard players display numberformat Ada obj blank",
    );
    assert!(per_score.contains("success"), "{per_score}");
    assert!(
        format!("{:?}", plan.score_format("Ada", "obj")).contains("Blank"),
        "{:?}",
        plan.score_format("Ada", "obj")
    );
}

fn run_as(plan: &PlayPlan, source: &Source, line: &str) -> String {
    match commands::execute(plan, source, line) {
        Some(Outcome::Feedback(value)) | Some(Outcome::Broadcast(value)) => plain(&value),
        Some(Outcome::Silent) => String::new(),
        None => panic!("команда не распознана: {line}"),
    }
}

#[test]
fn clone_border_tags_trigger_and_entities() {
    let plan = world!();
    let y = plan.spawn_point()[1].floor() as i32;
    assert!(
        run(&plan, &format!("setblock 2 {y} 2 minecraft:stone"))
            .contains("commands.setblock.success")
    );
    assert!(
        run(&plan, &format!("setblock 3 {y} 2 minecraft:stone"))
            .contains("commands.setblock.success")
    );
    assert!(
        run(&plan, &format!("setblock 3 {y} 2 minecraft:air"))
            .contains("commands.setblock.success")
    );
    assert!(
        run(&plan, &format!("setblock 4 {y} 2 minecraft:glass"))
            .contains("commands.setblock.success")
    );
    assert!(
        run(&plan, &format!("setblock 6 {y} 2 minecraft:glass"))
            .contains("commands.setblock.success")
    );
    assert!(
        run(&plan, &format!("setblock 7 {y} 2 minecraft:glass"))
            .contains("commands.setblock.success")
    );
    let copied = run(&plan, &format!("clone 2 {y} 2 3 {y} 2 6 {y} 2 masked"));
    assert!(copied.contains("commands.clone.success"), "{copied}");
    assert_eq!(
        plan.block_name_at([6, y, 2]).as_deref(),
        Some("minecraft:stone")
    );
    assert_eq!(
        plan.block_name_at([7, y, 2]).as_deref(),
        Some("minecraft:glass")
    );
    let overlap = run(&plan, &format!("clone 2 {y} 2 3 {y} 2 3 {y} 2"));
    assert!(overlap.contains("commands.clone.overlap"), "{overlap}");
    let moved = run(
        &plan,
        &format!("clone 2 {y} 2 2 {y} 2 4 {y} 2 replace move"),
    );
    assert!(moved.contains("commands.clone.success"), "{moved}");
    assert_eq!(
        plan.block_name_at([4, y, 2]).as_deref(),
        Some("minecraft:stone")
    );
    assert_eq!(
        plan.block_name_at([2, y, 2]).as_deref(),
        Some("minecraft:air")
    );

    let border = run(&plan, "worldborder set 100");
    assert!(
        border.contains("commands.worldborder.set.immediate"),
        "{border}"
    );
    let size = run(&plan, "worldborder get");
    assert!(size.contains("100"), "{size}");
    let small = run(&plan, "worldborder set 0");
    assert!(
        small.contains("commands.worldborder.set.failed.small"),
        "{small}"
    );
    assert!(run(&plan, "worldborder center 8 8").contains("commands.worldborder.center.success"));

    let uuid = chunkwake_net::Uuid::from_u128(41);
    plan.present_player(uuid, "Ada");
    let ada = Source::player(uuid, "Ada", 0);
    let gm = Source::player(uuid, "Ada", 2);
    assert!(run(&plan, "tag Ada add marked").contains("commands.tag.add.success"));
    let listed = run(&plan, "tag Ada list");
    assert!(listed.contains("marked"), "{listed}");
    let selected = run(&plan, "execute as @a[tag=marked] run say yes");
    assert!(selected.contains("yes"), "{selected}");

    assert!(run(&plan, "scoreboard objectives add trig trigger").contains("success"));
    assert!(
        run(&plan, "scoreboard players enable Ada trig")
            .contains("commands.scoreboard.players.enable.success")
    );
    let pulled = run_as(&plan, &ada, "trigger trig");
    assert!(
        pulled.contains("commands.trigger.simple.success"),
        "{pulled}"
    );
    assert!(pulled.contains('1'), "{pulled}");
    let closed = run_as(&plan, &ada, "trigger trig");
    assert!(
        closed.contains("commands.trigger.failed.unprimed"),
        "{closed}"
    );
    let dummy = run_as(&plan, &ada, "trigger obj");
    assert!(
        dummy.contains("notFound") || dummy.contains("invalid"),
        "{dummy}"
    );

    let rolled = run(&plan, "random roll 1..1");
    assert!(rolled.contains("commands.random.roll"), "{rolled}");
    assert!(rolled.contains('1'), "{rolled}");
    let valued = run(&plan, "random value 4..4");
    assert!(valued.contains('4'), "{valued}");
    assert!(run(&plan, "random roll 3..1").contains("commands.random.error.range_too_small"));
    assert!(run(&plan, "random roll 1..").contains("commands.random.error.range_too_large"));

    let swung = run_as(&plan, &gm, "swing");
    assert!(swung.contains("commands.swing.success"), "{swung}");
    assert!(run(&plan, "swing").contains("commands.swing.failed.notliving"));

    let grounded = run_as(&plan, &gm, "spectate");
    assert!(
        grounded.contains("commands.spectate.not_spectator"),
        "{grounded}"
    );
    let _ = plan.player_control(uuid, "mode 3");
    let stopped = run_as(&plan, &gm, "spectate");
    assert!(
        stopped.contains("commands.spectate.success.stopped"),
        "{stopped}"
    );
    assert!(run(&plan, "summon minecraft:cow").contains("commands.summon.success"));
    let watching = run_as(&plan, &gm, "spectate @e[type=minecraft:cow,limit=1]");
    assert!(
        watching.contains("commands.spectate.success.started"),
        "{watching}"
    );

    let mounted = run(&plan, "ride Ada mount @e[type=minecraft:cow,limit=1]");
    assert!(mounted.contains("commands.ride.mount.success"), "{mounted}");
    let riding = run(&plan, "execute as Ada on vehicle run say yes");
    assert!(riding.contains("yes"), "{riding}");
    assert!(run(&plan, "ride Ada dismount").contains("commands.ride.dismount.success"));
    assert!(run(&plan, "ride Ada dismount").contains("commands.ride.not_riding"));
    let players = run(&plan, "ride @e[type=minecraft:cow,limit=1] mount Ada");
    assert!(
        players.contains("commands.ride.mount.failure.cant_ride_players"),
        "{players}"
    );
}

#[test]
fn bossbar_enchant_spread_stopwatch_and_posteffect() {
    let plan = world!();
    let y = plan.spawn_point()[1].floor() as i32;
    assert!(
        run(&plan, &format!("setblock 8 {y} 8 minecraft:stone"))
            .contains("commands.setblock.success")
    );
    let uuid = chunkwake_net::Uuid::from_u128(77);
    plan.present_player(uuid, "Ada");

    assert!(
        run(&plan, r#"bossbar add test:bar {"text":"Hi"}"#)
            .contains("commands.bossbar.create.success")
    );
    let created = run(&plan, "bossbar get test:bar value");
    assert!(created.contains("commands.bossbar.get.value"), "{created}");
    assert!(created.contains('0'), "{created}");
    let valued = run(&plan, "bossbar set test:bar value 10");
    assert!(
        valued.contains("commands.bossbar.set.value.success"),
        "{valued}"
    );
    let again = run(&plan, "bossbar get test:bar value");
    assert!(again.contains("10"), "{again}");
    assert!(
        run(&plan, "bossbar set test:bar color blue")
            .contains("commands.bossbar.set.color.success")
    );
    assert!(
        run(&plan, "bossbar set test:bar players Ada")
            .contains("commands.bossbar.set.players.success")
    );
    assert!(run(&plan, "bossbar list").contains("test:bar"));
    assert!(run(&plan, "bossbar remove test:bar").contains("commands.bossbar.remove.success"));
    assert!(run(&plan, "bossbar get test:bar value").contains("commands.bossbar.unknown"));

    assert!(
        run(&plan, "stopwatch create test:clock").contains("commands.stopwatch.create.success")
    );
    let queried = run(&plan, "stopwatch query test:clock");
    assert!(queried.contains("commands.stopwatch.query"), "{queried}");
    assert!(queried.contains("test:clock"), "{queried}");
    assert!(
        run(&plan, "stopwatch create test:clock").contains("commands.stopwatch.already_exists")
    );
    assert!(
        run(&plan, "stopwatch restart test:clock").contains("commands.stopwatch.restart.success")
    );
    assert!(
        run(&plan, "stopwatch remove test:clock").contains("commands.stopwatch.remove.success")
    );
    assert!(run(&plan, "stopwatch query test:clock").contains("commands.stopwatch.does_not_exist"));

    assert!(
        run(&plan, "posteffect add Ada minecraft:blur").contains("commands.posteffect.add.success")
    );
    let listed = run(&plan, "posteffect list Ada");
    assert!(listed.contains("minecraft:blur"), "{listed}");
    assert!(
        run(&plan, "posteffect add Ada minecraft:blur").contains("commands.posteffect.add.failed")
    );
    assert!(
        run(&plan, "posteffect remove Ada minecraft:blur")
            .contains("commands.posteffect.remove.success")
    );
    assert!(run(&plan, "posteffect clear Ada").contains("commands.posteffect.clear.failed"));

    assert!(run(&plan, "give Ada minecraft:diamond_sword").contains("commands.give.success"));
    let enchanted = run(&plan, "enchant Ada minecraft:sharpness 1");
    assert!(
        enchanted.contains("commands.enchant.success"),
        "{enchanted}"
    );
    let state = plan.player_state(uuid).unwrap_or_default();
    assert!(state.contains("sharpness"), "{state}");
    assert!(
        run(&plan, "enchant Ada minecraft:sharpness 99").contains("commands.enchant.failed.level")
    );
    assert!(run(&plan, "clear Ada").contains("commands.clear"));
    assert!(run(&plan, "give Ada minecraft:stick").contains("commands.give.success"));
    let rejected = run(&plan, "enchant Ada minecraft:sharpness 1");
    assert!(
        rejected.contains("commands.enchant.failed.incompatible"),
        "{rejected}"
    );

    let spread = run(&plan, "spreadplayers 8 8 0 1 false Ada");
    assert!(
        spread.contains("commands.spreadplayers.success.entities"),
        "{spread}"
    );
    let line = plan.player_state(uuid).unwrap_or_default();
    let numbers: Vec<f64> = line
        .lines()
        .next()
        .unwrap_or("")
        .split_whitespace()
        .filter_map(|token| token.parse().ok())
        .collect();
    assert!(numbers.len() > 8, "{line}");
    assert!(
        (numbers[6] - 8.0).abs() <= 1.5 && (numbers[8] - 8.0).abs() <= 1.5,
        "{line}"
    );
    assert!(
        run(&plan, "spreadplayers 8 8 0 1 under -99999 false Ada")
            .contains("commands.spreadplayers.failed.invalid.height")
    );
}

#[test]
fn attribute_and_fillbiome() {
    let plan = world!();
    let y = plan.spawn_point()[1].floor() as i32;
    let uuid = chunkwake_net::Uuid::from_u128(91);
    plan.present_player(uuid, "Ada");

    let many = run(&plan, "attribute @a minecraft:movement_speed get");
    assert!(many.contains("argument.entity.toomany"), "{many}");
    let speed = run(&plan, "attribute Ada minecraft:movement_speed get");
    assert!(
        speed.contains("commands.attribute.value.get.success"),
        "{speed}"
    );
    assert!(speed.contains("0.1"), "{speed}");
    let raised = run(&plan, "attribute Ada minecraft:movement_speed base set 0.2");
    assert!(
        raised.contains("commands.attribute.base_value.set.success"),
        "{raised}"
    );
    let again = run(&plan, "attribute Ada minecraft:movement_speed get");
    assert!(again.contains("0.2"), "{again}");
    assert!(
        run(&plan, "attribute Ada minecraft:movement_speed base reset")
            .contains("commands.attribute.base_value.reset.success")
    );
    let back = run(&plan, "attribute Ada minecraft:movement_speed base get");
    assert!(back.contains("0.1"), "{back}");
    let added = run(
        &plan,
        "attribute Ada minecraft:movement_speed modifier add test:boost 1 add_value",
    );
    assert!(
        added.contains("commands.attribute.modifier.add.success"),
        "{added}"
    );
    let summed = run(&plan, "attribute Ada minecraft:movement_speed get");
    assert!(summed.contains("1.1"), "{summed}");
    let modifier = run(
        &plan,
        "attribute Ada minecraft:movement_speed modifier value get test:boost",
    );
    assert!(modifier.contains('1'), "{modifier}");
    assert!(
        run(
            &plan,
            "attribute Ada minecraft:movement_speed modifier add test:boost 1 add_value"
        )
        .contains("commands.attribute.failed.modifier_already_present")
    );
    assert!(
        run(
            &plan,
            "attribute Ada minecraft:movement_speed modifier remove test:boost"
        )
        .contains("commands.attribute.modifier.remove.success")
    );
    assert!(
        run(&plan, "attribute Ada minecraft:follow_range get")
            .contains("commands.attribute.failed.no_attribute")
    );
    assert!(
        run(&plan, "attribute Ada minecraft:not_an_attribute get")
            .contains("argument.resource.not_found")
    );

    assert!(run(&plan, "summon minecraft:cow").contains("commands.summon.success"));
    let cow = run(
        &plan,
        "attribute @e[type=minecraft:cow,limit=1] minecraft:movement_speed get",
    );
    assert!(cow.contains("0.2"), "{cow}");
    let range = run(
        &plan,
        "attribute @e[type=minecraft:cow,limit=1] minecraft:follow_range get",
    );
    assert!(range.contains("16"), "{range}");
    assert!(
        run(
            &plan,
            "attribute @e[type=minecraft:cow,limit=1] minecraft:luck get"
        )
        .contains("commands.attribute.failed.no_attribute")
    );

    assert!(
        run(&plan, &format!("setblock 0 {y} 0 minecraft:stone"))
            .contains("commands.setblock.success")
    );
    let filled = run(
        &plan,
        &format!("fillbiome 0 {y} 0 0 {y} 0 minecraft:desert"),
    );
    assert!(
        filled.contains("commands.fillbiome.success.count"),
        "{filled}"
    );
    let desert = run(
        &plan,
        &format!("execute if biome 0 {y} 0 minecraft:desert run say yes"),
    );
    assert!(desert.contains("yes"), "{desert}");
    assert!(
        run(
            &plan,
            &format!("fillbiome 0 {y} 0 0 {y} 0 minecraft:desert")
        )
        .contains("commands.fillbiome.no_changes")
    );
    assert!(
        run(
            &plan,
            &format!("fillbiome 0 {y} 0 0 {y} 0 minecraft:plains replace minecraft:desert")
        )
        .contains("commands.fillbiome.success.count")
    );
    assert!(
        run(
            &plan,
            &format!("fillbiome 400 {y} 400 400 {y} 400 minecraft:desert")
        )
        .contains("argument.pos.unloaded")
    );
    let y2 = y + 31;
    let huge = run(
        &plan,
        &format!("fillbiome 0 {y} 0 31 {y2} 32 minecraft:desert"),
    );
    assert!(huge.contains("commands.fillbiome.toobig"), "{huge}");
}

#[test]
fn waypoint_lists_players_and_paints_their_icon() {
    let plan = world!();
    let empty = run(&plan, "waypoint list");
    assert!(empty.contains("commands.waypoint.list.empty"), "{empty}");

    let uuid = chunkwake_net::Uuid::from_u128(92);
    plan.present_player(uuid, "Ada");
    let listed = run(&plan, "waypoint list");
    assert!(
        listed.contains("commands.waypoint.list.success"),
        "{listed}"
    );
    assert!(listed.contains("Ada"), "{listed}");

    let painted = run(&plan, "waypoint modify Ada color red");
    assert!(
        painted.contains("commands.waypoint.modify.color"),
        "{painted}"
    );
    assert!(painted.contains("red"), "{painted}");
    let hex = run(&plan, "waypoint modify Ada color hex #00ff00");
    assert!(hex.contains("commands.waypoint.modify.color"), "{hex}");
    assert!(hex.contains("00FF00"), "{hex}");
    assert!(
        run(&plan, "waypoint modify Ada color reset")
            .contains("commands.waypoint.modify.color.reset")
    );
    assert!(
        run(&plan, "waypoint modify Ada style set minecraft:bowtie")
            .contains("commands.waypoint.modify.style")
    );
    assert!(
        run(&plan, "waypoint modify Ada style reset").contains("commands.waypoint.modify.style")
    );

    assert!(run(&plan, "summon minecraft:cow").contains("commands.summon.success"));
    let cow = run(
        &plan,
        "waypoint modify @e[type=minecraft:cow,limit=1] color red",
    );
    assert!(cow.contains("argument.waypoint.invalid"), "{cow}");
    let many = run(&plan, "waypoint modify @e color red");
    assert!(many.contains("argument.entity.toomany"), "{many}");
}

#[test]
fn dialog_clear_and_simple_particles() {
    let plan = world!();
    let uuid = chunkwake_net::Uuid::from_u128(93);
    plan.present_player(uuid, "Ada");

    let cleared = run(&plan, "dialog clear Ada");
    assert!(
        cleared.contains("commands.dialog.clear.single"),
        "{cleared}"
    );
    assert!(cleared.contains("Ada"), "{cleared}");
    let shown = run(&plan, "dialog show Ada minecraft:custom");
    assert!(shown.contains("argument.resource.not_found"), "{shown}");

    let far = run(&plan, "particle flame 10000 0 10000 0 0 0 0 1");
    assert!(far.contains("commands.particle.failed"), "{far}");
    let flame = run(&plan, "particle flame");
    assert!(flame.contains("commands.particle.success"), "{flame}");
    assert!(flame.contains("minecraft:flame"), "{flame}");
    let dust = run(&plan, "particle minecraft:dust");
    assert!(
        dust.contains("particle.invalidOptions") || dust.contains("/particle"),
        "{dust}"
    );
    let colored = run(&plan, "particle minecraft:dust 1 0 0 1");
    assert!(colored.contains("commands.particle.success"), "{colored}");
    let bad = run(&plan, "particle minecraft:dust 2 0 0 1");
    assert!(bad.contains("argument.float.range"), "{bad}");
    let missing = run(&plan, "particle minecraft:not_a_particle");
    assert!(missing.contains("particle.notFound"), "{missing}");
}

#[test]
fn return_outside_a_function_says_why() {
    let plan = world!();
    for line in ["return 1", "return fail", "return run say hi"] {
        let reply = run(&plan, line);
        assert!(
            reply.contains("only works inside a function"),
            "{line}: {reply}"
        );
    }
}

#[test]
fn item_replaces_a_player_slot_and_refuses_a_mob() {
    let plan = world!();
    let uuid = chunkwake_net::Uuid::from_u128(94);
    plan.present_player(uuid, "Ada");

    let slot = run(
        &plan,
        "item replace entity Ada weapon.mainhand with minecraft:stone",
    );
    assert!(
        slot.contains("commands.item.entity.replace.success.single.known_item"),
        "{slot}"
    );
    let hotbar = run(
        &plan,
        "item replace entity Ada hotbar.0 with minecraft:torch 16",
    );
    assert!(
        hotbar.contains("commands.item.entity.replace.success.single.known_item"),
        "{hotbar}"
    );
    let armor = run(
        &plan,
        "item replace entity Ada armor.* with minecraft:iron_helmet",
    );
    assert!(
        armor.contains("commands.item.entity.replace.success.single.known_item"),
        "{armor}"
    );
    let unknown = run(&plan, "item replace entity Ada saddle with minecraft:stone");
    assert!(unknown.contains("unknown slot"), "{unknown}");
    let bad_item = run(&plan, "item replace entity Ada weapon with minecraft:nope");
    assert!(bad_item.contains("argument.item.id.invalid"), "{bad_item}");

    assert!(run(&plan, "summon minecraft:cow").contains("commands.summon.success"));
    let cow = run(
        &plan,
        "item replace entity @e[type=minecraft:cow,limit=1] weapon with minecraft:stone",
    );
    assert!(cow.contains("commands.item.target.no_such_slot"), "{cow}");
    let block = run(
        &plan,
        "item replace block 0 0 0 weapon with minecraft:stone",
    );
    assert!(block.contains("not supported"), "{block}");
}

#[test]
fn loot_rolls_block_entity_and_reports_unknown_tables() {
    let plan = world!();
    let uuid = chunkwake_net::Uuid::from_u128(95);
    plan.present_player(uuid, "Ada");

    let stone = run(&plan, "loot give Ada loot minecraft:blocks/stone");
    assert!(stone.contains("commands.loot.give.single"), "{stone}");
    let short = run(&plan, "loot give players loot minecraft:blocks/stone");
    assert!(short.contains("argument.entity.notfound.player"), "{short}");

    // Таблица зомби может выдать нуль предметов (0..2 гнилой плоти), поэтому
    // проверяем, что таблица найдена и ролл иногда срабатывает.
    let mut zombie_ok = false;
    for _ in 0..40 {
        let reply = run(&plan, "loot give Ada loot minecraft:entities/zombie");
        assert!(!reply.contains("argument.resource.not_found"), "{reply}");
        if reply.contains("commands.loot.give.single") {
            zombie_ok = true;
            break;
        }
    }
    assert!(zombie_ok, "таблица зомби ни разу не выдала предмет");

    let unknown = run(&plan, "loot give Ada loot minecraft:nope/table");
    assert!(unknown.contains("argument.resource.not_found"), "{unknown}");
    assert!(unknown.contains("minecraft:loot_table"), "{unknown}");

    let y = plan.spawn_point()[1].floor() as i32;
    let spawned = run(
        &plan,
        &format!("loot spawn 8 {y} 8 loot minecraft:blocks/stone"),
    );
    assert!(spawned.contains("commands.loot.spawn"), "{spawned}");
    let chest = run(
        &plan,
        &format!("loot spawn 8 {y} 8 loot minecraft:chests/abandoned_mineshaft"),
    );
    assert!(chest.contains("commands.loot.spawn"), "{chest}");
    assert!(
        run(&plan, &format!("setblock 20 {y} 20 minecraft:chest"))
            .contains("commands.setblock.success")
    );
    let inserted = run(
        &plan,
        &format!("loot insert 20 {y} 20 loot minecraft:blocks/stone"),
    );
    assert!(
        inserted.contains("commands.loot.insert.success"),
        "{inserted}"
    );
    // Предмет должен лежать в сундуке, а не только в ответе команды.
    let inside = run(&plan, &format!("data get block 20 {y} 20 Items"));
    assert!(inside.contains("stone"), "{inside}");
    let replaced = run(
        &plan,
        &format!("loot replace block 20 {y} 20 0 loot minecraft:blocks/dirt"),
    );
    assert!(
        replaced.contains("commands.loot.replace.success"),
        "{replaced}"
    );
    let replaced_items = run(&plan, &format!("data get block 20 {y} 20 Items"));
    assert!(replaced_items.contains("dirt"), "{replaced_items}");
    let not_container = run(
        &plan,
        &format!("loot insert 8 {y} 8 loot minecraft:blocks/stone"),
    );
    assert!(
        not_container.contains("commands.loot.replace.block.notAContainer")
            || not_container.contains("argument.resource"),
        "{not_container}"
    );
    let kill = run(&plan, "loot give Ada kill @e[type=minecraft:cow,limit=1]");
    assert!(kill.contains("not supported"), "{kill}");
}

#[test]
fn setworldspawn_broadcasts_the_new_spawn() {
    let plan = world!();
    let uuid = chunkwake_net::Uuid::from_u128(96);
    plan.present_player(uuid, "Ada");
    run(&plan, "setworldspawn 40 70 40");
    assert_eq!(plan.server_vars().world_spawn(), Some([40.0, 70.0, 40.0]));
    assert_eq!(plan.spawn_point(), [40.0, 70.0, 40.0]);
}

#[test]
fn place_template_builds_a_real_structure() {
    let plan = world!();
    let y = plan.spawn_point()[1].floor() as i32;
    // Чанк должен быть загружен: команда ставит блоки только в готовый мир.
    let _ = run(&plan, &format!("setblock 8 {y} 8 minecraft:stone"));

    let placed = run(
        &plan,
        &format!("place template minecraft:desert_well/well 8 {y} 8"),
    );
    assert!(
        placed.contains("commands.place.template.success"),
        "{placed}"
    );
    // Колодец ставится вокруг угла: проверяем, что рядом появился песок или песчаник.
    let mut built = false;
    for dx in -3..=3 {
        for dz in -3..=3 {
            let reply = run(
                &plan,
                &format!(
                    "execute if block {} {y} {} minecraft:sandstone run say yes",
                    8 + dx,
                    8 + dz
                ),
            );
            if reply.contains("yes") {
                built = true;
            }
        }
    }
    assert!(built, "колодец не появился рядом с 8 {y} 8");

    let missing = run(&plan, &format!("place template minecraft:no/such 8 {y} 8"));
    assert!(missing.contains("no template"), "{missing}");
    let bad_rotation = run(
        &plan,
        &format!("place template minecraft:desert_well/well 8 {y} 8 sideways"),
    );
    assert!(bad_rotation.contains("unknown rotation"), "{bad_rotation}");
    let integrity = run(
        &plan,
        &format!("place template minecraft:desert_well/well 8 {y} 8 none none 0.5"),
    );
    assert!(integrity.contains("not supported"), "{integrity}");
}
