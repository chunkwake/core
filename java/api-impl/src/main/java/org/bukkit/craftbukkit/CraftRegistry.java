package org.bukkit.craftbukkit;

import net.minecraft.core.RegistryAccess;

/** CraftBukkit-совместимость {@code CraftRegistry} (nightcore). */
public final class CraftRegistry {
    private CraftRegistry() {
    }

    public static RegistryAccess.Frozen getMinecraftRegistry() {
        return RegistryAccess.Frozen.empty();
    }
}
