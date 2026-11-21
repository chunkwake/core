package net.minecraft.network.protocol.common;

/** NMS {@code ServerCommonPacketListener} (CraftEngine). */
public interface ServerCommonPacketListener {
    void handleResourcePackResponse(ServerboundResourcePackPacket packet);
}
