package net.minecraft.world.level.material;

/** NMS {@code Fluids}: ванильные жидкости. */
public final class Fluids {
    public static final Fluid EMPTY = new Fluid("empty");
    public static final FlowingFluid WATER = new FlowingFluid("water");
    public static final FlowingFluid LAVA = new FlowingFluid("lava");
    public static final FlowingFluid FLOWING_WATER = new FlowingFluid("flowing_water");
    public static final FlowingFluid FLOWING_LAVA = new FlowingFluid("flowing_lava");

    private Fluids() {
    }
}
