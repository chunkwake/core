package net.minecraft.resources;

import net.minecraft.core.Registry;

/** NMS {@code ResourceKey}. */
public final class ResourceKey<T> {
    private final Identifier location;

    private ResourceKey(Identifier location) {
        this.location = location;
    }

    public static <T> ResourceKey<T> create(ResourceKey<? extends Registry<T>> registry,
            Identifier location) {
        return new ResourceKey<>(location);
    }

    public static <T> ResourceKey<Registry<T>> createRegistryKey(Identifier location) {
        return new ResourceKey<>(location);
    }

    public Identifier location() {
        return this.location;
    }

    @Override
    public String toString() {
        return "ResourceKey[" + this.location + "]";
    }
}
