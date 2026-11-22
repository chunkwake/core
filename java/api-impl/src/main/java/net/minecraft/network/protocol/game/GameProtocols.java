package net.minecraft.network.protocol.game;

/** NMS {@code GameProtocols}. */
public class GameProtocols {
    public static final net.minecraft.network.protocol.UnboundProtocol<ServerGamePacketListener,
            net.minecraft.network.RegistryFriendlyByteBuf, Context> SERVERBOUND_TEMPLATE = net.minecraft.network.protocol.EmptyProtocols.unbound(net.minecraft.network.ConnectionProtocol.PLAY, net.minecraft.network.protocol.PacketFlow.SERVERBOUND);
    public static final net.minecraft.network.ProtocolInfo<ServerGamePacketListener> SERVERBOUND = null;
    public static final net.minecraft.network.protocol.SimpleUnboundProtocol<ClientGamePacketListener,
            net.minecraft.network.RegistryFriendlyByteBuf> CLIENTBOUND_TEMPLATE = net.minecraft.network.protocol.EmptyProtocols.simple(net.minecraft.network.ConnectionProtocol.PLAY, net.minecraft.network.protocol.PacketFlow.CLIENTBOUND);
    public static final net.minecraft.network.ProtocolInfo<ClientGamePacketListener> CLIENTBOUND = null;

    public GameProtocols() {
    }

    /** NMS {@code GameProtocols.Context}. */
    public static class Context {
        public Context() {
        }
    }
}
