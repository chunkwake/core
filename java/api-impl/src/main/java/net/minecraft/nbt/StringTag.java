package net.minecraft.nbt;

/** NBT-строка. */
public final class StringTag implements Tag {
    private final String value;

    public StringTag(String value) {
        this.value = value == null ? "" : value;
    }

    public static StringTag valueOf(String value) {
        return new StringTag(value);
    }

    public String getAsString() {
        return this.value;
    }

    @Override
    public byte getId() {
        return 8;
    }

    @Override
    public String toString() {
        return this.value;
    }
}
