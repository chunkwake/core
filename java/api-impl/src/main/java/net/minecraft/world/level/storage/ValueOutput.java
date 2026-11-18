package net.minecraft.world.level.storage;

/** NMS {@code ValueOutput}: запись значений в NBT-подобное хранилище. */
public interface ValueOutput {
    ValueOutput child(String key);

    void putByte(String key, byte value);

    void putBoolean(String key, boolean value);

    void putShort(String key, short value);

    void putInt(String key, int value);

    void putLong(String key, long value);

    void putFloat(String key, float value);

    void putDouble(String key, double value);

    void putString(String key, String value);

    void store(String key, net.minecraft.nbt.Tag value);
}
