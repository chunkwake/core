package net.minecraft.network.protocol.login;

import net.minecraft.network.chat.Component;
import net.minecraft.network.protocol.Packet;

/** NMS {@code ClientboundLoginDisconnectPacket} (CraftEngine). */
public class ClientboundLoginDisconnectPacket implements Packet {
    private Component reason;

    public ClientboundLoginDisconnectPacket() {
    }

    public ClientboundLoginDisconnectPacket(Component reason) {
        this.reason = reason;
    }
}
