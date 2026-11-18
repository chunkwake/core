package net.minecraft.world.item.component;

import net.minecraft.nbt.CompoundTag;

/** NMS {@code CustomData}. */
public class CustomData {
    public static final CustomData EMPTY = new CustomData(new CompoundTag());

    private final CompoundTag tag;

    private CustomData(CompoundTag tag) {
        this.tag = tag;
    }

    public static CustomData of(CompoundTag tag) {
        return new CustomData(tag == null ? new CompoundTag() : tag);
    }

    public CompoundTag copyTag() {
        return this.tag;
    }
}
