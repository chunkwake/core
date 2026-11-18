package org.bukkit.craftbukkit;

/** CraftBukkit-носитель мира: нужен ProtocolLib (BukkitConverters). */
public class CraftWorld {
    private final Object handle;

    public CraftWorld() {
        this(null);
    }

    public CraftWorld(Object handle) {
        this.handle = handle;
    }

    public Object getHandle() {
        return this.handle;
    }

    public String getName() {
        return "world";
    }
}
