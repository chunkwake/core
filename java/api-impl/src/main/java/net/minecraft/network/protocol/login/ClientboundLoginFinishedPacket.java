package net.minecraft.network.protocol.login;

import com.mojang.authlib.GameProfile;
import net.minecraft.network.protocol.Packet;

/** NMS {@code ClientboundLoginFinishedPacket} (CraftEngine). */
public class ClientboundLoginFinishedPacket implements Packet {
    private GameProfile gameProfile;

    public ClientboundLoginFinishedPacket() {
    }

    public ClientboundLoginFinishedPacket(GameProfile gameProfile) {
        this.gameProfile = gameProfile;
    }

    public GameProfile getGameProfile() {
        return this.gameProfile;
    }
}
