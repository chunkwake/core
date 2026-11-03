//! Random-tick rules for plants and leaves.

use std::collections::{HashSet, VecDeque};

pub fn age_limit(name: &str) -> Option<u8> {
    match name {
        "minecraft:wheat"
        | "minecraft:carrots"
        | "minecraft:potatoes"
        | "minecraft:pumpkin_stem"
        | "minecraft:melon_stem" => Some(7),
        "minecraft:beetroots"
        | "minecraft:nether_wart"
        | "minecraft:sweet_berry_bush"
        | "minecraft:cocoa" => Some(3),
        "minecraft:cactus" | "minecraft:sugar_cane" => Some(15),
        _ => None,
    }
}

pub fn random_tick_candidate(name: &str) -> bool {
    age_limit(name).is_some()
        || name.ends_with("_leaves")
        || climbing_tip(name).is_some()
        || tree_wood(name).is_some()
        || matches!(
            name,
            "minecraft:bamboo"
                | "minecraft:bamboo_sapling"
                | "minecraft:brown_mushroom"
                | "minecraft:red_mushroom"
                | "minecraft:cave_vines_plant"
                | "minecraft:farmland"
                | "minecraft:grass_block"
                | "minecraft:mycelium"
                | "minecraft:snow"
                | "minecraft:ice"
        )
}

pub fn tree_wood(name: &str) -> Option<&str> {
    match name {
        "minecraft:oak_sapling" => Some("oak"),
        "minecraft:spruce_sapling" => Some("spruce"),
        "minecraft:birch_sapling" => Some("birch"),
        "minecraft:jungle_sapling" => Some("jungle"),
        "minecraft:acacia_sapling" => Some("acacia"),
        "minecraft:dark_oak_sapling" => Some("dark_oak"),
        "minecraft:cherry_sapling" => Some("cherry"),
        "minecraft:pale_oak_sapling" => Some("pale_oak"),
        "minecraft:poplar_sapling" => Some("poplar"),
        "minecraft:mangrove_propagule" => Some("mangrove"),
        _ => None,
    }
}

/// Growing head → stem left behind, and direction of growth.
pub fn climbing_tip(name: &str) -> Option<(&'static str, i32)> {
    match name {
        "minecraft:kelp" => Some(("minecraft:kelp_plant", 1)),
        "minecraft:twisting_vines" => Some(("minecraft:twisting_vines_plant", 1)),
        "minecraft:weeping_vines" => Some(("minecraft:weeping_vines_plant", -1)),
        "minecraft:cave_vines" => Some(("minecraft:cave_vines_plant", -1)),
        _ => None,
    }
}

/// The shortest path through leaves to any log, capped at the seven-state limit.
pub fn leaf_distance(
    origin: [i32; 3],
    mut block: impl FnMut([i32; 3]) -> Option<(bool, bool)>,
) -> u8 {
    let mut seen = HashSet::from([origin]);
    let mut queue = VecDeque::from([(origin, 0u8)]);
    while let Some((p, depth)) = queue.pop_front() {
        if depth == 6 {
            continue;
        }
        for d in [
            [1, 0, 0],
            [-1, 0, 0],
            [0, 1, 0],
            [0, -1, 0],
            [0, 0, 1],
            [0, 0, -1],
        ] {
            let next = [p[0] + d[0], p[1] + d[1], p[2] + d[2]];
            if !seen.insert(next) {
                continue;
            }
            match block(next) {
                Some((_, true)) => return depth + 1,
                Some((true, _)) => queue.push_back((next, depth + 1)),
                _ => {}
            }
        }
    }
    7
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn age_limits() {
        for name in ["wheat", "carrots", "potatoes", "melon_stem", "pumpkin_stem"] {
            assert_eq!(age_limit(&format!("minecraft:{name}")), Some(7));
        }
        for name in ["beetroots", "nether_wart", "sweet_berry_bush", "cocoa"] {
            assert_eq!(age_limit(&format!("minecraft:{name}")), Some(3));
        }
        assert_eq!(age_limit("minecraft:cactus"), Some(15));
        assert_eq!(age_limit("minecraft:sugar_cane"), Some(15));
    }

    #[test]
    fn climbing_tips_keep_direction_and_stem() {
        assert_eq!(
            climbing_tip("minecraft:kelp"),
            Some(("minecraft:kelp_plant", 1))
        );
        assert_eq!(
            climbing_tip("minecraft:twisting_vines"),
            Some(("minecraft:twisting_vines_plant", 1))
        );
        assert_eq!(
            climbing_tip("minecraft:weeping_vines"),
            Some(("minecraft:weeping_vines_plant", -1))
        );
        assert_eq!(
            climbing_tip("minecraft:cave_vines"),
            Some(("minecraft:cave_vines_plant", -1))
        );
        assert!(climbing_tip("minecraft:vine").is_none());
        assert!(random_tick_candidate("minecraft:cave_vines_plant"));
        assert!(random_tick_candidate("minecraft:bamboo_sapling"));
        assert!(random_tick_candidate("minecraft:brown_mushroom"));
    }

    #[test]
    fn tree_saplings_cover_version_263_species() {
        for wood in [
            "oak", "spruce", "birch", "jungle", "acacia", "dark_oak", "cherry", "pale_oak",
            "poplar",
        ] {
            assert_eq!(tree_wood(&format!("minecraft:{wood}_sapling")), Some(wood));
        }
        assert_eq!(tree_wood("minecraft:mangrove_propagule"), Some("mangrove"));
    }

    #[test]
    fn leaves_find_logs_only_through_leaves() {
        assert_eq!(
            leaf_distance([0, 0, 0], |p| match p {
                [3, 0, 0] => Some((false, true)),
                [1 | 2, 0, 0] => Some((true, false)),
                _ => None,
            }),
            3
        );
        assert_eq!(
            leaf_distance([0, 0, 0], |p| match p {
                [7, 0, 0] => Some((false, true)),
                [1..=6, 0, 0] => Some((true, false)),
                _ => None,
            }),
            7
        );
    }
}
