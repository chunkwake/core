package org.bukkit.craftbukkit;

/** CraftBukkit {@code CraftChunk} (CraftEngine). */
public class CraftChunk {
    private Object world;

    public CraftChunk() {
    }

    public static CraftChunk newInstance(Object handle) {
        return new CraftChunk();
    }

    public Object getWorld() {
        return this.world;
    }

    public void setWorld(Object world) {
        this.world = world;
    }
}
