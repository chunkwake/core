package net.minecraft.network.protocol.common;

import java.util.Map;
import net.minecraft.network.protocol.Packet;

/** NMS {@code ClientboundUpdateTagsPacket} (CraftEngine). */
public class ClientboundUpdateTagsPacket implements Packet {
    private Map<?, ?> tags;

    public ClientboundUpdateTagsPacket() {
    }

    public ClientboundUpdateTagsPacket(Map<?, ?> tags) {
        this.tags = tags;
    }

    public Map<?, ?> getTags() {
        return this.tags;
    }
}
