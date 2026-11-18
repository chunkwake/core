package net.minecraft.server.level;

import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.chunk.LevelChunk;

/** NMS {@code ServerChunkCache}. */
public class ServerChunkCache {
    public LevelChunk getChunkNow(int x, int z) {
        return null;
    }

    public LevelChunk getChunk(int x, int z, Object status, boolean load) {
        return null;
    }

    public boolean hasChunk(int x, int z) {
        return false;
    }

    public net.minecraft.world.level.chunk.ChunkAccess getChunk(ChunkPos pos) {
        return null;
    }
}
