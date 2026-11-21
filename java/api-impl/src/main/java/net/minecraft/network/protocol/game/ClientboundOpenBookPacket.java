package net.minecraft.network.protocol.game;

import net.minecraft.network.protocol.Packet;
import net.minecraft.world.InteractionHand;

/** NMS {@code ClientboundOpenBookPacket} (CraftEngine). */
public class ClientboundOpenBookPacket implements Packet {
    private InteractionHand hand;

    public ClientboundOpenBookPacket() {
    }

    public ClientboundOpenBookPacket(InteractionHand hand) {
        this.hand = hand;
    }
}
