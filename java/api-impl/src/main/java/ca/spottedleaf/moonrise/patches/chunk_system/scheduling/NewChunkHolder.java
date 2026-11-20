package ca.spottedleaf.moonrise.patches.chunk_system.scheduling;

import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.chunk.ChunkAccess;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.chunk.status.ChunkStatus;

/** Заглушка Moonrise {@code NewChunkHolder}: отдаёт живой чанк из Rust. */
public class NewChunkHolder {
    private final ServerLevel level;
    private final int x;
    private final int z;

    public NewChunkHolder(ServerLevel level, int x, int z) {
        this.level = level;
        this.x = x;
        this.z = z;
    }

    public ChunkAccess getChunkIfPresentUnchecked(ChunkStatus status) {
        return new LevelChunk(this.level, this.x, this.z);
    }
}
