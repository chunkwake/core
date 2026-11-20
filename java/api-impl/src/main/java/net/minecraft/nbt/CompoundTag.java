package net.minecraft.nbt;

import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Set;

/** NBT-соединение (словарь тегов). */
public final class CompoundTag implements Tag {
    private final Map<String, Tag> tags = new LinkedHashMap<>();

    public Tag put(String key, Tag value) {
        if (key == null) {
            return null;
        }
        if (value == null) {
            return this.tags.remove(key);
        }
        return this.tags.put(key, value);
    }

    public void putByte(String key, byte value) {
        this.put(key, ByteTag.valueOf(value));
    }

    public void putBoolean(String key, boolean value) {
        this.put(key, ByteTag.valueOf(value));
    }

    public void putShort(String key, short value) {
        this.put(key, ShortTag.valueOf(value));
    }

    public void putInt(String key, int value) {
        this.put(key, IntTag.valueOf(value));
    }

    public void putLong(String key, long value) {
        this.put(key, LongTag.valueOf(value));
    }

    public void putFloat(String key, float value) {
        this.put(key, FloatTag.valueOf(value));
    }

    public void putDouble(String key, double value) {
        this.put(key, DoubleTag.valueOf(value));
    }

    public void putString(String key, String value) {
        this.put(key, StringTag.valueOf(value));
    }

    public Tag get(String key) {
        return this.tags.get(key);
    }

    public boolean contains(String key) {
        return this.tags.containsKey(key);
    }

    public Set<String> keySet() {
        return this.tags.keySet();
    }

    public Set<String> getAllKeys() {
        return this.tags.keySet();
    }

    public boolean isEmpty() {
        return this.tags.isEmpty();
    }

    public int size() {
        return this.tags.size();
    }

    public byte getByteOr(String key, byte fallback) {
        return this.get(key) instanceof ByteTag tag ? tag.getAsByte() : fallback;
    }

    public short getShortOr(String key, short fallback) {
        return this.get(key) instanceof ShortTag tag ? tag.getAsShort() : fallback;
    }

    public int getIntOr(String key, int fallback) {
        return this.get(key) instanceof IntTag tag ? tag.getAsInt() : fallback;
    }

    public long getLongOr(String key, long fallback) {
        return this.get(key) instanceof LongTag tag ? tag.getAsLong() : fallback;
    }

    public float getFloatOr(String key, float fallback) {
        return this.get(key) instanceof FloatTag tag ? tag.getAsFloat() : fallback;
    }

    public double getDoubleOr(String key, double fallback) {
        return this.get(key) instanceof DoubleTag tag ? tag.getAsDouble() : fallback;
    }

    public String getStringOr(String key, String fallback) {
        return this.get(key) instanceof StringTag tag ? tag.getAsString() : fallback;
    }

    public CompoundTag getCompoundOrEmpty(String key) {
        return this.get(key) instanceof CompoundTag tag ? tag : new CompoundTag();
    }

    public ListTag getListOrEmpty(String key) {
        return this.get(key) instanceof ListTag tag ? tag : new ListTag();
    }

    @Override
    public byte getId() {
        return 10;
    }

    @Override
    public String toString() {
        return this.tags.toString();
    }
}
