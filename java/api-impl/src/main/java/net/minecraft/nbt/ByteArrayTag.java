package net.minecraft.nbt;

/** NBT byte[]. */
public final class ByteArrayTag implements Tag {
    private final byte[] value;

    public ByteArrayTag(byte[] value) {
        this.value = value;
    }

    public byte[] getAsByteArray() {
        return this.value;
    }

    @Override
    public byte getId() {
        return 7;
    }
}
