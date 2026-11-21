package net.minecraft.core.component;

import net.minecraft.world.item.component.CustomData;
import net.minecraft.world.item.component.ItemLore;

/** NMS {@code DataComponents} (ImageFrame). */
public final class DataComponents {
    public static final DataComponentType<CustomData> CUSTOM_DATA = new DataComponentType<>();
    public static final DataComponentType<ItemLore> LORE = new DataComponentType<>();
    /** {@code DataComponents.MAX_STACK_SIZE} (EconomyShopGUI). */
    public static final DataComponentType<Integer> MAX_STACK_SIZE = new DataComponentType<>();
    /** {@code DataComponents.MAX_DAMAGE} (EconomyShopGUI). */
    public static final DataComponentType<Integer> MAX_DAMAGE = new DataComponentType<>();
    /** {@code DataComponents.DAMAGE} (EconomyShopGUI). */
    public static final DataComponentType<Integer> DAMAGE = new DataComponentType<>();

    /** {@code DataComponents.UNBREAKABLE} (EconomyShopGUI). */
    public static final DataComponentType<Object> UNBREAKABLE = new DataComponentType<>();

    /** {@code DataComponents.CUSTOM_NAME} (EconomyShopGUI). */
    public static final DataComponentType<Object> CUSTOM_NAME = new DataComponentType<>();

    /** {@code DataComponents.ITEM_NAME} (EconomyShopGUI). */
    public static final DataComponentType<Object> ITEM_NAME = new DataComponentType<>();

    /** {@code DataComponents.RARITY} (EconomyShopGUI). */
    public static final DataComponentType<Object> RARITY = new DataComponentType<>();

    /** {@code DataComponents.ENCHANTMENTS} (EconomyShopGUI). */
    public static final DataComponentType<Object> ENCHANTMENTS = new DataComponentType<>();

    /** {@code DataComponents.CAN_PLACE_ON} (EconomyShopGUI). */
    public static final DataComponentType<Object> CAN_PLACE_ON = new DataComponentType<>();

    /** {@code DataComponents.CAN_BREAK} (EconomyShopGUI). */
    public static final DataComponentType<Object> CAN_BREAK = new DataComponentType<>();

    /** {@code DataComponents.ATTRIBUTE_MODIFIERS} (EconomyShopGUI). */
    public static final DataComponentType<Object> ATTRIBUTE_MODIFIERS = new DataComponentType<>();

    /** {@code DataComponents.CUSTOM_MODEL_DATA} (EconomyShopGUI). */
    public static final DataComponentType<Object> CUSTOM_MODEL_DATA = new DataComponentType<>();

    /** {@code DataComponents.TOOLTIP_DISPLAY} (EconomyShopGUI). */
    public static final DataComponentType<Object> TOOLTIP_DISPLAY = new DataComponentType<>();

    /** {@code DataComponents.REPAIR_COST} (EconomyShopGUI). */
    public static final DataComponentType<Object> REPAIR_COST = new DataComponentType<>();

    /** {@code DataComponents.CREATIVE_SLOT_LOCK} (EconomyShopGUI). */
    public static final DataComponentType<Object> CREATIVE_SLOT_LOCK = new DataComponentType<>();

    /** {@code DataComponents.ENCHANTMENT_GLINT_OVERRIDE} (EconomyShopGUI). */
    public static final DataComponentType<Object> ENCHANTMENT_GLINT_OVERRIDE = new DataComponentType<>();

    /** {@code DataComponents.INTANGIBLE_PROJECTILE} (EconomyShopGUI). */
    public static final DataComponentType<Object> INTANGIBLE_PROJECTILE = new DataComponentType<>();

    /** {@code DataComponents.FOOD} (EconomyShopGUI). */
    public static final DataComponentType<Object> FOOD = new DataComponentType<>();

    /** {@code DataComponents.CONSUMABLE} (EconomyShopGUI). */
    public static final DataComponentType<Object> CONSUMABLE = new DataComponentType<>();

    /** {@code DataComponents.USE_REMAINDER} (EconomyShopGUI). */
    public static final DataComponentType<Object> USE_REMAINDER = new DataComponentType<>();

    /** {@code DataComponents.USE_COOLDOWN} (EconomyShopGUI). */
    public static final DataComponentType<Object> USE_COOLDOWN = new DataComponentType<>();

    /** {@code DataComponents.DAMAGE_RESISTANT} (EconomyShopGUI). */
    public static final DataComponentType<Object> DAMAGE_RESISTANT = new DataComponentType<>();

    /** {@code DataComponents.TOOL} (EconomyShopGUI). */
    public static final DataComponentType<Object> TOOL = new DataComponentType<>();

    /** {@code DataComponents.ENCHANTABLE} (EconomyShopGUI). */
    public static final DataComponentType<Object> ENCHANTABLE = new DataComponentType<>();

    /** {@code DataComponents.EQUIPPABLE} (EconomyShopGUI). */
    public static final DataComponentType<Object> EQUIPPABLE = new DataComponentType<>();

    /** {@code DataComponents.REPAIRABLE} (EconomyShopGUI). */
    public static final DataComponentType<Object> REPAIRABLE = new DataComponentType<>();

    /** {@code DataComponents.GLIDER} (EconomyShopGUI). */
    public static final DataComponentType<Object> GLIDER = new DataComponentType<>();

    /** {@code DataComponents.DEATH_PROTECTION} (EconomyShopGUI). */
    public static final DataComponentType<Object> DEATH_PROTECTION = new DataComponentType<>();

    /** {@code DataComponents.STORED_ENCHANTMENTS} (EconomyShopGUI). */
    public static final DataComponentType<Object> STORED_ENCHANTMENTS = new DataComponentType<>();

