package net.minecraft.nbt;

/** NMS {@code IntArrayTag}. */
public class IntArrayTag implements Tag {
    private final int[] data;

    public IntArrayTag(int[] data) {
        this.data = data == null ? new int[0] : data;
    }

    public int[] getAsIntArray() {
        return this.data;
    }

    @Override
    public byte getId() {
        return 11;
    }

    @Override
    public String toString() {
        return java.util.Arrays.toString(this.data);
    }
}
