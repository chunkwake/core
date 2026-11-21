package net.minecraft.network.protocol.game;

import net.minecraft.network.protocol.Packet;

/** NMS {@code ServerboundRenameItemPacket} (CraftEngine). */
public class ServerboundRenameItemPacket implements Packet {
    private String name = "";

    public ServerboundRenameItemPacket() {
    }

    public ServerboundRenameItemPacket(String name) {
        this.name = name;
    }

    public String getName() { return this.name; }
    public void setName(String name) { this.name = name; }
}
