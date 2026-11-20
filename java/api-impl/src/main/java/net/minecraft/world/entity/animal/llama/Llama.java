package net.minecraft.world.entity.animal.llama;

/** NMS {@code Llama}: вложенный Variant. */
public class Llama {
    public enum Variant {
        CREAMY,
        WHITE,
        BROWN,
        GRAY;

        public static Variant byId(int id) {
            Variant[] all = values();
            return id >= 0 && id < all.length ? all[id] : CREAMY;
        }
    }
}
