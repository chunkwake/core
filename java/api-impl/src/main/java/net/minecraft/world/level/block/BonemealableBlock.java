package net.minecraft.world.level.block;

/** NMS {@code BonemealableBlock}: блок, который удобряется костной мукой. */
public interface BonemealableBlock {
    boolean isValidBonemealTarget(net.minecraft.world.level.LevelReader level,
            net.minecraft.core.BlockPos pos,
            net.minecraft.world.level.block.state.BlockState state, Object source);

    boolean isBonemealSuccess(net.minecraft.world.level.Level level,
            net.minecraft.util.RandomSource random, net.minecraft.core.BlockPos pos,
            net.minecraft.world.level.block.state.BlockState state, Object source);

    void performBonemeal(net.minecraft.server.level.ServerLevel level,
            net.minecraft.util.RandomSource random, net.minecraft.core.BlockPos pos,
            net.minecraft.world.level.block.state.BlockState state, Object source);
}
