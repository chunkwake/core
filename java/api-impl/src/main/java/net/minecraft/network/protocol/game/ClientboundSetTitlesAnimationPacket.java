package net.minecraft.network.protocol.game;

import net.minecraft.network.protocol.Packet;

/** NMS {@code ClientboundSetTitlesAnimationPacket} (CraftEngine). */
public class ClientboundSetTitlesAnimationPacket implements Packet {
    private int fadeIn;
    private int stay;
    private int fadeOut;

    public ClientboundSetTitlesAnimationPacket() {
    }

    public ClientboundSetTitlesAnimationPacket(int fadeIn, int stay, int fadeOut) {
        this.fadeIn = fadeIn;
        this.stay = stay;
        this.fadeOut = fadeOut;
    }
}
