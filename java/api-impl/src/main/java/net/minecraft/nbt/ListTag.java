package net.minecraft.nbt;

import java.util.ArrayList;
import java.util.List;

/** NBT-список. */
public final class ListTag extends java.util.AbstractList<Tag> implements Tag, java.util.RandomAccess {
    private final List<Tag> values = new ArrayList<>();

    public ListTag() {
    }

    @Override
    public Tag get(int index) {
        return this.values.get(index);
    }

    @Override
    public boolean isEmpty() {
        return this.size() == 0;
    }

    public java.util.Optional<CompoundTag> getCompound(int index) {
        Tag tag = this.get(index);
        return tag instanceof CompoundTag compound
                ? java.util.Optional.of(compound) : java.util.Optional.empty();
    }

    public int size() {
        return this.values.size();
    }

    @Override
    public boolean add(Tag tag) {
        return this.values.add(tag);
    }

    public void add(int index, Tag tag) {
        this.values.add(index, tag);
    }

    public Tag remove(int index) {
        return this.values.remove(index);
    }

    public byte getElementType() {
        for (Tag tag : this.values) {
            if (tag != null) {
                return tag.getId();
            }
        }
        return 0;
    }

    @Override
    public byte getId() {
        return 9;
    }

    @Override
    public String toString() {
        return this.values.toString();
    }
}
