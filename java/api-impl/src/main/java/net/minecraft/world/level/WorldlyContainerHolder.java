package net.minecraft.world.level;

import net.minecraft.core.BlockPos;
import net.minecraft.world.level.block.state.BlockState;

/** NMS {@code WorldlyContainerHolder}: контейнер, привязанный к блоку. */
public interface WorldlyContainerHolder {
    net.minecraft.world.WorldlyContainer getContainer(BlockState state, LevelAccessor level, BlockPos pos);
}
