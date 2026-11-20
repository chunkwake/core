package net.minecraft.core;

import java.util.Map;

/**
 * NMS {@code RegistryAccess} — интерфейс (как в ванили): squaremap и другие
 * плагины проверяют его как интерфейс, класс даёт
 * {@code IncompatibleClassChangeError}.
 */
public interface RegistryAccess {
    RegistryAccess EMPTY = new RegistryAccess() {
    };

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

    /** Заглушка {@code RegistryAccess$Frozen}. */
    final class Frozen implements RegistryAccess {
        private static final Frozen EMPTY = new Frozen();

        private Frozen() {
        }

        public static Frozen empty() {
            return EMPTY;
        }
    }
}
