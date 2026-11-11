//! Какие serverbound play-пакеты 26.3 сервер обрабатывает.
//!
//! Оракул — `reports/packets.json` из кэша данных; наша сторона — таблица
//! `sb("…")` в `play.rs`. Необработанные пакеты перечислены явно в
//! [`UNHANDLED`]: тест падает, если пакет обработали и не вычеркнули, или если
//! в ванили появился пакет, которого нет ни там, ни там. Без кэша тест
//! пропускается, как `commands_parity`.

use std::collections::BTreeSet;

/// Пакеты, которые клиент шлёт, а сервер пока молча пропускает.
///
/// Часть из них протокольная (`pong`, `chat_ack`, `client_tick_end`), часть —
/// дыры поведения (`entity_tag_query`).
/// Матрица: `docs/feature-matrix.md`, строка «Пакеты клиента».
const UNHANDLED: &[&str] = &[
    "block_entity_tag_query",
    "bundle_item_selected",
    "change_difficulty",
    "chat_ack",
    "chat_session_update",
    "client_tick_end",
    "configuration_acknowledged",
    "cookie_response",
    "custom_click_action",
    "debug_subscription_request",
    "entity_tag_query",
    "jigsaw_generate",
    "lock_difficulty",
    "ping_request",
    "place_recipe",
    "pong",
    "recipe_book_change_settings",
    "recipe_book_seen_recipe",
    "resource_pack",
    "seen_advancements",
    "select_trade",
    "set_command_block",
    "set_command_minecart",
    "set_game_rule",
    "set_jigsaw_block",
    "set_structure_block",
    "set_test_block",
    "spectator_action",
    "teleport_to_entity",
    "test_instance_block_action",
];

fn packets_json() -> Option<String> {
    let mut candidates = Vec::new();
    if let Ok(dir) = std::env::var("CHUNKWAKE_DATA") {
        candidates.push(std::path::PathBuf::from(dir));
    }
    for root in ["../..", "."] {
        for profile in ["release", "debug"] {
            candidates.push(
                std::path::Path::new(root)
                    .join("target")
                    .join(profile)
                    .join("generated")
                    .join("26.3"),
            );
        }
    }
    candidates
        .into_iter()
        .map(|d| d.join("reports").join("packets.json"))
        .find(|p| p.is_file())
        .and_then(|p| std::fs::read_to_string(p).ok())
}

/// Имена из `sb("…")` в исходнике `play.rs`.
fn handled() -> BTreeSet<String> {
    let src = include_str!("../src/play.rs");
    src.split("sb(\"")
        .skip(1)
        .filter_map(|rest| rest.split_once("\")").map(|(name, _)| name.to_owned()))
        .filter(|name| name.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
        .collect()
}

#[test]
fn serverbound_play_packets_are_accounted_for() {
    let Some(text) = packets_json() else {
        eprintln!("нет reports/packets.json — пропуск");
        return;
    };
    let json: serde_json::Value = serde_json::from_str(&text).expect("packets.json");
    let vanilla: BTreeSet<String> = json["play"]["serverbound"]
        .as_object()
        .expect("play.serverbound")
        .keys()
        .map(|k| k.trim_start_matches("minecraft:").to_owned())
        .collect();
    let handled = handled();
    let unhandled: BTreeSet<String> = UNHANDLED.iter().map(|s| (*s).to_owned()).collect();

    let stale: Vec<_> = handled.intersection(&unhandled).collect();
    assert!(
        stale.is_empty(),
        "обработаны, но числятся в UNHANDLED: {stale:?}"
    );

    let unknown: Vec<_> = handled.difference(&vanilla).collect();
    assert!(unknown.is_empty(), "sb(…) без пакета в 26.3: {unknown:?}");

    let missing: Vec<_> = vanilla
        .iter()
        .filter(|p| !handled.contains(*p) && !unhandled.contains(*p))
        .collect();
    assert!(
        missing.is_empty(),
        "новые пакеты 26.3 без решения: {missing:?}"
    );

    let gone: Vec<_> = unhandled.difference(&vanilla).collect();
    assert!(
        gone.is_empty(),
        "в UNHANDLED пакеты, которых нет в 26.3: {gone:?}"
    );
}
