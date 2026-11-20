package net.minecraft.world.entity;

/** Заглушка NMS EquipmentSlot: порядок = проводной id слота. */
public enum EquipmentSlot {
    MAINHAND,
    OFFHAND,
    FEET,
    LEGS,
    CHEST,
    HEAD,
    BODY,
    SADDLE;

    public static EquipmentSlot byId(int id) {
        EquipmentSlot[] all = values();
        return id >= 0 && id < all.length ? all[id] : MAINHAND;
    }
}
