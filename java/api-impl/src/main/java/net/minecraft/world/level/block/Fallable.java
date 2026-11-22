package net.minecraft.world.level.block;

/** NMS {@code Fallable}: блок, который умеет падать (песок, гравий). */
public interface Fallable {
    default void onLand(net.minecraft.world.level.Level level, net.minecraft.core.BlockPos pos,
            net.minecraft.world.level.block.state.BlockState state,
            net.minecraft.world.level.block.state.BlockState replacedState,
            net.minecraft.world.entity.item.FallingBlockEntity entity) {
    }

    default void onBrokenAfterFall(net.minecraft.world.level.Level level,
            net.minecraft.core.BlockPos pos,
            net.minecraft.world.entity.item.FallingBlockEntity entity) {
    }
}
