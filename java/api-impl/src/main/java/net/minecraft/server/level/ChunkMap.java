package net.minecraft.server.level;

/** NMS {@code ChunkMap} (тип поля ServerChunkCache). */
public class ChunkMap {
    /** Поле, которое ищет CraftEngine (ChunkMapProxy#getWorldGenContext). */
    private net.minecraft.world.level.chunk.status.WorldGenContext worldGenContext =
            new net.minecraft.world.level.chunk.status.WorldGenContext();

    /** Поле, которое ищет CraftEngine (ChunkMapProxy#getGenerator). */
    private net.minecraft.world.level.chunk.ChunkGenerator generator;

    public ChunkMap() {
    }
}
