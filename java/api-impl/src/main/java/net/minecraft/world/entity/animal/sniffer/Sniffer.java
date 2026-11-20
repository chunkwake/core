package net.minecraft.world.entity.animal.sniffer;

/** NMS {@code Sniffer}: FancyNpcs использует вложенный State. */
public class Sniffer {
    public enum State {
        IDLE,
        SCENTING,
        SNIFFING,
        SEARCHING,
        DIGGING,
        RISING,
        FEELING_HAPPY;

        public static State byId(int id) {
            State[] all = values();
            return id >= 0 && id < all.length ? all[id] : IDLE;
        }
    }
}
