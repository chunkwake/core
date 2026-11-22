package net.minecraft.network.protocol.status;

/** NMS {@code StatusProtocols}. */
public class StatusProtocols {
    public static final net.minecraft.network.protocol.SimpleUnboundProtocol<ServerStatusPacketListener, io.netty.buffer.ByteBuf>
            SERVERBOUND_TEMPLATE = net.minecraft.network.protocol.EmptyProtocols.simple(net.minecraft.network.ConnectionProtocol.STATUS, net.minecraft.network.protocol.PacketFlow.SERVERBOUND);
    public static final net.minecraft.network.ProtocolInfo<ServerStatusPacketListener> SERVERBOUND = null;
    public static final net.minecraft.network.protocol.SimpleUnboundProtocol<ClientStatusPacketListener, net.minecraft.network.FriendlyByteBuf>
            CLIENTBOUND_TEMPLATE = net.minecraft.network.protocol.EmptyProtocols.simple(net.minecraft.network.ConnectionProtocol.STATUS, net.minecraft.network.protocol.PacketFlow.CLIENTBOUND);
    public static final net.minecraft.network.ProtocolInfo<ClientStatusPacketListener> CLIENTBOUND = null;

    public StatusProtocols() {
    }
}
