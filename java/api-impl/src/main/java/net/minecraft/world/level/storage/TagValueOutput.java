package net.minecraft.world.level.storage;

import net.minecraft.core.HolderLookup;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.Tag;
import net.minecraft.util.ProblemReporter;

/** Рабочий NMS {@code TagValueOutput} поверх {@link CompoundTag}. */
public class TagValueOutput implements ValueOutput {
    private final CompoundTag tag;

    private TagValueOutput(CompoundTag tag) {
        this.tag = tag;
    }

    public static TagValueOutput createWithContext(ProblemReporter reporter,
            HolderLookup.Provider registries) {
        return new TagValueOutput(new CompoundTag());
    }

    public static TagValueOutput create(ProblemReporter reporter,
            HolderLookup.Provider registries, CompoundTag tag) {
        return new TagValueOutput(tag);
    }

    public CompoundTag buildResult() {
        return this.tag;
    }

    @Override
    public ValueOutput child(String key) {
        CompoundTag child = this.tag.getCompoundOrEmpty(key);
        this.tag.put(key, child);
        return new TagValueOutput(child);
    }

    @Override
    public void putByte(String key, byte value) {
        this.tag.putByte(key, value);
    }

    @Override
    public void putBoolean(String key, boolean value) {
        this.tag.putBoolean(key, value);
    }

    @Override
    public void putShort(String key, short value) {
        this.tag.putShort(key, value);
    }

    @Override
    public void putInt(String key, int value) {
        this.tag.putInt(key, value);
    }

    @Override
    public void putLong(String key, long value) {
        this.tag.putLong(key, value);
    }

    @Override
    public void putFloat(String key, float value) {
        this.tag.putFloat(key, value);
    }

    @Override
    public void putDouble(String key, double value) {
        this.tag.putDouble(key, value);
    }

    @Override
    public void putString(String key, String value) {
        this.tag.putString(key, value);
    }

    @Override
    public void store(String key, Tag value) {
        this.tag.put(key, value);
    }
}
