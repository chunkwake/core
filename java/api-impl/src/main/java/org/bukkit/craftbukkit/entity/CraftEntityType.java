package org.bukkit.craftbukkit.entity;

import net.minecraft.world.entity.EntityType;

/** CraftBukkit-совместимость {@code CraftEntityType} (MythicMobs). */
public final class CraftEntityType {
    private CraftEntityType() {
    }

    public static org.bukkit.entity.EntityType minecraftToBukkit(EntityType<?> type) {
        return org.bukkit.entity.EntityType.UNKNOWN;
    }

    public static org.bukkit.entity.EntityType minecraftToBukkit(String key) {
        return org.bukkit.entity.EntityType.UNKNOWN;
    }

    public static EntityType<?> bukkitToMinecraft(org.bukkit.entity.EntityType type) {
        return null;
    }

    public static EntityType<?> bukkitToMinecraft(String name) {
        return null;
    }

    public static EntityType<?> getMinecraftEntityType(org.bukkit.entity.EntityType type) {
        return bukkitToMinecraft(type);
    }
}
