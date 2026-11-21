package net.minecraft.core.registries;

import net.minecraft.core.Registry;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;

/** Заглушка NMS {@code Registries}: ключи основных реестров. */
public final class Registries {
    /** Поле, которое ищет CraftEngine (RegistriesProxy#getAttribute). */
    public static final ResourceKey<Registry<Object>> attribute =
            ResourceKey.createRegistryKey(Identifier.parse("minecraft:attribute"));
    public static final ResourceKey<Registry<Object>> ATTRIBUTE = attribute;
    public static final ResourceKey<Registry<Object>> lootTable =
            ResourceKey.createRegistryKey(Identifier.parse("minecraft:loot_table"));
    public static final ResourceKey<Registry<Object>> LOOT_TABLE = lootTable;
    public static final ResourceKey<Registry<Object>> recipe =
            ResourceKey.createRegistryKey(Identifier.parse("minecraft:recipe"));
    public static final ResourceKey<Registry<Object>> RECIPE = recipe;
    public static final ResourceKey<Registry<Object>> fluid =
            ResourceKey.createRegistryKey(Identifier.parse("minecraft:fluid"));
    public static final ResourceKey<Registry<Object>> FLUID = fluid;

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

    public static final ResourceKey<Registry<Object>> PIG_VARIANT = key("pig_variant");
    public static final ResourceKey<Registry<Object>> COW_VARIANT = key("cow_variant");
    public static final ResourceKey<Registry<Object>> CHICKEN_VARIANT = key("chicken_variant");
    public static final ResourceKey<Registry<Object>> STRUCTURE = key("structure");
    public static final ResourceKey<Registry<Object>> STRUCTURE_SET = key("structure_set");
    public static final ResourceKey<Registry<Object>> TEMPLATE_POOL = key("template_pool");
    public static final ResourceKey<Registry<Object>> PROCESSOR_LIST = key("processor_list");
    public static final ResourceKey<Registry<Object>> NOISE = key("noise");
    public static final ResourceKey<Registry<Object>> DENSITY_FUNCTION = key("density_function");
    public static final ResourceKey<Registry<Object>> BIOME_SOURCE = key("worldgen/biome_source");
    public static final ResourceKey<Registry<Object>> PLACEMENT_MODIFIER = key("worldgen/placement_modifier_type");
    public static final ResourceKey<Registry<Object>> FEATURE = key("worldgen/feature");
    public static final ResourceKey<Registry<Object>> CARVER = key("worldgen/carver");

    private static ResourceKey<Registry<Object>> key(String name) {
        return ResourceKey.createRegistryKey(Identifier.parse("minecraft:" + name));
    }

    private Registries() {
    }
}
