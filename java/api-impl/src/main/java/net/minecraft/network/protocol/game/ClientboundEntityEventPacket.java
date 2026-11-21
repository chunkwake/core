package net.minecraft.network.protocol.game;

import net.minecraft.network.protocol.Packet;
import net.minecraft.world.entity.Entity;

/** NMS {@code ClientboundEntityEventPacket} (CraftEngine). */
public class ClientboundEntityEventPacket implements Packet {
    private int entityId;
    private byte eventId;

    public ClientboundEntityEventPacket() {
    }

    public ClientboundEntityEventPacket(Entity entity, byte eventId) {
        this(entity == null ? 0 : entity.getId(), eventId);
    }

    public ClientboundEntityEventPacket(int entityId, byte eventId) {
        this.entityId = entityId;
        this.eventId = eventId;
    }

    public int getEntityId() {
        return this.entityId;
    }

    public byte getEventId() {
        return this.eventId;
    }
}
