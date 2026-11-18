package net.minecraft.network;

/** NMS {@code PacketListener} (ProtocolLib PacketListenerInvoker). */
public interface PacketListener {
    default void onDisconnect(net.minecraft.network.chat.Component reason) {
    }
}
