package net.minecraft.core;

import java.util.Optional;
import net.minecraft.resources.Identifier;

/** NMS {@code Registry} — интерфейс (как в 26.x). */
public interface Registry<T> {
    void register(Identifier id, T value);

    Optional<Holder.Reference<T>> get(Identifier id);

    default Optional<Holder.Reference<T>> get(String id) {
        return this.get(Identifier.parse(id));
    }

    T getValue(Identifier id);

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

    int getId(T value);

    T byId(int id);

    /**
     * {@code Registry.key()}: ключ самого реестра. Нужен squaremap
     * (`MinecraftRegistries.requireEntry`), вызывается на пустых реестрах.
     */
    default net.minecraft.resources.ResourceKey<? extends Registry<T>> key() {
        return null;
    }

    /** {@code Registry.iterator()} — плагины обходят реестр. */
    default java.util.Iterator<T> iterator() {
        return java.util.Collections.emptyIterator();
    }
}
