package net.minecraft.world.level;

import net.minecraft.core.BlockPos;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.material.FluidState;
import net.minecraft.world.phys.Vec3;
import net.minecraft.world.phys.shapes.CollisionContext;
import net.minecraft.world.phys.shapes.VoxelShape;

/** NMS {@code ClipContext}: параметры трассировки луча по миру. */
public class ClipContext {
    private final Vec3 from;
    private final Vec3 to;
    private final Block block;
    public final Fluid fluid;
    private final CollisionContext collisionContext;

    public ClipContext(Vec3 from, Vec3 to, Block block, Fluid fluid, Entity entity) {
        this(from, to, block, fluid, CollisionContext.empty());
    }

    public ClipContext(Vec3 from, Vec3 to, Block block, Fluid fluid,
            CollisionContext collisionContext) {
        this.from = from;
        this.to = to;
        this.block = block;
        this.fluid = fluid;
        this.collisionContext = collisionContext;
    }

    public Vec3 getTo() {
        return this.to;
    }

    public Vec3 getFrom() {
        return this.from;
    }

    public VoxelShape getBlockShape(BlockState state, BlockGetter level, BlockPos pos) {
        return VoxelShape.EMPTY;
    }

    public VoxelShape getFluidShape(FluidState state, BlockGetter level, BlockPos pos) {
        return VoxelShape.EMPTY;
    }

    /** NMS {@code ClipContext.Block}. */
    public enum Block {
        COLLIDER,
        OUTLINE,
        VISUAL,
        FALLDAMAGE_RESETTING
    }

    /** NMS {@code ClipContext.Fluid}. */
    public enum Fluid {
        NONE,
        SOURCE_ONLY,
        ANY
    }
}
