package net.minecraft.world.level;

import net.minecraft.core.BlockPos;
import net.minecraft.world.level.block.state.BlockState;

/** NMS {@code BlockGetter}. */
public interface BlockGetter {
    /** {@code BlockGetter.getBlockEntity(BlockPos)} (CraftEngine). */
    default net.minecraft.world.level.block.entity.BlockEntity getBlockEntity(
            BlockPos pos) {
        return null;
    }

    /** {@code BlockGetter.getBlockFloorHeight(BlockPos)} (CraftEngine). */
    default double getBlockFloorHeight(BlockPos pos) {
        return 0.0;
    }

    /** {@code BlockGetter.getBlockFloorHeight(VoxelShape, Supplier)}. */
    default double getBlockFloorHeight(net.minecraft.world.phys.shapes.VoxelShape shape,
            java.util.function.Supplier<net.minecraft.world.phys.shapes.VoxelShape> fallback) {
        return 0.0;
    }

    /** {@code BlockGetter.getFluidState(BlockPos)} (CraftEngine). */
    default net.minecraft.world.level.material.FluidState getFluidState(BlockPos pos) {
        return net.minecraft.world.level.material.FluidState.EMPTY;
    }

    default BlockState getBlockState(BlockPos pos) {
        return net.minecraft.world.level.block.Blocks.AIR.defaultBlockState();
    }

    default BlockState getBlockStateIfLoaded(BlockPos pos) {
        return getBlockState(pos);
    }

    default net.minecraft.world.level.material.FluidState getFluidIfLoaded(BlockPos pos) {
        return getFluidState(pos);
    }

    default net.minecraft.world.phys.BlockHitResult clip(ClipContext context) {
        return null;
    }
}