    /** {@code DataComponents.DYED_COLOR} (EconomyShopGUI). */
    public static final DataComponentType<Object> DYED_COLOR = new DataComponentType<>();

    /** {@code DataComponents.MAP_ID} (EconomyShopGUI). */
    public static final DataComponentType<Object> MAP_ID = new DataComponentType<>();

    /** {@code DataComponents.MAP_DECORATIONS} (EconomyShopGUI). */
    public static final DataComponentType<Object> MAP_DECORATIONS = new DataComponentType<>();

    /** {@code DataComponents.MAP_POST_PROCESSING} (EconomyShopGUI). */
    public static final DataComponentType<Object> MAP_POST_PROCESSING = new DataComponentType<>();

    /** {@code DataComponents.CHARGED_PROJECTILES} (EconomyShopGUI). */
    public static final DataComponentType<Object> CHARGED_PROJECTILES = new DataComponentType<>();

    /** {@code DataComponents.BUNDLE_CONTENTS} (EconomyShopGUI). */
    public static final DataComponentType<Object> BUNDLE_CONTENTS = new DataComponentType<>();

    /** {@code DataComponents.POTION_CONTENTS} (EconomyShopGUI). */
    public static final DataComponentType<Object> POTION_CONTENTS = new DataComponentType<>();

    /** {@code DataComponents.SUSPICIOUS_STEW_EFFECTS} (EconomyShopGUI). */
    public static final DataComponentType<Object> SUSPICIOUS_STEW_EFFECTS = new DataComponentType<>();

    /** {@code DataComponents.WRITABLE_BOOK_CONTENT} (EconomyShopGUI). */
    public static final DataComponentType<Object> WRITABLE_BOOK_CONTENT = new DataComponentType<>();

    /** {@code DataComponents.WRITTEN_BOOK_CONTENT} (EconomyShopGUI). */
    public static final DataComponentType<Object> WRITTEN_BOOK_CONTENT = new DataComponentType<>();

    /** {@code DataComponents.TRIM} (EconomyShopGUI). */
    public static final DataComponentType<Object> TRIM = new DataComponentType<>();

    /** {@code DataComponents.DEBUG_STICK_STATE} (EconomyShopGUI). */
    public static final DataComponentType<Object> DEBUG_STICK_STATE = new DataComponentType<>();

    /** {@code DataComponents.ENTITY_DATA} (EconomyShopGUI). */
    public static final DataComponentType<Object> ENTITY_DATA = new DataComponentType<>();

    /** {@code DataComponents.BUCKET_ENTITY_DATA} (EconomyShopGUI). */
    public static final DataComponentType<Object> BUCKET_ENTITY_DATA = new DataComponentType<>();

    /** {@code DataComponents.BLOCK_ENTITY_DATA} (EconomyShopGUI). */
    public static final DataComponentType<Object> BLOCK_ENTITY_DATA = new DataComponentType<>();

    /** {@code DataComponents.INSTRUMENT} (EconomyShopGUI). */
    public static final DataComponentType<Object> INSTRUMENT = new DataComponentType<>();

    /** {@code DataComponents.OMINOUS_BOTTLE_AMPLIFIER} (EconomyShopGUI). */
    public static final DataComponentType<Object> OMINOUS_BOTTLE_AMPLIFIER = new DataComponentType<>();

    /** {@code DataComponents.JUKEBOX_PLAYABLE} (EconomyShopGUI). */
    public static final DataComponentType<Object> JUKEBOX_PLAYABLE = new DataComponentType<>();

    /** {@code DataComponents.RECIPES} (EconomyShopGUI). */
    public static final DataComponentType<Object> RECIPES = new DataComponentType<>();

    /** {@code DataComponents.LODESTONE_TRACKER} (EconomyShopGUI). */
    public static final DataComponentType<Object> LODESTONE_TRACKER = new DataComponentType<>();

    /** {@code DataComponents.FIREWORK_EXPLOSION} (EconomyShopGUI). */
    public static final DataComponentType<Object> FIREWORK_EXPLOSION = new DataComponentType<>();

    /** {@code DataComponents.FIREWORKS} (EconomyShopGUI). */
    public static final DataComponentType<Object> FIREWORKS = new DataComponentType<>();

    /** {@code DataComponents.PROFILE} (EconomyShopGUI). */
    public static final DataComponentType<Object> PROFILE = new DataComponentType<>();

    /** {@code DataComponents.BANNER_PATTERNS} (EconomyShopGUI). */
    public static final DataComponentType<Object> BANNER_PATTERNS = new DataComponentType<>();

    /** {@code DataComponents.BASE_COLOR} (EconomyShopGUI). */
    public static final DataComponentType<Object> BASE_COLOR = new DataComponentType<>();

    /** {@code DataComponents.POT_DECORATIONS} (EconomyShopGUI). */
    public static final DataComponentType<Object> POT_DECORATIONS = new DataComponentType<>();

    /** {@code DataComponents.CONTAINER} (EconomyShopGUI). */
    public static final DataComponentType<Object> CONTAINER = new DataComponentType<>();

    /** {@code DataComponents.BLOCK_STATE} (EconomyShopGUI). */
    public static final DataComponentType<Object> BLOCK_STATE = new DataComponentType<>();

    /** {@code DataComponents.BEES} (EconomyShopGUI). */
    public static final DataComponentType<Object> BEES = new DataComponentType<>();

    /** {@code DataComponents.LOCK} (EconomyShopGUI). */
    public static final DataComponentType<Object> LOCK = new DataComponentType<>();

    /** {@code DataComponents.CONTAINER_LOOT} (EconomyShopGUI). */
    public static final DataComponentType<Object> CONTAINER_LOOT = new DataComponentType<>();

    private DataComponents() {
    }
}
