package net.minecraft.network.protocol.game;

import net.minecraft.network.protocol.Packet;

/** NMS {@code ClientboundServerDataPacket} (CraftEngine). */
public class ClientboundServerDataPacket implements Packet {
    private boolean enforcesSecureChat;

    public ClientboundServerDataPacket() {
    }

    public void setEnforcesSecureChat(boolean enforcesSecureChat) {
        this.enforcesSecureChat = enforcesSecureChat;
    }
}
