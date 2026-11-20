package net.minecraft.world.level.material;

/** NMS {@code Fluid}. */
public class Fluid {
    private final String name;

    public Fluid(String name) {
        this.name = name;
    }

    public String name() {
        return this.name;
    }

    public FluidState defaultFluidState() {
        return new FluidState(this);
    }

    @Override
    public String toString() {
        return this.name;
    }
}
