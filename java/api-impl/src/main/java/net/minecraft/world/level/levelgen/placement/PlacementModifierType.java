package net.minecraft.world.level.levelgen.placement;

import com.mojang.serialization.Codec;

/** NMS {@code PlacementModifierType}: тип placement-модификатора. */
public class PlacementModifierType {
    public static final PlacementModifierType BIOME_FILTER = new PlacementModifierType();
    public static final Codec<PlacementModifierType> CODEC = null;

    public PlacementModifierType() {
    }
}
