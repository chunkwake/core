package net.minecraft.server.level;

import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.chunk.LevelChunk;

/** NMS {@code ServerChunkCache}. */
public class ServerChunkCache {
    /** {@code ServerChunkCache.getGenerator()} (CraftEngine). */
    public net.minecraft.world.level.chunk.ChunkGenerator getGenerator() {
        return null;
    }

    /** Поле, которое squaremap читает для плана рендера. */
    public net.minecraft.server.level.ChunkMap chunkMap = new net.minecraft.server.level.ChunkMap();

    public LevelChunk getChunkNow(int x, int z) {
        return null;
    }

    public LevelChunk getChunk(int x, int z, Object status, boolean load) {
        return null;
    }

    public boolean hasChunk(int x, int z) {
        return false;
    }

    /** {@code ServerChunkCache.isChunkLoaded(int,int)} (Orebfuscator). */
    public boolean isChunkLoaded(int x, int z) {
        return false;
    }

    public net.minecraft.world.level.chunk.ChunkAccess getChunk(ChunkPos pos) {
        return null;
    }
}
