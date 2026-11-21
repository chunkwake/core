package net.minecraft.server.network;

/**
 * NMS-слушатель игрового соединения. Наследует {@code connection}
 * от {@link ServerCommonPacketListenerImpl}.
 */
public class ServerGamePacketListenerImpl extends ServerCommonPacketListenerImpl {
    /** Алиас, который ищет CraftEngine (getConnection). */
    public net.minecraft.network.Connection connection = super.connection;

    public ServerGamePacketListenerImpl() {
        super();
    }
}
