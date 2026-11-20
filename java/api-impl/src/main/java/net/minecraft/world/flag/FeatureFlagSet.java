package net.minecraft.world.flag;

/** Заглушка NMS FeatureFlagSet. */
public class FeatureFlagSet {
    private static final FeatureFlagSet EMPTY = new FeatureFlagSet();

    public static FeatureFlagSet of() {
        return EMPTY;
    }

    public FeatureFlagSet() {
    }
}
