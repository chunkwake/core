package net.minecraft.world.level;

import net.minecraft.core.BlockPos;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.block.state.BlockState;

/** NMS {@code LevelWriter}. */
public interface LevelWriter {
    boolean setBlock(BlockPos pos, BlockState state, int flags, int recursionLeft);

    default boolean setBlock(BlockPos pos, BlockState state, int flags) {
        return setBlock(pos, state, flags, 512);
    }

    default boolean setBlockAndUpdate(BlockPos pos, BlockState state) {
        return setBlock(pos, state, 3);
    }

    boolean removeBlock(BlockPos pos, boolean move);

    default boolean destroyBlock(BlockPos pos, boolean dropBlock) {
        return destroyBlock(pos, dropBlock, null);
    }

    default boolean destroyBlock(BlockPos pos, boolean dropBlock, Entity entity) {
        return destroyBlock(pos, dropBlock, entity, 512);
    }

    boolean destroyBlock(BlockPos pos, boolean dropBlock, Entity entity, int recursionLeft);

    default boolean addFreshEntity(Entity entity) {
        return false;
    }

    default boolean addFreshEntity(Entity entity,
            org.bukkit.event.entity.CreatureSpawnEvent.SpawnReason reason) {
        return false;
    }
}
