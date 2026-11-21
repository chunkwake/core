package net.minecraft.network.protocol.game;

import net.minecraft.network.chat.Component;
import net.minecraft.network.protocol.Packet;

/** NMS {@code ClientboundDisconnectPacket} (CraftEngine). */
public class ClientboundDisconnectPacket implements Packet {
    private Component reason;

    public ClientboundDisconnectPacket() {
    }

    public ClientboundDisconnectPacket(Component reason) {
        this.reason = reason;
    }
}
