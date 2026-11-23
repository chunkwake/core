package net.minecraft.commands;

import net.minecraft.core.HolderLookup;
import net.minecraft.world.flag.FeatureFlagSet;

/** NMS {@code CommandBuildContext} (интерфейс, как в Paper 26.x). */
public interface CommandBuildContext extends HolderLookup.Provider {
    static CommandBuildContext simple(HolderLookup.Provider provider, FeatureFlagSet features) {
        return () -> features;
    }

    FeatureFlagSet enabledFeatures();
}
