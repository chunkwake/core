package net.minecraft.nbt;

/** NBT int. */
public final class IntTag implements Tag {
    private final int value;

    public IntTag(int value) {
        this.value = value;
    }

    public static IntTag valueOf(int value) {
        return new IntTag(value);
    }

    public int getAsInt() {
        return this.value;
    }

    @Override
    public byte getId() {
        return 3;
    }

    @Override
    public String toString() {
        return Integer.toString(this.value);
    }
}
