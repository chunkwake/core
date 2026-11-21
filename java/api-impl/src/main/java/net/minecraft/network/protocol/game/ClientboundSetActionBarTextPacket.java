package net.minecraft.network.protocol.game;

import net.minecraft.network.chat.Component;
import net.minecraft.network.protocol.Packet;

/** NMS {@code ClientboundSetActionBarTextPacket} (CraftEngine). */
public class ClientboundSetActionBarTextPacket implements Packet {
    private Component text;

    public ClientboundSetActionBarTextPacket() {
    }

    public ClientboundSetActionBarTextPacket(Component text) {
        this.text = text;
    }
}
