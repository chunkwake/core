package net.minecraft.world.entity.animal.cow;

/** NMS {@code MushroomCow}: FancyNpcs использует вложенный Variant. */
public class MushroomCow {
    public enum Variant {
        RED,
        BROWN;

        public static Variant byId(int id) {
            Variant[] all = values();
            return id >= 0 && id < all.length ? all[id] : RED;
        }
    }
}
