package net.minecraft.world.level.storage;

/** NMS {@code WorldData}: интерфейс, как в Paper 26.x (CommandAPI). */
public interface WorldData {
    net.minecraft.world.level.WorldDataConfiguration getDataConfiguration();

    default void setDataConfiguration(net.minecraft.world.level.WorldDataConfiguration configuration) {
    }

    default net.minecraft.world.flag.FeatureFlagSet enabledFeatures() {
        return this.getDataConfiguration().enabledFeatures();
    }

    default boolean wasModded() {
        return false;
    }

    default java.util.Set<String> getKnownServerBrands() {
        return java.util.Set.of("chunkwake");
    }

    default java.util.Set<String> getRemovedFeatureFlags() {
        return java.util.Set.of();
    }

    default void setModdedInfo(String brand, boolean modded) {
    }

    default boolean isHardcore() {
        return false;
    }

    default int getVersion() {
        return 5023;
    }

    default String getLevelName() {
        return "world";
    }

    default boolean isFlatWorld() {
        return true;
    }

    default boolean isDebugWorld() {
        return false;
    }

    static WorldData create() {
        return new Simple();
    }

    final class Simple implements WorldData {
        private final net.minecraft.world.level.WorldDataConfiguration dataConfiguration =
                new net.minecraft.world.level.WorldDataConfiguration();

        @Override
        public net.minecraft.world.level.WorldDataConfiguration getDataConfiguration() {
            return this.dataConfiguration;
        }
    }
}
