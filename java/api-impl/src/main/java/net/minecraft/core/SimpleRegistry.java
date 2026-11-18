package net.minecraft.core;

import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Optional;
import net.minecraft.resources.Identifier;

/** Рабочая реализация {@link Registry}. */
public class SimpleRegistry<T> implements Registry<T> {
    private final Map<Identifier, T> values = new LinkedHashMap<>();

    @Override
    public void register(Identifier id, T value) {
        this.values.put(id, value);
    }

    @Override
    public Optional<Holder.Reference<T>> get(Identifier id) {
        T value = this.values.get(id);
        return value == null ? Optional.empty() : Optional.of(new Holder.Reference<>(value));
    }

    @Override
    public T getValue(Identifier id) {
        return this.values.get(id);
    }

    @Override
    public boolean containsKey(Identifier id) {
        return this.values.containsKey(id);
    }

    @Override
    public int size() {
        return this.values.size();
    }

    @Override
    public java.util.Set<Identifier> keySet() {
        return this.values.keySet();
    }

    @Override
    public java.util.Set<java.util.Map.Entry<Identifier, T>> entrySet() {
        return java.util.Collections.unmodifiableSet(this.values.entrySet());
    }

    @Override
    public java.util.stream.Stream<?> getTags() {
        return java.util.stream.Stream.empty();
    }

    @Override
    public int getId(T value) {
        int i = 0;
        for (T candidate : this.values.values()) {
            if (candidate == value || candidate.equals(value)) {
                return i;
            }
            i++;
        }
        return -1;
    }

    @Override
    public T byId(int id) {
        int i = 0;
        for (T value : this.values.values()) {
            if (i++ == id) {
                return value;
            }
        }
        return null;
    }
}
