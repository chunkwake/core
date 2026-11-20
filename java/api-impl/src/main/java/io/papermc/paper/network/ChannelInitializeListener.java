package io.papermc.paper.network;

import io.netty.channel.Channel;

/** Paper API: слушатель инициализации netty-канала (Emotecraft). */
public interface ChannelInitializeListener {
    void afterInitChannel(Channel channel);
}
