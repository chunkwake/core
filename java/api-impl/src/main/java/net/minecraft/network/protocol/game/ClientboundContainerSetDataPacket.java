package net.minecraft.network.protocol.game;

import net.minecraft.network.protocol.Packet;

/** NMS {@code ClientboundContainerSetDataPacket} (CraftEngine). */
public class ClientboundContainerSetDataPacket implements Packet {
    private int containerId;
    private int id;
    private int value;

    public ClientboundContainerSetDataPacket() {
    }

    public ClientboundContainerSetDataPacket(int containerId, int id, int value) {
        this.containerId = containerId;
        this.id = id;
        this.value = value;
    }
}
