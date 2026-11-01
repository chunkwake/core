//! Звуки, которые в ванили играет сервер (`Level.playSound`, `levelEvent`):
//! категории и звук установки блока по его типу звука (`SoundType`).

/// `SoundSource`: категория громкости у клиента.
pub const BLOCKS: i32 = 4;
pub const NEUTRAL: i32 = 6;
pub const PLAYERS: i32 = 7;
/// `SoundSource.RECORDS`: ноты и пластинки.
pub const RECORDS: i32 = 10;

/// `LevelEvent.PARTICLES_DESTROY_BLOCK`: частицы и звук ломки (клиент берёт звук блока сам).
pub const DESTROY_BLOCK: i32 = 2001;

/// Звук установки блока (`SoundType.getPlaceSound`). Таблицы типов звука в
/// данных нет: сначала собственный `block.<имя>.place`, затем группа по имени.
pub fn place_sound(block: &str, exists: impl Fn(&str) -> bool) -> String {
    let path = block.strip_prefix("minecraft:").unwrap_or(block);
    let own = format!("minecraft:block.{path}.place");
    if exists(&own) {
        return own;
    }
    let has = |s: &[&str]| s.iter().any(|p| path.contains(p));
    let wooden = has(&[
        "_planks",
        "_log",
        "_wood",
        "_stem",
        "_hyphae",
        "fence",
        "door",
        "sign",
        "chest",
        "barrel",
        "crafting_table",
        "bookshelf",
        "lectern",
        "composter",
        "loom",
        "cartography_table",
        "fletching_table",
        "smithing_table",
        "note_block",
        "jukebox",
        "beehive",
        "_slab",
        "_stairs",
        "_button",
        "_pressure_plate",
    ]) && !has(&[
        "stone",
        "brick",
        "iron",
        "copper",
        "quartz",
        "purpur",
        "prismarine",
        "deepslate",
        "tuff",
        "blackstone",
        "sandstone",
        "andesite",
        "diorite",
        "granite",
        "mud_brick",
        "polished",
    ]);
    let group = if has(&["glass"]) {
        "glass"
    } else if has(&["wool", "carpet"]) {
        "wool"
    } else if has(&["sandstone"]) {
        "stone"
    } else if has(&["sand"]) {
        "sand"
    } else if has(&["gravel", "dirt", "farmland", "clay", "podzol", "mycelium"])
        && !has(&["rooted"])
    {
        "gravel"
    } else if has(&[
        "grass",
        "leaves",
        "sapling",
        "fern",
        "flower",
        "tulip",
        "poppy",
        "dandelion",
        "orchid",
        "allium",
        "bluet",
        "daisy",
        "lily",
        "rose",
        "bush",
        "wheat",
        "carrots",
        "potatoes",
        "beetroots",
        "melon_stem",
        "pumpkin_stem",
        "sugar_cane",
        "kelp",
        "seagrass",
        "hay_block",
        "sponge",
        "tnt",
    ]) {
        "grass"
    } else if has(&["snow", "powder_snow"]) {
        "snow"
    } else if has(&["ladder"]) {
        "ladder"
    } else if has(&["anvil"]) {
        "anvil"
    } else if has(&["copper"]) {
        "copper"
    } else if has(&["cherry"]) && wooden {
        "cherry_wood"
    } else if has(&["bamboo"]) && wooden {
        "bamboo_wood"
    } else if has(&["crimson", "warped"]) && wooden {
        "nether_wood"
    } else if wooden {
        "wood"
    } else if has(&[
        "iron",
        "gold_block",
        "diamond_block",
        "emerald_block",
        "hopper",
        "cauldron",
        "rail",
        "chain",
        "lantern",
        "bell",
    ]) {
        "metal"
    } else if has(&["deepslate"]) {
        "deepslate"
    } else if has(&["netherrack", "nether_quartz_ore", "nether_gold_ore"]) {
        "netherrack"
    } else if has(&["nether_brick"]) {
        "nether_bricks"
    } else {
        "stone"
    };
    format!("minecraft:block.{group}.place")
}

/// Открывается ли рукой и каким звуком (`BlockSetType`/`WoodType`): двери, люки,
/// калитки; железные — только сигналом (`None`).
pub fn toggle_sound(block: &str, open: bool) -> Option<String> {
    if block.contains("iron_") {
        return None;
    }
    door_sound(block, open)
}

/// Звук двери, люка или калитки любого типа, в т. ч. железных (открытых сигналом).
pub fn door_sound(block: &str, open: bool) -> Option<String> {
    let path = block.strip_prefix("minecraft:").unwrap_or(block);
    let kind = if path.ends_with("_trapdoor") {
        "trapdoor"
    } else if path.ends_with("_door") {
        "door"
    } else if path.ends_with("_fence_gate") {
        "fence_gate"
    } else {
        return None;
    };
    let set = if path.starts_with("iron_") {
        "iron"
    } else if path.contains("copper") {
        "copper"
    } else if path.starts_with("cherry") {
        "cherry_wood"
    } else if path.starts_with("bamboo") {
        "bamboo_wood"
    } else if path.starts_with("crimson") || path.starts_with("warped") {
        "nether_wood"
    } else if kind == "fence_gate" {
        ""
    } else {
        "wooden"
    };
    let sep = if set.is_empty() { "" } else { "_" };
    let action = if open { "open" } else { "close" };
    Some(format!("minecraft:block.{set}{sep}{kind}.{action}"))
}

/// Ванильный разброс высоты: `random.nextFloat() * 0.1 + 0.9`.
pub fn pitch_09() -> f32 {
    rand::random::<f32>() * 0.1 + 0.9
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_by_name() {
        let none = |_: &str| false;
        assert_eq!(
            place_sound("minecraft:oak_planks", none),
            "minecraft:block.wood.place"
        );
        assert_eq!(
            place_sound("minecraft:crimson_planks", none),
            "minecraft:block.nether_wood.place"
        );
        assert_eq!(
            place_sound("minecraft:stone_bricks", none),
            "minecraft:block.stone.place"
        );
        assert_eq!(
            place_sound("minecraft:white_wool", none),
            "minecraft:block.wool.place"
        );
        assert_eq!(
            place_sound("minecraft:dirt", none),
            "minecraft:block.gravel.place"
        );
        assert_eq!(
            place_sound("minecraft:glass_pane", none),
            "minecraft:block.glass.place"
        );
        assert_eq!(
            toggle_sound("minecraft:oak_door", true).as_deref(),
            Some("minecraft:block.wooden_door.open")
        );
        assert_eq!(
            toggle_sound("minecraft:oak_fence_gate", false).as_deref(),
            Some("minecraft:block.fence_gate.close")
        );
        assert_eq!(
            toggle_sound("minecraft:waxed_oxidized_copper_trapdoor", true).as_deref(),
            Some("minecraft:block.copper_trapdoor.open")
        );
        assert_eq!(toggle_sound("minecraft:iron_door", true), None);
        let own = |s: &str| s == "minecraft:block.deepslate_bricks.place";
        assert_eq!(
            place_sound("minecraft:deepslate_bricks", own),
            "minecraft:block.deepslate_bricks.place"
        );
    }
}
