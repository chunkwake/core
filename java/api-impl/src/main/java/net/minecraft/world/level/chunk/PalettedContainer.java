package net.minecraft.world.level.chunk;

import com.mojang.serialization.Codec;
import java.util.HashMap;
import java.util.Map;

/** NMS {@code PalettedContainer} (упрощённая рабочая реализация). */
public class PalettedContainer<T> {
    private final T defaultValue;
    private final Map<Long, T> values = new HashMap<>();

    public PalettedContainer(T defaultValue, Strategy<T> strategy) {
        this.defaultValue = defaultValue;
    }

    public static <T> PalettedContainer<T> codecRW(Codec<T> codec, Strategy<T> strategy,
            T defaultValue) {
        return new PalettedContainer<>(defaultValue, strategy);
    }

    public T get(int x, int y, int z) {
        T value = this.values.get(key(x, y, z));
        return value == null ? this.defaultValue : value;
    }

    public void set(int x, int y, int z, T value) {
        this.values.put(key(x, y, z), value);
    }

    private static long key(int x, int y, int z) {
        return ((long) (x & 15) << 8) | ((long) (y & 15) << 4) | (z & 15);
    }

    public T defaultValue() {
        return this.defaultValue;
    }
}
