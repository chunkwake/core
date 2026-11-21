package net.minecraft.network.protocol.game;

import java.util.Set;
import net.minecraft.network.protocol.Packet;
import net.minecraft.world.entity.PositionMoveRotation;

/** NMS {@code ClientboundTeleportEntityPacket}. */
public class ClientboundTeleportEntityPacket implements Packet {
    private int id;
    private final PositionMoveRotation change;
    private final Set<?> relatives;
    private boolean onGround;
    private double x;
    private double y;
    private double z;
    private byte yRot;
    private byte xRot;

    public ClientboundTeleportEntityPacket(int id, PositionMoveRotation change,
            Set<?> relatives, boolean onGround) {
        this.id = id;
        this.change = change;
        this.relatives = relatives == null ? Set.of() : relatives;
        this.onGround = onGround;
    }

    public int id() {
        return this.id;
    }

    public PositionMoveRotation change() {
        return this.change;
    }

    public Set<?> relatives() {
        return this.relatives;
    }

    public boolean onGround() {
        return this.onGround;
    }

    public int getId() { return this.id; }
    public void setId(int id) { this.id = id; }
    public boolean getOnGround() { return this.onGround; }
    public void setOnGround(boolean onGround) { this.onGround = onGround; }
    public double getX() { return this.x; }
    public void setX(double x) { this.x = x; }
    public double getY() { return this.y; }
    public void setY(double y) { this.y = y; }
    public double getZ() { return this.z; }
    public void setZ(double z) { this.z = z; }
    public byte getYRot() { return this.yRot; }
    public void setYRot(byte yRot) { this.yRot = yRot; }
    public byte getXRot() { return this.xRot; }
    public void setXRot(byte xRot) { this.xRot = xRot; }
}
