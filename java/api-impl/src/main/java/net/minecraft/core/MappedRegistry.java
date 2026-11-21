package net.minecraft.core;

import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.Map;
import java.util.Optional;
import java.util.Set;
import java.util.stream.Stream;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;

/** NMS {@code MappedRegistry}: реестр id/ключ -> значение. */
public class MappedRegistry<T> implements Registry<T> {
    private final Map<Identifier, T> byKey = new LinkedHashMap<>();
    /** Поле, которое ищет CraftEngine (MappedRegistryProxy#isFrozen). */
    private boolean frozen;
    /** Поле, которое ищет CraftEngine (setUnregisteredIntrusiveHolders). */
    private Map<Object, Object> unregisteredIntrusiveHolders = new LinkedHashMap<>();
    private final Map<Identifier, Integer> ids = new LinkedHashMap<>();
    private final Map<Integer, T> byId = new LinkedHashMap<>();

    public MappedRegistry() {
    }

    @Override
    public void register(Identifier id, T value) {
        this.byKey.put(id, value);
        int next = this.ids.size();
        this.ids.put(id, next);
        this.byId.put(next, value);
    }

    @Override
    public Optional<Holder.Reference<T>> get(Identifier id) {
        T value = this.byKey.get(id);
        return value == null ? Optional.empty() : Optional.of(new Holder.Reference<>(value));
    }

    @Override
    public T getValue(Identifier id) {
        return this.byKey.get(id);
    }

    @Override
    public boolean containsKey(Identifier id) {
        return this.byKey.containsKey(id);
    }

    @Override
    public int size() {
        return this.byKey.size();
    }

    @Override
    public Set<Identifier> keySet() {
        return new LinkedHashSet<>(this.byKey.keySet());
    }

    @Override
    public Set<Map.Entry<ResourceKey<T>, T>> entrySet() {
        return Set.of();
    }

    @Override
    public Stream<?> getTags() {
        return Stream.empty();
    }

    @Override
    public Identifier getKey(T value) {
        for (Map.Entry<Identifier, T> entry : this.byKey.entrySet()) {
            if (entry.getValue() == value) {
                return entry.getKey();
            }
        }
        return null;
    }

    @Override
    public int getId(T value) {
        for (Map.Entry<Identifier, T> entry : this.byKey.entrySet()) {
            if (entry.getValue() == value) {
                return this.ids.getOrDefault(entry.getKey(), -1);
            }
        }
        return -1;
    }

    @Override
    public T byId(int id) {
        return this.byId.get(id);
    }

    @Override
    public java.util.Iterator<T> iterator() {
        return this.byKey.values().iterator();
    }
}
