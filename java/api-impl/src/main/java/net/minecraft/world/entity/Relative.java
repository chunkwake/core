package net.minecraft.world.entity;

/** NMS {@code Relative}: флаги относительных координат. */
public enum Relative {
    X, Y, Z, Y_ROT, X_ROT;

    public static int toFlags(Relative... values) {
        int flags = 0;
        for (Relative value : values) {
            flags |= 1 << value.ordinal();
        }
        return flags;
    }
}
