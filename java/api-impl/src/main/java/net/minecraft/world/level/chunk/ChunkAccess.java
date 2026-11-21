package net.minecraft.world.level.chunk;

import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.LevelHeightAccessor;
import net.minecraft.world.level.levelgen.Heightmap;

/** NMS {@code ChunkAccess} (в 26.3 — абстрактный класс). */
public abstract class ChunkAccess implements LevelHeightAccessor {
    /** {@code ChunkAccess.canvas$getAllBlockEntities()} (CraftEngine). */
    public java.util.Collection<net.minecraft.world.level.block.entity.BlockEntity> canvas$getAllBlockEntities() {
        return java.util.List.of();
    }

    /** Поле, которое ищет CraftEngine (ChunkAccessProxy#getPersistentDataContainer). */
    protected net.minecraft.nbt.CompoundTag persistentDataContainer =
            new net.minecraft.nbt.CompoundTag();

    protected ChunkAccess() {
    }

    public abstract ChunkPos getPos();

    @Override
    public abstract int getMinY();

    @Override
    public abstract int getHeight();

    public abstract LevelChunkSection[] getSections();

    public net.minecraft.world.level.chunk.status.ChunkStatus getPersistedStatus() {
        return net.minecraft.world.level.chunk.status.ChunkStatus.FULL;
    }

    public net.minecraft.world.level.levelgen.BelowZeroRetrogen getBelowZeroRetrogen() {
        return null;
    }

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
