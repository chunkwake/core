package net.minecraft.network.protocol.game;

import net.minecraft.network.chat.Component;
import net.minecraft.network.protocol.Packet;

/** NMS {@code ClientboundSetSubtitleTextPacket} (CraftEngine). */
public class ClientboundSetSubtitleTextPacket implements Packet {
    private Component text;

    public ClientboundSetSubtitleTextPacket() {
    }

    public ClientboundSetSubtitleTextPacket(Component text) {
        this.text = text;
    }
}
