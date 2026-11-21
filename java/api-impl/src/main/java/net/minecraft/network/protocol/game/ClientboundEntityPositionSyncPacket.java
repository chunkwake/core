package net.minecraft.network.protocol.game;

import net.minecraft.network.protocol.Packet;
import net.minecraft.world.entity.PositionMoveRotation;

/** NMS {@code ClientboundEntityPositionSyncPacket} (CraftEngine). */
public class ClientboundEntityPositionSyncPacket implements Packet {
    private int id;
    private PositionMoveRotation values;
    private float yRot;
    private float xRot;
    private boolean onGround;

    public ClientboundEntityPositionSyncPacket() {
    }

    public ClientboundEntityPositionSyncPacket(int id, PositionMoveRotation values, float yRot,
            float xRot, boolean onGround) {
        this.id = id;
        this.values = values;
        this.yRot = yRot;
        this.xRot = xRot;
        this.onGround = onGround;
    }

    public int getId() {
        return this.id;
    }

    public PositionMoveRotation getValues() {
        return this.values;
    }

    public boolean getOnGround() {
        return this.onGround;
    }
}
