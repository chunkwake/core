package net.minecraft.server.network;

import net.minecraft.network.Connection;

/**
 * NMS-слушатель игрового соединения. Плагины (DecentHolograms, TAB) получают
 * его из {@code ServerPlayer.connection} и вызывают {@code send(Packet)}.
 */
public class ServerGamePacketListenerImpl extends ServerCommonPacketListenerImpl {
    public final Connection connection;

    public ServerGamePacketListenerImpl() {
        this.connection = new Connection();
    }
}
