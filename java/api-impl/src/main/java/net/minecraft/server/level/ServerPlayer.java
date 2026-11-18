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
}
