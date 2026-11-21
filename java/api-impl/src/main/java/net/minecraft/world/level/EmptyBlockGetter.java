package net.minecraft.world.level;

import net.minecraft.core.BlockPos;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.material.FluidState;
import net.minecraft.world.level.material.Fluids;

/** NMS {@code EmptyBlockGetter.INSTANCE}: пустой мир для рендера. */
public final class EmptyBlockGetter implements BlockGetter {
    public static final EmptyBlockGetter INSTANCE = new EmptyBlockGetter();

    private EmptyBlockGetter() {
    }


    @Override
    public BlockState getBlockState(BlockPos pos) {
        return Blocks.AIR.defaultBlockState();
    }

    public FluidState getFluidState(BlockPos pos) {
        return Fluids.EMPTY.defaultFluidState();
    }

    public int getMinY() {
        return 0;
    }

    public int getHeight() {
        return 384;
    }

    public net.minecraft.world.level.block.entity.BlockEntity getBlockEntity(BlockPos pos) {
        return null;
    }
}
