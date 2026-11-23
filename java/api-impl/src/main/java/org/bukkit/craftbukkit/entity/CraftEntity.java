package org.bukkit.craftbukkit.entity;

/** Маркерный интерфейс CraftBukkit-сущности, который видят плагины. */
public interface CraftEntity {
    default Object getHandle() {
        return null;
    }

    default org.bukkit.persistence.PersistentDataContainer getPersistentDataContainer() {
        return null;
    }
}
