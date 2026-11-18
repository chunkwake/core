package net.minecraft.network.protocol.common;

import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.common.custom.CustomPacketPayload;

/** NMS {@code ClientboundCustomPayloadPacket}. */
public class ClientboundCustomPayloadPacket implements Packet {
    private final CustomPacketPayload payload;

    public ClientboundCustomPayloadPacket(CustomPacketPayload payload) {
        this.payload = payload;
    }

    public CustomPacketPayload payload() {
        return this.payload;
    }
}
