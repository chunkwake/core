package net.minecraft.network.protocol.game;

import net.minecraft.network.protocol.Packet;

/** NMS {@code ClientboundSetHealthPacket} (CraftEngine). */
public class ClientboundSetHealthPacket implements Packet {
    private float health;
    private int food;
    private float saturation;

    public ClientboundSetHealthPacket() {
    }

    public ClientboundSetHealthPacket(float health, int food, float saturation) {
        this.health = health;
        this.food = food;
        this.saturation = saturation;
    }
}
