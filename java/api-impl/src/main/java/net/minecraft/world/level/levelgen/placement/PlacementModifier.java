package net.minecraft.world.level.levelgen.placement;

import com.mojang.serialization.MapCodec;

/** NMS {@code PlacementModifier}: база placement-модификаторов. */
public interface PlacementModifier {
    com.mojang.serialization.Codec<PlacementModifier> CODEC = null;

    default MapCodec<?> type() {
        return null;
    }
}
