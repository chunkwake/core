package org.bukkit.craftbukkit;

/** CraftBukkit {@code CraftChunk} (CraftEngine). */
public class CraftChunk {
    private Object world;

    public CraftChunk() {
    }

    public static org.bukkit.Chunk newInstance(Object handle) {
        return null;
    }

    public Object getWorld() {
        return this.world;
    }

    public void setWorld(Object world) {
        this.world = world;
    }
}
