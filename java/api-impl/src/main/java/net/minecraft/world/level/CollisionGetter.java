package net.minecraft.world.level;

import java.util.List;
import net.minecraft.core.BlockPos;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.shapes.CollisionContext;
import net.minecraft.world.phys.shapes.VoxelShape;

/** NMS {@code CollisionGetter}. */
public interface CollisionGetter extends BlockGetter {
    default net.minecraft.world.level.border.WorldBorder getWorldBorder() {
        return null;
    }

    default BlockGetter getChunkForCollisions(int chunkX, int chunkZ) {
        return null;
    }

    default List<VoxelShape> getEntityCollisions(Entity entity, AABB box) {
        return List.of();
    }

    default Iterable<VoxelShape> getBlockCollisions(Entity entity, AABB box) {
        return List.of();
    }

    default boolean isUnobstructed(Entity entity, VoxelShape shape) {
        return true;
    }

    default boolean isUnobstructed(BlockState state, BlockPos pos, CollisionContext context) {
        return true;
    }

    default boolean isUnobstructed(Entity entity) {
        return true;
    }
}
