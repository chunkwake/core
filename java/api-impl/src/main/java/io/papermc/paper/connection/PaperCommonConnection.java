package io.papermc.paper.connection;

import net.minecraft.server.network.ServerCommonPacketListenerImpl;

/**
 * Paper API {@code PaperCommonConnection}: общий интерфейс соединения.
 *
 * <p>Emotecraft ищет здесь поле {@code packetListener} через
 * {@code MethodHandles.Lookup.findVarHandle}, поэтому поле должно быть
 * обычным (не static) и точно того типа.
 */
public class PaperCommonConnection<T extends ServerCommonPacketListenerImpl> {
    public ServerCommonPacketListenerImpl packetListener;

    protected PaperCommonConnection() {
    }

    public PaperCommonConnection(ServerCommonPacketListenerImpl packetListener) {
        this.packetListener = packetListener;
    }
}
