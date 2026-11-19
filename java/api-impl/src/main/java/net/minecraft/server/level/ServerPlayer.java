package net.minecraft.server.level;

import net.minecraft.server.network.ServerGamePacketListenerImpl;

/**
 * Минимальный NMS {@code ServerPlayer}: плагины берут его через
 * {@code CraftPlayer.getHandle()} и читают поле {@code connection}.
 */
public class ServerPlayer extends net.minecraft.world.entity.player.Player {
    public ServerGamePacketListenerImpl connection;

    public ServerPlayer() {
        this.connection = new ServerGamePacketListenerImpl();
    }

    private final net.minecraft.server.level.ServerLevel level =
            new net.minecraft.server.level.ServerLevel();

    /** {@code ServerPlayer.level()} (Orebfuscator). */
    public net.minecraft.server.level.ServerLevel level() {
        return this.level;
    }

    public net.minecraft.core.RegistryAccess registryAccess() {
        return net.minecraft.core.RegistryAccess.EMPTY_REGISTRY_ACCESS();
    }
}
