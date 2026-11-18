package net.minecraft.world.level.chunk;

import net.minecraft.world.level.ChunkPos;

/** NMS {@code ChunkAccess} (Orebfuscator). */
public interface ChunkAccess {
    ChunkPos getPos();

    int getMinY();

    int getHeight();

    default boolean isEmpty() {
        return false;
    }
}
