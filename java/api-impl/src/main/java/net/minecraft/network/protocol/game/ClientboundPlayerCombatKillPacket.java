package net.minecraft.network.protocol.game;

import net.minecraft.network.chat.Component;
import net.minecraft.network.protocol.Packet;

/** NMS {@code ClientboundPlayerCombatKillPacket} (CraftEngine). */
public class ClientboundPlayerCombatKillPacket implements Packet {
    private int playerId;
    private Component message;

    public ClientboundPlayerCombatKillPacket() {
    }

    public ClientboundPlayerCombatKillPacket(int playerId, Component message) {
        this.playerId = playerId;
        this.message = message;
    }
}
