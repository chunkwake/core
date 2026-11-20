package net.minecraft.nbt;

/** NMS {@code NbtUtils}. */
public final class NbtUtils {
    private NbtUtils() {
    }

    public static int getDataVersion(CompoundTag tag, int fallback) {
        return tag == null ? fallback : tag.getIntOr("DataVersion", fallback);
    }
}
