package net.minecraft.server.network;

import net.minecraft.network.protocol.Packet;

/** Интерфейс NMS-соединения игрока, которое умеет отправлять пакеты. */
public interface ServerPlayerConnection {
    void send(Packet packet);
}
