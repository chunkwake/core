package net.minecraft.world.entity.animal.equine;

/** NMS {@code Variant}: масти лошадей. */
public enum Variant {
    WHITE,
    CREAMY,
    CHESTNUT,
    BROWN,
    BLACK,
    GRAY,
    DARK_BROWN;

    public static Variant byId(int id) {
        Variant[] all = values();
        return id >= 0 && id < all.length ? all[id] : WHITE;
    }

    public int getId() {
        return this.ordinal();
    }
}
