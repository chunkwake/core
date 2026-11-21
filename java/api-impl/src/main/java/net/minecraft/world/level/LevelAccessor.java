package net.minecraft.world.level;

/** NMS {@code LevelAccessor}. */
public interface LevelAccessor extends LevelReader {
    default net.minecraft.util.RandomSource getRandom() {
        return null;
    }

    boolean setBlock(net.minecraft.core.BlockPos pos,
            net.minecraft.world.level.block.state.BlockState state, int flags);
}
