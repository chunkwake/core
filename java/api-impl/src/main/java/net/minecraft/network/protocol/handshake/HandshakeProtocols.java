package net.minecraft.network.protocol.handshake;

/** NMS {@code HandshakeProtocols}. */
public class HandshakeProtocols {
    public static final net.minecraft.network.protocol.SimpleUnboundProtocol<ServerHandshakePacketListener, net.minecraft.network.FriendlyByteBuf> SERVERBOUND_TEMPLATE = net.minecraft.network.protocol.EmptyProtocols.simple(net.minecraft.network.ConnectionProtocol.HANDSHAKING, net.minecraft.network.protocol.PacketFlow.SERVERBOUND);
    public static final net.minecraft.network.ProtocolInfo<ServerHandshakePacketListener> SERVERBOUND = null;

    public HandshakeProtocols() {
    }
}
