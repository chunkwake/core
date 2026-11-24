package dev.chunkwake.bridge;

import org.bukkit.Chunk;
import org.bukkit.ChunkSnapshot;
import org.bukkit.World;
import org.bukkit.block.Block;
import org.bukkit.block.BlockState;
import org.bukkit.entity.Entity;
import org.bukkit.plugin.Plugin;

/** Chunk, который отдаёт Rust-сторона (координаты загруженных колонн). */
final class ChunkImpl {
    private final WorldImpl world;
    private final int x;
    private final int z;
    private final java.util.Set<Plugin> tickets = java.util.concurrent.ConcurrentHashMap.newKeySet();
    private boolean forceLoaded;

    ChunkImpl(WorldImpl world, int x, int z) {
        this.world = world;
        this.x = x;
        this.z = z;
    }

    public int getX() { return this.x; }
    public int getZ() { return this.z; }
    public World getWorld() { return this.world.api; }

    public Block getBlock(int bx, int by, int bz) {
        return this.world.getBlockAt((this.x << 4) + bx, by, (this.z << 4) + bz);
    }

    public boolean isEntitiesLoaded() { return true; }
    public Entity[] getEntities() { return new Entity[0]; }
    public BlockState[] getTileEntities(boolean useSnapshot) { return new BlockState[0]; }
    public java.util.Collection<BlockState> getTileEntities(
            java.util.function.Predicate<? super Block> predicate, boolean useSnapshot) {
        return java.util.List.of();
    }

    public boolean isGenerated() { return true; }
    public boolean isLoaded() { return true; }
    public boolean load(boolean generate) { return true; }
    public boolean load() { return true; }
    public boolean unload(boolean save) { return false; }
    public boolean unload() { return false; }
    public boolean isSlimeChunk() { return false; }
    public boolean isForceLoaded() { return this.forceLoaded || !this.tickets.isEmpty(); }

    private void forceload(boolean add) {
        this.world.request("forceload " + (this.x << 4) + " " + (this.z << 4)
                + (add ? " add" : " remove"));
    }

    public void setForceLoaded(boolean force) {
        if (this.forceLoaded != force) {
            this.forceLoaded = force;
            this.forceload(force);
        }
    }

    public boolean addPluginChunkTicket(Plugin plugin) {
        boolean added = this.tickets.add(plugin);
        if (added && this.tickets.size() == 1) {
            this.forceload(true);
        }
        return added;
    }

    public boolean removePluginChunkTicket(Plugin plugin) {
        boolean removed = this.tickets.remove(plugin);
        if (removed && this.tickets.isEmpty() && !this.forceLoaded) {
            this.forceload(false);
        }
        return removed;
    }

    public java.util.Collection<Plugin> getPluginChunkTickets() {
        return java.util.Set.copyOf(this.tickets);
    }
    public long getInhabitedTime() { return 0L; }
    public ChunkSnapshot getChunkSnapshot(boolean includeMaxBlockY, boolean includeBiome,
            boolean includeBiomeTempRain, boolean includeLightData) {
        return null;
    }

    static Chunk api(WorldImpl world, int x, int z) {
        return Stub.of(Chunk.class, new ChunkImpl(world, x, z));
    }
}
