package net.minecraft.world;

import net.minecraft.core.BlockPos;
import net.minecraft.world.level.LevelAccessor;
import net.minecraft.world.level.block.state.BlockState;

/** NMS {@code WorldlyContainerHolder} (CraftEngine). */
public interface WorldlyContainerHolder {
    WorldlyContainer getContainer(BlockState state, LevelAccessor level, BlockPos pos);
}
