package net.minecraft.server.network;

import io.netty.channel.ChannelFuture;
import java.util.List;
import java.util.concurrent.CopyOnWriteArrayList;

/**
 * NMS {@code ServerConnectionListener}.
 *
 * <p>PacketEvents (GrimAC) читает {@code getDeclaredFields()} конкретного
 * класса, поэтому оба списка объявлены здесь (не в родителе): каналы и
 * принятые соединения. Наследование от легаси {@link ServerConnection}
 * оставляет ProtocolLib геттер с ожидаемым типом {@code ServerConnection}.
 */
public class ServerConnectionListener extends ServerConnection {
    /** Слушающие каналы (vanilla {@code channels}). */
    public final List<ChannelFuture> channels = new CopyOnWriteArrayList<>();
    /** Принятые соединения (vanilla {@code connections}). */
    public final List<net.minecraft.network.Connection> nmsConnections =
            new CopyOnWriteArrayList<>();
}
