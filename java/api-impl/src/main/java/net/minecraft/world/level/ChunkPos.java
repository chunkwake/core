package net.minecraft.world.level;

import net.minecraft.core.BlockPos;

/** NMS {@code ChunkPos}. */
public class ChunkPos {
    public final int x;
    public final int z;

    public ChunkPos(int x, int z) {
        this.x = x;
        this.z = z;
    }

    public ChunkPos(long packed) {
        this.x = (int) packed;
        this.z = (int) (packed >> 32);
    }

    public static ChunkPos of(BlockPos pos) {
        return new ChunkPos(pos.getX() >> 4, pos.getZ() >> 4);
    }

    public long toLong() {
        return ((long) this.x & 0xFFFFFFFFL) | (((long) this.z & 0xFFFFFFFFL) << 32);
    }

    public int getMinBlockX() {
        return this.x << 4;
    }

    public int getMinBlockZ() {
        return this.z << 4;
    }
}
