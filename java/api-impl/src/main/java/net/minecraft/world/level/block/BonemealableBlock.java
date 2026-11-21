package net.minecraft.world.level.block;

import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.LevelReader;
import net.minecraft.world.level.block.state.BlockState;

/** NMS {@code BonemealableBlock}: блок, который удобряется костной мукой. */
public interface BonemealableBlock {
    boolean isValidBonemealTarget(LevelReader level, BlockPos pos, BlockState state,
            BonemealSource source);

    boolean isBonemealSuccess(Level level, RandomSource random, BlockPos pos, BlockState state,
            BonemealSource source);

    void performBonemeal(ServerLevel level, RandomSource random, BlockPos pos, BlockState state,
            BonemealSource source);

    default BlockPos getParticlePos(BlockPos pos) {
        return pos;
    }

    default Type getType() {
        return Type.GROWER;
    }

    /** NMS {@code BonemealableBlock.Type}. */
    enum Type {
        NEIGHBOR_SPREADER,
        GROWER
    }
}
