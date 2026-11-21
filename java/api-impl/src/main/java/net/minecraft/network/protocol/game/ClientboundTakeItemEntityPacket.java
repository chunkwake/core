package net.minecraft.network.protocol.game;

import net.minecraft.network.protocol.Packet;

/** NMS {@code ClientboundTakeItemEntityPacket} (CraftEngine). */
public class ClientboundTakeItemEntityPacket implements Packet {
    private int itemId;
    private int playerId;
    private int amount;

    public ClientboundTakeItemEntityPacket() {
    }

    public ClientboundTakeItemEntityPacket(int itemId, int playerId, int amount) {
        this.itemId = itemId;
        this.playerId = playerId;
        this.amount = amount;
    }
}
