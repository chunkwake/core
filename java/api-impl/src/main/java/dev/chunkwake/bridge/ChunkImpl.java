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

    public boolean isEntitiesLoaded() { return isLoaded(); }

    public Entity[] getEntities() {
        java.util.List<Entity> out = new java.util.ArrayList<>();
        for (Entity entity : this.world.getEntities()) {
            org.bukkit.Location at = entity.getLocation();
            if ((at.getBlockX() >> 4) == this.x && (at.getBlockZ() >> 4) == this.z) {
                out.add(entity);
            }
        }
        return out.toArray(new Entity[0]);
    }
    public BlockState[] getTileEntities(boolean useSnapshot) {
        java.util.List<BlockState> out = new java.util.ArrayList<>();
        String page = this.world.request("blockentities " + this.x + " " + this.z);
        for (String line : page.split("\n")) {
            if (line.isEmpty()) {
                continue;
            }
            String[] f = line.split(" ");
            if (f.length < 3) {
                continue;
            }
            try {
                int bx = Integer.parseInt(f[0]);
                int by = Integer.parseInt(f[1]);
                int bz = Integer.parseInt(f[2]);
                out.add(this.world.getBlockAt(bx, by, bz).getState());
            } catch (NumberFormatException ignored) {
                // Битый ответ Rust — пропускаем строку.
            }
        }
        return out.toArray(new BlockState[0]);
    }

    public java.util.Collection<BlockState> getTileEntities(
            java.util.function.Predicate<? super Block> predicate, boolean useSnapshot) {
        java.util.List<BlockState> out = new java.util.ArrayList<>();
        for (BlockState state : getTileEntities(useSnapshot)) {
            if (predicate == null || predicate.test(state.getBlock())) {
                out.add(state);
            }
        }
        return out;
    }

    public boolean isGenerated() { return true; }
    public boolean isLoaded() {
        return "1".equals(this.world.request("chunkloaded " + this.x + " " + this.z).trim());
    }

    public boolean load(boolean generate) {
        return "1".equals(this.world.request("loadchunk " + this.x + " " + this.z
                + (generate ? " 1" : " 0")).trim());
    }

    public boolean load() {
        return load(true);
    }
    public boolean unload(boolean save) {
        return "1".equals(this.world.request("unloadchunk " + this.x + " " + this.z
                + (save ? " 1" : " 0")).trim());
    }

    public boolean unload() {
        return unload(true);
    }

    public boolean isSlimeChunk() {
        // `WorldgenRandom.seedSlimeChunk` + Spigot `slimeSeed` (987234911).
        long seed = this.world.api.getSeed()
                + (long) this.x * this.x * 4987142L
                + (long) this.x * 5947611L
                + (long) this.z * this.z * 4392871L
                + (long) this.z * 389711L;
        seed ^= 987234911L;
        return new java.util.Random(seed).nextInt(10) == 0;
    }
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
        int flags = (includeMaxBlockY ? 0b1 : 0)
                | (includeBiome ? 0b10 : 0)
                | (includeBiomeTempRain ? 0b100 : 0)
                | (includeLightData ? 0b1000 : 0);
        String hex = this.world.requestLarge(
                "chunksnapshot " + this.x + " " + this.z + " " + flags);
        if (hex.isEmpty()) {
            return null;
        }
        try {
            byte[] compressed = java.util.HexFormat.of().parseHex(hex);
            return ChunkSnapshotImpl.parse(this.world, this.x, this.z, flags, compressed);
        } catch (IllegalArgumentException e) {
            throw new IllegalStateException("chunk snapshot: " + e.getMessage(), e);
        }
    }

    static Chunk api(WorldImpl world, int x, int z) {
        return Stub.of(Chunk.class, new ChunkImpl(world, x, z));
    }
}
