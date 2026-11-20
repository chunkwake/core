package net.minecraft.world.level.chunk;

import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.LevelHeightAccessor;
import net.minecraft.world.level.levelgen.Heightmap;

/** NMS {@code ChunkAccess} (в 26.3 — абстрактный класс). */
public abstract class ChunkAccess implements LevelHeightAccessor {
    protected ChunkAccess() {
    }

    public abstract ChunkPos getPos();

    @Override
    public abstract int getMinY();

    @Override
    public abstract int getHeight();

    public abstract LevelChunkSection[] getSections();

    public boolean hasPrimedHeightmap(Heightmap.Types type) {
        return getOrCreateHeightmapUnprimed(type) != null;
    }

    public Heightmap getOrCreateHeightmapUnprimed(Heightmap.Types type) {
        return new Heightmap(type, this);
    }

    public boolean isEmpty() {
        return false;
    }
}
