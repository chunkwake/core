package net.minecraft.core;

import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Optional;
import net.minecraft.resources.Identifier;

/** Рабочая реализация {@link Registry}. */
public class SimpleRegistry<T> extends MappedRegistry<T> implements Registry<T> {
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
    public Identifier getKey(T value) {
        for (Map.Entry<Identifier, T> entry : this.values.entrySet()) {
            if (entry.getValue() == value) {
                return entry.getKey();
            }
        }
        return null;
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
    public java.util.Set<java.util.Map.Entry<net.minecraft.resources.ResourceKey<T>, T>> entrySet() {
        java.util.Set<java.util.Map.Entry<net.minecraft.resources.ResourceKey<T>, T>> out =
                new java.util.LinkedHashSet<>();
        this.values.forEach((id, value) -> out.add(
                new java.util.AbstractMap.SimpleEntry<>(
                        net.minecraft.resources.ResourceKey.entryKey(id), value)));
        return java.util.Collections.unmodifiableSet(out);
    }

    /** Теги реестра ({@code HolderSet.Named}); заполняется бутстрапом. */
    private final java.util.List<Object> tags = new java.util.ArrayList<>();

    public void registerTag(Object tag) {
        this.tags.add(tag);
    }

    @Override
    public java.util.stream.Stream<?> getTags() {
        return this.tags.stream();
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
