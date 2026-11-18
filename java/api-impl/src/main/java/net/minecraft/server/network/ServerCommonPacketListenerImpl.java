package net.minecraft.server.network;

import net.minecraft.network.protocol.Packet;

/**
 * База NMS-слушателя соединения. Нужна плагинам (DecentHolograms/TAB),
 * которые ищут {@code ServerCommonPacketListenerImpl} и вызывают
 * {@code send(Packet)}.
 */
public class ServerCommonPacketListenerImpl implements ServerPlayerConnection {
    @Override
    public void send(Packet packet) {
        // Пакеты Java-стороны пока не уходят в Rust-транспорт; точка для
        // будущего моста. Вызов не должен ломать плагин.
    }
}
