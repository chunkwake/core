//! Rules shared by farmland hydration and landing on farmland.

/// Water may hydrate farmland from four blocks away at its level or one above.
pub fn hydrated(pos: [i32; 3], raining: bool, mut water: impl FnMut([i32; 3]) -> bool) -> bool {
    if raining {
        return true;
    }
    for dx in -4..=4 {
        for dz in -4..=4 {
            for dy in 0..=1 {
                if water([pos[0] + dx, pos[1] + dy, pos[2] + dz]) {
                    return true;
                }
            }
        }
    }
    false
}

/// Vanilla's landing chance rises from zero at half a block to certainty at 1.5.
pub fn trample_chance(fall: f64) -> f64 {
    (fall - 0.5).clamp(0.0, 1.0)
}

pub fn crop_holds_farmland(name: &str) -> bool {
    matches!(
        name,
        "minecraft:wheat"
            | "minecraft:carrots"
            | "minecraft:potatoes"
            | "minecraft:beetroots"
            | "minecraft:melon_stem"
            | "minecraft:pumpkin_stem"
            | "minecraft:attached_melon_stem"
            | "minecraft:attached_pumpkin_stem"
            | "minecraft:torchflower_crop"
            | "minecraft:pitcher_crop"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn water_hydrates_only_inside_the_four_block_radius_and_two_levels() {
        let pos = [0, 64, 0];
        assert!(hydrated(pos, false, |p| p == [4, 65, -4]));
        assert!(!hydrated(pos, false, |p| p == [5, 64, 0]));
        assert!(!hydrated(pos, false, |p| p == [0, 63, 0]));
        assert!(hydrated(pos, true, |_| false));
    }

    #[test]
    fn landing_chance_ignores_short_falls() {
        assert_eq!(trample_chance(0.5), 0.0);
        assert_eq!(trample_chance(1.0), 0.5);
        assert_eq!(trample_chance(1.5), 1.0);
    }

    #[test]
    fn planted_crops_keep_dry_farmland() {
        assert!(crop_holds_farmland("minecraft:wheat"));
        assert!(crop_holds_farmland("minecraft:attached_melon_stem"));
        assert!(!crop_holds_farmland("minecraft:grass_block"));
    }
}
