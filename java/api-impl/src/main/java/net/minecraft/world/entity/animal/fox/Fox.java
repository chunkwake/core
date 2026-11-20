package net.minecraft.world.entity.animal.fox;

/** NMS {@code Fox}: FancyNpcs использует вложенный Variant. */
public class Fox {
    public enum Variant {
        RED,
        SNOW;

        public static Variant byId(int id) {
            Variant[] all = values();
            return id >= 0 && id < all.length ? all[id] : RED;
        }
    }
}
