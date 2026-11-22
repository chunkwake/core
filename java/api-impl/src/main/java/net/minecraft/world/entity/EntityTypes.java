package net.minecraft.world.entity;

/** NMS {@code EntityTypes} (набор типов). */
public final class EntityTypes {
    /** Поле, которое ищет CraftEngine (EntityTypesProxy#getItem). */
    public static final EntityType<Entity> item = new EntityType<>("minecraft:item");
    public static final EntityType<Entity> ITEM = item;
    public static final EntityType<Entity> fallingBlock = new EntityType<>("minecraft:falling_block");
    public static final EntityType<Entity> FALLING_BLOCK = fallingBlock;
    public static final EntityType<Entity> spectralArrow = new EntityType<>("minecraft:spectral_arrow");
    public static final EntityType<Entity> SPECTRAL_ARROW = spectralArrow;
    public static final EntityType<Entity> happyGhast = new EntityType<>("minecraft:happy_ghast");
    public static final EntityType<Entity> HAPPY_GHAST = happyGhast;
    public static final EntityType<Entity> tntMinecart = new EntityType<>("minecraft:tnt_minecart");
    public static final EntityType<Entity> TNT_MINECART = tntMinecart;
    public static final EntityType<Entity> shulker = new EntityType<>("minecraft:shulker");
    public static final EntityType<Entity> SHULKER = shulker;
    public static final EntityType<Entity> textDisplay = new EntityType<>("minecraft:text_display");
    public static final EntityType<Entity> TEXT_DISPLAY = textDisplay;
    public static final EntityType<Entity> oakBoat = new EntityType<>("minecraft:oak_boat");
    public static final EntityType<Entity> OAK_BOAT = oakBoat;
    public static final EntityType<Entity> trident = new EntityType<>("minecraft:trident");
    public static final EntityType<Entity> TRIDENT = trident;
    public static final EntityType<Entity> potion = new EntityType<>("minecraft:potion");
    public static final EntityType<Entity> POTION = potion;

    public static final EntityType<Entity> ARMOR_STAND = new EntityType<>("minecraft:armor_stand");
    public static final EntityType<Entity> ARROW = new EntityType<>("minecraft:arrow");
    public static final EntityType<Entity> BLOCK_DISPLAY = new EntityType<>("minecraft:block_display");
    public static final EntityType<Entity> CHEST_MINECART = new EntityType<>("minecraft:chest_minecart");
    public static final EntityType<Entity> COMMAND_BLOCK_MINECART = new EntityType<>("minecraft:command_block_minecart");
    public static final EntityType<Entity> EGG = new EntityType<>("minecraft:egg");
    public static final EntityType<Entity> ENDER_PEARL = new EntityType<>("minecraft:ender_pearl");
    public static final EntityType<Entity> ENDERMAN = new EntityType<>("minecraft:enderman");
    public static final EntityType<Entity> EXPERIENCE_BOTTLE = new EntityType<>("minecraft:experience_bottle");
    public static final EntityType<Entity> EYE_OF_ENDER = new EntityType<>("minecraft:eye_of_ender");
    public static final EntityType<Entity> FIREBALL = new EntityType<>("minecraft:fireball");
    public static final EntityType<Entity> FIREWORK_ROCKET = new EntityType<>("minecraft:firework_rocket");
    public static final EntityType<Entity> FURNACE_MINECART = new EntityType<>("minecraft:furnace_minecart");
    public static final EntityType<Entity> GLOW_ITEM_FRAME = new EntityType<>("minecraft:glow_item_frame");
    public static final EntityType<Entity> HOPPER_MINECART = new EntityType<>("minecraft:hopper_minecart");
    public static final EntityType<Entity> INTERACTION = new EntityType<>("minecraft:interaction");
    public static final EntityType<Entity> ITEM_DISPLAY = new EntityType<>("minecraft:item_display");
    public static final EntityType<Entity> MINECART = new EntityType<>("minecraft:minecart");
    public static final EntityType<Entity> OMINOUS_ITEM_SPAWNER = new EntityType<>("minecraft:ominous_item_spawner");
    public static final EntityType<Entity> SMALL_FIREBALL = new EntityType<>("minecraft:small_fireball");
    public static final EntityType<Entity> SNOWBALL = new EntityType<>("minecraft:snowball");
    public static final EntityType<Entity> SPAWNER_MINECART = new EntityType<>("minecraft:spawner_minecart");
    public static final EntityType<Entity> TNT = new EntityType<>("minecraft:tnt");
    public static final EntityType<Entity> WIND_CHARGE = new EntityType<>("minecraft:wind_charge");
    public static final EntityType<Entity> MARKER = new EntityType<>("minecraft:marker");
    public static final EntityType<Entity> ITEM_FRAME = new EntityType<>("minecraft:item_frame");
    public static final EntityType<Entity> PLAYER = new EntityType<>("minecraft:player");

    private EntityTypes() {
    }
}
