package net.minecraft.world.entity.animal.equine;

/** NMS {@code Markings}: отметины лошадей. */
public enum Markings {
    NONE,
    WHITE_DOTS,
    WHITE_FIELD,
    WHITE_STRIPES,
    BLACK_DOTS;

    public static Markings byId(int id) {
        Markings[] all = values();
        return id >= 0 && id < all.length ? all[id] : NONE;
    }
}
