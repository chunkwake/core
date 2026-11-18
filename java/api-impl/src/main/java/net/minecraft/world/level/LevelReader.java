package net.minecraft.world.level;

/** NMS {@code LevelReader}. */
public interface LevelReader extends BlockGetter {
    int getHeight();

    default boolean isEmptyBlock(net.minecraft.core.BlockPos pos) {
        return this.getBlockState(pos).is(net.minecraft.world.level.block.Blocks.AIR);
    }
}
