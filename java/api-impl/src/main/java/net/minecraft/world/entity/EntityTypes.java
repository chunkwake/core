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

    public static final EntityType<Entity> MARKER = new EntityType<>("minecraft:marker");
    public static final EntityType<Entity> ITEM_FRAME = new EntityType<>("minecraft:item_frame");
    public static final EntityType<Entity> PLAYER = new EntityType<>("minecraft:player");

    private EntityTypes() {
    }
}
