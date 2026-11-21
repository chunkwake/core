package net.minecraft.network.protocol.game;

import net.minecraft.network.protocol.Packet;
import net.minecraft.resources.ResourceKey;

/** NMS {@code ClientboundLoginPacket} (CraftEngine). */
public class ClientboundLoginPacket implements Packet {
    private ResourceKey<?> dimension;
    private CommonPlayerSpawnInfo commonPlayerSpawnInfo;
    private boolean enforcesSecureChat;

    public ClientboundLoginPacket() {
    }

    public ClientboundLoginPacket(CommonPlayerSpawnInfo spawnInfo) {
        this.commonPlayerSpawnInfo = spawnInfo;
    }

    public ResourceKey<?> getDimension() {
        return this.dimension;
    }

    public CommonPlayerSpawnInfo getCommonPlayerSpawnInfo() {
        return this.commonPlayerSpawnInfo;
    }

    public void setEnforcesSecureChat(boolean enforcesSecureChat) {
        this.enforcesSecureChat = enforcesSecureChat;
    }
}
