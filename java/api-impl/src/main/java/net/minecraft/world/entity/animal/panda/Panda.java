package net.minecraft.world.entity.animal.panda;

/** NMS {@code Panda}: FancyNpcs использует вложенный Gene. */
public class Panda {
    public enum Gene {
        NORMAL,
        LAZY,
        WORRIED,
        PLAYFUL,
        BROWN,
        AGGRESSIVE,
        WEAK;

        public static Gene byId(int id) {
            Gene[] all = values();
            return id >= 0 && id < all.length ? all[id] : NORMAL;
        }
    }
}
