package dev.chunkwake.bridge;

import org.bukkit.Location;
import org.bukkit.Material;
import org.bukkit.World;
import org.bukkit.block.Block;
import org.bukkit.block.BlockFace;

/** {@link Block}: живой блок мира (тип читается из Rust; {@code key} — снимок события). */
final class BlockImpl {
    final Block api = Stub.of(Block.class, this);
    private final World world;
    private final int x, y, z;
    /** Ключ блока на момент события, например {@code minecraft:stone}; {@code null} — читать из мира. */
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
    public Block getRelative(int dx, int dy, int dz) { return new BlockImpl(world, x + dx, y + dy, z + dz, null).api; }
    public Block getRelative(BlockFace f) { return getRelative(f.getModX(), f.getModY(), f.getModZ()); }
    public Block getRelative(BlockFace f, int n) {
        return getRelative(f.getModX() * n, f.getModY() * n, f.getModZ() * n);
    }

    public Material getType() {
        String k = key != null ? key : WorldImpl.request(world, "get " + x + " " + y + " " + z);
        Material m = k.isEmpty() ? null : Material.matchMaterial(k);
        return m == null ? Material.AIR : m;
    }

    public boolean isEmpty() { return getType().isAir(); }

    public void setType(Material type) { setType(type, true); }

    /** Состояние по умолчанию для типа; с физикой соседи (редстоун) обновятся в тике. */
    public void setType(Material type, boolean physics) {
        if (!type.isBlock()) {
            throw new IllegalArgumentException("Material " + type + " is not a block");
        }
        WorldImpl.request(world, "set " + x + " " + y + " " + z + " " + type.getKey() + (physics ? " 1" : " 0"));
    }

    public org.bukkit.block.data.BlockData getBlockData() {
        return BlockDataImpl.parse(WorldImpl.request(world, "data " + x + " " + y + " " + z));
    }

    /** Свет неба и блоков одной строкой `sky block`: BlueMap рисует по нему освещение. */
    private String light() {
        return WorldImpl.request(world, "light " + x + " " + y + " " + z);
    }

    public byte getLightFromSky() {
        String line = light();
        return line.isEmpty() ? 15 : Byte.parseByte(line.substring(0, line.indexOf(' ')));
    }

    public byte getLightFromBlocks() {
        String line = light();
        return line.isEmpty() ? 0 : Byte.parseByte(line.substring(line.indexOf(' ') + 1).trim());
    }

    public byte getLightLevel() {
        return (byte) Math.max(getLightFromSky(), getLightFromBlocks());
    }

    public void setBlockData(org.bukkit.block.data.BlockData data) { setBlockData(data, true); }

    public void setBlockData(org.bukkit.block.data.BlockData data, boolean physics) {
        WorldImpl.request(world, "set " + x + " " + y + " " + z + " " + data.getAsString() + (physics ? " 1" : " 0"));
    }

    /** Снимок блока: у табличек — {@link org.bukkit.block.Sign}, иначе простой {@code BlockState}. */
    public org.bukkit.block.BlockState getState() {
        org.bukkit.block.Sign sign = SignImpl.of(api);
        return sign != null ? sign : Stub.of(org.bukkit.block.BlockState.class, new Events.StateImpl(api));
    }

    public org.bukkit.block.BlockState getState(boolean useSnapshot) { return getState(); }

    public void setMetadata(String k, org.bukkit.metadata.MetadataValue v) {
        Bridge.server.metadata().blocks.setMetadata(api, k, v);
    }
    public java.util.List<org.bukkit.metadata.MetadataValue> getMetadata(String k) {
        return Bridge.server.metadata().blocks.getMetadata(api, k);
    }
    public boolean hasMetadata(String k) { return Bridge.server.metadata().blocks.hasMetadata(api, k); }
    public void removeMetadata(String k, org.bukkit.plugin.Plugin owner) {
        Bridge.server.metadata().blocks.removeMetadata(api, k, owner);
    }

    @Override
    public boolean equals(Object o) {
        return o instanceof BlockImpl b && b.world == world && b.x == x && b.y == y && b.z == z;
    }

    @Override
    public int hashCode() {
        return (x * 31 + y) * 31 + z;
    }

    @Override
    public String toString() {
        return "ChunkwakeBlock{" + key + " at " + x + "," + y + "," + z + "}";
    }
}
