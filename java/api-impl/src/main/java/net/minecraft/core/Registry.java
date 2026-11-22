package net.minecraft.core;

import java.util.Optional;
import net.minecraft.resources.Identifier;

/** NMS {@code Registry} — интерфейс (как в 26.x). */
public interface Registry<T> extends HolderLookup.RegistryLookup<T>, Iterable<T> {
    void register(Identifier id, T value);

    Optional<Holder.Reference<T>> get(Identifier id);

    default Optional<Holder.Reference<T>> get(String id) {
        return this.get(Identifier.parse(id));
    }

    T getValue(Identifier id);

    /** {@code Registry.getHolder(Identifier)} (CraftEngine). */
    default java.util.Optional<Holder.Reference<T>> getHolder(Identifier id) {
        return this.get(id);
    }

    /** {@code Registry.getHolder(ResourceKey)} (CraftEngine). */
    default java.util.Optional<Holder.Reference<T>> getHolder(
            net.minecraft.resources.ResourceKey<T> key) {
        return this.get(key.location());
    }

    /** {@code Registry.asLookup()} (CraftEngine). */
    default HolderLookup.RegistryLookup<T> asLookup() {
        return this;
    }

    /** {@code Registry.getKey(T)} (CraftEngine). */
    Identifier getKey(T value);

    /** {@code Registry.registerForHolder(...)} (CraftEngine). */
    @SuppressWarnings("unchecked")
    static <R, T extends R> Holder.Reference<T> registerForHolder(
            Registry<R> registry, net.minecraft.resources.ResourceKey<R> key, T value) {
        registry.register(key.location(), value);
        return (Holder.Reference<T>) (Holder.Reference<?>) new Holder.Reference<R>(value, key);
    }

    @SuppressWarnings("unchecked")
    static <R, T extends R> Holder.Reference<T> registerForHolder(
            Registry<R> registry, Identifier id, T value) {
        registry.register(id, value);
        return (Holder.Reference<T>) (Holder.Reference<?>) new Holder.Reference<R>(value);
    }

    default java.util.Optional<T> getOptional(Identifier id) {
        return java.util.Optional.ofNullable(this.getValue(id));
    }

    boolean containsKey(Identifier id);

    int size();

    java.util.Set<Identifier> keySet();

    java.util.Set<java.util.Map.Entry<net.minecraft.resources.ResourceKey<T>, T>> entrySet();

    java.util.stream.Stream<?> getTags();

    default java.util.Optional<?> getTag(net.minecraft.resources.Identifier id) {
        return java.util.Optional.empty();
    }

    /** {@code Registry.getOrThrow(ResourceKey)} (MythicMobs). */
    default Holder.Reference<T> getOrThrow(
            net.minecraft.resources.ResourceKey<T> key) {
        return this.get(key.location()).orElse(null);
    }

    default Holder.Reference<T> getHolderOrThrow(
            net.minecraft.resources.ResourceKey<T> key) {
        return this.getOrThrow(key);
    }

    /** {@code Registry.holderByNameCodec()} (squaremap SectionDecoder). */
    default com.mojang.serialization.Codec<net.minecraft.core.Holder<T>> holderByNameCodec() {
        return com.mojang.serialization.Codec.STRING.xmap(
                name -> null, holder -> "minecraft:air");
    }

    int getId(T value);

    T byId(int id);

    /**
     * {@code Registry.key()}: ключ самого реестра (нужен squaremap и
     * FancyNpcs, который кастует реестр к {@code RegistryLookup}).
     */
    @Override
    default net.minecraft.resources.ResourceKey<? extends Registry<? extends T>> key() {
        return null;
    }

    @Override
    default com.mojang.serialization.Lifecycle registryLifecycle() {
        return com.mojang.serialization.Lifecycle.stable();
    }

    /** {@code Registry.iterator()} — плагины обходят реестр. */
    default java.util.Iterator<T> iterator() {
        return java.util.Collections.emptyIterator();
    }
}
