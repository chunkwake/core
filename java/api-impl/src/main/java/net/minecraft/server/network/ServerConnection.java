package net.minecraft.server.network;

import io.netty.channel.ChannelFuture;
import java.util.List;
import java.util.concurrent.CopyOnWriteArrayList;

/**
 * Легаси NMS {@code ServerConnection} (ProtocolLib).
 *
 * <p>ProtocolLib ищет у {@code MinecraftServer} геттер, возвращающий именно
 * этот тип, и затем список {@code List<ChannelFuture>}.
 */
public class ServerConnection {
    public final List<ChannelFuture> connections = new CopyOnWriteArrayList<>();
}
