package ca.spottedleaf.moonrise.patches.chunk_system.scheduling;

import net.minecraft.server.level.ServerLevel;

/** Заглушка Moonrise {@code ChunkHolderManager}. */
public class ChunkHolderManager {
    private final ServerLevel level;

    public ChunkHolderManager(ServerLevel level) {
        this.level = level;
    }

    public NewChunkHolder getChunkHolder(int x, int z) {
        return new NewChunkHolder(this.level, x, z);
    }
}
