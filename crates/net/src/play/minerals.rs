//! Аметист и редстоун-руда: рост почек на будящем аметисте
//! (`BuddingAmethystBlock.randomTick`) и свечение руды от касания
//! (`RedStoneOreBlock.interact`), гаснущей на случайном тике.

use super::PlayPlan;

const FACING: [&str; 6] = ["down", "up", "north", "south", "west", "east"];

/// Следующая стадия почки: малая → средняя → большая → друза.
fn next_bud(name: &str) -> Option<&'static str> {
    Some(match name {
        "minecraft:small_amethyst_bud" => "minecraft:medium_amethyst_bud",
        "minecraft:medium_amethyst_bud" => "minecraft:large_amethyst_bud",
        "minecraft:large_amethyst_bud" => "minecraft:amethyst_cluster",
        _ => return None,
    })
}

pub(super) fn is_redstone_ore(name: &str) -> bool {
    matches!(
        name,
        "minecraft:redstone_ore" | "minecraft:deepslate_redstone_ore"
    )
}

impl PlayPlan {
    /// Случайный тик аметиста и руды; `false` — блок не отсюда.
    pub(super) fn tick_mineral(&self, pos: [i32; 3], state: u32) -> bool {
        let name = self.state_name(state);
        if is_redstone_ore(name) {
            if self.survival.placement.prop(state, "lit") == Some("true")
                && let Some(off) = self.survival.placement.with_prop(state, "lit", "false")
            {
                self.set_block(pos, off);
                self.redstone_update(&crate::redstone::around(pos));
            }
            return true;
        }
        if name != "minecraft:budding_amethyst" {
            return false;
        }
        if !rand::random::<u32>().is_multiple_of(5) {
            return true;
        }
        let dir = rand::random::<u32>() as usize % 6;
        self.grow_bud(pos, dir);
        true
    }

    /// Почка на грани `dir`: в пустоте или полной воде — малая, на почке с тем
    /// же `facing` — следующая стадия; `waterlogged` по воде-источнику.
    fn grow_bud(&self, pos: [i32; 3], dir: usize) {
        let pl = &self.survival.placement;
        let at = crate::redstone::step(pos, dir);
        let Some(there) = self.block(at) else { return };
        let there_name = self.state_name(there);
        let water_level = (there_name == "minecraft:water")
            .then(|| pl.prop(there, "level").and_then(|l| l.parse::<u8>().ok()))
            .flatten();
        let next = if chunkwake_world::has_flag(there, chunkwake_world::flags::AIR)
            // `FluidState.isFull`: источник или падающая вода.
            || water_level.is_some_and(|l| l == 0 || l >= 8)
        {
            "minecraft:small_amethyst_bud"
        } else if let Some(n) = next_bud(there_name)
            && pl.prop(there, "facing") == Some(FACING[dir])
        {
            n
        } else {
            return;
        };
        // `getFluidState().is(Fluids.WATER)`: только вода-источник.
        let source = water_level == Some(0) || pl.prop(there, "waterlogged") == Some("true");
        let Some(&base) = self.survival.block_defaults.get(next) else {
            return;
        };
        let Some(state) = pl
            .with_prop(base, "facing", FACING[dir])
            .and_then(|s| pl.with_prop(s, "waterlogged", if source { "true" } else { "false" }))
        else {
            return;
        };
        self.set_block(at, state);
        self.redstone_update(&crate::redstone::around(at));
    }

    /// `RedStoneOreBlock.interact`: удар, шаг или клик зажигают руду.
    /// `true` — в клетке редстоун-руда.
    pub(super) fn touch_redstone_ore(&self, pos: [i32; 3]) -> bool {
        let Some(state) = self.block(pos) else {
            return false;
        };
        if !is_redstone_ore(self.state_name(state)) {
            return false;
        }
        if self.survival.placement.prop(state, "lit") == Some("false")
            && let Some(lit) = self.survival.placement.with_prop(state, "lit", "true")
        {
            self.set_block(pos, lit);
            self.redstone_update(&crate::redstone::around(pos));
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buds_grow_in_vanilla_order() {
        assert_eq!(
            next_bud("minecraft:small_amethyst_bud"),
            Some("minecraft:medium_amethyst_bud")
        );
        assert_eq!(
            next_bud("minecraft:large_amethyst_bud"),
            Some("minecraft:amethyst_cluster")
        );
        assert_eq!(next_bud("minecraft:amethyst_cluster"), None);
        assert!(is_redstone_ore("minecraft:deepslate_redstone_ore"));
    }
}
