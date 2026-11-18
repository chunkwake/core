package net.minecraft.network.protocol.game;

import net.minecraft.network.protocol.Packet;
import net.minecraft.network.chat.Component;

/** NMS {@code ClientboundSystemChatPacket}: текст + overlay (action bar). */
public class ClientboundSystemChatPacket implements Packet {
    private Component content;
    private boolean overlay;

    public ClientboundSystemChatPacket() {
    }

    public ClientboundSystemChatPacket(Component content, boolean overlay) {
        this.content = content;
        this.overlay = overlay;
    }

    public Component content() {
        return this.content;
    }

    public boolean overlay() {
        return this.overlay;
    }
}
