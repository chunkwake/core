//! Горючесть блоков и возраст огня. Распространение применяет PlayPlan.

pub const TICK_DELAY: u64 = 30;

pub fn is_fire(name: &str) -> bool {
    matches!(name, "minecraft:fire" | "minecraft:soul_fire")
}

/// `(chance to ignite, chance to burn away)` по семействам FireBlock.
pub fn flammability(name: &str) -> Option<(u32, u32)> {
    let n = name.strip_prefix("minecraft:")?;
    if n.starts_with("crimson_") || n.starts_with("warped_") || n == "nether_wart_block" {
        return None;
    }
    if n == "tnt" {
        return Some((15, 100));
    }
    if n.ends_with("_leaves") || n == "azalea_leaves" || n == "flowering_azalea_leaves" {
        return Some((30, 60));
    }
    if n.contains("wool") || n.ends_with("_carpet") || n == "moss_carpet" {
        return Some((30, 60));
    }
    if n == "hay_block" || n == "dried_kelp_block" || n == "target" {
        return Some((60, 20));
    }
    if n == "bookshelf" || n == "chiseled_bookshelf" || n == "lectern" {
        return Some((30, 20));
    }
    if matches!(n, "chest" | "trapped_chest" | "barrel" | "crafting_table"
        | "cartography_table" | "fletching_table" | "smithing_table"
        | "loom" | "composter" | "ladder" | "scaffolding"
        | "beehive" | "bee_nest" | "note_block" | "jukebox") {
        return Some((5, 20));
    }
    let wood_name = n.strip_prefix("stripped_").unwrap_or(n);
    let wood = [
        "oak_",
        "spruce_",
        "birch_",
        "jungle_",
        "acacia_",
        "dark_oak_",
        "mangrove_",
        "cherry_",
        "pale_oak_",
        "poplar_",
        "bamboo_",
    ]
    .iter()
    .any(|prefix| wood_name.starts_with(prefix));
    if wood
        && (n.ends_with("_planks")
            || n.ends_with("_slab")
            || n.ends_with("_stairs")
            || n.ends_with("_fence")
            || n.ends_with("_fence_gate")
            || n.ends_with("_door")
            || n.ends_with("_trapdoor")
            || n.ends_with("_sign")
            || n.ends_with("_hanging_sign")
            || n.ends_with("_log")
            || n.ends_with("_wood")
            || n.ends_with("_button")
            || n.ends_with("_pressure_plate")
            || n == "bamboo_block"
            || n == "stripped_bamboo_block")
    {
        return Some((5, 20));
    }
    if n.ends_with("_sapling")
        || n.ends_with("_flower")
        || n.ends_with("_bush")
        || n.ends_with("_grass")
        || n.ends_with("_fern")
        || n.ends_with("_vine")
        || n == "vine"
        || n == "dead_bush"
        || n == "bamboo"
        || n == "sugar_cane"
        || n == "wheat"
        || n == "carrots"
        || n == "potatoes"
        || n == "beetroots"
        || n == "torchflower"
        || n == "pink_petals"
        || n == "moss_block"
    {
        return Some((60, 100));
    }
    None
}

pub fn next_age(age: u8, random: u32) -> u8 {
    age.saturating_add((random % 3 / 2) as u8).min(15)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuel_families_and_fireproof_stems() {
        assert_eq!(flammability("minecraft:tnt"), Some((15, 100)));
        assert!(flammability("minecraft:oak_planks").is_some());
        assert!(flammability("minecraft:stripped_oak_log").is_some());
        assert!(flammability("minecraft:oak_leaves").is_some());
        assert_eq!(flammability("minecraft:warped_stem"), None);
        assert_eq!(flammability("minecraft:stone"), None);
        assert_eq!(flammability("minecraft:stone_slab"), None);
    }

    #[test]
    fn age_stays_in_range() {
        assert_eq!(next_age(0, 0), 0);
        assert_eq!(next_age(0, 2), 1);
        assert_eq!(next_age(15, 2), 15);
    }
}
