package dev.chunkwake.bridge;

import net.minecraft.core.SimpleRegistry;
import net.minecraft.resources.Identifier;

/**
 * Ванильные записи реестров, которых нет в built-in отчёте 26.3
 * (biome / dimension_type — это datapack-реестры).
 *
 * <p>squaremap читает конфиг (`WorldConfig.biomeSettings`,
 * `WorldAdvanced.colorOverrideBiome*`) и требует, чтобы `Registries.BIOME`
 * содержал реальные ключи (`minecraft:bamboo_jungle` и т. д.). Список взят
 * из `data/minecraft/worldgen/biome` и `data/minecraft/dimension_type`
 * серверного jar 26.3, поэтому ключи совпадают с ванильными.
 */
public final class VanillaRegistries {
    private VanillaRegistries() {
    }

    private static final String[] BIOMES = {
            "badlands",
            "bamboo_jungle",
            "basalt_deltas",
            "beach",
            "birch_forest",
            "cherry_grove",
            "cold_ocean",
            "crimson_forest",
            "dappled_forest",
            "dark_forest",
            "deep_cold_ocean",
            "deep_dark",
            "deep_frozen_ocean",
            "deep_lukewarm_ocean",
            "deep_ocean",
            "desert",
            "dripstone_caves",
            "end_barrens",
            "end_highlands",
            "end_midlands",
            "eroded_badlands",
            "flower_forest",
            "forest",
            "frozen_ocean",
            "frozen_peaks",
            "frozen_river",
            "grove",
            "ice_spikes",
            "jagged_peaks",
            "jungle",
            "lukewarm_ocean",
            "lush_caves",
            "mangrove_swamp",
            "meadow",
            "mushroom_fields",
            "nether_wastes",
            "ocean",
            "old_growth_birch_forest",
            "old_growth_pine_taiga",
            "old_growth_spruce_taiga",
            "pale_garden",
            "plains",
            "river",
            "savanna",
            "savanna_plateau",
            "small_end_islands",
            "snowy_beach",
            "snowy_plains",
            "snowy_slopes",
            "snowy_taiga",
            "soul_sand_valley",
            "sparse_jungle",
            "stony_peaks",
            "stony_shore",
            "sulfur_caves",
            "sunflower_plains",
            "swamp",
            "taiga",
            "the_end",
            "the_void",
            "warm_ocean",
            "warped_forest",
            "windswept_forest",
            "windswept_gravelly_hills",
            "windswept_hills",
            "windswept_savanna",
            "wooded_badlands",
    };

    private static final String[] DIMENSION_TYPES = {
            "overworld",
            "overworld_caves",
            "the_end",
            "the_nether",
    };

    /** Создаёт реестр с ванильными записями по ключу реестра. */
    public static SimpleRegistry<Object> create(String registryKey) {
        SimpleRegistry<Object> registry = new SimpleRegistry<>();
        if ("minecraft:biome".equals(registryKey)) {
            for (String name : BIOMES) {
                registry.register(Identifier.parse("minecraft:" + name),
                        new net.minecraft.world.level.biome.Biome());
            }
        } else if ("minecraft:dimension_type".equals(registryKey)) {
            for (String name : DIMENSION_TYPES) {
                registry.register(Identifier.parse("minecraft:" + name),
                        new net.minecraft.world.level.dimension.DimensionType());
            }
        }
        return registry;
    }
}
