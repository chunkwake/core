package net.minecraft.core.registries;

import net.minecraft.core.Registry;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;

/** Заглушка NMS {@code Registries}: ключи основных реестров. */
public final class Registries {
    public static final ResourceKey<Registry<Object>> BIOME =
            key("biome");
    public static final ResourceKey<Registry<Object>> DIMENSION_TYPE =
            key("dimension_type");
    public static final ResourceKey<Registry<Object>> BLOCK =
            key("block");
    public static final ResourceKey<Registry<Object>> ITEM =
            key("item");

    private static ResourceKey<Registry<Object>> key(String name) {
        return ResourceKey.createRegistryKey(Identifier.parse("minecraft:" + name));
    }

    private Registries() {
    }
}
