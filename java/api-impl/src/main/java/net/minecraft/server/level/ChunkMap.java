package net.minecraft.server.level;

/** NMS {@code ChunkMap} (тип поля ServerChunkCache). */
public class ChunkMap {
    /** Поле, которое ищет CraftEngine (ChunkMapProxy#getGenerator). */
    private net.minecraft.world.level.chunk.ChunkGenerator generator;

    public ChunkMap() {
    }
}
