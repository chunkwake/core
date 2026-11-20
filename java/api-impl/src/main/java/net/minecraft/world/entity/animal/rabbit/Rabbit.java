package net.minecraft.world.entity.animal.rabbit;

/** NMS {@code Rabbit}: вложенный Variant. */
public class Rabbit {
    public enum Variant {
        BROWN,
        WHITE,
        BLACK,
        GOLD,
        SALT,
        EVIL;

        public static Variant byId(int id) {
            Variant[] all = values();
            return id >= 0 && id < all.length ? all[id] : BROWN;
        }
    }
}
