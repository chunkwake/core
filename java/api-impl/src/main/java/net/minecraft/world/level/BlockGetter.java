package net.minecraft.world.level;

import net.minecraft.core.BlockPos;
import net.minecraft.world.level.block.state.BlockState;

/** NMS {@code BlockGetter}. */
public interface BlockGetter {
    /** {@code BlockGetter.getFluidState(BlockPos)} (CraftEngine). */
    default net.minecraft.world.level.material.FluidState getFluidState(BlockPos pos) {
        return net.minecraft.world.level.material.FluidState.EMPTY;
    }

    default BlockState getBlockState(BlockPos pos) {
        return net.minecraft.world.level.block.Blocks.AIR.defaultBlockState();
    }
}
