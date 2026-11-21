package org.bukkit.craftbukkit.util;

import java.util.Map;

/** CraftBukkit {@code CraftMagicNumbers} (CraftEngine). */
public final class CraftMagicNumbers {
    private static final Map<Object, org.bukkit.Material> BLOCK_MATERIAL = new java.util.HashMap<>();

    private CraftMagicNumbers() {
    }

    public static Map<Object, org.bukkit.Material> getBlockMaterial() {
        return BLOCK_MATERIAL;
    }
}
