package net.minecraft.nbt;

/** NBT long[]. */
public final class LongArrayTag implements Tag {
    private final long[] value;

    public LongArrayTag(long[] value) {
        this.value = value;
    }

    public long[] getAsLongArray() {
        return this.value;
    }

    @Override
    public byte getId() {
        return 11;
    }
}
