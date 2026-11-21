package net.minecraft.world.entity;

/** NMS {@code EntityTypes} (набор типов). */
public final class EntityTypes {
    /** Поле, которое ищет CraftEngine (EntityTypesProxy#getItem). */
    public static final EntityType<Entity> item = new EntityType<>("minecraft:item");
    public static final EntityType<Entity> ITEM = item;

    public static final EntityType<Entity> MARKER = new EntityType<>("minecraft:marker");
    public static final EntityType<Entity> ITEM_FRAME = new EntityType<>("minecraft:item_frame");
    public static final EntityType<Entity> PLAYER = new EntityType<>("minecraft:player");

    private EntityTypes() {
    }
}
