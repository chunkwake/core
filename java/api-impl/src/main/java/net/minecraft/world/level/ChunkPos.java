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

    /** {@code ChunkPos.rangeClosed(ChunkPos, int)} (CraftEngine). */
    public static java.util.stream.Stream<ChunkPos> rangeClosed(ChunkPos center, int radius) {
        java.util.List<ChunkPos> out = new java.util.ArrayList<>();
        for (int dx = -radius; dx <= radius; dx++) {
            for (int dz = -radius; dz <= radius; dz++) {
                out.add(new ChunkPos(center.x + dx, center.z + dz));
            }
        }
        return out.stream();
    }

    /** {@code ChunkPos.rangeClosed(ChunkPos, ChunkPos)} (CraftEngine). */
    public static java.util.stream.Stream<ChunkPos> rangeClosed(ChunkPos from, ChunkPos to) {
        java.util.List<ChunkPos> out = new java.util.ArrayList<>();
        for (int x = Math.min(from.x, to.x); x <= Math.max(from.x, to.x); x++) {
            for (int z = Math.min(from.z, to.z); z <= Math.max(from.z, to.z); z++) {
                out.add(new ChunkPos(x, z));
            }
        }
        return out.stream();
    }

    /** {@code ChunkPos.getWorldPosition()} (CraftEngine). */
    public net.minecraft.core.BlockPos getWorldPosition() {
        return new net.minecraft.core.BlockPos(this.x << 4, 0, this.z << 4);
    }

    public long pack() {
        return this.toLong();
    }

    public int x() {
        return this.x;
    }

    public int z() {
        return this.z;
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
