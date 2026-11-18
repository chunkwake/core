package net.minecraft.core;

import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Optional;
import net.minecraft.resources.Identifier;

/** Рабочий NMS {@code Registry}: id → значение. */
public class Registry<T> {
    private final Map<Identifier, T> values = new LinkedHashMap<>();

    public void register(Identifier id, T value) {
        this.values.put(id, value);
    }

    public Optional<Holder.Reference<T>> get(Identifier id) {
        T value = this.values.get(id);
        return value == null ? Optional.empty() : Optional.of(new Holder.Reference<>(value));
    }

    public Optional<Holder.Reference<T>> get(String id) {
        return this.get(Identifier.parse(id));
    }

    public T getValue(Identifier id) {
        return this.values.get(id);
    }

    public boolean containsKey(Identifier id) {
        return this.values.containsKey(id);
    }

    public int size() {
        return this.values.size();
    }

    public java.util.Set<Identifier> keySet() {
        return this.values.keySet();
    }
}
