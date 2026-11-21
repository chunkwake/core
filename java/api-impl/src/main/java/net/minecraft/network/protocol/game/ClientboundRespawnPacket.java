package net.minecraft.network.protocol.game;

import net.minecraft.network.protocol.Packet;
import net.minecraft.resources.ResourceKey;

/** NMS {@code ClientboundRespawnPacket} (CraftEngine). */
public class ClientboundRespawnPacket implements Packet {
    private ResourceKey<?> dimension;
    private CommonPlayerSpawnInfo commonPlayerSpawnInfo;
    private byte dataToKeep;

    public ClientboundRespawnPacket() {
    }

    public ClientboundRespawnPacket(CommonPlayerSpawnInfo info, byte dataToKeep) {
        this.commonPlayerSpawnInfo = info;
        this.dataToKeep = dataToKeep;
    }

    public ResourceKey<?> getDimension() {
        return this.dimension;
    }

    public CommonPlayerSpawnInfo getCommonPlayerSpawnInfo() {
        return this.commonPlayerSpawnInfo;
    }
}
