package dev.chunkwake.bridge;

import java.util.Locale;
import java.util.Map;
import org.bukkit.Material;
import org.bukkit.material.MaterialData;

/**
 * Конвертация legacy-материалов (`LEGACY_*`, id + data) в современные, как
 * {@code CraftLegacy.modernize} в CraftBukkit. Плагины (в том числе
 * EssentialsX) перебирают {@code Material.values()} и вызывают
 * {@code isTransparent()} -> {@code isBlock()} -> {@code asBlockType()}, а тот
 * для legacy-материала зовёт {@code UnsafeValues.fromLegacy(...)}.
 */
final class LegacyMaterials {
    private static final String[] COLORS = {
        "WHITE", "ORANGE", "MAGENTA", "LIGHT_BLUE", "YELLOW", "LIME", "PINK", "GRAY",
        "LIGHT_GRAY", "CYAN", "PURPLE", "BLUE", "BROWN", "GREEN", "RED", "BLACK",
    };

    /** Точные переименования, которые не выводятся из имени. */
    private static final Map<String, String> RENAMES = Map.ofEntries(
            Map.entry("GRASS", "GRASS_BLOCK"),
            Map.entry("LONG_GRASS", "SHORT_GRASS"),
            Map.entry("YELLOW_FLOWER", "DANDELION"),
            Map.entry("RED_ROSE", "POPPY"),
            Map.entry("SIGN", "OAK_SIGN"),
            Map.entry("WALL_SIGN", "OAK_WALL_SIGN"),
            Map.entry("SIGN_POST", "OAK_SIGN"),
            Map.entry("WOOD_DOOR", "OAK_DOOR"),
            Map.entry("WOODEN_DOOR", "OAK_DOOR"),
            Map.entry("FENCE", "OAK_FENCE"),
            Map.entry("FENCE_GATE", "OAK_FENCE_GATE"),
            Map.entry("WOOD", "OAK_PLANKS"),
            Map.entry("LOG", "OAK_LOG"),
            Map.entry("LOG_2", "OAK_LOG"),
            Map.entry("LEAVES", "OAK_LEAVES"),
            Map.entry("LEAVES_2", "OAK_LEAVES"),
            Map.entry("SAPLING", "OAK_SAPLING"),
            Map.entry("STEP", "STONE_SLAB"),
            Map.entry("DOUBLE_STEP", "STONE_SLAB"),
            Map.entry("WOOD_STEP", "OAK_SLAB"),
            Map.entry("WOOD_DOUBLE_STEP", "OAK_SLAB"),
            Map.entry("WOOD_STAIRS", "OAK_STAIRS"),
            Map.entry("SPRUCE_WOOD_STAIRS", "SPRUCE_STAIRS"),
            Map.entry("BIRCH_WOOD_STAIRS", "BIRCH_STAIRS"),
            Map.entry("JUNGLE_WOOD_STAIRS", "JUNGLE_STAIRS"),
            Map.entry("BOAT", "OAK_BOAT"),
            Map.entry("BOAT_ACACIA", "ACACIA_BOAT"),
            Map.entry("BOAT_BIRCH", "BIRCH_BOAT"),
            Map.entry("BOAT_DARK_OAK", "DARK_OAK_BOAT"),
            Map.entry("BOAT_JUNGLE", "JUNGLE_BOAT"),
            Map.entry("BOAT_SPRUCE", "SPRUCE_BOAT"),
            Map.entry("PORK", "PORKCHOP"),
            Map.entry("GRILLED_PORK", "COOKED_PORKCHOP"),
            Map.entry("SULPHUR", "GUNPOWDER"),
            Map.entry("NETHER_BRICK_ITEM", "NETHER_BRICK"),
            Map.entry("FIREWORK_CHARGE", "FIREWORK_STAR"),
            Map.entry("EXP_BOTTLE", "EXPERIENCE_BOTTLE"),
            Map.entry("SNOW_BALL", "SNOWBALL"),
            Map.entry("WATER_LILY", "LILY_PAD"),
            Map.entry("SUGAR_CANE_BLOCK", "SUGAR_CANE"),
            Map.entry("NETHER_WARTS", "NETHER_WART"),
            Map.entry("CARROT_ITEM", "CARROT"),
            Map.entry("POTATO_ITEM", "POTATO"),
            Map.entry("BEETROOT_ITEM", "BEETROOT"),
            Map.entry("SEEDS", "WHEAT_SEEDS"),
            Map.entry("CAULDRON_ITEM", "CAULDRON"),
            Map.entry("BREWING_STAND_ITEM", "BREWING_STAND"),
            Map.entry("ENCHANTMENT_TABLE", "ENCHANTING_TABLE"),
            Map.entry("WORKBENCH", "CRAFTING_TABLE"),
            Map.entry("BOOK_AND_QUILL", "WRITABLE_BOOK"),
            Map.entry("WATCH", "CLOCK"),
            Map.entry("EMPTY_MAP", "MAP"),
            Map.entry("STATIONARY_WATER", "WATER"),
            Map.entry("FLOWING_WATER", "WATER"),
            Map.entry("STATIONARY_LAVA", "LAVA"),
            Map.entry("FLOWING_LAVA", "LAVA"),
            Map.entry("MOB_SPAWNER", "SPAWNER"),
            Map.entry("MONSTER_EGG", "AIR"),
            Map.entry("MONSTER_EGGS", "INFESTED_STONE"),
            Map.entry("GOLD_RECORD", "MUSIC_DISC_13"),
            Map.entry("GREEN_RECORD", "MUSIC_DISC_CAT"),
            Map.entry("RECORD_3", "MUSIC_DISC_BLOCKS"),
            Map.entry("RECORD_4", "MUSIC_DISC_CHIRP"),
            Map.entry("RECORD_5", "MUSIC_DISC_FAR"),
            Map.entry("RECORD_6", "MUSIC_DISC_MALL"),
            Map.entry("RECORD_7", "MUSIC_DISC_MELLOHI"),
            Map.entry("RECORD_8", "MUSIC_DISC_STAL"),
            Map.entry("RECORD_9", "MUSIC_DISC_STRAD"),
            Map.entry("RECORD_10", "MUSIC_DISC_WARD"),
            Map.entry("RECORD_11", "MUSIC_DISC_11"),
            Map.entry("RECORD_12", "MUSIC_DISC_WAIT"));

