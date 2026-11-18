package net.minecraft.server.network;

import io.netty.channel.ChannelFuture;
import java.util.List;
import java.util.concurrent.CopyOnWriteArrayList;

/**
 * Минимальный NMS-класс для ProtocolLib.
 *
 * <p>ProtocolLib ищет в этом классе поля {@code List<ChannelFuture>} и
 * оборачивает их своим {@code ListeningList}, чтобы вешать обработчики на
 * новые каналы. Пока Java-сторона не владеет реальными Netty-каналами
 * (сеть в Rust), список остаётся пустым — но цепочка инициализации
 * ProtocolLib проходит.
 */
public class ServerConnection {
    public final List<ChannelFuture> connections = new CopyOnWriteArrayList<>();
}
