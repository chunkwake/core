package net.minecraft.network.protocol.game;

import net.minecraft.network.protocol.Packet;
import net.minecraft.world.InteractionHand;

/** NMS {@code ServerboundSwingPacket} (CraftEngine). */
public class ServerboundSwingPacket implements Packet {
    private InteractionHand hand = InteractionHand.MAIN_HAND;

    public ServerboundSwingPacket() {
    }

    public ServerboundSwingPacket(InteractionHand hand) {
        this.hand = hand;
    }

    public InteractionHand getHand() { return this.hand; }
    public void setHand(InteractionHand hand) { this.hand = hand; }
}
