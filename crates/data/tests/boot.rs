//! Проверка образа запуска на реальных данных:
//! `CHUNKWAKE_GENERATED=.../generated/26.3 cargo test -p chunkwake-data --test boot`.

use chunkwake_data::boot;
use chunkwake_data::world::Blocks;

fn generated() -> Option<std::path::PathBuf> {
    std::env::var_os("CHUNKWAKE_GENERATED").map(std::path::PathBuf::from)
}

/// Образ из кэша совпадает с тем, что дают загрузчики по дереву данных.
#[test]
fn image_matches_loaders_if_available() {
    let Some(root) = generated() else {
        return;
    };
    let boot = boot::load_or_build(&root).expect("boot image");
    // Состояния блоков: тот же `blocks.json`, что читает `Blocks::load`.
    let blocks = Blocks::load(&root).expect("blocks.json");
    assert_eq!(boot.blocks.state_count(), blocks.state_count());
    for id in 0..blocks.state_count() as u32 {
        assert_eq!(boot.blocks.state(id), blocks.state(id), "state {id}");
    }
    // Строки `light.csv` — в том же порядке и с теми же значениями.
    let rows = chunkwake_data::world::load_light_rows(&root).expect("light.csv");
    assert_eq!(boot.light, rows.light);
    assert_eq!(boot.hardness, rows.hardness);
    assert_eq!(boot.sturdy, rows.sturdy);
    assert_eq!(boot.requires_tool, rows.requires_tool);
    // Реестры и теги: те же имена и те же сетевые ID.
    assert!(boot.packets.len() > 100);
    assert_eq!(
        boot.registries.synchronized.len(),
        chunkwake_data::registries::SYNCHRONIZED.len()
    );
    assert!(
        boot.registries
            .network_id("worldgen/biome", "minecraft:plains")
            .is_ok()
    );
    // Предметы и чары на месте.
    assert!(boot.items.len() > 1000);
    assert!(!boot.enchanting.defs.is_empty());
    assert_eq!(boot.enchantment_names.len(), boot.enchanting.defs.len());
}

/// Повторное чтение того же образа даёт те же данные: кэш не «портится».
#[test]
fn cached_image_is_stable_if_available() {
    let Some(root) = generated() else {
        return;
    };
    let first = boot::load_or_build(&root).expect("boot image");
    let second = boot::load_or_build(&root).expect("boot image");
    assert_eq!(first.world_version, second.world_version);
    assert_eq!(first.blocks, second.blocks);
    assert_eq!(first.items, second.items);
    assert_eq!(first.registries, second.registries);
    assert_eq!(first.recipes.len(), second.recipes.len());
    assert_eq!(first.enchanting.table, second.enchanting.table);
}

#[test]
fn brewing_recipes_cover_26_3_if_available() {
    let Some(root) = generated() else { return };
    let builtin = chunkwake_data::registries::load_builtin(&root).unwrap();
    let items = chunkwake_data::registries::builtin_entries_of(&builtin, "item").unwrap();
    let potions = chunkwake_data::registries::builtin_entries_of(&builtin, "potion").unwrap();
    let data = chunkwake_data::recipes::load_brewing(&root, &items, &potions).unwrap();
    assert_eq!(data.recipes.len(), 279);
    let id = |map: &std::collections::HashMap<String, i32>, name: &str| map[name];
    assert_eq!(
        data.recipes.get(&(
            id(&items, "minecraft:potion"),
            id(&potions, "minecraft:water"),
            id(&items, "minecraft:nether_wart")
        )),
        Some(&(
            id(&items, "minecraft:potion"),
            id(&potions, "minecraft:awkward")
        )),
    );
}
