package net.minecraft.network.protocol.common;

import java.util.Optional;
import java.util.UUID;
import net.minecraft.network.protocol.Packet;

/** NMS {@code ClientboundResourcePackPopPacket} (CraftEngine). */
public class ClientboundResourcePackPopPacket implements Packet {
    public ClientboundResourcePackPopPacket() {
    }

    public ClientboundResourcePackPopPacket(Optional<UUID> id) {
    }
}
