package net.minecraft.server.network;

import net.minecraft.network.Connection;
import net.minecraft.network.protocol.Packet;

/**
 * Минимальный NMS-слушатель соединения. Нужен плагинам
 * (DecentHolograms/TAB), которые ищут поле {@code connection}.
 */
public class ServerCommonPacketListenerImpl implements ServerPlayerConnection {
    public final Connection connection;
    /** Поле, которое ищет CraftEngine (ServerCommonPacketListenerImplProxy). */
    public boolean closed;

    public ServerCommonPacketListenerImpl() {
        this(new Connection());
    }

    public ServerCommonPacketListenerImpl(Connection connection) {
        this.connection = connection;
    }

    @Override
    public void send(Packet packet) {
        // Плагины (DecentHolograms/TAB) шлют через
        // `player.getHandle().connection.send(packet)`, где `connection` —
        // этот слушатель. Делегируем в сетевой Connection (NMS→wire мост).
        if (this.connection != null) {
            this.connection.send(packet);
        }
    }
}
