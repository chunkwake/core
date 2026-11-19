package net.minecraft.server.network;

import net.minecraft.network.Connection;
import net.minecraft.network.protocol.Packet;

/**
 * Минимальный NMS-слушатель соединения. Нужен плагинам
 * (DecentHolograms/TAB), которые ищут поле {@code connection}.
 */
public class ServerCommonPacketListenerImpl implements ServerPlayerConnection {
    public final Connection connection;

    public ServerCommonPacketListenerImpl() {
        this(new Connection());
    }

    public ServerCommonPacketListenerImpl(Connection connection) {
        this.connection = connection;
    }

    @Override
    public void send(Packet packet) {
        // Отправкой управляет Rust-транспорт; здесь пусто.
    }
}
