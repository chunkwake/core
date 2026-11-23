package net.minecraft.core;

import java.util.Map;

/**
 * NMS {@code RegistryAccess} — интерфейс (как в ванили): squaremap и другие
 * плагины проверяют его как интерфейс, класс даёт
 * {@code IncompatibleClassChangeError}.
 */
public interface RegistryAccess extends HolderLookup.Provider {
    /** В 26.3 {@code RegistryAccess.EMPTY} имеет тип {@code Frozen}. */
    RegistryAccess.Frozen EMPTY = new LayeredRegistryAccess<>(java.util.List.of());

    static RegistryAccess EMPTY_REGISTRY_ACCESS() {
        return EMPTY;
    }

    static RegistryAccess empty() {
        return EMPTY;
    }

    default Map<Object, Object> registries() {
        return Map.of();
    }

    /** {@code RegistryAccess.lookupOrThrow}: пустой реестр нужного ключа. */
    @SuppressWarnings("unchecked")
    default <E> Registry<E> lookupOrThrow(
            net.minecraft.resources.ResourceKey<? extends Registry<? extends E>> key) {
        String name = key.location().toString();
        return (Registry<E>) EMPTY_REGISTRIES.computeIfAbsent(name,
                k -> dev.chunkwake.bridge.VanillaRegistries.create(k));
    }

    default <E> java.util.Optional<Registry<E>> lookup(
            net.minecraft.resources.ResourceKey<? extends Registry<? extends E>> key) {
        return java.util.Optional.of(this.lookupOrThrow(key));
    }

    /** Пустые реестры по ключу (biome/dimension_type/...). */
    java.util.Map<String, Registry<?>> EMPTY_REGISTRIES =
            new java.util.concurrent.ConcurrentHashMap<>();

    /**
     * {@code RegistryAccess.Frozen} — в ванили интерфейс
     * ({@code extends RegistryAccess}); класс даёт
     * {@code IncompatibleClassChangeError} (AxGraves).
     */
    interface Frozen extends RegistryAccess {
        Frozen EMPTY = new Frozen() {
        };

        static Frozen empty() {
            return EMPTY;
        }

        /** {@code Frozen.createSerializationContext} (AxGraves). */
        default <T> net.minecraft.resources.RegistryOps<T> createSerializationContext(
                com.mojang.serialization.DynamicOps<T> ops) {
            return net.minecraft.resources.RegistryOps.create(ops, this);
        }
    }
}
