package net.minecraft.network;

import io.netty.channel.Channel;
import io.netty.channel.ChannelHandlerContext;
import io.netty.channel.ChannelInboundHandlerAdapter;
import io.netty.channel.ChannelOutboundHandlerAdapter;
import io.netty.channel.embedded.EmbeddedChannel;
import net.minecraft.network.chat.Component;
import net.minecraft.network.protocol.Packet;

/**
 * Минимальное NMS {@code Connection}: транспортный фасад для ProtocolLib
 * (TAB/DecentHolograms и инжектор). Реальный ввод-вывод идёт через Rust.
 *
 * <p>В пайплайне есть заглушки {@code encoder}/{@code decoder}: инжектор
 * ProtocolLib ищет именно их, чтобы вставить свои обработчики.
 */
public class Connection {
    public Channel channel;
    private PacketListener listener;

    public Connection() {
        EmbeddedChannel embedded = new EmbeddedChannel();
        embedded.pipeline().addLast("encoder", new ChannelOutboundHandlerAdapter());
        embedded.pipeline().addLast("decoder", new ChannelInboundHandlerAdapter());
        // `packet_handler` нужен плагинам (DecentHolograms), которые
        // вставляют свои обработчики через addBefore("packet_handler", ...).
        embedded.pipeline().addLast("packet_handler", new ChannelInboundHandlerAdapter());
        this.channel = embedded;
    }

    public void send(Packet packet) {
    }

    public void read(ChannelHandlerContext context, Packet packet) {
    }

    public void disconnect(Component reason) {
    }

    public PacketListener getPacketListener() {
        return this.listener;
    }

    public void setPacketListener(PacketListener listener) {
        this.listener = listener;
    }
}
