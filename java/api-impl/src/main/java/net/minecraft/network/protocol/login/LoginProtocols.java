package net.minecraft.network.protocol.login;

/** NMS {@code LoginProtocols}. */
public class LoginProtocols {
    public static final net.minecraft.network.protocol.SimpleUnboundProtocol<ServerLoginPacketListener, net.minecraft.network.FriendlyByteBuf>
            SERVERBOUND_TEMPLATE = net.minecraft.network.protocol.EmptyProtocols.simple(net.minecraft.network.ConnectionProtocol.LOGIN, net.minecraft.network.protocol.PacketFlow.SERVERBOUND);
    public static final net.minecraft.network.ProtocolInfo<ServerLoginPacketListener> SERVERBOUND = null;
    public static final net.minecraft.network.protocol.SimpleUnboundProtocol<ClientLoginPacketListener, net.minecraft.network.FriendlyByteBuf>
            CLIENTBOUND_TEMPLATE = net.minecraft.network.protocol.EmptyProtocols.simple(net.minecraft.network.ConnectionProtocol.LOGIN, net.minecraft.network.protocol.PacketFlow.CLIENTBOUND);
    public static final net.minecraft.network.ProtocolInfo<ClientLoginPacketListener> CLIENTBOUND = null;

    public LoginProtocols() {
    }
}
