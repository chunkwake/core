package net.minecraft.world.level;

import net.minecraft.core.BlockPos;

/**
 * NMS {@code ScheduledTickAccess}: планировщик тиков блоков.
 */
public interface ScheduledTickAccess {
    default void scheduleTick(BlockPos pos, net.minecraft.world.level.block.Block block, int delay) {
    }

    default void scheduleTick(BlockPos pos, net.minecraft.world.level.block.Block block, int delay,
            net.minecraft.world.ticks.TickPriority priority) {
    }

    default void scheduleTick(BlockPos pos, net.minecraft.world.level.material.Fluid fluid, int delay) {
    }

    default void scheduleTick(BlockPos pos, net.minecraft.world.level.material.Fluid fluid, int delay,
            net.minecraft.world.ticks.TickPriority priority) {
    }
}
