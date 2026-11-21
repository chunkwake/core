package net.minecraft.network.protocol.game;

import net.minecraft.network.protocol.Packet;

/** NMS {@code ServerboundPickItemFromEntityPacket} (CraftEngine). */
public class ServerboundPickItemFromEntityPacket implements Packet {
    private int id;

    public ServerboundPickItemFromEntityPacket() {
    }

    public ServerboundPickItemFromEntityPacket(int id, boolean includeData) {
        this.id = id;
    }

    public int getId() { return this.id; }
}
