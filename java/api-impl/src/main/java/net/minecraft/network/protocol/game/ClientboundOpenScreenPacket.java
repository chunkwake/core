package net.minecraft.network.protocol.game;

import net.minecraft.network.chat.Component;
import net.minecraft.network.protocol.Packet;
import net.minecraft.world.inventory.MenuType;

/** NMS {@code ClientboundOpenScreenPacket} (CraftEngine). */
public class ClientboundOpenScreenPacket implements Packet {
    private int containerId;
    private MenuType<?> type;
    private Component title;

    public ClientboundOpenScreenPacket() {
    }

    public ClientboundOpenScreenPacket(int containerId, MenuType<?> type, Component title) {
        this.containerId = containerId;
        this.type = type;
        this.title = title;
    }
}
