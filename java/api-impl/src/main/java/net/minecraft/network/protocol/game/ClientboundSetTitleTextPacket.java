package net.minecraft.network.protocol.game;

import net.minecraft.network.chat.Component;
import net.minecraft.network.protocol.Packet;

/** NMS {@code ClientboundSetTitleTextPacket} (реальное поле text). */
public class ClientboundSetTitleTextPacket implements Packet {
    private Component text;

    public ClientboundSetTitleTextPacket() {
    }

    public ClientboundSetTitleTextPacket(Component text) {
        this.text = text;
    }

    public Component text() {
        return this.text;
    }
}
