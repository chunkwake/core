package net.minecraft.nbt;

/** NMS {@code TagParser}: разбор SNBT/NBT-строк. */
public final class TagParser {
    private TagParser() {
    }

    public static CompoundTag parseCompoundFully(String input) {
        return new CompoundTag();
    }

    public static Tag parseTag(String input) {
        return new CompoundTag();
    }
}
