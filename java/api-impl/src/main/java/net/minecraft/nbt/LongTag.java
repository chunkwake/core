package net.minecraft.nbt;

/** NBT long. */
public final class LongTag implements Tag {
    private final long value;

    public LongTag(long value) {
        this.value = value;
    }

    public static LongTag valueOf(long value) {
        return new LongTag(value);
    }

    public long getAsLong() {
        return this.value;
    }

    @Override
    public byte getId() {
        return 4;
    }

    @Override
    public String toString() {
        return Long.toString(this.value);
    }
}
