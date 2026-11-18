package net.minecraft.network.protocol.game;

import net.minecraft.network.protocol.Packet;

/**
 * Заглушка серверного пакета {@code ChatCommand}.
 *
 * <p>Имя совпадает с тем, что ProtocolLib выводит для
 * {@code PacketType.Play.Client.CHAT_COMMAND} (Mojang-маппинги). Поле
 * {@code command} даёт {@code PacketContainer.getStrings().read(0)}.
 */
public class ServerboundChatCommandPacket implements Packet {
    public String command;

    public ServerboundChatCommandPacket() {
    }

    public ServerboundChatCommandPacket(String command) {
        this.command = command;
    }
}
