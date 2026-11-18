package net.minecraft.nbt;

/** NBT float. */
public final class FloatTag implements Tag {
    private final float value;

    public FloatTag(float value) {
        this.value = value;
    }

    public static FloatTag valueOf(float value) {
        return new FloatTag(value);
    }

    public float getAsFloat() {
        return this.value;
    }

    @Override
    public byte getId() {
        return 5;
    }

    @Override
    public String toString() {
        return Float.toString(this.value);
    }
}
