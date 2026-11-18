package net.minecraft.world.level.chunk;

import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;

/** NMS {@code LevelChunkSection} (Orebfuscator). */
public class LevelChunkSection {
    private final int sectionY;

    public LevelChunkSection(int sectionY) {
        this.sectionY = sectionY;
    }

    public int getSectionY() {
        return this.sectionY;
    }

    public BlockState getBlockState(int x, int y, int z) {
        return Blocks.AIR.defaultBlockState();
    }

    public BlockState setBlockState(int x, int y, int z, BlockState state) {
        return Blocks.AIR.defaultBlockState();
    }

    public boolean hasOnlyAir() {
        return true;
    }

    public boolean maybeHas(java.util.function.Predicate<BlockState> predicate) {
        return false;
    }

    public net.minecraft.world.level.chunk.PalettedContainer<BlockState> getStates() {
        return new net.minecraft.world.level.chunk.PalettedContainer<>(Blocks.AIR.defaultBlockState(),
                net.minecraft.world.level.chunk.Strategy.createForBlockStates(
                        net.minecraft.world.level.block.Block.BLOCK_STATE_REGISTRY));
    }
}
