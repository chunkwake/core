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

    public static final ResourceKey<Registry<Object>> FROG_VARIANT = key("frog_variant");
    public static final ResourceKey<Registry<Object>> CAT_VARIANT = key("cat_variant");
    public static final ResourceKey<Registry<Object>> WOLF_VARIANT = key("wolf_variant");
    public static final ResourceKey<Registry<Object>> PAINTING_VARIANT = key("painting_variant");
    public static final ResourceKey<Registry<Object>> VILLAGER_TYPE = key("villager_type");
    public static final ResourceKey<Registry<Object>> VILLAGER_PROFESSION =
            key("villager_profession");
    public static final ResourceKey<Registry<Object>> BANNER_PATTERN = key("banner_pattern");
    public static final ResourceKey<Registry<Object>> ENCHANTMENT = key("enchantment");
    public static final ResourceKey<Registry<Object>> JUKEBOX_SONG = key("jukebox_song");
    public static final ResourceKey<Registry<Object>> DAMAGE_TYPE = key("damage_type");
    public static final ResourceKey<Registry<Object>> TRIM_MATERIAL = key("trim_material");
    public static final ResourceKey<Registry<Object>> TRIM_PATTERN = key("trim_pattern");
    public static final ResourceKey<Registry<Object>> WORLDGEN_BIOME = key("worldgen/biome");

    private static ResourceKey<Registry<Object>> key(String name) {
        return ResourceKey.createRegistryKey(Identifier.parse("minecraft:" + name));
    }

    private Registries() {
    }
}
