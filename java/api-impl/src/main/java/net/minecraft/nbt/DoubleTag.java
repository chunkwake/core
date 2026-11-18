package net.minecraft.nbt;

/** NBT double. */
public final class DoubleTag implements Tag {
    private final double value;

    public DoubleTag(double value) {
        this.value = value;
    }

    public static DoubleTag valueOf(double value) {
        return new DoubleTag(value);
    }

    public double getAsDouble() {
        return this.value;
    }

    @Override
    public byte getId() {
        return 6;
    }

    @Override
    public String toString() {
        return Double.toString(this.value);
    }
}
