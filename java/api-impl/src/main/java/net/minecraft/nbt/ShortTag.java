package net.minecraft.nbt;

/** NBT short. */
public final class ShortTag implements Tag {
    private final short value;

    public ShortTag(short value) {
        this.value = value;
    }

    public static ShortTag valueOf(short value) {
        return new ShortTag(value);
    }

    public short getAsShort() {
        return this.value;
    }

    @Override
    public byte getId() {
        return 2;
    }

    @Override
    public String toString() {
        return Short.toString(this.value);
    }
}
