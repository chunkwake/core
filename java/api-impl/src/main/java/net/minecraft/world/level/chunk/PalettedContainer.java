package net.minecraft.world.level.chunk;

import com.mojang.serialization.Codec;
import java.util.HashMap;
import java.util.Map;

/** NMS {@code PalettedContainer} (упрощённая рабочая реализация). */
public class PalettedContainer<T> implements PalettedContainerRO<T> {
    /** Поле, которое ищет CraftEngine (PalettedContainerProxy#getData). */
    private Data<T> data = new Data<>();

    /** NMS {@code PalettedContainer.Data}. */
    public static class Data<T> {
        public Data() {
        }
    }

    /** {@code PalettedContainer.getAndSet(int,int,int,T)} (CraftEngine). */
    public T getAndSet(int x, int y, int z, T value) {
        long key = (((long) x) & 0xFFFFL) << 32 | (((long) z) & 0xFFFFL);
        T previous = this.values.getOrDefault(key, this.defaultValue);
        this.values.put(key, value);
        return previous;
    }

    private final T defaultValue;
    private final Strategy<T> strategy;
    private final Map<Long, T> values = new HashMap<>();

    public PalettedContainer(T defaultValue, Strategy<T> strategy) {
        this.defaultValue = defaultValue;
        this.strategy = strategy;
    }

    public static <T> PalettedContainer<T> codecRW(Codec<T> codec, Strategy<T> strategy,
            T defaultValue) {
        return new PalettedContainer<>(defaultValue, strategy);
    }

    /** Ванильный доступ по одному индексу: x | z<<4 | y<<8. */
    public T get(int index) {
        return this.get(index & 15, (index >> 8) & 15, (index >> 4) & 15);
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

    @Override
    public PalettedContainer<T> copy() {
        PalettedContainer<T> copy = new PalettedContainer<>(this.defaultValue, this.strategy);
        copy.values.putAll(this.values);
        return copy;
    }

    public boolean maybeHas(java.util.function.Predicate<T> predicate) {
        for (T value : this.values.values()) {
            if (predicate.test(value)) {
                return true;
            }
        }
        return predicate.test(this.defaultValue);
    }

    public T defaultValue() {
        return this.defaultValue;
    }
}