    /** Семейства, где data задаёт цвет. */
    private static final Map<String, String> COLORED = Map.ofEntries(
            Map.entry("WOOL", "%s_WOOL"),
            Map.entry("STAINED_GLASS", "%s_STAINED_GLASS"),
            Map.entry("STAINED_GLASS_PANE", "%s_STAINED_GLASS_PANE"),
            Map.entry("CARPET", "%s_CARPET"),
            Map.entry("CONCRETE", "%s_CONCRETE"),
            Map.entry("CONCRETE_POWDER", "%s_CONCRETE_POWDER"),
            Map.entry("STAINED_CLAY", "%s_TERRACOTTA"),
            Map.entry("BANNER", "%s_BANNER"),
            Map.entry("WALL_BANNER", "%s_WALL_BANNER"),
            Map.entry("BED", "%s_BED"),
            Map.entry("BED_BLOCK", "%s_BED"));

    private LegacyMaterials() {
    }

    /**
     * `CraftLegacy.modernize`: legacy-материал + data -> современный.
     * Неизвестный блок -> `STONE`, неизвестный предмет -> `AIR` (никогда `null`).
     */
    @SuppressWarnings("deprecation")
    static Material modernize(MaterialData data, boolean block) {
        Material type = data == null ? null : data.getItemType();
        if (type == null) {
            return block ? Material.STONE : Material.AIR;
        }
        if (!type.isLegacy()) {
            return type;
        }
        byte raw = data.getData();
        int id = raw & 0xFF;
        String name = type.name().substring("LEGACY_".length());

        String renamed = RENAMES.get(name);
        if (renamed != null) {
            Material exact = Material.matchMaterial(renamed);
            if (exact != null) {
                return exact;
            }
        }
        String family = COLORED.get(name);
        if (family != null) {
            Material colored = Material.matchMaterial(String.format(family, COLORS[id & 15]));
            if (colored != null) {
                return colored;
            }
        }
        // data-зависимые варианты без цвета.
        switch (name) {
            case "INK_SACK" -> {
                String dye = switch (id) {
                    case 0 -> "INK_SAC";
                    case 1 -> "RED_DYE";
                    case 2 -> "GREEN_DYE";
                    case 3 -> "COCOA_BEANS";
                    case 4 -> "LAPIS_LAZULI";
                    case 5 -> "PURPLE_DYE";
                    case 6 -> "CYAN_DYE";
                    case 7 -> "LIGHT_GRAY_DYE";
                    case 8 -> "GRAY_DYE";
                    case 9 -> "PINK_DYE";
                    case 10 -> "LIME_DYE";
                    case 11 -> "YELLOW_DYE";
                    case 12 -> "LIGHT_BLUE_DYE";
                    case 13 -> "MAGENTA_DYE";
                    case 14 -> "ORANGE_DYE";
                    default -> "BONE_MEAL";
                };
                Material dyeMaterial = Material.matchMaterial(dye);
                if (dyeMaterial != null) {
                    return dyeMaterial;
                }
            }
            case "RAW_FISH" -> {
                Material fish = Material.matchMaterial(switch (id) {
                    case 1 -> "SALMON";
                    case 2 -> "TROPICAL_FISH";
                    case 3 -> "PUFFERFISH";
                    default -> "COD";
                });
                if (fish != null) {
                    return fish;
                }
            }
            case "COOKED_FISH" -> {
                Material fish = Material.matchMaterial(id == 1 ? "COOKED_SALMON" : "COOKED_COD");
                if (fish != null) {
                    return fish;
                }
            }
            case "SMOOTH_BRICK" -> {
                Material bricks = Material.matchMaterial(id == 3 ? "CHISELED_STONE_BRICKS"
                        : id == 2 ? "CRACKED_STONE_BRICKS"
                        : id == 1 ? "MOSSY_STONE_BRICKS"
                        : "STONE_BRICKS");
                if (bricks != null) {
                    return bricks;
                }
            }
            case "SANDSTONE" -> {
                Material sand = Material.matchMaterial(id == 1 ? "CHISELED_SANDSTONE"
                        : id == 2 ? "CUT_SANDSTONE"
                        : "SANDSTONE");
                if (sand != null) {
                    return sand;
                }
            }
            case "RED_SANDSTONE" -> {
                Material sand = Material.matchMaterial(id == 1 ? "CHISELED_RED_SANDSTONE"
                        : id == 2 ? "CUT_RED_SANDSTONE"
                        : "RED_SANDSTONE");
                if (sand != null) {
                    return sand;
                }
            }
            case "QUARTZ_BLOCK" -> {
                Material quartz = Material.matchMaterial(id == 1 ? "CHISELED_QUARTZ_BLOCK"
                        : id == 2 ? "QUARTZ_PILLAR"
                        : "QUARTZ_BLOCK");
                if (quartz != null) {
                    return quartz;
                }
            }
            default -> {
            }
        }
        // Имя без LEGACY_ обычно совпадает с современным (`LEGACY_STONE` -> `STONE`).
        Material plain = Material.matchMaterial(name.toLowerCase(Locale.ROOT));
        if (plain != null && !plain.isLegacy()) {
            return plain;
        }
        return block ? Material.STONE : Material.AIR;
    }

    /** Современный материал для legacy-константы (без data). */
    static Material modernize(Material type, boolean block) {
        return modernize(new MaterialData(type), block);
    }
}
