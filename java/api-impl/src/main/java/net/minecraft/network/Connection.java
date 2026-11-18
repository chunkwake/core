package net.minecraft.network;

import io.netty.channel.Channel;
import io.netty.channel.embedded.EmbeddedChannel;

/**
 * Минимальное NMS {@code Connection}: хранит Netty-канал. Нужно TAB
 * ({@code ServerPlayer.connection.connection.channel}) и подобным плагинам.
 */
public class Connection {
    public Channel channel;

    public Connection() {
        this.channel = new EmbeddedChannel();
    }
}
