package net.minecraft.resources;

import net.minecraft.core.Registry;

/** NMS {@code ResourceKey}. */
public final class ResourceKey<T> {
    private final Identifier location;
    /**
     * Поля, которые CraftEngine (ResourceKeyProxy) ищет по именам:
     * в 26.3 оба имеют тип Identifier.
     */
    private Identifier registryName;
    private Identifier identifier;

    private ResourceKey(Identifier location) {
        this.location = location;
        this.identifier = location;
        this.registryName = Identifier.parse("minecraft:root");
    }

    public static <T> ResourceKey<T> create(ResourceKey<? extends Registry<T>> registry,
            Identifier location) {
        ResourceKey<T> key = new ResourceKey<>(location);
        if (registry != null) {
            key.registryName = registry.location();
        }
        return key;
    }

    public static <T> ResourceKey<Registry<T>> createRegistryKey(Identifier location) {
        return new ResourceKey<>(location);
    }

    public Identifier location() {
        return this.location;
    }

    /** 26.x имя: {@code ResourceKey.identifier()}. */
    public Identifier identifier() {
        return this.location;
    }

    /** Ключ записи реестра (registry-тип неизвестен). */
    public static <T> ResourceKey<T> entryKey(Identifier location) {
        return new ResourceKey<>(location);
    }

    @Override
    public String toString() {
        return "ResourceKey[" + this.location + "]";
    }
}
