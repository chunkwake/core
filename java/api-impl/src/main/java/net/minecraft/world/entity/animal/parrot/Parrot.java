package net.minecraft.world.entity.animal.parrot;

/** NMS {@code Parrot}: FancyNpcs использует вложенный Variant. */
public class Parrot {
    public enum Variant {
        RED_BLUE,
        BLUE,
        GREEN,
        YELLOW_BLUE,
        GRAY;

        public static Variant byId(int id) {
            Variant[] all = values();
            return id >= 0 && id < all.length ? all[id] : RED_BLUE;
        }
    }
}
