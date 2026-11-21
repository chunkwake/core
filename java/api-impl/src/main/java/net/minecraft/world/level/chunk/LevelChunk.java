package net.minecraft.world.level.chunk;

import net.minecraft.core.BlockPos;
import net.minecraft.world.level.block.state.BlockState;

/**
 * NMS {@code LevelChunk} поверх Rust-мира: секции читают блоки через
 * {@code ServerLevel.blockStateAt}. Нужен squaremap/BlueMap-подобным
 * плагинам, которые обходят чанки.
 */
public class LevelChunk extends ChunkAccess {
    private final net.minecraft.server.level.ServerLevel level;
    private final int x;
    private final int z;
    private final LevelChunkSection[] sections;

    public LevelChunk() {
        this(null, 0, 0);
    }

    public LevelChunk(int x, int z) {
        this(null, x, z);
    }

    public LevelChunk(net.minecraft.server.level.ServerLevel level, int x, int z) {
        this.level = level;
        this.x = x;
        this.z = z;
        this.sections = new LevelChunkSection[24];
    }

    /** Поле, которое ищет CraftEngine (setBlockEntities). */
    private java.util.Map<net.minecraft.core.BlockPos, net.minecraft.world.level.block.entity.BlockEntity> blockEntities =
            new java.util.HashMap<>();

    /** {@code LevelChunk.canvas$getAllBlockEntities()} (CraftEngine). */
    public java.util.Collection<net.minecraft.world.level.block.entity.BlockEntity> canvas$getAllBlockEntities() {
        return java.util.List.of();
    }

    /** Поле, которое ищет CraftEngine (LevelChunkProxy#getPersistentDataContainer). */
    private net.minecraft.nbt.CompoundTag persistentDataContainer =
            new net.minecraft.nbt.CompoundTag();

    /** {@code LevelChunk.markUnsaved()} (CraftEngine). */
    public void markUnsaved() {
    }

    public int getPosX() { return this.x; }
    public int getPosZ() { return this.z; }

    public net.minecraft.world.level.ChunkPos getPos() {
        return new net.minecraft.world.level.ChunkPos(this.x, this.z);
    }

    @Override
    public int getMinY() { return -64; }

    @Override
    public int getHeight() { return 384; }

    @Override
    public boolean isEmpty() { return false; }

    public int getMinSectionY() { return -4; }
    public int getMaxSectionY() { return 19; }

    /** Секция по индексу (0 = самая нижняя, как в ванили). */
    public LevelChunkSection getSection(int index) {
        int i = index + 4;
        if (i < 0 || i >= this.sections.length) {
            return new LevelChunkSection(0);
        }
        LevelChunkSection section = this.sections[i];
        if (section == null) {
            section = new LevelChunkSection(this.level, this.x, index, this.z);
            this.sections[i] = section;
        }
        return section;
    }

    public LevelChunkSection[] getSections() {
        for (int i = 0; i < this.sections.length; i++) {
            if (this.sections[i] == null) {
                this.sections[i] = new LevelChunkSection(this.level, this.x, i - 4, this.z);
            }
        }
        return this.sections;
    }

    public BlockState getBlockState(BlockPos pos) {
        if (this.level == null) {
            return net.minecraft.world.level.block.Blocks.AIR.defaultBlockState();
        }
        return this.level.getBlockState(pos);
    }

    public void updateBlockEntityTicker(
            net.minecraft.world.level.block.entity.BlockEntity blockEntity) {
    }
}
