package net.minecraft.world.phys;

import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;

/** NMS {@code BlockHitResult}. */
public class BlockHitResult extends HitResult {
    private final BlockPos blockPos;
    private final Direction direction;
    /** Поле, которое ищет CraftEngine (BlockHitResultProxy#isInside). */
    private boolean inside;

    public BlockHitResult(Vec3 location, Direction direction, BlockPos blockPos, boolean miss) {
        super(location);
        this.blockPos = blockPos;
        this.direction = direction;
    }

    @Override
    public Type getType() {
        return this.blockPos == null ? Type.MISS : Type.BLOCK;
    }

    public BlockPos getBlockPos() {
        return this.blockPos;
    }

    public Direction getDirection() {
        return this.direction;
    }
}
