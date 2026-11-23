package net.minecraft.data.worldgen.features;

import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.level.levelgen.feature.Feature;

/** NMS {@code VegetationFeatures} (Pl3xMap). */
public final class VegetationFeatures {
    public static final ResourceKey<Feature> FLOWER_DEFAULT = key("flower_default");
    public static final ResourceKey<Feature> FLOWER_FLOWER_FOREST = key("flower_flower_forest");
    public static final ResourceKey<Feature> FLOWER_SWAMP = key("flower_swamp");
    public static final ResourceKey<Feature> FLOWER_PLAIN = key("flower_plain");
    public static final ResourceKey<Feature> FLOWER_MEADOW = key("flower_meadow");
    public static final ResourceKey<Feature> FLOWER_CHERRY = key("flower_cherry");
    public static final ResourceKey<Feature> WILDFLOWER = key("wildflower");
    public static final ResourceKey<Feature> FLOWER_PALE_GARDEN = key("flower_pale_garden");

    private VegetationFeatures() {
    }

    private static ResourceKey<Feature> key(String path) {
        return ResourceKey.entryKey(Identifier.parse("minecraft:" + path));
    }
}
