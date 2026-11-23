package net.minecraft.world.level.levelgen.feature;

import com.mojang.serialization.MapCodec;

/** NMS {@code SimpleBlockFeature}: база для CE custom features. */
public class SimpleBlockFeature implements Feature {
    /** {@code SimpleBlockFeature.CODEC} (CraftEngine feature injector). */
    public static final MapCodec<SimpleBlockFeature> CODEC =
            MapCodec.unit(SimpleBlockFeature::new);

    public SimpleBlockFeature() {
    }
}
