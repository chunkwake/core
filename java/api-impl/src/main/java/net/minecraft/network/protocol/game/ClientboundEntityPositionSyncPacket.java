package net.minecraft.network.protocol.game;

import net.minecraft.network.protocol.Packet;
import net.minecraft.world.entity.PositionPath;

/** NMS {@code ClientboundEntityPositionSyncPacket} (CraftEngine). */
public class ClientboundEntityPositionSyncPacket implements Packet {
    private int id;
    private PositionPath position;
    private float yRot;
    private float xRot;
    private boolean onGround;

    public ClientboundEntityPositionSyncPacket() {
    }

    public ClientboundEntityPositionSyncPacket(int id, PositionPath position, float yRot,
            float xRot, boolean onGround) {
        this.id = id;
        this.position = position;
        this.yRot = yRot;
        this.xRot = xRot;
        this.onGround = onGround;
    }

    public int getId() { return this.id; }
    public PositionPath getValues() { return this.position; }
    public boolean getOnGround() { return this.onGround; }
}
