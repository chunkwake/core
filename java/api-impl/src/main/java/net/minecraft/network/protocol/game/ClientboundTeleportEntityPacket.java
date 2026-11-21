package net.minecraft.network.protocol.game;

import java.util.Set;
import net.minecraft.network.protocol.Packet;
import net.minecraft.world.entity.PositionMoveRotation;

/** NMS {@code ClientboundTeleportEntityPacket}. */
public class ClientboundTeleportEntityPacket implements Packet {
    private final int id;
    private final PositionMoveRotation change;
    private final Set<?> relatives;
    private final boolean onGround;
    /** Поле, которое ищет CraftEngine (getYRot). */
    private float yRot;

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
}
