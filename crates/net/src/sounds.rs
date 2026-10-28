//! Звуки, которые в ванили играет сервер (`Level.playSound`, `levelEvent`):
//! категории и звук установки блока по его типу звука (`SoundType`).

/// `SoundSource`: категория громкости у клиента.
pub const BLOCKS: i32 = 4;
pub const PLAYERS: i32 = 7;

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
        "_planks", "_log", "_wood", "_stem", "_hyphae", "fence", "door", "sign", "chest", "barrel",
        "crafting_table", "bookshelf", "lectern", "composter", "loom", "cartography_table",
        "fletching_table", "smithing_table", "note_block", "jukebox", "beehive", "_slab", "_stairs",
        "_button", "_pressure_plate",
    ]) && !has(&["stone", "brick", "iron", "copper", "quartz", "purpur", "prismarine", "deepslate", "tuff", "blackstone", "sandstone", "andesite", "diorite", "granite", "mud_brick", "polished"]);
    let group = if has(&["glass"]) {
        "glass"
    } else if has(&["wool", "carpet"]) {
        "wool"
    } else if has(&["sandstone"]) {
        "stone"
    } else if has(&["sand"]) {
        "sand"
    } else if has(&["gravel", "dirt", "farmland", "clay", "podzol", "mycelium"]) && !has(&["rooted"]) {
        "gravel"
    } else if has(&["grass", "leaves", "sapling", "fern", "flower", "tulip", "poppy", "dandelion", "orchid", "allium", "bluet", "daisy", "lily", "rose", "bush", "wheat", "carrots", "potatoes", "beetroots", "melon_stem", "pumpkin_stem", "sugar_cane", "kelp", "seagrass", "hay_block", "sponge", "tnt"]) {
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
    } else if has(&["iron", "gold_block", "diamond_block", "emerald_block", "hopper", "cauldron", "rail", "chain", "lantern", "bell"]) {
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
        assert_eq!(place_sound("minecraft:oak_planks", none), "minecraft:block.wood.place");
        assert_eq!(place_sound("minecraft:crimson_planks", none), "minecraft:block.nether_wood.place");
        assert_eq!(place_sound("minecraft:stone_bricks", none), "minecraft:block.stone.place");
        assert_eq!(place_sound("minecraft:white_wool", none), "minecraft:block.wool.place");
        assert_eq!(place_sound("minecraft:dirt", none), "minecraft:block.gravel.place");
        assert_eq!(place_sound("minecraft:glass_pane", none), "minecraft:block.glass.place");
        let own = |s: &str| s == "minecraft:block.deepslate_bricks.place";
        assert_eq!(place_sound("minecraft:deepslate_bricks", own), "minecraft:block.deepslate_bricks.place");
    }
}
