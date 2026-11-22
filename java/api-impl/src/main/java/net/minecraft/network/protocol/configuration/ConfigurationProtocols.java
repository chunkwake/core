package net.minecraft.network.protocol.configuration;

/** NMS {@code ConfigurationProtocols}. */
public class ConfigurationProtocols {
    public static final net.minecraft.network.protocol.SimpleUnboundProtocol<ServerConfigurationPacketListener, net.minecraft.network.FriendlyByteBuf>
            SERVERBOUND_TEMPLATE = net.minecraft.network.protocol.EmptyProtocols.simple(net.minecraft.network.ConnectionProtocol.CONFIGURATION, net.minecraft.network.protocol.PacketFlow.SERVERBOUND);
    public static final net.minecraft.network.ProtocolInfo<ServerConfigurationPacketListener> SERVERBOUND = null;
    public static final net.minecraft.network.protocol.SimpleUnboundProtocol<ClientConfigurationPacketListener, net.minecraft.network.FriendlyByteBuf>
            CLIENTBOUND_TEMPLATE = net.minecraft.network.protocol.EmptyProtocols.simple(net.minecraft.network.ConnectionProtocol.CONFIGURATION, net.minecraft.network.protocol.PacketFlow.CLIENTBOUND);
    public static final net.minecraft.network.ProtocolInfo<ClientConfigurationPacketListener> CLIENTBOUND = null;

    public ConfigurationProtocols() {
    }
}
