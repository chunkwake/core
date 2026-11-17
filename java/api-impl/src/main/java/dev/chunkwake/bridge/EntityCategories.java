package dev.chunkwake.bridge;

import java.util.HashMap;
import java.util.Map;
import org.bukkit.entity.SpawnCategory;

/**
 * Категории спавна сущностей по ванильному {@code MobCategory} из
 * {@code server-26.3.jar} ({@code EntityTypes}).
 * Ключ — имя константы {@link org.bukkit.entity.EntityType}.
 */
final class EntityCategories {
    private static final Map<String, SpawnCategory> BY_NAME = new HashMap<>();

    private EntityCategories() {
    }

    private static void put(String name, SpawnCategory category) {
        BY_NAME.put(name, category);
    }

    static SpawnCategory of(String name) {
        return BY_NAME.getOrDefault(name, SpawnCategory.MISC);
    }

    static int size() {
        return BY_NAME.size();
    }

    static java.util.Set<String> names() {
        return java.util.Set.copyOf(BY_NAME.keySet());
    }

    static {
        put("ACACIA_BOAT", SpawnCategory.MISC);
        put("ACACIA_CHEST_BOAT", SpawnCategory.MISC);
        put("ALLAY", SpawnCategory.ANIMAL);
        put("AREA_EFFECT_CLOUD", SpawnCategory.MISC);
        put("ARMADILLO", SpawnCategory.ANIMAL);
        put("ARMOR_STAND", SpawnCategory.MISC);
        put("ARROW", SpawnCategory.MISC);
        put("AXOLOTL", SpawnCategory.AXOLOTL);
        put("BAMBOO_CHEST_RAFT", SpawnCategory.MISC);
        put("BAMBOO_RAFT", SpawnCategory.MISC);
        put("BAT", SpawnCategory.AMBIENT);
        put("BEE", SpawnCategory.ANIMAL);
        put("BIRCH_BOAT", SpawnCategory.MISC);
        put("BIRCH_CHEST_BOAT", SpawnCategory.MISC);
        put("BLAZE", SpawnCategory.MONSTER);
        put("BLOCK_DISPLAY", SpawnCategory.MISC);
        put("BOGGED", SpawnCategory.MONSTER);
        put("BREEZE", SpawnCategory.MONSTER);
        put("BREEZE_WIND_CHARGE", SpawnCategory.MISC);
        put("CAMEL", SpawnCategory.ANIMAL);
        put("CAMEL_HUSK", SpawnCategory.MONSTER);
        put("CAT", SpawnCategory.ANIMAL);
        put("CAVE_SPIDER", SpawnCategory.MONSTER);
        put("CHERRY_BOAT", SpawnCategory.MISC);
        put("CHERRY_CHEST_BOAT", SpawnCategory.MISC);
        put("CHEST_MINECART", SpawnCategory.MISC);
        put("CHICKEN", SpawnCategory.ANIMAL);
        put("COD", SpawnCategory.WATER_AMBIENT);
        put("COMMAND_BLOCK_MINECART", SpawnCategory.MISC);
        put("COPPER_GOLEM", SpawnCategory.MISC);
        put("COW", SpawnCategory.ANIMAL);
        put("CREAKING", SpawnCategory.MONSTER);
        put("CREEPER", SpawnCategory.MONSTER);
        put("CUSHION", SpawnCategory.MISC);
        put("DARK_OAK_BOAT", SpawnCategory.MISC);
        put("DARK_OAK_CHEST_BOAT", SpawnCategory.MISC);
        put("DOLPHIN", SpawnCategory.WATER_ANIMAL);
        put("DONKEY", SpawnCategory.ANIMAL);
        put("DRAGON_FIREBALL", SpawnCategory.MISC);
        put("DROWNED", SpawnCategory.MONSTER);
        put("EGG", SpawnCategory.MISC);
        put("ELDER_GUARDIAN", SpawnCategory.MONSTER);
        put("ENDERMAN", SpawnCategory.MONSTER);
        put("ENDERMITE", SpawnCategory.MONSTER);
        put("ENDER_DRAGON", SpawnCategory.MONSTER);
        put("ENDER_PEARL", SpawnCategory.MISC);
        put("END_CRYSTAL", SpawnCategory.MISC);
        put("EVOKER", SpawnCategory.MONSTER);
        put("EVOKER_FANGS", SpawnCategory.MISC);
        put("EXPERIENCE_BOTTLE", SpawnCategory.MISC);
        put("EXPERIENCE_ORB", SpawnCategory.MISC);
        put("EYE_OF_ENDER", SpawnCategory.MISC);
        put("FALLING_BLOCK", SpawnCategory.MISC);
        put("FIREBALL", SpawnCategory.MISC);
        put("FIREWORK_ROCKET", SpawnCategory.MISC);
        put("FISHING_BOBBER", SpawnCategory.MISC);
        put("FOX", SpawnCategory.ANIMAL);
        put("FROG", SpawnCategory.ANIMAL);
        put("FURNACE_MINECART", SpawnCategory.MISC);
        put("GHAST", SpawnCategory.MONSTER);
        put("GIANT", SpawnCategory.MONSTER);
        put("GLOW_ITEM_FRAME", SpawnCategory.MISC);
        put("GLOW_SQUID", SpawnCategory.WATER_UNDERGROUND_CREATURE);
        put("GOAT", SpawnCategory.ANIMAL);
        put("GUARDIAN", SpawnCategory.MONSTER);
        put("HAPPY_GHAST", SpawnCategory.ANIMAL);
        put("HOGLIN", SpawnCategory.MONSTER);
        put("HOPPER_MINECART", SpawnCategory.MISC);
        put("HORSE", SpawnCategory.ANIMAL);
        put("HUSK", SpawnCategory.MONSTER);
        put("ILLUSIONER", SpawnCategory.MONSTER);
        put("INTERACTION", SpawnCategory.MISC);
        put("IRON_GOLEM", SpawnCategory.MISC);
        put("ITEM", SpawnCategory.MISC);
        put("ITEM_DISPLAY", SpawnCategory.MISC);
        put("ITEM_FRAME", SpawnCategory.MISC);
        put("JUNGLE_BOAT", SpawnCategory.MISC);
        put("JUNGLE_CHEST_BOAT", SpawnCategory.MISC);
        put("LEASH_KNOT", SpawnCategory.MISC);
        put("LIGHTNING_BOLT", SpawnCategory.MISC);
        put("LINGERING_POTION", SpawnCategory.MISC);
        put("LLAMA", SpawnCategory.ANIMAL);
        put("LLAMA_SPIT", SpawnCategory.MISC);
        put("MAGMA_CUBE", SpawnCategory.MONSTER);
        put("MANGROVE_BOAT", SpawnCategory.MISC);
        put("MANGROVE_CHEST_BOAT", SpawnCategory.MISC);
        put("MANNEQUIN", SpawnCategory.MISC);
        put("MARKER", SpawnCategory.MISC);
        put("MINECART", SpawnCategory.MISC);
        put("MOOSHROOM", SpawnCategory.ANIMAL);
        put("MULE", SpawnCategory.ANIMAL);
        put("NAUTILUS", SpawnCategory.WATER_ANIMAL);
        put("OAK_BOAT", SpawnCategory.MISC);
        put("OAK_CHEST_BOAT", SpawnCategory.MISC);
        put("OCELOT", SpawnCategory.ANIMAL);
        put("OMINOUS_ITEM_SPAWNER", SpawnCategory.MISC);
        put("PAINTING", SpawnCategory.MISC);
        put("PALE_OAK_BOAT", SpawnCategory.MISC);
        put("PALE_OAK_CHEST_BOAT", SpawnCategory.MISC);
        put("PANDA", SpawnCategory.ANIMAL);
        put("PARCHED", SpawnCategory.MONSTER);
        put("PARROT", SpawnCategory.ANIMAL);
        put("PHANTOM", SpawnCategory.MONSTER);
        put("PIG", SpawnCategory.ANIMAL);
        put("PIGLIN", SpawnCategory.MONSTER);
        put("PIGLIN_BRUTE", SpawnCategory.MONSTER);
        put("PILLAGER", SpawnCategory.MONSTER);
        put("POLAR_BEAR", SpawnCategory.ANIMAL);
        put("POPLAR_BOAT", SpawnCategory.MISC);
        put("POPLAR_CHEST_BOAT", SpawnCategory.MISC);
        put("PUFFERFISH", SpawnCategory.WATER_AMBIENT);
        put("RABBIT", SpawnCategory.ANIMAL);
        put("RAVAGER", SpawnCategory.MONSTER);
        put("SALMON", SpawnCategory.WATER_AMBIENT);
        put("SHEEP", SpawnCategory.ANIMAL);
        put("SHULKER", SpawnCategory.MONSTER);
        put("SHULKER_BULLET", SpawnCategory.MISC);
        put("SILVERFISH", SpawnCategory.MONSTER);
        put("SKELETON", SpawnCategory.MONSTER);
        put("SKELETON_HORSE", SpawnCategory.ANIMAL);
        put("SLIME", SpawnCategory.MONSTER);
        put("SMALL_FIREBALL", SpawnCategory.MISC);
        put("SNIFFER", SpawnCategory.ANIMAL);
        put("SNOWBALL", SpawnCategory.MISC);
        put("SNOW_GOLEM", SpawnCategory.MISC);
        put("SPAWNER_MINECART", SpawnCategory.MISC);
        put("SPECTRAL_ARROW", SpawnCategory.MISC);
        put("SPIDER", SpawnCategory.MONSTER);
        put("SPLASH_POTION", SpawnCategory.MISC);
        put("SPRUCE_BOAT", SpawnCategory.MISC);
        put("SPRUCE_CHEST_BOAT", SpawnCategory.MISC);
        put("SQUID", SpawnCategory.WATER_ANIMAL);
        put("STRAY", SpawnCategory.MONSTER);
        put("STRIDER", SpawnCategory.ANIMAL);
        put("SULFUR_CUBE", SpawnCategory.MONSTER);
        put("TADPOLE", SpawnCategory.ANIMAL);
        put("TEXT_DISPLAY", SpawnCategory.MISC);
        put("TNT", SpawnCategory.MISC);
        put("TNT_MINECART", SpawnCategory.MISC);
        put("TRADER_LLAMA", SpawnCategory.ANIMAL);
        put("TRIDENT", SpawnCategory.MISC);
        put("TROPICAL_FISH", SpawnCategory.WATER_AMBIENT);
        put("TURTLE", SpawnCategory.ANIMAL);
        put("VEX", SpawnCategory.MONSTER);
        put("VILLAGER", SpawnCategory.MISC);
        put("VINDICATOR", SpawnCategory.MONSTER);
        put("WANDERING_TRADER", SpawnCategory.ANIMAL);
        put("WARDEN", SpawnCategory.MONSTER);
        put("WIND_CHARGE", SpawnCategory.MISC);
        put("WITCH", SpawnCategory.MONSTER);
        put("WITHER", SpawnCategory.MONSTER);
        put("WITHER_SKELETON", SpawnCategory.MONSTER);
        put("WITHER_SKULL", SpawnCategory.MISC);
        put("WOLF", SpawnCategory.ANIMAL);
        put("ZOGLIN", SpawnCategory.MONSTER);
        put("ZOMBIE", SpawnCategory.MONSTER);
        put("ZOMBIE_HORSE", SpawnCategory.MONSTER);
        put("ZOMBIE_NAUTILUS", SpawnCategory.MONSTER);
        put("ZOMBIE_VILLAGER", SpawnCategory.MONSTER);
        put("ZOMBIFIED_PIGLIN", SpawnCategory.MONSTER);
    }
}
