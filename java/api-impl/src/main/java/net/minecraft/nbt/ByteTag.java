package net.minecraft.nbt;

/** NBT byte (в том числе boolean). */
public final class ByteTag implements Tag {
    private final byte value;

    public ByteTag(byte value) {
        this.value = value;
    }

    public static ByteTag valueOf(byte value) {
        return new ByteTag(value);
    }

    public static ByteTag valueOf(boolean value) {
        return new ByteTag((byte) (value ? 1 : 0));
    }

    public byte getAsByte() {
        return this.value;
    }

    public boolean getAsBoolean() {
        return this.value != 0;
    }

    @Override
    public byte getId() {
        return 1;
    }

    @Override
    public String toString() {
        return Byte.toString(this.value);
    }
}
