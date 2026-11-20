package net.minecraft.network.protocol.common;

import net.minecraft.network.codec.StreamCodec;
import net.minecraft.network.protocol.common.custom.CustomPacketPayload;

/** NMS {@code ServerboundCustomPayloadPacket} (Emotecraft). */
public class ServerboundCustomPayloadPacket {
    public static final StreamCodec<Object> STREAM_CODEC =
            net.minecraft.network.codec.SimpleStreamCodec.INSTANCE;

    private final CustomPacketPayload payload;

    public ServerboundCustomPayloadPacket(CustomPacketPayload payload) {
        this.payload = payload;
    }

    public CustomPacketPayload payload() {
        return this.payload;
    }
}
