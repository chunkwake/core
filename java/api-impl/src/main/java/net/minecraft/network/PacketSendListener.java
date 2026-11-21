package net.minecraft.network;

/** NMS {@code PacketSendListener} (CraftEngine). */
public interface PacketSendListener {
    static PacketSendListener thenRun(Runnable runnable) {
        return new PacketSendListener() {
        };
    }
}
