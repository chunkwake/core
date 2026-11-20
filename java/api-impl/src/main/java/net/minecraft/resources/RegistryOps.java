package net.minecraft.resources;

import com.mojang.serialization.DynamicOps;
import net.minecraft.core.RegistryAccess;

/** NMS {@code RegistryOps}: обёртка DynamicOps с реестрами. */
public class RegistryOps<T> implements DynamicOps<T> {
    private final DynamicOps<T> delegate;
    private final RegistryAccess registries;

    protected RegistryOps(DynamicOps<T> delegate, RegistryAccess registries) {
        this.delegate = delegate;
        this.registries = registries;
    }

    public static <T> RegistryOps<T> create(DynamicOps<T> delegate, RegistryAccess registries) {
        return new RegistryOps<>(delegate, registries);
    }

    public RegistryAccess registryAccess() {
        return this.registries;
    }

    @Override
    public T empty() {
        return this.delegate.empty();
    }

    @Override
    public T emptyMap() {
        return this.delegate.emptyMap();
    }

    @Override
    public T emptyList() {
        return this.delegate.emptyList();
    }

    @Override
    public <U> U convertTo(DynamicOps<U> outOps, T input) {
        return this.delegate.convertTo(outOps, input);
    }

    @Override
    public T remove(T input, String key) {
        return this.delegate.remove(input, key);
    }

    @Override
    public com.mojang.serialization.DataResult<Number> getNumberValue(T input) {
        return this.delegate.getNumberValue(input);
    }

    @Override
    public T createNumeric(Number value) {
        return this.delegate.createNumeric(value);
    }

    @Override
    public com.mojang.serialization.DataResult<Boolean> getBooleanValue(T input) {
        return this.delegate.getBooleanValue(input);
    }

    @Override
    public T createBoolean(boolean value) {
        return this.delegate.createBoolean(value);
    }

    @Override
    public com.mojang.serialization.DataResult<String> getStringValue(T input) {
        return this.delegate.getStringValue(input);
    }

    @Override
    public T createString(String value) {
        return this.delegate.createString(value);
    }

    @Override
    public com.mojang.serialization.DataResult<T> mergeToList(T list, T value) {
        return this.delegate.mergeToList(list, value);
    }

    @Override
    public com.mojang.serialization.DataResult<T> mergeToMap(T map, T key, T value) {
        return this.delegate.mergeToMap(map, key, value);
    }

    @Override
    public com.mojang.serialization.DataResult<java.util.stream.Stream<com.mojang.datafixers.util.Pair<T, T>>>
            getMapValues(T input) {
        return this.delegate.getMapValues(input);
    }

    @Override
    public T createMap(java.util.stream.Stream<com.mojang.datafixers.util.Pair<T, T>> entries) {
        return this.delegate.createMap(entries);
    }

    @Override
    public com.mojang.serialization.DataResult<java.util.stream.Stream<T>> getStream(T input) {
        return this.delegate.getStream(input);
    }

    @Override
    public T createList(java.util.stream.Stream<T> input) {
        return this.delegate.createList(input);
    }
}
