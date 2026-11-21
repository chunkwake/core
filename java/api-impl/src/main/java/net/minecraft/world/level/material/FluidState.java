package net.minecraft.world.level.material;

/** NMS {@code FluidState}: состояние жидкости (water/lava/empty). */
public final class FluidState {
    /** {@code FluidState.createLegacyBlock()} (CraftEngine). */
    public net.minecraft.world.level.block.state.BlockState createLegacyBlock() {
        return null;
    }

    /** {@code FluidState.is(TagKey)} (CraftEngine). */
    public boolean is(net.minecraft.tags.TagKey<Fluid> tag) {
        return false;
    }

    /** {@code FluidState.getAmount()} (CraftEngine). */
    public int getAmount() {
        return 0;
    }

    public static final FluidState EMPTY = new FluidState(Fluids.EMPTY);
    public static final FluidState WATER = new FluidState(Fluids.WATER);

    private final Fluid fluid;

    public FluidState(Fluid fluid) {
        this.fluid = fluid == null ? Fluids.EMPTY : fluid;
    }

    public boolean isEmpty() {
        return this.fluid == Fluids.EMPTY;
    }

    /** {@code FluidState.getType()}: тип жидкости. */
    public Fluid getType() {
        return this.fluid;
    }
}
