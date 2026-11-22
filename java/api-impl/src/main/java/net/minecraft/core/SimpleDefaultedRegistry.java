package net.minecraft.core;

import net.minecraft.resources.Identifier;

/** Реализация {@link DefaultedRegistry}. */
public class SimpleDefaultedRegistry<T> extends SimpleRegistry<T> implements DefaultedRegistry<T> {
    private final Identifier defaultKey;

    public SimpleDefaultedRegistry(Identifier defaultKey) {
        this.defaultKey = defaultKey;
    }

    @Override
    public Identifier getDefaultKey() {
        return this.defaultKey;
    }

    @Override
    public T defaultValue() {
        return this.getValue(this.defaultKey);
    }

    @Override
    public int getId(T value) {
        int id = super.getId(value);
        return id < 0 ? 0 : id;
    }

    @Override
    public T byId(int id) {
        T value = super.byId(id);
        return value == null ? defaultValue() : value;
    }
}
