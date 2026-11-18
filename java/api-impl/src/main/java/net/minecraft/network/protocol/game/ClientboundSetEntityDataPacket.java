package net.minecraft.network.protocol.game;

import net.minecraft.network.protocol.Packet;

/** NMS {@code ClientboundSetEntityDataPacket} (id + opaque metadata). */
public class ClientboundSetEntityDataPacket implements Packet {
    private int id;
    private byte[] packedItems = new byte[0];

    public ClientboundSetEntityDataPacket() {
    }

    public ClientboundSetEntityDataPacket(int id, byte[] packedItems) {
        this.id = id;
        this.packedItems = packedItems == null ? new byte[0] : packedItems;
    }

    public int id() {
        return this.id;
    }

    public byte[] packedItems() {
        return this.packedItems;
    }
}
