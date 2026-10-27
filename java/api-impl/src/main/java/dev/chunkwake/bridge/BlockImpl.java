package dev.chunkwake.bridge;

import org.bukkit.Location;
import org.bukkit.Material;
import org.bukkit.World;
import org.bukkit.block.Block;

/** {@link Block}: снимок блока на момент события. */
final class BlockImpl {
    final Block api = Stub.of(Block.class, this);
    private final World world;
    private final int x, y, z;
    /** Ключ блока, например {@code minecraft:stone}. */
    private final String key;

    BlockImpl(World world, int x, int y, int z, String key) {
        this.world = world;
        this.x = x;
        this.y = y;
        this.z = z;
        this.key = key;
    }

    public int getX() { return x; }
    public int getY() { return y; }
    public int getZ() { return z; }
    public World getWorld() { return world; }
    public Location getLocation() { return new Location(world, x, y, z); }

    public Material getType() {
        Material m = Material.matchMaterial(key);
        return m == null ? Material.AIR : m;
    }

    @Override
    public String toString() {
        return "ChunkwakeBlock{" + key + " at " + x + "," + y + "," + z + "}";
    }
}
