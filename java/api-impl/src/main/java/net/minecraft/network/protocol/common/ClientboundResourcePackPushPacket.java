package net.minecraft.network.protocol.common;

import java.util.Optional;
import java.util.UUID;
import net.minecraft.network.protocol.Packet;

/** NMS {@code ClientboundResourcePackPushPacket} (CraftEngine). */
public class ClientboundResourcePackPushPacket implements Packet {
    public ClientboundResourcePackPushPacket() {
    }

    public ClientboundResourcePackPushPacket(UUID id, String url, String hash,
            boolean required, Optional<?> prompt) {
    }

    public ClientboundResourcePackPushPacket(UUID id, String url, String hash,
            boolean required, Object prompt) {
    }

    public ClientboundResourcePackPushPacket(String url, String hash,
            boolean required, Object prompt) {
    }
}
