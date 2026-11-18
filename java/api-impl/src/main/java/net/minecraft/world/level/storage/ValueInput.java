package net.minecraft.world.level.storage;

import java.util.Optional;

/** NMS {@code ValueInput}: чтение значений. */
public interface ValueInput {
    Optional<ValueInput> child(String key);

    Optional<Byte> getByte(String key);

    Optional<Boolean> getBoolean(String key);

    Optional<Short> getShort(String key);

    Optional<Integer> getInt(String key);

    Optional<Long> getLong(String key);

    Optional<Float> getFloat(String key);

    Optional<Double> getDouble(String key);

    Optional<String> getString(String key);

    Optional<net.minecraft.nbt.Tag> get(String key);
}
