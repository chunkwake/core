package net.minecraft.world.level;

/** NMS {@code WorldDataConfiguration} (CommandAPI). */
public final class WorldDataConfiguration {
    private final DataPackConfig dataPacks = new DataPackConfig();
    private final net.minecraft.world.flag.FeatureFlagSet enabledFeatures =
            net.minecraft.world.flag.FeatureFlagSet.of();

    public WorldDataConfiguration() {
    }

    public DataPackConfig dataPacks() {
        return this.dataPacks;
    }

    public net.minecraft.world.flag.FeatureFlagSet enabledFeatures() {
        return this.enabledFeatures;
    }
}
