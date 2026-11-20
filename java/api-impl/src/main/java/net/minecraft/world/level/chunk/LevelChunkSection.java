package net.minecraft.world.level.chunk;

import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;

/**
 * NMS {@code LevelChunkSection}. С реальным {@code ServerLevel} читает блоки
 * из Rust по глобальным координатам; без него — воздух (Orebfuscator).
 */
public class LevelChunkSection {
    private final int sectionY;
    private final net.minecraft.server.level.ServerLevel level;
    private final int chunkX;
    private final int chunkZ;

    public LevelChunkSection(int sectionY) {
        this(null, 0, sectionY, 0);
    }

    public LevelChunkSection(net.minecraft.server.level.ServerLevel level, int chunkX,
            int sectionY, int chunkZ) {
        this.level = level;
        this.chunkX = chunkX;
        this.sectionY = sectionY;
        this.chunkZ = chunkZ;
    }

    public int getSectionY() {
        return this.sectionY;
    }

    public BlockState getBlockState(int x, int y, int z) {
        if (this.level == null) {
            return Blocks.AIR.defaultBlockState();
        }
        return this.level.getBlockState(new net.minecraft.core.BlockPos(
                this.chunkX * 16 + x, this.sectionY * 16 + y, this.chunkZ * 16 + z));
    }

    public BlockState setBlockState(int x, int y, int z, BlockState state) {
        return Blocks.AIR.defaultBlockState();
    }

    public boolean hasOnlyAir() {
        return false;
    }

    public boolean maybeHas(java.util.function.Predicate<BlockState> predicate) {
        return true;
    }

    public net.minecraft.world.level.chunk.PalettedContainer<BlockState> getStates() {
        return new net.minecraft.world.level.chunk.PalettedContainer<>(Blocks.AIR.defaultBlockState(),
                net.minecraft.world.level.chunk.Strategy.createForBlockStates(
                        net.minecraft.world.level.block.Block.BLOCK_STATE_REGISTRY));
    }
}
