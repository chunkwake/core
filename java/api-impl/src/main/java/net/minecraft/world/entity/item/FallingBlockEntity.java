package net.minecraft.world.entity.item;

import net.minecraft.core.BlockPos;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.state.BlockState;

/** NMS {@code FallingBlockEntity} (MythicMobs). */
public class FallingBlockEntity extends Entity {
    private BlockState blockState;

    public FallingBlockEntity(EntityType<?> type, Level level) {
        super(type, level);
    }

    public static FallingBlockEntity fall(Level level, BlockPos pos, BlockState state) {
        FallingBlockEntity entity = new FallingBlockEntity(null, level);
        entity.blockState = state;
        return entity;
    }

    public BlockState getBlockState() {
        return this.blockState;
    }

    public void setHurtsEntities(float damage, int maxDamage) {
    }

    public void tick() {
    }

    public void setStartPos(BlockPos pos) {
    }
}
